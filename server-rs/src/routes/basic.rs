use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use axum::{
    response::{Html, IntoResponse},
    http::StatusCode,
};
use axum::extract::Path;
use axum::http::{header, HeaderMap, HeaderValue};
use tokio::fs::read;
use crate::{env, heli, json::json::{key_value, rs}};




pub async fn root() -> impl IntoResponse {
    // Define the path to the index.html file
    let index_path = "./data/dist/index.html";

    // Read the file contents
    match fs::read_to_string(index_path) {
        Ok(contents) => Html(contents).into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            "Error: index.html not found in ./data/dist".to_string(),
        )
            .into_response(),
    }
}

pub async fn serve_asset(Path(file): Path<String>) -> impl IntoResponse {
   // println!("Requested asset file: {}", file); // Debug: Print the requested file name

    // Define the path to the assets folder
    let asset_path = PathBuf::from(format!("./data/dist/assets/{}", file));

    // Debug: Print the constructed file path
    //println!("Serving asset: {:?}", asset_path);

    // Read the file contents
    match read(&asset_path).await {
        Ok(contents) => {
            // Determine the MIME type based on the file extension
            let mime_type = match asset_path.extension().and_then(|ext| ext.to_str()) {
                Some("js") => "application/javascript",
                Some("css") => "text/css",
                Some("html") => "text/html",
                Some("png") => "image/png",
                Some("jpg") | Some("jpeg") => "image/jpeg",
                Some("svg") => "image/svg+xml",
                Some("wasm") => "application/wasm",
                _ => "application/octet-stream", // Default MIME type
            };

            // Set the Content-Type header
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime_type));

            (StatusCode::OK, headers, contents).into_response()
        }
        Err(err) => {
            println!("Error reading asset: {:?}", err); // Debug the error
            (
                StatusCode::NOT_FOUND,
                format!("Error: Asset '{}' not found", file),
            )
                .into_response()
        }
    }
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