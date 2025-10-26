use axum::{
    extract::{ws::WebSocket, WebSocketUpgrade},
    response::Response,
};
use futures_util::StreamExt;

/// WebSocket echo handler for testing
pub async fn websocket_handler(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    tracing::info!("WebSocket connection established");

    while let Some(msg) = socket.recv().await {
        match msg {
            Ok(msg) => {
                match msg {
                    axum::extract::ws::Message::Text(text) => {
                        tracing::debug!("Received text: {}", text);
                        
                        // Echo back with server info
                        let response = format!(r#"{{"echo":"{}","server":"{}"}}"#, 
                            text, 
                            std::env::var("NODE_NAME").unwrap_or_else(|_| "backend".to_string())
                        );
                        
                        if socket.send(axum::extract::ws::Message::Text(response.into())).await.is_err() {
                            break;
                        }
                    }
                    axum::extract::ws::Message::Binary(data) => {
                        tracing::debug!("Received binary data: {} bytes", data.len());
                        if socket.send(axum::extract::ws::Message::Binary(data)).await.is_err() {
                            break;
                        }
                    }
                    axum::extract::ws::Message::Ping(data) => {
                        if socket.send(axum::extract::ws::Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    axum::extract::ws::Message::Pong(_) => {}
                    axum::extract::ws::Message::Close(_) => {
                        tracing::info!("WebSocket close received");
                        break;
                    }
                }
            }
            Err(e) => {
                tracing::error!("WebSocket error: {}", e);
                break;
            }
        }
    }

    tracing::info!("WebSocket connection closed");
}
