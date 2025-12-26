use axum::{
    extract::{ws::WebSocket, State, WebSocketUpgrade},
    http::StatusCode,
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use super::balancer::LoadBalancer;

pub async fn websocket_proxy_handler(
    ws: WebSocketUpgrade,
    State(load_balancer): State<Arc<LoadBalancer>>,
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

    tracing::debug!("WebSocket connection routed to: {}", server.name);

    Ok(ws.on_upgrade(move |socket| handle_websocket(socket, server, load_balancer)))
}

async fn handle_websocket(
    client_ws: WebSocket,
    server: super::balancer::BackendServer,
    _load_balancer: Arc<LoadBalancer>,
) {
    // Increment connection count
    server.increment_connections().await;

    // Build WebSocket URL to backend
    let ws_url = format!("ws://{}:{}/ws", server.host, server.port);
    
    tracing::info!("Establishing WebSocket connection to backend: {}", ws_url);

    // Connect to backend WebSocket
    let backend_result = connect_async(&ws_url).await;
    
    let (backend_ws, _) = match backend_result {
        Ok(conn) => conn,
        Err(e) => {
            tracing::error!("Failed to connect to backend WebSocket {}: {}", server.name, e);
            server.decrement_connections().await;
            return;
        }
    };

    // Split both WebSocket connections
    let (mut client_sink, mut client_stream) = client_ws.split();
    let (mut backend_sink, mut backend_stream) = backend_ws.split();

    // Create two tasks to proxy messages bidirectionally
    let client_to_backend = async {
        while let Some(msg) = client_stream.next().await {
            match msg {
                Ok(axum_msg) => {
                    // Convert Axum WebSocket message to tungstenite message
                    let backend_msg = match axum_msg {
                        axum::extract::ws::Message::Text(text) => Message::Text(text.to_string().into()),
                        axum::extract::ws::Message::Binary(data) => Message::Binary(data),
                        axum::extract::ws::Message::Ping(data) => Message::Ping(data),
                        axum::extract::ws::Message::Pong(data) => Message::Pong(data),
                        axum::extract::ws::Message::Close(frame) => {
                            if let Some(f) = frame {
                                Message::Close(Some(tokio_tungstenite::tungstenite::protocol::CloseFrame {
                                    code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::from(f.code),
                                    reason: tokio_tungstenite::tungstenite::Utf8Bytes::from(f.reason.to_string()),
                                }))
                            } else {
                                Message::Close(None)
                            }
                        }
                    };

                    if backend_sink.send(backend_msg).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::debug!("Client WebSocket error: {}", e);
                    break;
                }
            }
        }
    };

    let backend_to_client = async {
        while let Some(msg) = backend_stream.next().await {
            match msg {
                Ok(backend_msg) => {
                    // Convert tungstenite message to Axum WebSocket message
                    let client_msg = match backend_msg {
                        Message::Text(text) => axum::extract::ws::Message::Text(text.to_string().into()),
                        Message::Binary(data) => axum::extract::ws::Message::Binary(data.to_vec().into()),
                        Message::Ping(data) => axum::extract::ws::Message::Ping(data.to_vec().into()),
                        Message::Pong(data) => axum::extract::ws::Message::Pong(data.to_vec().into()),
                        Message::Close(frame) => {
                            if let Some(f) = frame {
                                axum::extract::ws::Message::Close(Some(axum::extract::ws::CloseFrame {
                                    code: f.code.into(),
                                    reason: f.reason.to_string().into(),
                                }))
                            } else {
                                axum::extract::ws::Message::Close(None)
                            }
                        }
                        Message::Frame(_) => continue, // Skip raw frames
                    };

                    if client_sink.send(client_msg).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::debug!("Backend WebSocket error: {}", e);
                    break;
                }
            }
        }
    };

    // Run both proxy tasks concurrently
    tokio::select! {
        _ = client_to_backend => {
            tracing::debug!("Client to backend stream closed");
        }
        _ = backend_to_client => {
            tracing::debug!("Backend to client stream closed");
        }
    }

    // Decrement connection count
    server.decrement_connections().await;
    tracing::info!("WebSocket connection closed for server: {}", server.name);
}
