use std::collections::HashMap;
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

// our mods
use crate::{env::uptime, json::json::{key_value, rs}};
use crate::middleware::ratelimit::{RateLimiter, rate_limit_middleware};
use axum::response::Html;
use std::fs;
use std::sync::Arc;
use colorized::{Color, Colors, colorize_println};

#[tokio::main]
async fn main() {
    welcome().await;
    

    // Read the config.heli file
    let heli_config = heli::parse::HeliConfig::parse("./config.heli").expect("Failed to parse config.heli");

    let uptime_file = heli_config.get_string("uptime.file").unwrap_or("data/uptime.json".to_string());
    let uptime_tracker = env::uptime::UptimeTracker::start(&uptime_file);
    env::uptime::UptimeTracker::save(); // Save uptime on each status check
    println!("Uptime tracker started: {:?}", uptime_tracker);
    colorize_println("Starting server...", Colors::BrightGreenFg);
    
    // Setup rate limiter with Redis
    let redis_host = heli_config.get_string("redis.host").unwrap_or("127.0.0.1".to_string());
    let redis_port = heli_config.get_int("redis.port").unwrap_or(6379);
    let redis_url = format!("redis://{}:{}", redis_host, redis_port);
    let per_sec = heli_config.get_int("ratelimit.persec").unwrap_or(10) as u32;
    let per_min = heli_config.get_int("ratelimit.permin").unwrap_or(120) as u32;
    let rate_limiter = Arc::new(RateLimiter::new(&redis_url, per_sec, per_min).expect("Failed to connect to Redis"));

    // Build our application with routes
    let app = Router::new()
        // Basic routes
        .route("/", get(routes::basic::root))
        .route("/assets/{file}", get(routes::basic::serve_asset)) // Serve assets


        .route("/api/", get(routes::api::root)) // API route
        .route("/status", get(routes::basic::status)) // Status route


        .fallback(any(serve_index)) // Fallback for all other routes
        .layer(axum::middleware::from_fn(move |req, next| {
            let limiter = rate_limiter.clone();
            rate_limit_middleware(limiter, req, next)
        }));
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