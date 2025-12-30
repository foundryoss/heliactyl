use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use tracing::{error, info};

use crate::{
    docker::VolumeManager,
    models::{ApiResponse, CreateVolumeRequest},
    types::AppState,
};

/// Create a new volume
pub async fn create_volume(
    State(state): State<AppState>,
    Json(req): Json<CreateVolumeRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Creating volume: {}", req.name);
    
    let manager = VolumeManager::new(state.docker.client());
    match manager.create(req).await {
        Ok(name) => Ok(Json(ApiResponse::success(name))),
        Err(e) => {
            error!("Failed to create volume: {}", e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// List all volumes
pub async fn list_volumes(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<crate::models::VolumeInfo>>>, StatusCode> {
    let manager = VolumeManager::new(state.docker.client());
    match manager.list().await {
        Ok(volumes) => Ok(Json(ApiResponse::success(volumes))),
        Err(e) => {
            error!("Failed to list volumes: {}", e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Remove a volume
pub async fn remove_volume(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Removing volume: {}", name);
    
    let manager = VolumeManager::new(state.docker.client());
    match manager.remove(&name).await {
        Ok(_) => Ok(Json(ApiResponse::success(format!("Volume {} removed", name)))),
        Err(e) => {
            error!("Failed to remove volume {}: {}", name, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}