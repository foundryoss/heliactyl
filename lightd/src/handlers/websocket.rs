//! WebSocket handler - Non-blocking token generation and connection handling
//!
//! Uses lock-free state manager for instant lookups.

use axum::{
    extract::{ws::WebSocketUpgrade, Query, State},
    http::StatusCode,
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};

use crate::{
    models::ApiResponse,
    types::AppState,
    websocket::WebSocketHandler,
};

#[derive(Debug, Deserialize)]
pub struct GenerateTokenQuery {
    pub container_id: String,
}

#[derive(Debug, Deserialize)]
pub struct WebSocketQuery {
    pub token: String,
}

#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub token: String,
    pub expires_at: String,
    pub container_id: String,
    pub container_uuid: String,
}

/// GET /websocket/generate?container_id=xxx
/// Non-blocking - uses lock-free state manager
#[axum::debug_handler]
pub async fn generate_websocket_token(
    State(state): State<AppState>,
    Query(params): Query<GenerateTokenQuery>,
) -> Result<Json<ApiResponse<TokenResponse>>, StatusCode> {
    info!("Generating WebSocket token for container: {}", params.container_id);

    // Lock-free lookup
    let (container_id, container_uuid) = {
        // Try to find by container ID first
        if let Some((uuid, _)) = state.state_manager.find_by_container_id(&params.container_id) {
            (params.container_id.clone(), uuid)
        } else if let Some(container_state) = state.state_manager.get_container(&params.container_id) {
            // Try as UUID
            if let Some(cid) = &container_state.container_id {
                (cid.clone(), params.container_id.clone())
            } else {
                return Ok(Json(ApiResponse::error("Container ID not found".into())));
            }
        } else {
            return Ok(Json(ApiResponse::error("Container not found".into())));
        }
    };

    match state
        .websocket_tokens
        .generate_token(container_id.clone(), container_uuid.clone())
        .await
    {
        Ok(token) => {
            let resp = TokenResponse {
                token: token.token,
                expires_at: token.expires_at.to_rfc3339(),
                container_id: token.container_id,
                container_uuid,
            };
            Ok(Json(ApiResponse::success(resp)))
        }
        Err(e) => {
            error!("Failed to generate token: {}", e);
            Ok(Json(ApiResponse::error("Failed to generate token".into())))
        }
    }
}

/// GET /websocket?token=xxx - WebSocket upgrade
/// Non-blocking - validation is instant
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WebSocketQuery>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    info!("WebSocket connection attempt");

    let token = match state.websocket_tokens.validate_token(&params.token).await {
        Some(t) => t,
        None => {
            error!("Invalid or expired token");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    info!(
        "Valid token for container: {} (uuid: {})",
        token.container_id, token.container_uuid
    );

    Ok(ws.on_upgrade(move |socket| async move {
        let handler = WebSocketHandler::new(token, socket, Arc::new(state));
        handler.handle().await;
    }))
}
