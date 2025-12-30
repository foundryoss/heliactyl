use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use tracing::{error, info};

use crate::{
    docker::FilesystemManager,
    models::{ApiResponse, FileSystemRequest, WriteFileRequest, CreateDirectoryRequest, DeleteRequest, 
             ChmodRequest, ChownRequest, CreateArchiveRequest, ExtractArchiveRequest, 
             CreateZipRequest, ExtractZipRequest, CopyFileRequest},
    types::AppState,
};

/// Helper function to check if a container is suspended
async fn check_container_suspended(state: &AppState, container_id: &str) -> Result<bool, String> {
    let state_manager = state.state_manager.lock().await;
    if let Some((uuid, _)) = state_manager.find_by_container_id(container_id) {
        Ok(state_manager.is_container_suspended(uuid))
    } else {
        Err("Container not found in daemon state".to_string())
    }
}

/// List files and directories in a container path
pub async fn list_directory(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Query(req): Query<FileSystemRequest>,
) -> Result<Json<ApiResponse<crate::docker::DirectoryListing>>, StatusCode> {
    let path = req.path.unwrap_or_else(|| "/".to_string());
    info!("Listing directory {} in container {}", path, container_id);
    
    // Pre-validate the path before passing to manager
    let manager = FilesystemManager::new(state.docker.client());
    
    // Use a dummy call to sanitize_path to validate the path
    // We'll create a temporary manager instance to access the sanitize method
    match manager.validate_path(&path) {
        Ok(_) => {
            match manager.list_directory(&container_id, &path).await {
                Ok(listing) => Ok(Json(ApiResponse::success(listing))),
                Err(e) => {
                    error!("Failed to list directory {} in container {}: {}", path, container_id, e);
                    Ok(Json(ApiResponse::error(e.to_string())))
                }
            }
        }
        Err(e) => {
            error!("Invalid path {} for container {}: {}", path, container_id, e);
            Ok(Json(ApiResponse::error(format!("Invalid path: {}", e))))
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
    
    // Pre-validate the path before passing to manager
    match manager.validate_path(&clean_path) {
        Ok(_) => {
            match manager.get_file_content(&container_id, &clean_path).await {
                Ok(content) => Ok(Json(ApiResponse::success(content))),
                Err(e) => {
                    error!("Failed to get file content {} from container {}: {}", clean_path, container_id, e);
                    Ok(Json(ApiResponse::error(e.to_string())))
                }
            }
        }
        Err(e) => {
            error!("Invalid path {} for container {}: {}", clean_path, container_id, e);
            Ok(Json(ApiResponse::error(format!("Invalid path: {}", e))))
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
    
    // Check if container is suspended
    match check_container_suspended(&state, &container_id).await {
        Ok(true) => {
            return Ok(Json(ApiResponse::error("Cannot perform file operations on suspended container".to_string())));
        }
        Ok(false) => {}, // Not suspended, continue
        Err(e) => {
            return Ok(Json(ApiResponse::error(e)));
        }
    }
    
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

/// Change file permissions (chmod)
pub async fn chmod_path(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<ChmodRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Changing permissions of {} to {} in container {}", req.path, req.permissions, container_id);
    
    let manager = FilesystemManager::new(state.docker.client());
    match manager.chmod(&container_id, &req.path, &req.permissions).await {
        Ok(_) => Ok(Json(ApiResponse::success(format!("Permissions changed to {} for {}", req.permissions, req.path)))),
        Err(e) => {
            error!("Failed to chmod {} in container {}: {}", req.path, container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Change file ownership (chown)
pub async fn chown_path(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<ChownRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    let ownership = if let Some(ref group) = req.group {
        format!("{}:{}", req.owner, group)
    } else {
        req.owner.clone()
    };
    
    info!("Changing ownership of {} to {} in container {}", req.path, ownership, container_id);
    
    let manager = FilesystemManager::new(state.docker.client());
    match manager.chown(&container_id, &req.path, &req.owner, req.group.as_deref()).await {
        Ok(_) => Ok(Json(ApiResponse::success(format!("Ownership changed to {} for {}", ownership, req.path)))),
        Err(e) => {
            error!("Failed to chown {} in container {}: {}", req.path, container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Create tar archive
pub async fn create_archive(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<CreateArchiveRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Creating archive {} from {} in container {}", req.archive_path, req.source_path, container_id);
    
    let manager = FilesystemManager::new(state.docker.client());
    match manager.create_archive(&container_id, &req.source_path, &req.archive_path, req.compression.as_deref()).await {
        Ok(result) => Ok(Json(ApiResponse::success(result))),
        Err(e) => {
            error!("Failed to create archive in container {}: {}", container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Extract tar archive
pub async fn extract_archive(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<ExtractArchiveRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Extracting archive {} to {} in container {}", req.archive_path, req.destination_path, container_id);
    
    let manager = FilesystemManager::new(state.docker.client());
    match manager.extract_archive(&container_id, &req.archive_path, &req.destination_path).await {
        Ok(result) => Ok(Json(ApiResponse::success(result))),
        Err(e) => {
            error!("Failed to extract archive in container {}: {}", container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Create zip archive
pub async fn create_zip(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<CreateZipRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Creating zip {} from {} in container {}", req.zip_path, req.source_path, container_id);
    
    let manager = FilesystemManager::new(state.docker.client());
    match manager.create_zip(&container_id, &req.source_path, &req.zip_path).await {
        Ok(result) => Ok(Json(ApiResponse::success(result))),
        Err(e) => {
            error!("Failed to create zip in container {}: {}", container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Extract zip archive
pub async fn extract_zip(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<ExtractZipRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Extracting zip {} to {} in container {}", req.zip_path, req.destination_path, container_id);
    
    let manager = FilesystemManager::new(state.docker.client());
    match manager.extract_zip(&container_id, &req.zip_path, &req.destination_path).await {
        Ok(result) => Ok(Json(ApiResponse::success(result))),
        Err(e) => {
            error!("Failed to extract zip in container {}: {}", container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Copy or move files
pub async fn copy_file(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<CopyFileRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    let operation = if req.move_file.unwrap_or(false) { "move" } else { "copy" };
    info!("{}ing {} to {} in container {}", operation, req.source_path, req.destination_path, container_id);
    
    let manager = FilesystemManager::new(state.docker.client());
    match manager.copy_file(&container_id, &req.source_path, &req.destination_path, req.move_file.unwrap_or(false)).await {
        Ok(_) => Ok(Json(ApiResponse::success(format!("Successfully {}d {} to {}", operation, req.source_path, req.destination_path)))),
        Err(e) => {
            error!("Failed to {} file in container {}: {}", operation, container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}