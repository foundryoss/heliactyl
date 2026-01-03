//! Power Executor - Completely isolated power action execution
//! 
//! Uses a dedicated thread with its own tokio runtime to execute power actions.
//! Main server sends commands via channel and never blocks.

use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot, RwLock};
use tracing::{info, warn, error};
use bollard::Docker;
use bollard::container::{StopContainerOptions, KillContainerOptions, StartContainerOptions};

/// Power action command
#[derive(Debug)]
pub enum PowerCommand {
    Start { container_id: String, uuid: String },
    Stop { container_id: String, uuid: String },
    Kill { container_id: String, uuid: String },
    Restart { container_id: String, uuid: String },
    Suspend { container_id: String, uuid: String, reason: String },
}

/// Result of a power action
#[derive(Debug, Clone)]
pub struct PowerResult {
    pub success: bool,
    pub container_id: String,
    pub uuid: String,
    pub action: String,
    pub message: String,
}

/// Callback for state updates after power action completes
pub type StateCallback = Arc<dyn Fn(String, String, String) + Send + Sync>;

/// Power Executor - runs in its own thread with own runtime
pub struct PowerExecutor {
    sender: mpsc::UnboundedSender<(PowerCommand, String)>,
    /// Track pending actions per container
    pending_actions: Arc<RwLock<HashMap<String, String>>>,
}

impl PowerExecutor {
    /// Create a new PowerExecutor with its own dedicated thread
    pub fn new(docker_socket: String, state_callback: StateCallback) -> Self {
        let (tx, rx) = mpsc::unbounded_channel::<(PowerCommand, String)>();
        let pending_actions = Arc::new(RwLock::new(HashMap::new()));
        let pending_clone = pending_actions.clone();
        
        // Spawn a dedicated OS thread with its own tokio runtime
        std::thread::Builder::new()
            .name("power-executor".to_string())
            .spawn(move || {
                // Create a new single-threaded runtime for this thread
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create power executor runtime");
                
                rt.block_on(async move {
                    Self::run_executor(rx, docker_socket, state_callback, pending_clone).await;
                });
            })
            .expect("Failed to spawn power executor thread");
        
        Self {
            sender: tx,
            pending_actions,
        }
    }
    
    /// Run the executor loop
    async fn run_executor(
        mut rx: mpsc::UnboundedReceiver<(PowerCommand, String)>,
        docker_socket: String,
        state_callback: StateCallback,
        pending_actions: Arc<RwLock<HashMap<String, String>>>,
    ) {
        info!("Power executor started on dedicated thread");
        
        // Create Docker client for this thread
        let docker = match Docker::connect_with_socket(&docker_socket, 120, bollard::API_DEFAULT_VERSION) {
            Ok(d) => d,
            Err(e) => {
                error!("Power executor failed to connect to Docker: {}", e);
                return;
            }
        };
        
        while let Some((cmd, action_id)) = rx.recv().await {
            let docker = docker.clone();
            let callback = state_callback.clone();
            let pending = pending_actions.clone();
            
            // Spawn each action as a separate task within this runtime
            tokio::spawn(async move {
                let result = Self::execute_command(&docker, cmd).await;
                
                // Update state via callback
                callback(result.uuid.clone(), result.action.clone(), 
                    if result.success { "completed".to_string() } else { "failed".to_string() });
                
                // Remove from pending
                pending.write().await.remove(&result.container_id);
                
                if result.success {
                    info!("Power action completed: {} {} - {}", result.action, result.container_id, result.message);
                } else {
                    error!("Power action failed: {} {} - {}", result.action, result.container_id, result.message);
                }
            });
        }
    }
    
    /// Execute a single power command
    async fn execute_command(docker: &Docker, cmd: PowerCommand) -> PowerResult {
        match cmd {
            PowerCommand::Start { container_id, uuid } => {
                Self::do_start(docker, container_id, uuid).await
            }
            PowerCommand::Stop { container_id, uuid } => {
                Self::do_stop(docker, container_id, uuid).await
            }
            PowerCommand::Kill { container_id, uuid } => {
                Self::do_kill(docker, container_id, uuid).await
            }
            PowerCommand::Restart { container_id, uuid } => {
                Self::do_restart(docker, container_id, uuid).await
            }
            PowerCommand::Suspend { container_id, uuid, reason } => {
                Self::do_suspend(docker, container_id, uuid, reason).await
            }
        }
    }
    
