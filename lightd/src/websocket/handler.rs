use axum::{
    extract::{ws::WebSocketUpgrade, Query, State},
    http::StatusCode,
    response::{Json, Response},
};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info};

use crate::{
    models::{ApiResponse, WebSocketTokenRequest, WebSocketTokenResponse, ContainerStats},
    types::AppState,
    websocket::WebSocketConnection,
};

#[derive(Debug, Deserialize)]
pub struct WebSocketQuery {
    token: String,
}

/// Generate a WebSocket token for a container

pub async fn generate_websocket_token(
    State(state): State<AppState>,
    Json(payload): Json<YourPayloadType>,
) -> impl IntoResponse {
    info!("Generating WebSocket token for container: {}", req.container_id);

    // Resolve container ID (could be UUID or Docker container ID)
    let (container_id, container_uuid) = {
        let state_manager = state.state_manager.lock().await;
        
        // First try to find by container ID
        if let Some((uuid, _)) = state_manager.find_by_container_id(&req.container_id) {
            (req.container_id.clone(), uuid.clone())
        }
        // If not found, try to find by UUID
        else if let Some(container_state) = state_manager.get_container(&req.container_id) {
            if let Some(container_id) = &container_state.container_id {
                (container_id.clone(), req.container_id.clone())
            } else {
                return Ok(Json(ApiResponse::error("Container ID not found for this UUID".to_string())));
            }
        } else {
            return Ok(Json(ApiResponse::error("Container not found".to_string())));
        }
    };

    match state.websocket_tokens.generate_token(container_id, container_uuid.clone()).await {
        Ok(token) => {
            let response = WebSocketTokenResponse {
                token: token.token,
                expires_at: token.expires_at,
                container_id: token.container_id,
                container_uuid,
            };
            Ok(Json(ApiResponse::success(response)))
        }
        Err(e) => {
            error!("Failed to generate WebSocket token: {}", e);
            Ok(Json(ApiResponse::error("Failed to generate token".to_string())))
        }
    }
}

/// WebSocket upgrade handler
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WebSocketQuery>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    info!("WebSocket connection attempt with token: {}", params.token);

    // Validate token
    let token = match state.websocket_tokens.validate_token(&params.token).await {
        Some(token) => token,
        None => {
            error!("Invalid or expired WebSocket token: {}", params.token);
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    info!("Valid WebSocket token for container: {}", token.container_id);

    // Create broadcast channels for this container
    let (stats_tx, stats_rx) = broadcast::channel::<ContainerStats>(100);
    let (logs_tx, logs_rx) = broadcast::channel::<String>(1000);
    let (status_tx, status_rx) = broadcast::channel::<String>(100);

    // Store broadcasters in state for other parts of the system to use
    {
        let mut broadcasters = state.websocket_broadcasters.write().await;
        broadcasters.insert(token.container_id.clone(), (stats_tx, logs_tx, status_tx));
    }

    // Start background tasks for this container
    let container_id = token.container_id.clone();
    let state_clone = state.clone();
    
    // Stats broadcasting task
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
        let manager = crate::docker::ContainerManager::new(state_clone.docker.client());
        
        loop {
            interval.tick().await;
            
            // Get current stats
            if let Ok(stats_json) = manager.get_stats(&container_id).await {
                if let Ok(docker_stats) = serde_json::from_str::<serde_json::Value>(&stats_json) {
                    let stats = convert_docker_stats(&docker_stats, &container_id, &state_clone).await;
                    
                    // Broadcast to all connected clients for this container
                    if let Some((stats_tx, _, _)) = state_clone.websocket_broadcasters.read().await.get(&container_id) {
                        let _ = stats_tx.send(stats);
                    }
                }
            }
        }
    });

    // Log streaming task
    let container_id_clone = token.container_id.clone();
    let state_clone2 = state.clone();
    tokio::spawn(async move {
        let manager = crate::docker::ContainerManager::new(state_clone2.docker.client());
        
        // Stream logs
        if let Ok(logs) = manager.get_logs(&container_id_clone, true, Some("0")).await {
            for line in logs.lines() {
                if let Some((_, logs_tx, _)) = state_clone2.websocket_broadcasters.read().await.get(&container_id_clone) {
                    let _ = logs_tx.send(line.to_string());
                }
            }
        }
    });

    // Status monitoring task
    let container_id_clone2 = token.container_id.clone();
    let state_clone3 = state.clone();
    tokio::spawn(async move {
        let mut last_status = String::new();
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(2));
        
        loop {
            interval.tick().await;
            
            let current_status = {
                let state_manager = state_clone3.state_manager.lock().await;
                if let Some((_, container_state)) = state_manager.find_by_container_id(&container_id_clone2) {
                    container_state.state.clone()
                } else {
                    "unknown".to_string()
                }
            };
            
            if current_status != last_status {
                if let Some((_, _, status_tx)) = state_clone3.websocket_broadcasters.read().await.get(&container_id_clone2) {
                    let _ = status_tx.send(current_status.clone());
                }
                last_status = current_status;
            }
        }
    });

    Ok(ws.on_upgrade(move |socket| async move {
        let connection = WebSocketConnection::new(
            token,
            socket,
            Arc::new(state),
            stats_rx,
            logs_rx,
            status_rx,
        );
        connection.handle().await;
    }))
}

async fn convert_docker_stats(
    docker_stats: &serde_json::Value,
    container_id: &str,
    state: &AppState,
) -> ContainerStats {
    let memory_usage = docker_stats["memory_stats"]["usage"].as_u64().unwrap_or(0);
    let memory_limit = docker_stats["memory_stats"]["limit"].as_u64().unwrap_or(0);
    
    let cpu_delta = docker_stats["cpu_stats"]["cpu_usage"]["total_usage"].as_u64().unwrap_or(0) as f64
        - docker_stats["precpu_stats"]["cpu_usage"]["total_usage"].as_u64().unwrap_or(0) as f64;
    let system_delta = docker_stats["cpu_stats"]["system_cpu_usage"].as_u64().unwrap_or(0) as f64
        - docker_stats["precpu_stats"]["system_cpu_usage"].as_u64().unwrap_or(0) as f64;
    let cpu_count = docker_stats["cpu_stats"]["online_cpus"].as_u64().unwrap_or(1) as f64;
    
    let cpu_percent = if system_delta > 0.0 && cpu_delta > 0.0 {
        (cpu_delta / system_delta) * cpu_count * 100.0
    } else {
        0.0
    };

    let rx_bytes = docker_stats["networks"]["eth0"]["rx_bytes"].as_u64().unwrap_or(0);
    let tx_bytes = docker_stats["networks"]["eth0"]["tx_bytes"].as_u64().unwrap_or(0);

    // Get container state
    let state_manager = state.state_manager.lock().await;
    let container_state = if let Some((_, container_state)) = state_manager.find_by_container_id(container_id) {
        container_state.state.clone()
    } else {
        "unknown".to_string()
    };
    drop(state_manager);

    ContainerStats {
        memory_bytes: memory_usage,
        memory_limit_bytes: memory_limit,
        cpu_absolute: cpu_percent,
        network: crate::models::NetworkStats { rx_bytes, tx_bytes },
        uptime: 0, // Would need to calculate from container start time
        state: container_state,
        disk_bytes: 0, // Would need additional Docker API calls
    }
}