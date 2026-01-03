//! Power Actions Service - Handles container power operations in background tasks
//! 
//! This service ensures that container start/stop/kill/restart operations don't
//! block the main HTTP server, even when containers are CPU-intensive or unresponsive.
//! 
//! Now uses lock-free StateManager for zero blocking on state updates.

use std::sync::Arc;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::{RwLock, oneshot};
use tokio::time::timeout;
use tracing::{info, warn, error};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use dashmap::DashMap;

use crate::docker::ContainerManager;
use crate::state_manager::StateManager;
use crate::container_tracker::ContainerTrackingManager;

/// Power action types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PowerAction {
    Start,
    Stop,
    Kill,
    Restart,
    Suspend,
}

impl std::fmt::Display for PowerAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PowerAction::Start => write!(f, "start"),
            PowerAction::Stop => write!(f, "stop"),
            PowerAction::Kill => write!(f, "kill"),
            PowerAction::Restart => write!(f, "restart"),
            PowerAction::Suspend => write!(f, "suspend"),
        }
    }
}

/// Status of a power action
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionStatus {
    Pending,
    Running,
    Completed,
    Failed,
    TimedOut,
}

/// Result of a power action
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerActionResult {
    pub action_id: String,
    pub container_id: String,
    pub container_uuid: String,
    pub action: PowerAction,
    pub status: ActionStatus,
    pub message: Option<String>,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// In-flight action tracking
struct InFlightAction {
    result: PowerActionResult,
    #[allow(dead_code)]
    cancel_tx: Option<oneshot::Sender<()>>,
}

/// Configuration for power action timeouts
#[derive(Debug, Clone)]
pub struct PowerActionConfig {
    pub stop_timeout: Duration,
    pub kill_timeout: Duration,
    pub start_timeout: Duration,
    pub restart_timeout: Duration,
    pub history_retention: Duration,
}

impl Default for PowerActionConfig {
    fn default() -> Self {
        Self {
            stop_timeout: Duration::from_secs(10),
            kill_timeout: Duration::from_secs(5),
            start_timeout: Duration::from_secs(30),
            restart_timeout: Duration::from_secs(45),
            history_retention: Duration::from_secs(300),
        }
    }
}

/// Service for handling power actions in background tasks
/// Uses lock-free data structures for maximum concurrency
pub struct PowerActionService {
    docker: Arc<bollard::Docker>,
    state_manager: Arc<StateManager>,
    container_tracker: Arc<ContainerTrackingManager>,
    config: PowerActionConfig,
    /// Lock-free action tracking
    actions: Arc<DashMap<String, InFlightAction>>,
    /// Lock-free container locks
    container_locks: Arc<DashMap<String, String>>,
}

impl PowerActionService {
    pub fn new(
        docker: Arc<bollard::Docker>,
        state_manager: Arc<StateManager>,
        container_tracker: Arc<ContainerTrackingManager>,
    ) -> Self {
        Self::with_config(docker, state_manager, container_tracker, PowerActionConfig::default())
    }

    pub fn with_config(
        docker: Arc<bollard::Docker>,
        state_manager: Arc<StateManager>,
        container_tracker: Arc<ContainerTrackingManager>,
        config: PowerActionConfig,
    ) -> Self {
        let service = Self {
            docker,
            state_manager,
            container_tracker,
            config,
            actions: Arc::new(DashMap::new()),
            container_locks: Arc::new(DashMap::new()),
        };
        
        service.start_cleanup_task();
        service
    }

    fn start_cleanup_task(&self) {
        let actions = self.actions.clone();
        let retention = self.config.history_retention;
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                
                let now = Utc::now();
                actions.retain(|_, action| {
                    match action.result.status {
                        ActionStatus::Completed | ActionStatus::Failed | ActionStatus::TimedOut => {
                            if let Some(completed_at) = action.result.completed_at {
                                let age = now.signed_duration_since(completed_at);
                                age.num_seconds() < retention.as_secs() as i64
                            } else {
                                true
                            }
                        }
                        _ => true,
                    }
                });
            }
        });
    }

    fn generate_action_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    /// Check if a container has an action in progress (lock-free)
    pub fn has_pending_action(&self, container_id: &str) -> Option<String> {
        self.container_locks.get(container_id).map(|r| r.value().clone())
    }

    /// Get the status of an action (lock-free)
    pub fn get_action_status(&self, action_id: &str) -> Option<PowerActionResult> {
        self.actions.get(action_id).map(|a| a.result.clone())
    }

    /// Execute a power action in the background (non-blocking)
    pub async fn execute_action(
        &self,
        container_id: String,
        container_uuid: String,
        action: PowerAction,
        reason: Option<String>,
    ) -> Result<PowerActionResult, String> {
        // Check for existing action (lock-free)
        if let Some(existing) = self.container_locks.get(&container_id) {
            return Err(format!(
                "Container {} already has action in progress: {}",
                container_id, existing.value()
            ));
        }

        let action_id = Self::generate_action_id();
        let now = Utc::now();

        let result = PowerActionResult {
            action_id: action_id.clone(),
            container_id: container_id.clone(),
            container_uuid: container_uuid.clone(),
            action,
            status: ActionStatus::Pending,
            message: None,
            started_at: now,
            completed_at: None,
        };

        let (cancel_tx, cancel_rx) = oneshot::channel();

        // Register action (lock-free)
        self.actions.insert(action_id.clone(), InFlightAction {
            result: result.clone(),
            cancel_tx: Some(cancel_tx),
        });

        // Lock container (lock-free)
        self.container_locks.insert(container_id.clone(), action_id.clone());

        // Spawn background task
        let docker = self.docker.clone();
        let state_manager = self.state_manager.clone();
        let container_tracker = self.container_tracker.clone();
        let actions = self.actions.clone();
        let container_locks = self.container_locks.clone();
        let config = self.config.clone();
        let action_id_clone = action_id.clone();
        let container_id_clone = container_id.clone();
        let container_uuid_clone = container_uuid.clone();

        tokio::spawn(async move {
            // Update status to running
            if let Some(mut action) = actions.get_mut(&action_id_clone) {
                action.result.status = ActionStatus::Running;
            }

            info!("Executing power action {} on container {} ({})", 
                  action, container_id_clone, container_uuid_clone);

            let execution_result = Self::execute_action_internal(
                &docker,
                &container_id_clone,
                action,
                &config,
                cancel_rx,
                reason,
            ).await;

            let (final_status, message) = match execution_result {
                Ok(msg) => {
                    info!("Power action {} completed for container {}: {}", 
                          action, container_id_clone, msg);
                    (ActionStatus::Completed, Some(msg))
                }
                Err(e) => {
                    if e.contains("timed out") {
                        warn!("Power action {} timed out for container {}: {}", 
                              action, container_id_clone, e);
                        (ActionStatus::TimedOut, Some(e))
                    } else {
                        error!("Power action {} failed for container {}: {}", 
                               action, container_id_clone, e);
                        (ActionStatus::Failed, Some(e))
                    }
                }
            };

            // Update state (lock-free operations)
            let new_state = match (action, &final_status) {
                (PowerAction::Start, ActionStatus::Completed) => Some("running"),
                (PowerAction::Stop, ActionStatus::Completed) => Some("stopped"),
                (PowerAction::Stop, ActionStatus::TimedOut) => Some("stopped"),
                (PowerAction::Kill, _) => Some("killed"),
                (PowerAction::Restart, ActionStatus::Completed) => Some("running"),
                (PowerAction::Suspend, ActionStatus::Completed) => Some("suspended"),
                _ => None,
            };

            if let Some(state) = new_state {
                // These are lock-free operations
                let _ = state_manager.update_container_state(&container_uuid_clone, state).await;
                let _ = container_tracker.update_container_status(&container_uuid_clone, state).await;
            }

            // Update action result (lock-free)
            if let Some(mut action_entry) = actions.get_mut(&action_id_clone) {
                action_entry.result.status = final_status;
                action_entry.result.message = message;
                action_entry.result.completed_at = Some(Utc::now());
            }

            // Release container lock (lock-free)
            container_locks.remove(&container_id_clone);
        });

        Ok(result)
    }

    async fn execute_action_internal(
        docker: &bollard::Docker,
        container_id: &str,
        action: PowerAction,
        config: &PowerActionConfig,
        _cancel_rx: oneshot::Receiver<()>,
        reason: Option<String>,
    ) -> Result<String, String> {
        let manager = ContainerManager::new(docker.clone());

        match action {
            PowerAction::Start => {
                match timeout(config.start_timeout, manager.start(container_id)).await {
                    Ok(Ok(_)) => Ok("Container started successfully".to_string()),
                    Ok(Err(e)) => Err(format!("Failed to start: {}", e)),
                    Err(_) => Err("Start operation timed out".to_string()),
                }
            }
            
            PowerAction::Stop => {
                info!("Attempting graceful stop for container {}", container_id);
                match timeout(config.stop_timeout, manager.stop(container_id)).await {
                    Ok(Ok(_)) => Ok("Container stopped gracefully".to_string()),
                    Ok(Err(e)) => {
                        warn!("Graceful stop failed for {}, attempting force kill: {}", container_id, e);
                        Self::force_kill(docker, container_id, config).await
                    }
                    Err(_) => {
                        warn!("Graceful stop timed out for {}, force killing", container_id);
                        Self::force_kill(docker, container_id, config).await
                    }
                }
            }

            PowerAction::Kill => {
                match timeout(config.kill_timeout, manager.kill(container_id)).await {
                    Ok(Ok(_)) => Ok("Container killed".to_string()),
                    Ok(Err(e)) => Err(format!("Failed to kill: {}", e)),
                    Err(_) => Err("Kill operation timed out".to_string()),
                }
            }

            PowerAction::Restart => {
                let stop_result = timeout(config.stop_timeout, manager.stop(container_id)).await;
                
                match stop_result {
                    Ok(Ok(_)) | Err(_) => {
                        if stop_result.is_err() {
                            let _ = Self::force_kill(docker, container_id, config).await;
                        }
                        
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        
                        match timeout(config.start_timeout, manager.start(container_id)).await {
                            Ok(Ok(_)) => Ok("Container restarted successfully".to_string()),
                            Ok(Err(e)) => Err(format!("Failed to start after stop: {}", e)),
                            Err(_) => Err("Start operation timed out during restart".to_string()),
                        }
                    }
                    Ok(Err(_e)) => {
                        let _ = Self::force_kill(docker, container_id, config).await;
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        
                        match timeout(config.start_timeout, manager.start(container_id)).await {
                            Ok(Ok(_)) => Ok("Container restarted (after force kill)".to_string()),
                            Ok(Err(e)) => Err(format!("Failed to restart: {}", e)),
                            Err(_) => Err("Restart timed out".to_string()),
                        }
                    }
                }
            }

            PowerAction::Suspend => {
                let reason_msg = reason.unwrap_or_else(|| "Container suspended".to_string());
                match timeout(config.kill_timeout, manager.suspend(container_id, Some(reason_msg.clone()))).await {
                    Ok(Ok(_)) => Ok(format!("Container suspended: {}", reason_msg)),
                    Ok(Err(e)) => {
                        warn!("Suspend failed for {}, force killing: {}", container_id, e);
                        Self::force_kill(docker, container_id, config).await
                            .map(|_| format!("Container force-killed (suspend failed): {}", reason_msg))
                    }
                    Err(_) => {
                        warn!("Suspend timed out for {}, force killing", container_id);
                        Self::force_kill(docker, container_id, config).await
                            .map(|_| format!("Container force-killed (suspend timed out): {}", reason_msg))
                    }
                }
            }
        }
    }

    async fn force_kill(
        docker: &bollard::Docker,
        container_id: &str,
        config: &PowerActionConfig,
    ) -> Result<String, String> {
        use bollard::container::KillContainerOptions;

        info!("Force killing container {} with SIGKILL", container_id);

        let kill_future = docker.kill_container(
            container_id,
            Some(KillContainerOptions { signal: "SIGKILL" }),
        );

        match timeout(config.kill_timeout, kill_future).await {
            Ok(Ok(_)) => Ok("Container force killed with SIGKILL".to_string()),
            Ok(Err(e)) => {
                let error_str = e.to_string();
                if error_str.contains("is not running") || error_str.contains("No such container") {
                    Ok("Container already stopped".to_string())
                } else {
                    Err(format!("Force kill failed: {}", e))
                }
            }
            Err(_) => Err("Force kill timed out - container may be unresponsive".to_string()),
        }
    }
}
