use std::collections::HashMap;

use axum::{
    routing::get,
    Router,
};
mod json;
mod env;
mod heli;
mod routes;

// our mods
use crate::{env::uptime, json::json::{key_value, rs}};



#[tokio::main]
async fn main() {
    welcome().await;
    let uptime_tracker = env::uptime::UptimeTracker::start("data/uptime.json");
    env::uptime::UptimeTracker::save(); // Save uptime on each status check
    println!("Uptime tracker started: {:?}", uptime_tracker);
    // build our application with a single route
    let app = Router::new()
        .route("/", get(routes::basic::root))
        .route("/status", get(routes::basic::status));
      //  .route("/api", get(api));

    // Read the config.heli file
    let heli_config = heli::parse::HeliConfig::parse("./config.heli").expect("Failed to parse config.heli");
    let port = heli_config.get_int("port").unwrap_or(3000); // Default to 3000 if not specified
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await.unwrap();
    println!("Server running on port {}", port);
    println!("Ratelimit per second: {:?}", heli_config.get_int("ratelimit.persec"));
    println!("Ratelimit per minute: {:?}", heli_config.get_int("ratelimit.permin"));
    axum::serve(listener, app).await.unwrap();

    
    // deadzone, code written here doesn't work
}

async fn welcome(){
     let ascii_art = r#"
  ___ ___         .__  .__               __          .__           __________                __   
 /   |   \   ____ |  | |__|____    _____/  |_ ___.__.|  |          \______   \__ __  _______/  |_ 
/    ~    \_/ __ \|  | |  \__  \ _/ ___\   __<   |  ||  |    ______ |       _/  |  \/  ___/\   __\
\    Y    /\  ___/|  |_|  |/ __ \\  \___|  |  \___  ||  |__ /_____/ |    |   \  |  /\___ \  |  |  
 \___|_  /  \___  >____/__(____  /\___  >__|  / ____||____/         |____|_  /____//____  > |__|  
       \/       \/             \/     \/      \/                           \/           \/        
    "#;
    println!("{}", ascii_art);
    println!(""); // space
   // println!("{}", "=".repeat(30));

}