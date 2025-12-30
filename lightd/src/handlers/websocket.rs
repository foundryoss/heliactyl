use axum::{
    extract::{ws::WebSocketUpgrade, Query, State},
    http::StatusCode,
    response::Response,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info};

use crate::{
    models::{ApiResponse, ContainerStats, NetworkStats},
    types::AppState,
    websocket::WebSocketConnection,
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
#[axum::debug_handler]
pub async fn generate_websocket_token(
    State(state): State<AppState>,
    Query(params): Query<GenerateTokenQuery>,
) -> Result<Json<ApiResponse<TokenResponse>>, StatusCode> {
    info!("Generating WebSocket token for container: {}", params.container_id);

    let (container_id, container_uuid) = {
        let state_manager = state.state_manager.lock().await;
        
        if let Some((uuid, _)) = state_manager.find_by_container_id(&params.container_id) {
            (params.container_id.clone(), uuid.clone())
        } else if let Some(container_state) = state_manager.get_container(&params.container_id) {
            if let Some(cid) = &container_state.container_id {
                (cid.clone(), params.container_id.clone())
            } else {
                return Ok(Json(ApiResponse::error("Container ID not found".into())));
            }
        } else {
            return Ok(Json(ApiResponse::error("Container not found".into())));
        }
    };

    match state.websocket_tokens.generate_token(container_id.clone(), container_uuid.clone()).await {
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

    info!("Valid token for container: {}", token.container_id);

    let (stats_tx, stats_rx) = broadcast::channel::<ContainerStats>(100);
    let (logs_tx, logs_rx) = broadcast::channel::<String>(1000);
    let (status_tx, status_rx) = broadcast::channel::<String>(100);

    {
        let mut broadcasters = state.websocket_broadcasters.write().await;
        broadcasters.insert(token.container_id.clone(), (stats_tx, logs_tx, status_tx));
    }

    let container_id = token.container_id.clone();
    let state_clone = state.clone();
    
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
        let manager = crate::docker::ContainerManager::new(state_clone.docker.client());
        
        loop {
            interval.tick().await;
            if let Ok(stats_json) = manager.get_stats(&container_id).await {
                if let Ok(docker_stats) = serde_json::from_str::<serde_json::Value>(&stats_json) {
                    let stats = convert_docker_stats(&docker_stats, &container_id, &state_clone).await;
                    if let Some((tx, _, _)) = state_clone.websocket_broadcasters.read().await.get(&container_id) {
                        let _ = tx.send(stats);
                    }
                }
            }
        }
    });

    let container_id2 = token.container_id.clone();
    let state_clone2 = state.clone();
    tokio::spawn(async move {
        let manager = crate::docker::ContainerManager::new(state_clone2.docker.client());
        if let Ok(logs) = manager.get_logs(&container_id2, true, Some("0")).await {
            for line in logs.lines() {
                if let Some((_, tx, _)) = state_clone2.websocket_broadcasters.read().await.get(&container_id2) {
                    let _ = tx.send(line.to_string());
                }
            }
        }
    });

    let container_id3 = token.container_id.clone();
    let state_clone3 = state.clone();
    tokio::spawn(async move {
        let mut last_status = String::new();
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(2));
        loop {
            interval.tick().await;
            let current = {
                let sm = state_clone3.state_manager.lock().await;
                sm.find_by_container_id(&container_id3)
                    .map(|(_, cs)| cs.state.clone())
                    .unwrap_or_else(|| "unknown".into())
            };
            if current != last_status {
                if let Some((_, _, tx)) = state_clone3.websocket_broadcasters.read().await.get(&container_id3) {
                    let _ = tx.send(current.clone());
                }
                last_status = current;
            }
        }
    });

    Ok(ws.on_upgrade(move |socket| async move {
        let conn = WebSocketConnection::new(
            token,
            socket,
            Arc::new(state),
            stats_rx,
            logs_rx,
            status_rx,
        );
        conn.handle().await;
    }))
}

async fn convert_docker_stats(docker_stats: &serde_json::Value, container_id: &str, state: &AppState) -> ContainerStats {
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

    // Get REAL state from Docker, not from state_manager
    let container_state = match state.docker.client.inspect_container(container_id, None).await {
        Ok(info) => {
            if let Some(state_info) = info.state {
                if state_info.paused.unwrap_or(false) {
                    "paused".to_string()
                } else if state_info.running.unwrap_or(false) {
                    "running".to_string()
                } else if state_info.restarting.unwrap_or(false) {
                    "restarting".to_string()
                } else if state_info.dead.unwrap_or(false) {
                    "dead".to_string()
                } else {
                    state_info.status.map(|s| s.to_string()).unwrap_or_else(|| "stopped".to_string())
                }
            } else {
                "unknown".to_string()
            }
        }
        Err(_) => "offline".to_string(),
    };

    // Check if suspended in our state manager
    let is_suspended = {
        let sm = state.state_manager.lock().await;
        sm.find_by_container_id(container_id)
            .map(|(uuid, _)| sm.is_container_suspended(uuid))
            .unwrap_or(false)
    };

    ContainerStats {
        memory_bytes: memory_usage,
        memory_limit_bytes: memory_limit,
        cpu_absolute: cpu_percent,
        network: NetworkStats { rx_bytes, tx_bytes },
        uptime: 0,
        state: container_state,
        disk_bytes: 0,
        is_suspended,
    }
}
