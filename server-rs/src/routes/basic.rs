use std::collections::HashMap;

use crate::{env, heli, json::json::{key_value, rs}};


pub async fn root() -> axum::response::Json<serde_json::Value> {
    let data = serde_json::json!({
        "endpoints": {
            "/": "GET - Root endpoint",
            "/api": "GET - API endpoint (to be implemented)",
            "/status": "GET - Status endpoint (to be implemented)"
        }
    });
    rs(200, data)
}

pub async fn status() -> axum::response::Json<serde_json::Value> {

   

    let mut data = HashMap::new();
    let uptime = env::uptime::UptimeTracker::current().to_string();
    let heli_config = heli::parse::HeliConfig::parse("./config.heli").expect("Failed to parse config.heli");
    env::uptime::UptimeTracker::save(); // Save uptime on each status check
    data.insert("health", "ok");
    data.insert("uptime", &uptime); // for now, we should prob implement actual uptime tracking
    let ratelimit_per_minute = heli_config.get_int("ratelimit.per-minute").unwrap_or(-1).to_string();
    data.insert("ratelimit-per-minute", &ratelimit_per_minute);
    let ratelimit_per_second = heli_config.get_int("ratelimit.persec").unwrap_or(-1).to_string();
    data.insert("ratelimit-per-second", &ratelimit_per_second);
    let node = heli_config.get_string("node").unwrap_or_default();
    data.insert("node", &node);

    key_value(200, data)
      
}