use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use tracing::{error, info};

use crate::{
    docker::FilesystemManager,
    models::{ApiResponse, FileSystemRequest, WriteFileRequest, CreateDirectoryRequest, DeleteRequest},
    types::AppState,
};

/// List files and directories in a container path
pub async fn list_directory(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Query(req): Query<FileSystemRequest>,
) -> Result<Json<ApiResponse<crate::docker::DirectoryListing>>, StatusCode> {
    let path = req.path.unwrap_or_else(|| "/".to_string());
    info!("Listing directory {} in container {}", path, container_id);
    
    let manager = FilesystemManager::new(state.docker.client());
    match manager.list_directory(&container_id, &path).await {
        Ok(listing) => Ok(Json(ApiResponse::success(listing))),
        Err(e) => {
            error!("Failed to list directory {} in container {}: {}", path, container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Get file content from container
pub async fn get_file_content(
    State(state): State<AppState>,
    Path((container_id, file_path)): Path<(String, String)>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    // The file_path comes from the *path capture, so it should be the full path
    let clean_path = if file_path.starts_with('/') {
        file_path
    } else {
        format!("/{}", file_path)
    };
    
    info!("Getting file content {} from container {}", clean_path, container_id);
    
    let manager = FilesystemManager::new(state.docker.client());
    match manager.get_file_content(&container_id, &clean_path).await {
        Ok(content) => Ok(Json(ApiResponse::success(content))),
        Err(e) => {
            error!("Failed to get file content {} from container {}: {}", clean_path, container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Write file content to container
pub async fn write_file(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<WriteFileRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Writing file {} in container {}", req.path, container_id);
    
    let manager = FilesystemManager::new(state.docker.client());
    match manager.write_file(&container_id, &req.path, &req.content).await {
        Ok(_) => Ok(Json(ApiResponse::success(format!("File {} written successfully", req.path)))),
        Err(e) => {
            error!("Failed to write file {} in container {}: {}", req.path, container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Create directory in container
pub async fn create_directory(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<CreateDirectoryRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Creating directory {} in container {}", req.path, container_id);
    
    let manager = FilesystemManager::new(state.docker.client());
    match manager.create_directory(&container_id, &req.path).await {
        Ok(_) => Ok(Json(ApiResponse::success(format!("Directory {} created successfully", req.path)))),
        Err(e) => {
            error!("Failed to create directory {} in container {}: {}", req.path, container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Delete file or directory from container
pub async fn delete_path(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<DeleteRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Deleting {} from container {}", req.path, container_id);
    
    let manager = FilesystemManager::new(state.docker.client());
    match manager.delete(&container_id, &req.path).await {
        Ok(_) => Ok(Json(ApiResponse::success(format!("Path {} deleted successfully", req.path)))),
        Err(e) => {
            error!("Failed to delete {} from container {}: {}", req.path, container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}