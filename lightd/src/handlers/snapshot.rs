//! Snapshot handlers - Non-blocking snapshot operations
//!
//! Uses lock-free state manager for all lookups.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

use crate::{
    docker::SnapshotManager,
    models::ApiResponse,
    types::AppState,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateSnapshotRequest {
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RestoreSnapshotRequest {
    pub container_uuid: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotResponse {
    pub snapshot_id: String,
    pub container_uuid: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RestoreResponse {
    pub new_container_uuid: String,
    pub new_container_id: String,
    pub snapshot_id: String,
    pub status: String,
    pub message: String,
}

/// Create a snapshot of a container
pub async fn create_snapshot(
    State(state): State<AppState>,
    Path(container_uuid): Path<String>,
    Json(_req): Json<CreateSnapshotRequest>,
) -> Result<Json<ApiResponse<SnapshotResponse>>, StatusCode> {
    info!("Creating snapshot for container: {}", container_uuid);
    
    // Lock-free lookup
    let container_state = match state.state_manager.get_container(&container_uuid) {
        Some(s) => s,
        None => return Ok(Json(ApiResponse::error(format!("Container {} not found", container_uuid)))),
    };
    
    let container_id = match &container_state.container_id {
        Some(id) => id.clone(),
        None => return Ok(Json(ApiResponse::error("Container has no Docker ID".to_string()))),
    };
    
    // Lock-free check
    if state.state_manager.is_container_locked(&container_uuid) {
        return Ok(Json(ApiResponse::error("Container is currently locked".to_string())));
    }
    
    // Lock container
    let _ = state.state_manager.lock_container(&container_uuid, "Creating snapshot").await;
    
    let snapshot_manager = SnapshotManager::new(
        state.docker.client().clone(),
        &state.config.storage.base_path,
    );
    
    if let Err(e) = snapshot_manager.init().await {
        error!("Failed to initialize snapshot storage: {}", e);
        let _ = state.state_manager.unlock_container(&container_uuid).await;
        return Ok(Json(ApiResponse::error("Failed to initialize snapshot storage".to_string())));
    }
    
    let result = snapshot_manager.create_snapshot(&container_id, &container_uuid, &*state.state_manager).await;
    
    let _ = state.state_manager.unlock_container(&container_uuid).await;
    
    match result {
        Ok(snapshot_id) => {
            let response = SnapshotResponse {
                snapshot_id: snapshot_id.clone(),
                container_uuid,
                status: "success".to_string(),
                message: format!("Snapshot {} created successfully", snapshot_id),
            };
            Ok(Json(ApiResponse::success(response)))
        }
        Err(e) => {
            error!("Failed to create snapshot: {}", e);
            Ok(Json(ApiResponse::error(format!("Failed to create snapshot: {}", e))))
        }
    }
}

/// Restore a container from snapshot
pub async fn restore_snapshot(
    State(state): State<AppState>,
    Path(snapshot_id): Path<String>,
    Json(req): Json<RestoreSnapshotRequest>,
) -> Result<Json<ApiResponse<RestoreResponse>>, StatusCode> {
    info!("Restoring snapshot: {}", snapshot_id);
    
    let container_uuid = req.container_uuid;
    
    // Lock-free lookup
    let container_state = match state.state_manager.get_container(&container_uuid) {
        Some(s) => s,
        None => return Ok(Json(ApiResponse::error(format!("Container {} not found", container_uuid)))),
    };
    
    let container_id = match &container_state.container_id {
        Some(id) => id.clone(),
        None => return Ok(Json(ApiResponse::error("Container has no Docker ID".to_string()))),
    };
    
    if state.state_manager.is_container_locked(&container_uuid) {
        return Ok(Json(ApiResponse::error("Container is currently locked".to_string())));
    }
    
    let _ = state.state_manager.lock_container(&container_uuid, "Restoring snapshot").await;
    
    let snapshot_manager = SnapshotManager::new(
        state.docker.client().clone(),
        &state.config.storage.base_path,
    );
    
    if let Err(e) = snapshot_manager.init().await {
        error!("Failed to initialize snapshot storage: {}", e);
        let _ = state.state_manager.unlock_container(&container_uuid).await;
        return Ok(Json(ApiResponse::error("Failed to initialize snapshot storage".to_string())));
    }
    
    if let Err(e) = snapshot_manager.get_snapshot_metadata(&snapshot_id).await {
        error!("Snapshot {} not found: {}", snapshot_id, e);
        let _ = state.state_manager.unlock_container(&container_uuid).await;
        return Ok(Json(ApiResponse::error(format!("Snapshot {} not found", snapshot_id))));
    }
    
    let result = snapshot_manager.restore_snapshot(&snapshot_id, &container_id).await;
    
    let _ = state.state_manager.unlock_container(&container_uuid).await;
    
    match result {
        Ok(_) => {
            let response = RestoreResponse {
                new_container_uuid: container_uuid.clone(),
                new_container_id: container_id.to_string(),
                snapshot_id: snapshot_id.clone(),
                status: "success".to_string(),
                message: format!("Snapshot {} restored to container {}", snapshot_id, container_uuid),
            };
            Ok(Json(ApiResponse::success(response)))
        }
        Err(e) => {
            error!("Failed to restore snapshot {}: {}", snapshot_id, e);
            Ok(Json(ApiResponse::error(format!("Failed to restore snapshot: {}", e))))
        }
    }
}

/// List all available snapshots
pub async fn list_snapshots(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<crate::docker::snapshot::SnapshotInfo>>>, StatusCode> {
    info!("Listing all snapshots");
    
    let snapshot_manager = SnapshotManager::new(
        state.docker.client().clone(),
        &state.config.storage.base_path,
    );
    
    if let Err(e) = snapshot_manager.init().await {
        error!("Failed to initialize snapshot storage: {}", e);
        return Ok(Json(ApiResponse::error("Failed to initialize snapshot storage".to_string())));
    }
    
    match snapshot_manager.list_snapshots().await {
        Ok(snapshots) => Ok(Json(ApiResponse::success(snapshots))),
        Err(e) => {
            error!("Failed to list snapshots: {}", e);
            Ok(Json(ApiResponse::error(format!("Failed to list snapshots: {}", e))))
        }
    }
}

/// Get snapshot metadata
pub async fn get_snapshot_info(
    State(state): State<AppState>,
    Path(snapshot_id): Path<String>,
) -> Result<Json<ApiResponse<crate::docker::snapshot::SnapshotMetadata>>, StatusCode> {
    info!("Getting info for snapshot: {}", snapshot_id);
    
    let snapshot_manager = SnapshotManager::new(
        state.docker.client().clone(),
        &state.config.storage.base_path,
    );
    
    if let Err(e) = snapshot_manager.init().await {
        error!("Failed to initialize snapshot storage: {}", e);
        return Ok(Json(ApiResponse::error("Failed to initialize snapshot storage".to_string())));
    }
    
    match snapshot_manager.get_snapshot_metadata(&snapshot_id).await {
        Ok(metadata) => Ok(Json(ApiResponse::success(metadata))),
        Err(e) => {
            error!("Failed to get snapshot {} metadata: {}", snapshot_id, e);
            Ok(Json(ApiResponse::error(format!("Snapshot {} not found", snapshot_id))))
        }
    }
}

/// Delete a snapshot
pub async fn delete_snapshot(
    State(state): State<AppState>,
    Path(snapshot_id): Path<String>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Deleting snapshot: {}", snapshot_id);
    
    let snapshot_manager = SnapshotManager::new(
        state.docker.client().clone(),
        &state.config.storage.base_path,
    );
    
    if let Err(e) = snapshot_manager.init().await {
        error!("Failed to initialize snapshot storage: {}", e);
        return Ok(Json(ApiResponse::error("Failed to initialize snapshot storage".to_string())));
    }
    
    match snapshot_manager.delete_snapshot(&snapshot_id).await {
        Ok(_) => Ok(Json(ApiResponse::success(format!("Snapshot {} deleted successfully", snapshot_id)))),
        Err(e) => {
            error!("Failed to delete snapshot {}: {}", snapshot_id, e);
            Ok(Json(ApiResponse::error(format!("Failed to delete snapshot: {}", e))))
        }
    }
}

/// List snapshots for a specific container
pub async fn list_container_snapshots(
    State(state): State<AppState>,
    Path(container_uuid): Path<String>,
) -> Result<Json<ApiResponse<Vec<crate::docker::snapshot::SnapshotInfo>>>, StatusCode> {
    info!("Listing snapshots for container: {}", container_uuid);
    
    let snapshot_manager = SnapshotManager::new(
        state.docker.client().clone(),
        &state.config.storage.base_path,
    );
    
    if let Err(e) = snapshot_manager.init().await {
        error!("Failed to initialize snapshot storage: {}", e);
        return Ok(Json(ApiResponse::error("Failed to initialize snapshot storage".to_string())));
    }
    
    match snapshot_manager.list_snapshots().await {
        Ok(snapshots) => {
            let container_snapshots: Vec<_> = snapshots
                .into_iter()
                .filter(|s| s.container_uuid == container_uuid)
                .collect();
            
            Ok(Json(ApiResponse::success(container_snapshots)))
        }
        Err(e) => {
            error!("Failed to list snapshots for container {}: {}", container_uuid, e);
            Ok(Json(ApiResponse::error(format!("Failed to list snapshots: {}", e))))
        }
    }
}
