use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use super::balancer::LoadBalancer;

pub async fn proxy_handler(
    State(load_balancer): State<Arc<LoadBalancer>>,
    uri: Uri,
    headers: HeaderMap,
    req: Request,
) -> Result<Response, (StatusCode, String)> {
    // Get the next available server
    let server = load_balancer
        .get_next_server()
        .await
        .ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "No healthy backend servers available".to_string(),
            )
        })?;

    // Increment connection count
    server.increment_connections().await;

    // Build the target URL
    let path_and_query = uri.path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/");
    
    let target_url = format!("{}{}", server.url(), path_and_query);

    tracing::debug!("Proxying request to: {} (server: {})", target_url, server.name);

    // Create HTTP client
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| {
            server.decrement_connections();
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to create client: {}", e))
        })?;

    // Convert method
    let method = match req.method().as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        "HEAD" => reqwest::Method::HEAD,
        "OPTIONS" => reqwest::Method::OPTIONS,
        _ => reqwest::Method::GET,
    };

    // Build request
    let mut proxy_req = client.request(method, &target_url);

    // Copy headers (skip host and connection headers)
    for (key, value) in headers.iter() {
        let key_str = key.as_str();
        if key_str != "host" && key_str != "connection" && key_str != "content-length" {
            if let Ok(val_str) = value.to_str() {
                proxy_req = proxy_req.header(key_str, val_str);
            }
        }
    }

    // Get body
    let body_bytes = axum::body::to_bytes(req.into_body(), usize::MAX)
        .await
        .map_err(|e| {
            server.decrement_connections();
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to read body: {}", e))
        })?;

    if !body_bytes.is_empty() {
        proxy_req = proxy_req.body(body_bytes.to_vec());
    }

    // Send request
    let response = proxy_req.send().await.map_err(|e| {
        server.decrement_connections();
        tracing::error!("Proxy request failed to {}: {}", server.name, e);
        (StatusCode::BAD_GATEWAY, format!("Backend request failed: {}", e))
    })?;

    // Decrement connection count
    server.decrement_connections().await;

    // Build response
    let status = response.status();
    let mut resp_builder = Response::builder().status(status.as_u16());

    // Copy response headers
    for (key, value) in response.headers().iter() {
        let key_str = key.as_str();
        if key_str != "connection" && key_str != "transfer-encoding" {
            resp_builder = resp_builder.header(key_str, value);
        }
    }

    // Get response body
    let body_bytes = response.bytes().await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to read response: {}", e))
    })?;

    let response = resp_builder
        .body(Body::from(body_bytes))
        .map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to build response: {}", e))
        })?;

    Ok(response)
}