    async fn do_start(docker: &Docker, container_id: String, uuid: String) -> PowerResult {
        info!("Executor: Starting container {}", container_id);
        
        match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            docker.start_container::<String>(&container_id, None)
        ).await {
            Ok(Ok(_)) => {
                // Wait a moment and verify container is actually running
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                
                // Check if container is still running
                match docker.inspect_container(&container_id, None).await {
                    Ok(info) => {
                        if let Some(state) = info.state {
                            if state.running == Some(true) {
                                PowerResult {
                                    success: true,
                                    container_id,
                                    uuid,
                                    action: "start".to_string(),
                                    message: "Container started".to_string(),
                                }
                            } else {
                                // Container started but immediately crashed
                                let exit_code = state.exit_code.unwrap_or(0);
                                let status = if state.oom_killed == Some(true) {
                                    "oom_killed"
                                } else if exit_code == 137 {
                                    "oom_killed"
                                } else {
                                    "exited"
                                };
                                
                                warn!("Container {} started but immediately exited with code {}", container_id, exit_code);
                                
                                PowerResult {
                                    success: false,
                                    container_id,
                                    uuid,
                                    action: "start".to_string(),
                                    message: format!("Container started but {} immediately (exit {})", status, exit_code),
                                }
                            }
                        } else {
                            PowerResult {
                                success: true,
                                container_id,
                                uuid,
                                action: "start".to_string(),
                                message: "Container started (state unknown)".to_string(),
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Container {} started but couldn't verify state: {}", container_id, e);
                        PowerResult {
                            success: true,
                            container_id,
                            uuid,
                            action: "start".to_string(),
                            message: "Container started (couldn't verify)".to_string(),
                        }
                    }
                }
            },
            Ok(Err(e)) => {
                let err_str = e.to_string();
                // Already running is not an error
                if err_str.contains("already started") || err_str.contains("is already running") {
                    PowerResult {
                        success: true,
                        container_id,
                        uuid,
                        action: "start".to_string(),
                        message: "Container already running".to_string(),
                    }
                } else {
                    PowerResult {
                        success: false,
                        container_id,
                        uuid,
                        action: "start".to_string(),
                        message: format!("Failed: {}", e),
                    }
                }
            }
            Err(_) => PowerResult {
                success: false,
                container_id,
                uuid,
                action: "start".to_string(),
                message: "Timeout".to_string(),
            },
        }
    }
    
    async fn do_stop(docker: &Docker, container_id: String, uuid: String) -> PowerResult {
        info!("Executor: Stopping container {}", container_id);
        
        // Try graceful stop first (5 second timeout to Docker)
        let stop_opts = StopContainerOptions { t: 5 };
        
        match tokio::time::timeout(
            std::time::Duration::from_secs(10),
            docker.stop_container(&container_id, Some(stop_opts))
        ).await {
            Ok(Ok(_)) => {
                return PowerResult {
                    success: true,
                    container_id,
                    uuid,
                    action: "stop".to_string(),
                    message: "Container stopped gracefully".to_string(),
                };
            }
            Ok(Err(e)) => {
                let err_str = e.to_string();
                if err_str.contains("is not running") || err_str.contains("No such container") {
                    return PowerResult {
                        success: true,
                        container_id,
                        uuid,
                        action: "stop".to_string(),
                        message: "Container already stopped".to_string(),
                    };
                }
                warn!("Graceful stop failed for {}, trying force kill: {}", container_id, e);
            }
            Err(_) => {
                warn!("Stop timed out for {}, trying force kill", container_id);
            }
        }
        
        // Force kill
        Self::do_force_kill(docker, &container_id, &uuid).await
    }
    
    async fn do_kill(docker: &Docker, container_id: String, uuid: String) -> PowerResult {
        info!("Executor: Killing container {}", container_id);
        Self::do_force_kill(docker, &container_id, &uuid).await
    }
    
    async fn do_force_kill(docker: &Docker, container_id: &str, uuid: &str) -> PowerResult {
        let kill_opts = KillContainerOptions { signal: "SIGKILL" };
        
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            docker.kill_container(container_id, Some(kill_opts))
        ).await {
            Ok(Ok(_)) => PowerResult {
                success: true,
                container_id: container_id.to_string(),
                uuid: uuid.to_string(),
                action: "kill".to_string(),
                message: "Container killed".to_string(),
            },
            Ok(Err(e)) => {
                let err_str = e.to_string();
                if err_str.contains("is not running") || err_str.contains("No such container") {
                    PowerResult {
                        success: true,
                        container_id: container_id.to_string(),
                        uuid: uuid.to_string(),
                        action: "kill".to_string(),
                        message: "Container already dead".to_string(),
                    }
                } else {
                    PowerResult {
                        success: false,
                        container_id: container_id.to_string(),
                        uuid: uuid.to_string(),
                        action: "kill".to_string(),
                        message: format!("Kill failed: {}", e),
                    }
                }
            }
            Err(_) => PowerResult {
                success: false,
                container_id: container_id.to_string(),
                uuid: uuid.to_string(),
                action: "kill".to_string(),
                message: "Kill timed out".to_string(),
            },
        }
    }
    
