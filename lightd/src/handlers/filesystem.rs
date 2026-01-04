use axum::{
    extract::{Path, Query, State, Multipart},
    http::StatusCode,
    response::Json,
};
use tracing::{error, info};

use crate::{
    docker::FilesystemManagerDirect,
    docker::FilesystemManager,
    models::{ApiResponse, FileSystemRequest, WriteFileRequest, CreateDirectoryRequest, DeleteRequest, 
             CopyFileRequest, ChmodRequest, ChownRequest, CreateArchiveRequest, ExtractArchiveRequest,
             CreateZipRequest, ExtractZipRequest},
    types::AppState,
};

/// List files and directories in a container path
pub async fn list_directory(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Query(req): Query<FileSystemRequest>,
) -> Result<Json<ApiResponse<crate::docker::filesystem_direct::DirectoryListing>>, StatusCode> {
    // Frontend sends / as root, we need to convert to /home/container internally
    let frontend_path = req.path.unwrap_or_else(|| "/".to_string());
    
    // Convert frontend path (/) to backend path (/home/container)
    let backend_path = if frontend_path == "/" || frontend_path.is_empty() {
        "/home/container".to_string()
    } else if frontend_path.starts_with("/home/container") {
        frontend_path.clone()
    } else if frontend_path.starts_with('/') {
        format!("/home/container{}", frontend_path)
    } else {
        format!("/home/container/{}", frontend_path)
    };
    
    // Resolve container_id to UUID - the volumes are stored by UUID
    let uuid = resolve_container_to_uuid(&state, &container_id).await;
    
    info!("Listing directory {} (frontend: {}) in container {} (uuid: {})", backend_path, frontend_path, container_id, uuid);
    
    let volumes_path = state.config.storage.volumes_path.clone();
    let path_clone = backend_path.clone();
    let uuid_clone = uuid.clone();
    
    let result = tokio::task::spawn_blocking(move || {
        let manager = FilesystemManagerDirect::new(volumes_path);
        manager.list_directory(&uuid_clone, &path_clone)
    }).await;
    
    match result {
        Ok(Ok(mut listing)) => {
            // Convert backend paths back to frontend paths (strip /home/container)
            listing.path = if listing.path == "/home/container" {
                "/".to_string()
            } else {
                listing.path.trim_start_matches("/home/container").to_string()
            };
            
            for file in &mut listing.files {
                file.path = if file.path == "/home/container" {
                    "/".to_string()
                } else {
                    file.path.trim_start_matches("/home/container").to_string()
                };
            }
            
            Ok(Json(ApiResponse::success(listing)))
        }
        Ok(Err(e)) => {
            error!("Failed to list directory {} in container {}: {}", backend_path, container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
        Err(e) => {
            error!("Task panicked listing directory: {}", e);
            Ok(Json(ApiResponse::error("Internal server error".to_string())))
        }
    }
}

/// Resolve a container_id (which could be Docker ID or UUID) to the UUID
/// The volumes are stored by UUID, so we need to find the correct UUID
async fn resolve_container_to_uuid(state: &AppState, container_id: &str) -> String {
    // First check if it's already a UUID in the state manager
    if let Some(_) = state.state_manager.get_container(container_id) {
        return container_id.to_string();
    }
    
    // Try to find by Docker container ID in the container tracker
    if let Ok(Some(tracker)) = state.container_tracker.find_by_container_id(container_id).await {
        return tracker.custom_uuid;
    }
    
    // Try loading directly from container tracker (in case container_id is the UUID)
    if let Ok(tracker) = state.container_tracker.load_container(container_id).await {
        return tracker.custom_uuid;
    }
    
    // Fallback to the original container_id
    container_id.to_string()
}

/// Get file content from container
pub async fn get_file_content(
    State(state): State<AppState>,
    Path((container_id, file_path)): Path<(String, String)>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    let clean_path = if file_path.starts_with('/') {
        format!("/home/container{}", file_path)
    } else {
        format!("/home/container/{}", file_path)
    };
    
    // Resolve to UUID
    let uuid = resolve_container_to_uuid(&state, &container_id).await;
    
    info!("Getting file content {} from container {} (uuid: {})", clean_path, container_id, uuid);
    
    let volumes_path = state.config.storage.volumes_path.clone();
    let clean_path_clone = clean_path.clone();
    let uuid_clone = uuid.clone();
    
    // Use timeout for large files, spawn_blocking for CPU-bound reading
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::task::spawn_blocking(move || {
            let manager = FilesystemManagerDirect::new(volumes_path);
            manager.get_file_content(&uuid_clone, &clean_path_clone)
        })
    ).await;
    
    match result {
        Ok(Ok(Ok(content))) => Ok(Json(ApiResponse::success(content))),
        Ok(Ok(Err(e))) => {
            error!("Failed to get file content {} from container {}: {}", clean_path, container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
        Ok(Err(e)) => {
            error!("Task panicked reading file: {}", e);
            Ok(Json(ApiResponse::error("Internal server error".to_string())))
        }
        Err(_) => {
            error!("Timeout reading file {} from container {}", clean_path, container_id);
            Ok(Json(ApiResponse::error("Operation timed out".to_string())))
        }
    }
}

/// Write file content to container
pub async fn write_file(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<WriteFileRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    // Convert frontend path to backend path
    let backend_path = if req.path == "/" || req.path.is_empty() {
        "/home/container".to_string()
    } else if req.path.starts_with("/home/container") {
        req.path.clone()
    } else if req.path.starts_with('/') {
        format!("/home/container{}", req.path)
    } else {
        format!("/home/container/{}", req.path)
    };
    
    // Resolve to UUID
    let uuid = resolve_container_to_uuid(&state, &container_id).await;
    
    info!("Writing file {} in container {} (uuid: {})", backend_path, container_id, uuid);
    
    let volumes_path = state.config.storage.volumes_path.clone();
    let uuid_clone = uuid.clone();
    let path_clone = backend_path.clone();
    let content = req.content.clone();
    
    // Spawn blocking task and return immediately (fire-and-forget)
    tokio::task::spawn_blocking(move || {
        let manager = FilesystemManagerDirect::new(volumes_path);
        if let Err(e) = manager.write_file(&uuid_clone, &path_clone, &content) {
            error!("Failed to write file {} in container {}: {}", path_clone, uuid_clone, e);
        }
    });
    
    // Return immediately
    Ok(Json(ApiResponse::success(format!("File {} write initiated", req.path))))
}

/// Create directory in container
pub async fn create_directory(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<CreateDirectoryRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    // Convert frontend path (/) to backend path (/home/container)
    let backend_path = if req.path == "/" || req.path.is_empty() {
        "/home/container".to_string()
    } else if req.path.starts_with("/home/container") {
        req.path.clone()
    } else if req.path.starts_with('/') {
        format!("/home/container{}", req.path)
    } else {
        format!("/home/container/{}", req.path)
    };
    
    // Resolve to UUID
    let uuid = resolve_container_to_uuid(&state, &container_id).await;
    
    info!("Creating directory {} in container {} (uuid: {})", backend_path, container_id, uuid);
    
    let volumes_path = state.config.storage.volumes_path.clone();
    let uuid_clone = uuid.clone();
    let path_clone = backend_path.clone();
    
    // Spawn blocking task and return immediately (fire-and-forget)
    tokio::task::spawn_blocking(move || {
        let manager = FilesystemManagerDirect::new(volumes_path);
        if let Err(e) = manager.create_directory(&uuid_clone, &path_clone) {
            error!("Failed to create directory {} in container {}: {}", path_clone, uuid_clone, e);
        }
    });
    
    Ok(Json(ApiResponse::success(format!("Directory {} creation initiated", req.path))))
}

/// Delete file or directory from container
pub async fn delete_path(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<DeleteRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    // Convert frontend path to backend path
    let backend_path = if req.path == "/" || req.path.is_empty() {
        "/home/container".to_string()
    } else if req.path.starts_with("/home/container") {
        req.path.clone()
    } else if req.path.starts_with('/') {
        format!("/home/container{}", req.path)
    } else {
        format!("/home/container/{}", req.path)
    };
    
    // Resolve to UUID
    let uuid = resolve_container_to_uuid(&state, &container_id).await;
    
    info!("Deleting {} from container {} (uuid: {})", backend_path, container_id, uuid);
    
    let volumes_path = state.config.storage.volumes_path.clone();
    let uuid_clone = uuid.clone();
    let path_clone = backend_path.clone();
    
    // Spawn blocking task and return immediately (fire-and-forget)
    tokio::task::spawn_blocking(move || {
        let manager = FilesystemManagerDirect::new(volumes_path);
        if let Err(e) = manager.delete(&uuid_clone, &path_clone) {
            error!("Failed to delete {} from container {}: {}", path_clone, uuid_clone, e);
        }
    });
    
    Ok(Json(ApiResponse::success(format!("Deletion of {} initiated", req.path))))
}

/// Copy or move files
pub async fn copy_file(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<CopyFileRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    // Convert frontend paths to backend paths
    let source_backend = if req.source_path == "/" || req.source_path.is_empty() {
        "/home/container".to_string()
    } else if req.source_path.starts_with("/home/container") {
        req.source_path.clone()
    } else if req.source_path.starts_with('/') {
        format!("/home/container{}", req.source_path)
    } else {
        format!("/home/container/{}", req.source_path)
    };
    
    let dest_backend = if req.destination_path == "/" || req.destination_path.is_empty() {
        "/home/container".to_string()
    } else if req.destination_path.starts_with("/home/container") {
        req.destination_path.clone()
    } else if req.destination_path.starts_with('/') {
        format!("/home/container{}", req.destination_path)
    } else {
        format!("/home/container/{}", req.destination_path)
    };
    
    // Resolve to UUID
    let uuid = resolve_container_to_uuid(&state, &container_id).await;
    
    let operation = if req.move_file.unwrap_or(false) { "move" } else { "copy" };
    info!("{}ing {} to {} in container {} (uuid: {})", operation, source_backend, dest_backend, container_id, uuid);
    
    let volumes_path = state.config.storage.volumes_path.clone();
    let uuid_clone = uuid.clone();
    let source_clone = source_backend.clone();
    let dest_clone = dest_backend.clone();
    let is_move = req.move_file.unwrap_or(false);
    let operation_clone = operation.to_string();
    
    // Spawn blocking task and return immediately (fire-and-forget)
    tokio::task::spawn_blocking(move || {
        let manager = FilesystemManagerDirect::new(volumes_path);
        
        let result = if is_move {
            manager.rename(&uuid_clone, &source_clone, &dest_clone)
        } else {
            // For copy, use byte-safe filesystem copy (supports tar.gz, zip, etc.)
            manager.copy_file(&uuid_clone, &source_clone, &dest_clone)
        };
        
        if let Err(e) = result {
            error!("Failed to {} file in container {}: {}", operation_clone, uuid_clone, e);
        }
    });
    
    Ok(Json(ApiResponse::success(format!("{} operation initiated from {} to {}", operation, req.source_path, req.destination_path))))
}

/// Change file permissions (chmod)
pub async fn chmod_path(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<ChmodRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    // Convert frontend path to backend path
    let backend_path = if req.path == "/" || req.path.is_empty() {
        "/home/container".to_string()
    } else if req.path.starts_with("/home/container") {
        req.path.clone()
    } else if req.path.starts_with('/') {
        format!("/home/container{}", req.path)
    } else {
        format!("/home/container/{}", req.path)
    };
    
    info!("Changing permissions of {} to {} in container {}", backend_path, req.permissions, container_id);
    
    let volumes_path = state.config.storage.volumes_path.clone();
    let uuid = container_id.clone();
    let path_clone = backend_path.clone();
    let permissions = req.permissions.clone();
    
    let result = tokio::task::spawn_blocking(move || {
        let manager = FilesystemManagerDirect::new(volumes_path);
        manager.chmod(&uuid, &path_clone, &permissions)
    }).await;
    
    match result {
        Ok(Ok(())) => Ok(Json(ApiResponse::success(format!("Permissions changed to {} for {}", req.permissions, req.path)))),
        Ok(Err(e)) => {
            error!("Failed to chmod {} in container {}: {}", backend_path, container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
        Err(e) => {
            error!("Task panicked during chmod: {}", e);
            Ok(Json(ApiResponse::error("Internal server error".to_string())))
        }
    }
}

/// Change file ownership (chown)
pub async fn chown_path(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<ChownRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    // Convert frontend path to backend path
    let backend_path = if req.path == "/" || req.path.is_empty() {
        "/home/container".to_string()
    } else if req.path.starts_with("/home/container") {
        req.path.clone()
    } else if req.path.starts_with('/') {
        format!("/home/container{}", req.path)
    } else {
        format!("/home/container/{}", req.path)
    };
    
    info!("Changing ownership of {} to {} in container {}", backend_path, req.owner, container_id);
    
    let volumes_path = state.config.storage.volumes_path.clone();
    let uuid = container_id.clone();
    let path_clone = backend_path.clone();
    let owner = req.owner.clone();
    let group = req.group.clone();
    
    let result = tokio::task::spawn_blocking(move || {
        let manager = FilesystemManagerDirect::new(volumes_path);
        manager.chown(&uuid, &path_clone, &owner, group.as_deref())
    }).await;
    
    match result {
        Ok(Ok(())) => Ok(Json(ApiResponse::success(format!("Ownership changed to {} for {}", req.owner, req.path)))),
        Ok(Err(e)) => {
            error!("Failed to chown {} in container {}: {}", backend_path, container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
        Err(e) => {
            error!("Task panicked during chown: {}", e);
            Ok(Json(ApiResponse::error("Internal server error".to_string())))
        }
    }
}

/// Create a tar archive
pub async fn create_archive(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<CreateArchiveRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    // Convert frontend paths to backend paths
    let source_backends: Vec<String> = req.source_paths.iter().map(|source_path| {
        if source_path == "/" || source_path.is_empty() {
            "/home/container".to_string()
        } else if source_path.starts_with("/home/container") {
            source_path.clone()
        } else if source_path.starts_with('/') {
            format!("/home/container{}", source_path)
        } else {
            format!("/home/container/{}", source_path)
        }
    }).collect();
    
    let archive_backend = if req.archive_path.starts_with("/home/container") {
        req.archive_path.clone()
    } else if req.archive_path.starts_with('/') {
        format!("/home/container{}", req.archive_path)
    } else {
        format!("/home/container/{}", req.archive_path)
    };
    
    info!("Creating archive {} from {:?} in container {}", archive_backend, source_backends, container_id);
    
    let volumes_path = state.config.storage.volumes_path.clone();
    let uuid = container_id.clone();
    let sources_clone = source_backends.clone();
    let archive_clone = archive_backend.clone();
    let compression = req.compression.clone();
    
    let result = tokio::task::spawn_blocking(move || {
        let manager = FilesystemManagerDirect::new(volumes_path);
        manager.create_archive(&uuid, &sources_clone, &archive_clone, compression.as_deref())
    }).await;
    
    match result {
        Ok(Ok(message)) => Ok(Json(ApiResponse::success(message))),
        Ok(Err(e)) => {
            error!("Failed to create archive in container {}: {}", container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
        Err(e) => {
            error!("Task panicked during archive creation: {}", e);
            Ok(Json(ApiResponse::error("Internal server error".to_string())))
        }
    }
}

/// Extract a tar archive
pub async fn extract_archive(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<ExtractArchiveRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    // Convert frontend paths to backend paths
    let archive_backend = if req.archive_path.starts_with("/home/container") {
        req.archive_path.clone()
    } else if req.archive_path.starts_with('/') {
        format!("/home/container{}", req.archive_path)
    } else {
        format!("/home/container/{}", req.archive_path)
    };
    
    let dest_backend = if req.destination_path == "/" || req.destination_path.is_empty() {
        "/home/container".to_string()
    } else if req.destination_path.starts_with("/home/container") {
        req.destination_path.clone()
    } else if req.destination_path.starts_with('/') {
        format!("/home/container{}", req.destination_path)
    } else {
        format!("/home/container/{}", req.destination_path)
    };
    
    info!("Extracting archive {} to {} in container {}", archive_backend, dest_backend, container_id);
    
    // Get the Docker container ID from UUID
    let docker_container_id = match state.container_tracker.load_container(&container_id).await {
        Ok(info) => info.container_id,
        _ => container_id.clone(),
    };
    
    let volumes_path = state.config.storage.volumes_path.clone();
    let uuid = container_id.clone();
    let archive_clone = archive_backend.clone();
    let dest_clone = dest_backend.clone();
    
    let result = tokio::task::spawn_blocking(move || {
        let manager = FilesystemManagerDirect::new(volumes_path);
        manager.extract_archive(&uuid, &archive_clone, &dest_clone)
    }).await;
    
    match result {
        Ok(Ok(message)) => Ok(Json(ApiResponse::success(message))),
        Ok(Err(e)) => {
            error!("Failed to extract archive in container {}: {}", container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
        Err(e) => {
            error!("Task panicked during archive extraction: {}", e);
            Ok(Json(ApiResponse::error("Internal server error".to_string())))
        }
    }
}

/// Create a zip archive
pub async fn create_zip(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<CreateZipRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    // Convert frontend paths to backend paths
    let source_backends: Vec<String> = req.source_paths.iter().map(|source_path| {
        if source_path == "/" || source_path.is_empty() {
            "/home/container".to_string()
        } else if source_path.starts_with("/home/container") {
            source_path.clone()
        } else if source_path.starts_with('/') {
            format!("/home/container{}", source_path)
        } else {
            format!("/home/container/{}", source_path)
        }
    }).collect();
    
    let zip_backend = if req.zip_path.starts_with("/home/container") {
        req.zip_path.clone()
    } else if req.zip_path.starts_with('/') {
        format!("/home/container{}", req.zip_path)
    } else {
        format!("/home/container/{}", req.zip_path)
    };
    
    info!("Creating zip {} from {:?} in container {}", zip_backend, source_backends, container_id);
    
    let volumes_path = state.config.storage.volumes_path.clone();
    let uuid = container_id.clone();
    let sources_clone = source_backends.clone();
    let zip_clone = zip_backend.clone();
    
    let result = tokio::task::spawn_blocking(move || {
        let manager = FilesystemManagerDirect::new(volumes_path);
        manager.create_zip(&uuid, &sources_clone, &zip_clone)
    }).await;
    
    match result {
        Ok(Ok(message)) => Ok(Json(ApiResponse::success(message))),
        Ok(Err(e)) => {
            error!("Failed to create zip in container {}: {}", container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
        Err(e) => {
            error!("Task panicked during zip creation: {}", e);
            Ok(Json(ApiResponse::error("Internal server error".to_string())))
        }
    }
}

/// Extract a zip archive
pub async fn extract_zip(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Json(req): Json<ExtractZipRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    // Convert frontend paths to backend paths
    let zip_backend = if req.zip_path.starts_with("/home/container") {
        req.zip_path.clone()
    } else if req.zip_path.starts_with('/') {
        format!("/home/container{}", req.zip_path)
    } else {
        format!("/home/container/{}", req.zip_path)
    };
    
    let dest_backend = if req.destination_path == "/" || req.destination_path.is_empty() {
        "/home/container".to_string()
    } else if req.destination_path.starts_with("/home/container") {
        req.destination_path.clone()
    } else if req.destination_path.starts_with('/') {
        format!("/home/container{}", req.destination_path)
    } else {
        format!("/home/container/{}", req.destination_path)
    };
    
    info!("Extracting zip {} to {} in container {}", zip_backend, dest_backend, container_id);
    
    // Get the Docker container ID from UUID
    let docker_container_id = match state.container_tracker.load_container(&container_id).await {
        Ok(info) => info.container_id,
        _ => container_id.clone(),
    };
    
    let volumes_path = state.config.storage.volumes_path.clone();
    let uuid = container_id.clone();
    let zip_clone = zip_backend.clone();
    let dest_clone = dest_backend.clone();
    
    let result = tokio::task::spawn_blocking(move || {
        let manager = FilesystemManagerDirect::new(volumes_path);
        manager.extract_zip(&uuid, &zip_clone, &dest_clone)
    }).await;
    
    match result {
        Ok(Ok(message)) => Ok(Json(ApiResponse::success(message))),
        Ok(Err(e)) => {
            error!("Failed to extract zip in container {}: {}", container_id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
        Err(e) => {
            error!("Task panicked during zip extraction: {}", e);
            Ok(Json(ApiResponse::error("Internal server error".to_string())))
        }
    }
}

/// Upload file to container via multipart form
pub async fn upload_file(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    // Resolve to UUID
    let uuid = resolve_container_to_uuid(&state, &container_id).await;
    
    let mut uploaded_files = Vec::new();
    let mut target_path = "/".to_string();
    
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        error!("Failed to read multipart field: {}", e);
        StatusCode::BAD_REQUEST
    })? {
        let name = field.name().unwrap_or("").to_string();
        
        if name == "path" {
            // This is the target directory path
            target_path = field.text().await.map_err(|e| {
                error!("Failed to read path field: {}", e);
                StatusCode::BAD_REQUEST
            })?;
            continue;
        }
        
        if name == "file" || name == "files" {
            let file_name = field.file_name().unwrap_or("uploaded_file").to_string();
            let data = field.bytes().await.map_err(|e| {
                error!("Failed to read file data: {}", e);
                StatusCode::BAD_REQUEST
            })?;
            
            // Convert target path to backend path
            let backend_dir = if target_path == "/" || target_path.is_empty() {
                "/home/container".to_string()
            } else if target_path.starts_with("/home/container") {
                target_path.clone()
            } else if target_path.starts_with('/') {
                format!("/home/container{}", target_path)
            } else {
                format!("/home/container/{}", target_path)
            };
            
            let file_path = format!("{}/{}", backend_dir.trim_end_matches('/'), file_name);
            
            info!("Uploading file {} to {} in container {} (uuid: {})", file_name, file_path, container_id, uuid);
            
            let volumes_path = state.config.storage.volumes_path.clone();
            let uuid_clone = uuid.clone();
            let file_path_clone = file_path.clone();
            let file_name_clone = file_name.clone();
            
            // Write file synchronously
            let write_result = tokio::task::spawn_blocking(move || {
                let manager = FilesystemManagerDirect::new(volumes_path);
                manager.write_file_bytes(&uuid_clone, &file_path_clone, &data)
            }).await;
            
            match write_result {
                Ok(Ok(())) => {
                    uploaded_files.push(file_name_clone);
                }
                Ok(Err(e)) => {
                    error!("Failed to write uploaded file {}: {}", file_name, e);
                    return Ok(Json(ApiResponse::error(format!("Failed to upload {}: {}", file_name, e))));
                }
                Err(e) => {
                    error!("Task panicked during file upload: {}", e);
                    return Ok(Json(ApiResponse::error("Internal server error".to_string())));
                }
            }
        }
    }
    
    if uploaded_files.is_empty() {
        return Ok(Json(ApiResponse::error("No files uploaded".to_string())));
    }
    
    Ok(Json(ApiResponse::success(format!("Uploaded {} file(s): {}", uploaded_files.len(), uploaded_files.join(", ")))))
}
