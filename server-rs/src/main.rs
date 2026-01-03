use axum::{
    routing::{get, any},
    Router,
};
mod json;
mod env;
mod heli;
mod routes;
mod middleware;
mod database;
mod auth;
mod models;
mod oauth;
mod loadbalancer;
mod utils;
mod cli;
mod daemon;

// our mods

use crate::middleware::ratelimit::{RateLimiter, rate_limit_middleware};
use axum::response::Html;
use std::fs;
use std::sync::Arc;
use colorized::{Color, Colors, colorize_println};

#[tokio::main]
async fn main() {
    use clap::Parser;
    
    // Parse CLI arguments
    let cli = cli::Cli::parse();
    
    match cli.command {
        cli::Commands::Users(users_cmd) => {
            // Handle user commands
            if let Err(e) = cli::users::handle_users_command(users_cmd).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
            return;
        }
        cli::Commands::Serve { config } => {
            // Continue with server startup
            welcome().await;
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::INFO)
                .init();
            
            start_server(&config).await;
        }
    }
}

async fn start_server(config_path: &str) {
    let heli_config = heli::parse::HeliConfig::parse(config_path)
        .expect(&format!("Failed to parse config file: {}", config_path));
    
    // Connect to MongoDB - check for environment variable first
    let mongo_uri = if let Ok(env_uri) = std::env::var("MONGODB") {
        env_uri
    } else {
        let db_host = heli_config.get_string("database.host").unwrap_or("localhost".to_string());
        let db_port = heli_config.get_int("database.port").unwrap_or(27017);
        let db_username = heli_config.get_string("database.username").unwrap_or("admin".to_string());
        let db_password = heli_config.get_string("database.password").unwrap_or("".to_string());
        
        // Check if it's MongoDB Atlas (contains .mongodb.net)
        if db_host.contains(".mongodb.net") {
            // MongoDB Atlas connection string with SRV
            if db_password.is_empty() {
                format!("mongodb+srv://{}/?retryWrites=true&w=majority&appName=HeliactylRust", db_host)
            } else {
                format!("mongodb+srv://{}:{}@{}/?retryWrites=true&w=majority&appName=HeliactylRust", 
                    db_username, db_password, db_host)
            }
        } else {
            // Local MongoDB connection string
            if db_password.is_empty() {
                format!("mongodb://{}:{}", db_host, db_port)
            } else {
                format!("mongodb://{}:{}@{}:{}", 
                    db_username, db_password, db_host, db_port)
            }
        }
    };
    
    let db_name = heli_config.get_string("database.name").unwrap_or("Heli-db".to_string());
    
    colorize_println("Database Configuration:", Colors::BrightGreenFg);
    colorize_println("  -> Connecting to MongoDB...", Colors::BrightCyanFg);
    
    let mongo_client = database::mongo::MongoClient::new(&mongo_uri, &db_name)
        .await
        .expect("Failed to connect to MongoDB");
    
    match mongo_client.ping().await {
        Ok(_) => colorize_println("  ✓ MongoDB connected successfully!", Colors::BrightGreenFg),
        Err(e) => {
            colorize_println(&format!("  ✗ MongoDB connection failed: {}", e), Colors::BrightRedFg);
            std::process::exit(1);
        }
    }

    let uptime_file = heli_config.get_string("uptime.file").unwrap_or("data/uptime.json".to_string());
    let uptime_tracker = env::uptime::UptimeTracker::start(&uptime_file);
    env::uptime::UptimeTracker::save();
    println!("Uptime tracker started: {:?}", uptime_tracker);
    colorize_println("Starting server...", Colors::BrightGreenFg);
    
    // Setup rate limiter with Redis
    let redis_host = heli_config.get_string("redis.host").unwrap_or("127.0.0.1".to_string());
    let redis_port = heli_config.get_int("redis.port").unwrap_or(6379);
    let redis_url = format!("redis://{}:{}", redis_host, redis_port);
    let per_sec = heli_config.get_int("ratelimit.persec").unwrap_or(10) as u32;
    let per_min = heli_config.get_int("ratelimit.permin").unwrap_or(120) as u32;
    let rate_limiter = Arc::new(RateLimiter::new(&redis_url, per_sec, per_min).expect("Failed to connect to Redis"));

    // Setup session manager
    let session_manager = Arc::new(
        database::session::SessionManager::new(&redis_url)
            .expect("Failed to create session manager")
    );

    // Setup audit service
    let audit_service = Arc::new(database::audit::AuditService::new(Arc::new(mongo_client.clone())));
    colorize_println("  ✓ Audit service initialized", Colors::BrightGreenFg);

    // Setup daemon client
    let daemon_host = heli_config.get_string("daemon.host").unwrap_or("localhost".to_string());
    let daemon_port = heli_config.get_int("daemon.port").unwrap_or(8083);
    let daemon_api_key = heli_config.get_string("daemon.api_key").unwrap_or("pkg-lat-daemon-token-2024".to_string());
    let daemon_url = format!("http://{}:{}", daemon_host, daemon_port);
    let daemon_client = Arc::new(daemon::DaemonClient::new(daemon_url, daemon_api_key));
    colorize_println("  ✓ Daemon client initialized", Colors::BrightGreenFg);

    // Check if load balancing is enabled (before moving heli_config)
    let servers_config = heli_config.get_servers();
    let load_balancer_enabled = servers_config.is_some() && !servers_config.as_ref().unwrap().is_empty();

    // Setup app state
    let jwt_secret = heli_config.get_string("key").unwrap_or("default-secret-key".to_string());
    let app_state = Arc::new(auth::oauth::AppState {
        mongo: Arc::new(mongo_client),
        jwt_secret,
        session_manager: session_manager.clone(),
        audit: audit_service,
        daemon_client,
        config: Arc::new(heli_config.clone()),
    });

    // Setup auth state for protected routes
    let auth_state = Arc::new(middleware::auth::AuthState {
        session_manager,
        mongo: app_state.mongo.clone(),
    });

    let app = if load_balancer_enabled {
        colorize_println("Load Balancer Configuration:", Colors::BrightGreenFg);
        
        // Setup load balancer
        let servers_map = servers_config.unwrap();
        let mut backend_servers = Vec::new();
        
        for (name, config) in servers_map.iter() {
            colorize_println(&format!("  -> Backend: {} ({}:{})", name, config.host, config.port), Colors::BrightCyanFg);
            backend_servers.push(loadbalancer::BackendServer::new(
                name.clone(),
                config.host.clone(),
                config.port,
            ));
        }
        
        let load_balancer = Arc::new(loadbalancer::LoadBalancer::new(
            backend_servers,
            loadbalancer::balancer::LoadBalancingStrategy::LeastConnections,
        ));

        // Start health checker
        let health_checker = loadbalancer::health::HealthChecker::new(load_balancer.clone(), 10);
        tokio::spawn(async move {
            health_checker.start().await;
        });

        // Create load balancer stats route
        let lb_clone = load_balancer.clone();
        let stats_route = Router::new()
            .route("/api/loadbalancer/stats", get(move || {
                let lb = lb_clone.clone();
                async move {
                    let stats = lb.get_stats().await;
                    axum::Json(stats)
                }
            }));

        // WebSocket route
        let lb_ws = load_balancer.clone();
        let ws_route = Router::new()
            .route("/ws", get(loadbalancer::websocket_proxy_handler))
            .with_state(lb_ws);

        // Proxy all other routes to backend servers
        Router::new()
            .merge(stats_route)
            .merge(ws_route)
            .fallback(loadbalancer::proxy_handler)
            .with_state(load_balancer)
            .layer(axum::middleware::from_fn(move |req, next| {
                let limiter = rate_limiter.clone();
                rate_limit_middleware(limiter, req, next)
            }))
            .layer(tower_http::trace::TraceLayer::new_for_http())
    } else {
        colorize_println("Load Balancer: Disabled (no backend servers configured)", Colors::BrightYellowFg);
        
        // Protected routes that require authentication
        let protected_routes = Router::new()
            .route("/api/user/me", get(routes::user::get_me))
            .route("/api/user/audit", get(routes::audit::get_my_audit_logs))
            .route("/api/tenants", get(routes::tenants::list_tenants))
            .route("/api/tenants", axum::routing::post(routes::tenants::create_tenant))
            .route("/api/tenants/{tenant_id}", axum::routing::delete(routes::tenants::delete_tenant))
            .route("/api/tenants/{tenant_id}/resources", get(routes::tenants::get_tenant_resources))
            .route("/api/tenants/{tenant_id}/members", get(routes::tenants::get_tenant_members))
            .route("/api/tenants/{tenant_id}/members", axum::routing::post(routes::tenants::add_tenant_member))
            .route("/api/tenants/{tenant_id}/members/by-email", axum::routing::post(routes::tenants::add_tenant_member_by_email))
            .route("/api/tenants/{tenant_id}/members/{user_id}", axum::routing::delete(routes::tenants::remove_tenant_member))
            .route("/api/tenants/{tenant_id}/transfer", axum::routing::post(routes::tenants::transfer_tenant_ownership))
            .route("/api/tenants/{tenant_id}/audit", get(routes::audit::get_tenant_audit_logs))
            .route("/api/tenants/{tenant_id}/servers", get(routes::servers::list_servers))
            .route("/api/tenants/{tenant_id}/servers", axum::routing::post(routes::servers::create_server))
            .route("/api/tenants/{tenant_id}/servers/{server_id}", axum::routing::delete(routes::servers::delete_server))
            .route("/api/tenants/{tenant_id}/servers/{server_id}/start", axum::routing::post(routes::servers::start_server))
            .route("/api/tenants/{tenant_id}/servers/{server_id}/stop", axum::routing::post(routes::servers::stop_server))
            .route("/api/tenants/{tenant_id}/servers/{server_id}/restart", axum::routing::post(routes::servers::restart_server))
            .route("/api/tenants/{tenant_id}/servers/{server_id}/kill", axum::routing::post(routes::servers::kill_server))
            .route("/api/tenants/{tenant_id}/servers/{server_id}/logs", get(routes::servers::get_server_logs))
            .route("/api/tenants/{tenant_id}/servers/{server_id}/websocket", get(routes::servers::get_server_websocket))
            // Filesystem routes
            .route("/api/tenants/{tenant_id}/servers/{server_id}/files", get(routes::servers::list_server_files))
            .route("/api/tenants/{tenant_id}/servers/{server_id}/files/read", get(routes::servers::read_server_file))
            .route("/api/tenants/{tenant_id}/servers/{server_id}/files/write", axum::routing::post(routes::servers::write_server_file))
            .route("/api/tenants/{tenant_id}/servers/{server_id}/files/delete", axum::routing::post(routes::servers::delete_server_file))
            .route("/api/tenants/{tenant_id}/servers/{server_id}/files/folder", axum::routing::post(routes::servers::create_server_folder))
            .route("/api/tenants/{tenant_id}/servers/{server_id}/files/rename", axum::routing::post(routes::servers::rename_server_file))
            .route("/api/tenants/{tenant_id}/servers/{server_id}/files/copy", axum::routing::post(routes::servers::copy_server_file))
            .route("/api/tenants/{tenant_id}/servers/{server_id}/files/compress", axum::routing::post(routes::servers::compress_server_files))
            .route("/api/tenants/{tenant_id}/servers/{server_id}/files/decompress", axum::routing::post(routes::servers::decompress_server_file))
            .route("/api/tenants/{tenant_id}/servers/{server_id}/files/chmod", axum::routing::post(routes::servers::chmod_server_file))
            .route("/api/tenants/{tenant_id}/servers/{server_id}/files/chown", axum::routing::post(routes::servers::chown_server_file))
            .route("/api/tenants/{tenant_id}/servers/{server_id}/files/upload", axum::routing::post(routes::servers::upload_server_file))
            // Wallet routes
            .route("/api/wallet", get(routes::wallet::get_wallet))
            .route("/api/wallet/transactions", get(routes::wallet::get_ru_transactions))
            .route("/api/wallet/ru/add", axum::routing::post(routes::wallet::add_ru_credits))
            .route("/api/wallet/ru/deduct", axum::routing::post(routes::wallet::deduct_ru_credits))
            .route("/api/wallet/ru/estimate/{node_id}", get(routes::wallet::get_ru_estimate))
            // Admin routes
            .route("/api/admin/ru/give", axum::routing::post(routes::wallet::admin_give_credits))
            .route("/api/admin/ru/set", axum::routing::post(routes::wallet::admin_set_credits))
            .route("/api/admin/users/{user_id}/wallet", get(routes::wallet::admin_get_user_wallet))
            .route("/api/admin/users", get(routes::admin::list_users))
            .route("/api/admin/users/{user_id}", get(routes::admin::get_user))
            .route("/api/admin/users/{user_id}", axum::routing::patch(routes::admin::update_user))
            .route("/api/admin/users/{user_id}", axum::routing::delete(routes::admin::delete_user))
            .route("/api/admin/users/{user_id}/admin", axum::routing::post(routes::admin::set_user_admin))
            .route("/api/admin/users/{user_id}/reset-password", axum::routing::post(routes::admin::reset_user_password))
            .route("/api/admin/diagnostics", get(routes::admin::diagnostics))
            .route("/api/admin/nodes", get(routes::admin::list_nodes))
            .route("/api/admin/nodes", axum::routing::post(routes::admin::create_node))
            .route("/api/admin/nodes/{node_id}", get(routes::admin::get_node))
            .route("/api/admin/nodes/{node_id}", axum::routing::patch(routes::admin::update_node))
            .route("/api/admin/nodes/{node_id}", axum::routing::delete(routes::admin::delete_node))
            .route("/api/admin/software", get(routes::admin::list_software))
            .route("/api/admin/software", axum::routing::post(routes::admin::create_software))
            .route("/api/admin/software/{software_id}", axum::routing::patch(routes::admin::update_software))
            .route("/api/admin/software/{software_id}", axum::routing::delete(routes::admin::delete_software))
            // Config routes (admin only)
            .route("/api/admin/config", get(routes::config::get_config))
            .layer(axum::middleware::from_fn_with_state(
                auth_state.clone(),
                middleware::auth::auth_middleware
            ))
            .with_state(app_state.clone());

        // Build our application with routes (normal mode)
        Router::new()
            .route("/", get(routes::basic::root))
            .route("/assets/{file}", get(routes::basic::serve_asset))
            .route("/api/", get(routes::api::root))
            .route("/api/auth/register", axum::routing::post(auth::oauth::register))
            .route("/api/auth/login", axum::routing::post(auth::oauth::login))
            // Lightd daemon endpoint (uses API key auth, not session auth)
            .route("/api/lightd/deduct", axum::routing::post(routes::wallet::daemon_deduct_credits))
            .route("/status", get(routes::basic::status))
            .merge(protected_routes)
            .fallback(any(serve_index))
            .layer(axum::middleware::from_fn(move |req, next| {
                let limiter = rate_limiter.clone();
                rate_limit_middleware(limiter, req, next)
            }))
            .with_state(app_state)
            .layer(tower_http::trace::TraceLayer::new_for_http())
    };
    let host = heli_config.get_string("server.host").unwrap_or("0.0.0.0".to_string()); // Default to 0.0.0.0 if not specified
    let port = heli_config.get_int("server.port").unwrap_or(3000); // Default to 3000 if not specified
    let listener = tokio::net::TcpListener::bind(format!("{}:{}", host, port)).await.unwrap();
    println!("{}", "Server Configuration:".color(Colors::BrightGreenFg));
    println!("  {} {}:{}", "-> Server running on".color(Colors::BrightCyanFg), host, port);
    println!(
        "  {} {:?}",
        "-> Ratelimit per second:".color(Colors::BrightCyanFg),
        heli_config.get_int("ratelimit.persec")
    );
    println!(
        "  {} {:?}",
        "-> Ratelimit per minute:".color(Colors::BrightCyanFg),
        heli_config.get_int("ratelimit.permin")
    );

    println!("{}", "Jobs Configuration:".color(Colors::BrightGreenFg));
    println!(
        "  {} {:?}",
        "-> Egg sync interval (minutes):".color(Colors::BrightCyanFg),
        heli_config.get_int("jobs.eggSyncIntervalMinutes")
    );
    println!(
        "  {} {:?}",
        "-> Reconcile interval (minutes):".color(Colors::BrightCyanFg),
        heli_config.get_int("jobs.reconcileIntervalMinutes")
    );
    println!(
        "  {} {:?}",
        "-> Startup run:".color(Colors::BrightCyanFg),
        heli_config.get_string("jobs.startupRun")
    );
    axum::serve(listener, app).await.unwrap();
}

async fn serve_index() -> Html<String> {
    // Serve the index.html file for all unmatched routes
    let index_path = "./data/dist/index.html";
    match fs::read_to_string(index_path) {
        Ok(contents) => Html(contents),
        Err(_) => Html("<h1>404 - Not Found</h1>".to_string()),
    }
}

async fn welcome() {
    let ascii_art = r#"
  ___ ___         .__  .__               __          .__           __________                __   
 /   |   \   ____ |  | |__|____    _____/  |_ ___.__.|  |          \______   \__ __  _______/  |_ 
/    ~    \_/ __ \|  | |  \__  \ _/ ___\   __<   |  ||  |    ______ |       _/  |  \/  ___/\   __\
\    Y    /\  ___/|  |_|  |/ __ \\  \___|  |  \___  ||  |__ /_____/ |    |   \  |  /\___ \  |  |  
 \___|_  /  \___  >____/__(____  /\___  >__|  / ____||____/         |____|_  /____//____  > |__|  
       \/       \/             \/     \/      \/                           \/           \/        
Using HeliScript 0.1v
    "#;
    colorized::colorize_println(ascii_art, colorized::Colors::BrightCyanFg);
    println!(""); // space
}