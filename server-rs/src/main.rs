use std::collections::HashMap;
use axum::{
    routing::{get, any},
    Router,
};
mod json;
mod env;
mod heli;
mod routes;

// our mods
use crate::{env::uptime, json::json::{key_value, rs}};
use axum::response::Html;
use std::fs;

#[tokio::main]
async fn main() {
    welcome().await;
    let uptime_tracker = env::uptime::UptimeTracker::start("data/uptime.json");
    env::uptime::UptimeTracker::save(); // Save uptime on each status check
    println!("Uptime tracker started: {:?}", uptime_tracker);

    // Build our application with routes
    let app = Router::new()
        // Basic routes
        .route("/", get(routes::basic::root))
        .route("/assets/{file}", get(routes::basic::serve_asset)) // Serve assets


        .route("/api", get(routes::apibase::root)) // API route
        .route("/status", get(routes::basic::status)) // Status route


        .fallback(any(serve_index)); // Fallback for all other routes

    // Read the config.heli file
    let heli_config = heli::parse::HeliConfig::parse("./config.heli").expect("Failed to parse config.heli");
    let host = heli_config.get_string("server.host").unwrap_or("0.0.0.0".to_string()); // Default to 0.0.0.0 if not specified
    let port = heli_config.get_int("server.port").unwrap_or(3000); // Default to 3000 if not specified
    let listener = tokio::net::TcpListener::bind(format!("{}:{}", host, port)).await.unwrap();
    println!("Server running on  {}:{}", host, port);
    println!("Ratelimit per second: {:?}", heli_config.get_int("ratelimit.persec"));
    println!("Ratelimit per minute: {:?}", heli_config.get_int("ratelimit.permin"));
    println!("Jobs");
    println!("  Egg sync interval (minutes): {:?}", heli_config.get_int("jobs.eggSyncIntervalMinutes"));
    println!("  Reconcile interval (minutes): {:?}", heli_config.get_int("jobs.reconcileIntervalMinutes"));
    println!("  Startup run: {:?}", heli_config.get_string("jobs.startupRun"));
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
    println!("{}", ascii_art);
    println!(""); // space
}