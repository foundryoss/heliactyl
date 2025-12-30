use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
};
use tracing::{error, info};

use crate::{
    docker::ContainerManager,
    models::{ApiResponse, CreateContainerRequest, SuspendRequest, UnsuspendRequest, ContainerTracker, ResourceLimits, UpdateContainerRequest, UpdateLimitsRequest, InstallationStatus, ContainerLookupResponse},
    types::AppState,
    container_tracker::ContainerTrackingManager,
    state_manager::ContainerState,
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

/// Helper function to resolve container identifier (UUID or Container ID) to Container ID
async fn resolve_container_id(state: &AppState, identifier: &str) -> Result<String, String> {
    let state_manager = state.state_manager.lock().await;
    
    // First try to find by container ID
    if let Some((_, _)) = state_manager.find_by_container_id(identifier) {
        return Ok(identifier.to_string());
    }
    
    // If not found, try to find by UUID
    if let Some(container_state) = state_manager.get_container(identifier) {
        if let Some(container_id) = &container_state.container_id {
            return Ok(container_id.clone());
        } else {
            return Err("Container ID not found for this UUID".to_string());
        }
    }
    
    Err("Container not found with this identifier".to_string())
}

/// Create a new container
pub async fn create_container(
    State(state): State<AppState>,
    Json(req): Json<CreateContainerRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    info!("Creating container with image: {}", req.image);
    
    // Generate UUID if not provided
    let custom_uuid = req.custom_uuid.clone()
        .unwrap_or_else(|| ContainerTrackingManager::generate_uuid());
    
    // Create container state for daemon state tracking
    let container_state = ContainerState {
        state: "creating".to_string(),
        container_id: None,
        image: req.image.clone(),
        name: req.name.clone().unwrap_or_else(|| "unnamed".to_string()),
        description: req.description.clone(),
        startup_command: req.startup_command.as_ref().map(|cmd| cmd.join(" ")),
        disk_limit: req.limits.as_ref().and_then(|l| l.disk.clone()),
        memory_limit: req.limits.as_ref().and_then(|l| l.memory.clone()),
        cpu_limit: req.limits.as_ref().and_then(|l| l.cpu.clone()),
        swap_limit: req.limits.as_ref().and_then(|l| l.swap.clone()),
        pids_limit: req.limits.as_ref().and_then(|l| l.pids),
        threads_limit: req.limits.as_ref().and_then(|l| l.threads),
        attached_volumes: req.volumes.as_ref().map(|v| v.iter().map(|vm| vm.source.clone()).collect()).unwrap_or_default(),
        ports: req.ports.clone().unwrap_or_default(),
        env: req.env.clone(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        locked: None,
        lock_reason: None,
        locked_at: None,
        restart_policy: None,
        install_content: req.install_content.clone(),
        update_content: req.update_content.clone(),
    };

    // Add to daemon state
    if let Err(e) = state.state_manager.lock().await.add_container(&custom_uuid, container_state).await {
        error!("Failed to add container to daemon state: {}", e);
    }
    
    let manager = ContainerManager::new(state.docker.client());
    match manager.create_with_networking(req.clone(), &state.network, &custom_uuid).await {
        Ok((container_id, allocations)) => {
            // Update daemon state with container ID and status
            if let Err(e) = state.state_manager.lock().await.update_container_id(&custom_uuid, &container_id).await {
                error!("Failed to update container ID in daemon state: {}", e);
            }
            if let Err(e) = state.state_manager.lock().await.update_container_state(&custom_uuid, "created").await {
                error!("Failed to update container state in daemon state: {}", e);
            }

            // Create container tracker for backward compatibility
            let tracker = ContainerTracker {
                custom_uuid: custom_uuid.clone(),
                container_id: container_id.clone(),
                name: req.name.clone().unwrap_or_else(|| "unnamed".to_string()),
                image: req.image.clone(),
                description: req.description.clone(),
                startup_command: req.startup_command.clone(),
                created_at: chrono::Utc::now(),
                limits: req.limits.clone().unwrap_or_else(|| ResourceLimits {
                    cpu: None,
                    memory: None,
                    disk: None,
                    swap: None,
                    pids: None,
                    threads: None,
                }),
                allocated_ports: allocations.clone(),
                attached_volumes: req.volumes.clone().unwrap_or_default(),
                ports: req.ports.clone().unwrap_or_default(),
                env: req.env.clone(),
                status: "created".to_string(),
                install_content: req.install_content.clone(),
                update_content: req.update_content.clone(),
            };
            
            // Save container tracking data
            if let Err(e) = state.container_tracker.save_container(&tracker).await {
                error!("Failed to save container tracking data: {}", e);
            }

            // Start the container first
            if let Err(e) = manager.start(&container_id).await {
                error!("Failed to start container for installation: {}", e);
                if let Err(state_err) = state.state_manager.lock().await.update_container_state(&custom_uuid, "failed").await {
                    error!("Failed to update container state to failed: {}", state_err);
                }
                return Ok(Json(ApiResponse::error(format!("Failed to start container: {}", e))));
            }

            // Update state to running
            if let Err(e) = state.state_manager.lock().await.update_container_state(&custom_uuid, "running").await {
                error!("Failed to update container state to running: {}", e);
            }

            // Run installation script if provided
            if let Some(install_script) = &req.install_content {
                info!("Running installation script for container: {}", container_id);
                
                // Update state to installing and lock container
                if let Err(e) = state.state_manager.lock().await.update_container_state(&custom_uuid, "installing").await {
                    error!("Failed to update container state to installing: {}", e);
                }
                if let Err(e) = state.state_manager.lock().await.lock_container(&custom_uuid, "Running installation script").await {
                    error!("Failed to lock container during installation: {}", e);
                }

                // Run the installation script
                match manager.run_installation(&container_id, install_script).await {
                    Ok(install_logs) => {
                        info!("Installation completed successfully for container: {}", container_id);
                        
                        // Update state to ready and unlock
                        if let Err(e) = state.state_manager.lock().await.update_container_state(&custom_uuid, "ready").await {
                            error!("Failed to update container state to ready: {}", e);
                        }
                        if let Err(e) = state.state_manager.lock().await.unlock_container(&custom_uuid).await {
                            error!("Failed to unlock container after installation: {}", e);
                        }
                        
                        info!("Installation logs: {}", install_logs);
                    }
                    Err(e) => {
                        error!("Installation failed for container {}: {}", container_id, e);
                        
                        // Update state to failed and unlock
                        if let Err(state_err) = state.state_manager.lock().await.update_container_state(&custom_uuid, "install_failed").await {
                            error!("Failed to update container state to install_failed: {}", state_err);
                        }
                        if let Err(state_err) = state.state_manager.lock().await.unlock_container(&custom_uuid).await {
                            error!("Failed to unlock container after failed installation: {}", state_err);
                        }
                        
                        return Ok(Json(ApiResponse::error(format!("Installation failed: {}", e))));
                    }
                }
            } else {
                // No installation script, mark as ready
                if let Err(e) = state.state_manager.lock().await.update_container_state(&custom_uuid, "ready").await {
                    error!("Failed to update container state to ready: {}", e);
                }
            }
            
            let response = serde_json::json!({
                "container_id": container_id,
                "custom_uuid": custom_uuid,
                "allocated_ports": allocations.iter().map(|alloc| {
                    serde_json::json!({
                        "container_port": alloc.container_port,
                        "host_port": alloc.host_port,
                        "host_ip": alloc.host_ip,
                        "protocol": alloc.protocol
                    })
                }).collect::<Vec<_>>(),
                "limits": tracker.limits
            });
            Ok(Json(ApiResponse::success(response)))
        }
        Err(e) => {
            error!("Failed to create container: {}", e);
            // Update daemon state to failed
            if let Err(state_err) = state.state_manager.lock().await.update_container_state(&custom_uuid, "failed").await {
                error!("Failed to update container state to failed: {}", state_err);
            }
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// List all containers
pub async fn list_containers(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<crate::models::ContainerInfo>>>, StatusCode> {
    let manager = ContainerManager::new(state.docker.client());
    match manager.list().await {
        Ok(containers) => Ok(Json(ApiResponse::success(containers))),
        Err(e) => {
            error!("Failed to list containers: {}", e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Start a container
pub async fn start_container(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Starting container: {}", id);
    
    // Check if container is suspended
    match check_container_suspended(&state, &id).await {
        Ok(true) => {
            return Ok(Json(ApiResponse::error("Cannot start suspended container. Unsuspend it first.".to_string())));
        }
        Ok(false) => {}, // Not suspended, continue
        Err(e) => {
            return Ok(Json(ApiResponse::error(e)));
        }
    }
    
    let manager = ContainerManager::new(state.docker.client());
    match manager.start(&id).await {
        Ok(_) => {
            // Update daemon state - find container by ID and update status
            let state_manager = state.state_manager.lock().await;
            if let Some((uuid, _)) = state_manager.find_by_container_id(&id) {
                let uuid = uuid.clone();
                drop(state_manager); // Release lock before async operation
                if let Err(e) = state.state_manager.lock().await.update_container_state(&uuid, "running").await {
                    error!("Failed to update container state in daemon state: {}", e);
                }
            }
            
            // Update container tracker status - find by container ID first
            if let Ok(Some(tracker)) = state.container_tracker.find_by_container_id(&id).await {
                if let Err(e) = state.container_tracker.update_container_status(&tracker.custom_uuid, "running").await {
                    error!("Failed to update container tracker status: {}", e);
                }
            }
            
            Ok(Json(ApiResponse::success(format!("Container {} started", id))))
        }
        Err(e) => {
            error!("Failed to start container {}: {}", id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Stop a container
pub async fn stop_container(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Stopping container: {}", id);
    
    let manager = ContainerManager::new(state.docker.client());
    match manager.stop(&id).await {
        Ok(_) => {
            // Update daemon state
            let state_manager = state.state_manager.lock().await;
            if let Some((uuid, _)) = state_manager.find_by_container_id(&id) {
                let uuid = uuid.clone();
                drop(state_manager);
                if let Err(e) = state.state_manager.lock().await.update_container_state(&uuid, "stopped").await {
                    error!("Failed to update container state in daemon state: {}", e);
                }
            }
            
            // Update container tracker status
            if let Ok(Some(tracker)) = state.container_tracker.find_by_container_id(&id).await {
                if let Err(e) = state.container_tracker.update_container_status(&tracker.custom_uuid, "stopped").await {
                    error!("Failed to update container tracker status: {}", e);
                }
            }
            
            Ok(Json(ApiResponse::success(format!("Container {} stopped", id))))
        }
        Err(e) => {
            error!("Failed to stop container {}: {}", id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Kill a container
pub async fn kill_container(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Killing container: {}", id);
    
    let manager = ContainerManager::new(state.docker.client());
    match manager.kill(&id).await {
        Ok(_) => {
            // Update daemon state
            let state_manager = state.state_manager.lock().await;
            if let Some((uuid, _)) = state_manager.find_by_container_id(&id) {
                let uuid = uuid.clone();
                drop(state_manager);
                if let Err(e) = state.state_manager.lock().await.update_container_state(&uuid, "killed").await {
                    error!("Failed to update container state in daemon state: {}", e);
                }
            }
            
            // Update container tracker status
            if let Ok(Some(tracker)) = state.container_tracker.find_by_container_id(&id).await {
                if let Err(e) = state.container_tracker.update_container_status(&tracker.custom_uuid, "killed").await {
                    error!("Failed to update container tracker status: {}", e);
                }
            }
            
            Ok(Json(ApiResponse::success(format!("Container {} killed", id))))
        }
        Err(e) => {
            error!("Failed to kill container {}: {}", id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Suspend a container (kill and lock completely)
pub async fn suspend_container(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<SuspendRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    let reason = req.message.unwrap_or_else(|| "Container suspended by administrator".to_string());
    info!("Suspending container: {} with reason: {}", id, reason);
    
    // Find container UUID first
    let state_manager = state.state_manager.lock().await;
    if let Some((uuid, _container_state)) = state_manager.find_by_container_id(&id) {
        let uuid = uuid.clone();
        drop(state_manager);
        
        // Check if already suspended
        if state.state_manager.lock().await.is_container_suspended(&uuid) {
            return Ok(Json(ApiResponse::error("Container is already suspended".to_string())));
        }
        
        let manager = ContainerManager::new(state.docker.client());
        match manager.suspend(&id, Some(reason.clone())).await {
            Ok(_) => {
                // Update daemon state to suspended
                if let Err(e) = state.state_manager.lock().await.suspend_container(&uuid, &reason).await {
                    error!("Failed to update container state to suspended: {}", e);
                }
                
                // Update container tracker status
                if let Ok(Some(tracker)) = state.container_tracker.find_by_container_id(&id).await {
                    if let Err(e) = state.container_tracker.update_container_status(&tracker.custom_uuid, "suspended").await {
                        error!("Failed to update container tracker status: {}", e);
                    }
                }
                
                Ok(Json(ApiResponse::success(format!("Container {} suspended: {}", id, reason))))
            }
            Err(e) => {
                error!("Failed to suspend container {}: {}", id, e);
                Ok(Json(ApiResponse::error(e.to_string())))
            }
        }
    } else {
        Ok(Json(ApiResponse::error("Container not found in daemon state".to_string())))
    }
}

/// Unsuspend a container (remove suspension lock)
pub async fn unsuspend_container(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UnsuspendRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    let reason = req.message.unwrap_or_else(|| "Container unsuspended by administrator".to_string());
    info!("Unsuspending container: {} with reason: {}", id, reason);
    
    // Find container UUID first
    let state_manager = state.state_manager.lock().await;
    if let Some((uuid, _container_state)) = state_manager.find_by_container_id(&id) {
        let uuid = uuid.clone();
        drop(state_manager);
        
        // Check if actually suspended
        if !state.state_manager.lock().await.is_container_suspended(&uuid) {
            return Ok(Json(ApiResponse::error("Container is not suspended".to_string())));
        }
        
        // Update daemon state to unsuspended (stopped)
        if let Err(e) = state.state_manager.lock().await.unsuspend_container(&uuid).await {
            error!("Failed to unsuspend container in daemon state: {}", e);
            return Ok(Json(ApiResponse::error("Failed to unsuspend container".to_string())));
        }
        
        // Update container tracker status
        if let Ok(Some(tracker)) = state.container_tracker.find_by_container_id(&id).await {
            if let Err(e) = state.container_tracker.update_container_status(&tracker.custom_uuid, "stopped").await {
                error!("Failed to update container tracker status: {}", e);
            }
        }
        
        Ok(Json(ApiResponse::success(format!("Container {} unsuspended: {}. Container is now stopped and can be started.", id, reason))))
    } else {
        Ok(Json(ApiResponse::error("Container not found in daemon state".to_string())))
    }
}

/// Attach to a container (create exec session for shell access)
pub async fn attach_container(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<crate::models::AttachResponse>>, StatusCode> {
    info!("Creating shell session for container: {}", id);
    
    // Check if container is suspended
    match check_container_suspended(&state, &id).await {
        Ok(true) => {
            return Ok(Json(ApiResponse::error("Cannot attach to suspended container".to_string())));
        }
        Ok(false) => {}, // Not suspended, continue
        Err(e) => {
            return Ok(Json(ApiResponse::error(e)));
        }
    }
    
    let manager = ContainerManager::new(state.docker.client());
    match manager.attach(&id).await {
        Ok(exec_id) => {
            let response = crate::models::AttachResponse {
                exec_id,
                container_id: id,
                status: "shell_session_created".to_string(),
            };
            Ok(Json(ApiResponse::success(response)))
        }
        Err(e) => {
            error!("Failed to create shell session for container {}: {}", id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Remove a container
pub async fn remove_container(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Removing container: {}", id);
    
    let manager = ContainerManager::new(state.docker.client());
    match manager.remove(&id).await {
        Ok(_) => {
            // Release any allocated ports for this container
            let mut net_mgr = state.network.lock().await;
            net_mgr.release_container_ports(&id);
            
            // Remove from daemon state
            let state_manager = state.state_manager.lock().await;
            if let Some((uuid, _)) = state_manager.find_by_container_id(&id) {
                let uuid = uuid.clone();
                drop(state_manager);
                if let Err(e) = state.state_manager.lock().await.remove_container(&uuid).await {
                    error!("Failed to remove container from daemon state: {}", e);
                }
            }
            
            // Remove from container tracker
            if let Ok(Some(tracker)) = state.container_tracker.find_by_container_id(&id).await {
                if let Err(e) = state.container_tracker.remove_container(&tracker.custom_uuid).await {
                    error!("Failed to remove container from tracker: {}", e);
                }
            }
            
            Ok(Json(ApiResponse::success(format!("Container {} removed", id))))
        }
        Err(e) => {
            error!("Failed to remove container {}: {}", id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}
/// Execute a command in a container
pub async fn exec_container(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<crate::models::ExecRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Executing command in container {}: {:?}", id, req.command);
    
    // Check if container is suspended
    match check_container_suspended(&state, &id).await {
        Ok(true) => {
            return Ok(Json(ApiResponse::error("Cannot execute commands in suspended container".to_string())));
        }
        Ok(false) => {}, // Not suspended, continue
        Err(e) => {
            return Ok(Json(ApiResponse::error(e)));
        }
    }
    
    let manager = ContainerManager::new(state.docker.client());
    let cmd_refs: Vec<&str> = req.command.iter().map(|s| s.as_str()).collect();
    
    match manager.exec_command(&id, cmd_refs).await {
        Ok(output) => Ok(Json(ApiResponse::success(output))),
        Err(e) => {
            error!("Failed to execute command in container {}: {}", id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Get container logs
pub async fn get_container_logs(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<crate::models::LogsRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Getting logs for container: {}", id);
    
    let manager = ContainerManager::new(state.docker.client());
    let follow = req.follow.unwrap_or(false);
    let tail = req.tail.as_deref();
    
    match manager.get_logs(&id, follow, tail).await {
        Ok(logs) => Ok(Json(ApiResponse::success(logs))),
        Err(e) => {
            error!("Failed to get logs for container {}: {}", id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Get container stats
pub async fn get_container_stats(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Getting stats for container: {}", id);
    
    let manager = ContainerManager::new(state.docker.client());
    match manager.get_stats(&id).await {
        Ok(stats) => Ok(Json(ApiResponse::success(stats))),
        Err(e) => {
            error!("Failed to get stats for container {}: {}", id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Inspect container
pub async fn inspect_container(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Inspecting container: {}", id);
    
    let manager = ContainerManager::new(state.docker.client());
    match manager.inspect(&id).await {
        Ok(info) => Ok(Json(ApiResponse::success(info))),
        Err(e) => {
            error!("Failed to inspect container {}: {}", id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Debug container issues
pub async fn debug_container(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Debugging container: {}", id);
    
    let manager = ContainerManager::new(state.docker.client());
    match manager.debug_container(&id).await {
        Ok(debug_info) => Ok(Json(ApiResponse::success(debug_info))),
        Err(e) => {
            error!("Failed to debug container {}: {}", id, e);
            Ok(Json(ApiResponse::error(e.to_string())))
        }
    }
}

/// Update a container with new update script
pub async fn update_container(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateContainerRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Updating container: {}", id);
    
    // Check if container is suspended
    match check_container_suspended(&state, &id).await {
        Ok(true) => {
            return Ok(Json(ApiResponse::error("Cannot update suspended container".to_string())));
        }
        Ok(false) => {}, // Not suspended, continue
        Err(e) => {
            return Ok(Json(ApiResponse::error(e)));
        }
    }
    
    // Check if container is locked (installing/updating)
    let state_manager = state.state_manager.lock().await;
    if let Some((uuid, container_state)) = state_manager.find_by_container_id(&id) {
        if container_state.locked.unwrap_or(false) {
            return Ok(Json(ApiResponse::error("Container is currently locked (installing/updating)".to_string())));
        }
        let uuid = uuid.clone();
        drop(state_manager);
        
        // Lock container for update
        if let Err(e) = state.state_manager.lock().await.lock_container(&uuid, "Running update script").await {
            error!("Failed to lock container during update: {}", e);
            return Ok(Json(ApiResponse::error("Failed to lock container for update".to_string())));
        }
        
        // Update state to updating
        if let Err(e) = state.state_manager.lock().await.update_container_state(&uuid, "updating").await {
            error!("Failed to update container state to updating: {}", e);
        }
        
        let manager = ContainerManager::new(state.docker.client());
        
        // Run the update script
        match manager.run_update(&id, &req.update_content).await {
            Ok(update_logs) => {
                info!("Update completed successfully for container: {}", id);
                
                // Update state to ready and unlock
                if let Err(e) = state.state_manager.lock().await.update_container_state(&uuid, "ready").await {
                    error!("Failed to update container state to ready: {}", e);
                }
                if let Err(e) = state.state_manager.lock().await.unlock_container(&uuid).await {
                    error!("Failed to unlock container after update: {}", e);
                }
                
                Ok(Json(ApiResponse::success(format!("Container {} updated successfully. Logs: {}", id, update_logs))))
            }
            Err(e) => {
                error!("Update failed for container {}: {}", id, e);
                
                // Update state to failed and unlock
                if let Err(state_err) = state.state_manager.lock().await.update_container_state(&uuid, "update_failed").await {
                    error!("Failed to update container state to update_failed: {}", state_err);
                }
                if let Err(state_err) = state.state_manager.lock().await.unlock_container(&uuid).await {
                    error!("Failed to unlock container after failed update: {}", state_err);
                }
                
                Ok(Json(ApiResponse::error(format!("Update failed: {}", e))))
            }
        }
    } else {
        Ok(Json(ApiResponse::error("Container not found in daemon state".to_string())))
    }
}

/// Update container resource limits
pub async fn update_container_limits(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateLimitsRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Updating limits for container: {}", id);
    
    // Check if container is suspended
    match check_container_suspended(&state, &id).await {
        Ok(true) => {
            return Ok(Json(ApiResponse::error("Cannot update limits of suspended container".to_string())));
        }
        Ok(false) => {}, // Not suspended, continue
        Err(e) => {
            return Ok(Json(ApiResponse::error(e)));
        }
    }
    
    // Check if container is locked (installing/updating)
    let state_manager = state.state_manager.lock().await;
    if let Some((uuid, container_state)) = state_manager.find_by_container_id(&id) {
        if container_state.locked.unwrap_or(false) {
            return Ok(Json(ApiResponse::error("Container is currently locked (installing/updating)".to_string())));
        }
        let uuid = uuid.clone();
        drop(state_manager);
        
        let restart = req.restart_container.unwrap_or(false);
        let manager = ContainerManager::new(state.docker.client());
        
        // Update limits in container
        match manager.update_limits(&id, &req.limits, restart).await {
            Ok(result) => {
                // Update daemon state with new limits
                if let Err(e) = state.state_manager.lock().await.update_container_limits(&uuid, &req.limits).await {
                    error!("Failed to save updated limits to daemon state: {}", e);
                }
                
                // Update container tracker with new limits
                if let Ok(Some(mut tracker)) = state.container_tracker.find_by_container_id(&id).await {
                    tracker.limits = req.limits.clone();
                    if let Err(e) = state.container_tracker.save_container(&tracker).await {
                        error!("Failed to update container tracker with new limits: {}", e);
                    }
                }
                
                Ok(Json(ApiResponse::success(result)))
            }
            Err(e) => {
                error!("Failed to update limits for container {}: {}", id, e);
                Ok(Json(ApiResponse::error(e.to_string())))
            }
        }
    } else {
        Ok(Json(ApiResponse::error("Container not found in daemon state".to_string())))
    }
}

/// Get container installation/update status
pub async fn get_container_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<ApiResponse<InstallationStatus>>, StatusCode> {
    info!("Getting status for container: {}", id);
    
    let state_manager = state.state_manager.lock().await;
    if let Some((_, container_state)) = state_manager.find_by_container_id(&id) {
        let status = InstallationStatus {
            status: container_state.state.clone(),
            progress: container_state.lock_reason.clone(),
            logs: None, // Could be enhanced to include recent logs
        };
        Ok(Json(ApiResponse::success(status)))
    } else {
        Ok(Json(ApiResponse::error("Container not found in daemon state".to_string())))
    }
}

/// Get container ID from UUID
pub async fn get_container_by_uuid(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
) -> Result<Json<ApiResponse<ContainerLookupResponse>>, StatusCode> {
    info!("Looking up container by UUID: {}", uuid);
    
    let state_manager = state.state_manager.lock().await;
    if let Some(container_state) = state_manager.get_container(&uuid) {
        if let Some(container_id) = &container_state.container_id {
            let response = ContainerLookupResponse {
                uuid: uuid.clone(),
                container_id: container_id.clone(),
                name: container_state.name.clone(),
                state: container_state.state.clone(),
                image: container_state.image.clone(),
            };
            Ok(Json(ApiResponse::success(response)))
        } else {
            Ok(Json(ApiResponse::error("Container ID not found for this UUID".to_string())))
        }
    } else {
        Ok(Json(ApiResponse::error("Container not found with this UUID".to_string())))
    }
}

/// Start a container by UUID
pub async fn start_container_by_uuid(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Starting container by UUID: {}", uuid);
    
    match resolve_container_id(&state, &uuid).await {
        Ok(container_id) => start_container(State(state), Path(container_id)).await,
        Err(e) => Ok(Json(ApiResponse::error(e))),
    }
}

/// Stop a container by UUID
pub async fn stop_container_by_uuid(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Stopping container by UUID: {}", uuid);
    
    match resolve_container_id(&state, &uuid).await {
        Ok(container_id) => stop_container(State(state), Path(container_id)).await,
        Err(e) => Ok(Json(ApiResponse::error(e))),
    }
}

/// Suspend a container by UUID
pub async fn suspend_container_by_uuid(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
    Json(req): Json<SuspendRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Suspending container by UUID: {}", uuid);
    
    match resolve_container_id(&state, &uuid).await {
        Ok(container_id) => suspend_container(State(state), Path(container_id), Json(req)).await,
        Err(e) => Ok(Json(ApiResponse::error(e))),
    }
}

/// Unsuspend a container by UUID
pub async fn unsuspend_container_by_uuid(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
    Json(req): Json<UnsuspendRequest>,
) -> Result<Json<ApiResponse<String>>, StatusCode> {
    info!("Unsuspending container by UUID: {}", uuid);
    
    match resolve_container_id(&state, &uuid).await {
        Ok(container_id) => unsuspend_container(State(state), Path(container_id), Json(req)).await,
        Err(e) => Ok(Json(ApiResponse::error(e))),
    }
}