    async fn do_restart(docker: &Docker, container_id: String, uuid: String) -> PowerResult {
        info!("Executor: Restarting container {}", container_id);
        
        // Stop first
        let stop_result = Self::do_stop(docker, container_id.clone(), uuid.clone()).await;
        if !stop_result.success && !stop_result.message.contains("already") {
            return PowerResult {
                success: false,
                container_id,
                uuid,
                action: "restart".to_string(),
                message: format!("Stop phase failed: {}", stop_result.message),
            };
        }
        
        // Brief delay
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        
        // Start
        let start_result = Self::do_start(docker, container_id.clone(), uuid.clone()).await;
        PowerResult {
            success: start_result.success,
            container_id,
            uuid,
            action: "restart".to_string(),
            message: if start_result.success { 
                "Container restarted".to_string() 
            } else { 
                format!("Start phase failed: {}", start_result.message) 
            },
        }
    }
    
    async fn do_suspend(docker: &Docker, container_id: String, uuid: String, reason: String) -> PowerResult {
        info!("Executor: Suspending container {} - {}", container_id, reason);
        
        // Just kill it - suspension state is handled by main server
        let result = Self::do_force_kill(docker, &container_id, &uuid).await;
        PowerResult {
            success: result.success,
            container_id,
            uuid,
            action: "suspend".to_string(),
            message: if result.success {
                format!("Container suspended: {}", reason)
            } else {
                result.message
            },
        }
    }
    
    /// Send a power command (non-blocking, returns immediately)
    pub fn send(&self, cmd: PowerCommand) -> Result<String, String> {
        let action_id = uuid::Uuid::new_v4().to_string();
        
        // Check if container already has pending action
        let container_id = match &cmd {
            PowerCommand::Start { container_id, .. } => container_id,
            PowerCommand::Stop { container_id, .. } => container_id,
            PowerCommand::Kill { container_id, .. } => container_id,
            PowerCommand::Restart { container_id, .. } => container_id,
            PowerCommand::Suspend { container_id, .. } => container_id,
        };
        
        // Note: We skip the pending check here since we're fire-and-forget
        // Multiple commands to same container are ok - Docker handles it
        
        self.sender.send((cmd, action_id.clone()))
            .map_err(|e| format!("Failed to send command: {}", e))?;
        
        Ok(action_id)
    }
    
    /// Check if container has pending action
    pub async fn has_pending(&self, container_id: &str) -> bool {
        self.pending_actions.read().await.contains_key(container_id)
    }
}
