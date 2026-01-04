//! Async Power Actions - Truly non-blocking container power operations
//!
//! All operations are fire-and-forget. Status updates are sent via the event hub.
//! No locks are held during Docker operations.

use std::sync::Arc;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{info, warn, error, debug};
use bollard::Docker;
use bollard::container::{StopContainerOptions, KillContainerOptions};

use super::container_events::{ContainerEventHub, ContainerEvent};

/// Power action types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    Start,
    Stop,
    Kill,
    Restart,
}

impl std::fmt::Display for PowerAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PowerAction::Start => write!(f, "start"),
            PowerAction::Stop => write!(f, "stop"),
            PowerAction::Kill => write!(f, "kill"),
            PowerAction::Restart => write!(f, "restart"),
        }
    }
}

/// Tracks in-flight power actions to prevent duplicate concurrent operations
struct ActionTracker {
    /// container_id -> current action
    actions: HashMap<String, PowerAction>,
}

impl ActionTracker {
    fn new() -> Self {
        Self { actions: HashMap::new() }
    }

    fn try_start(&mut self, container_id: &str, action: PowerAction) -> bool {
        if self.actions.contains_key(container_id) {
            false
        } else {
            self.actions.insert(container_id.to_string(), action);
            true
        }
    }

    fn finish(&mut self, container_id: &str) {
        self.actions.remove(container_id);
    }

    fn current_action(&self, container_id: &str) -> Option<PowerAction> {
        self.actions.get(container_id).copied()
    }
}

/// Async power action manager
pub struct AsyncPowerManager {
    docker: Arc<Docker>,
    event_hub: Arc<ContainerEventHub>,
    tracker: Arc<RwLock<ActionTracker>>,
}

impl AsyncPowerManager {
    pub fn new(docker: Arc<Docker>, event_hub: Arc<ContainerEventHub>) -> Self {
        Self {
            docker,
            event_hub,
            tracker: Arc::new(RwLock::new(ActionTracker::new())),
        }
    }

    /// Check if container has an action in progress
    pub async fn has_pending_action(&self, container_id: &str) -> Option<String> {
        self.tracker.read().await.current_action(container_id).map(|a| a.to_string())
    }

    /// Start a container - fire and forget
    /// Returns immediately, sends events via hub
    pub async fn start(&self, container_id: String, uuid: String) -> Result<(), String> {
        // Try to acquire action lock
        {
            let mut tracker = self.tracker.write().await;
            if !tracker.try_start(&container_id, PowerAction::Start) {
                return Err(format!("Container {} already has an action in progress", container_id));
            }
        }

        let docker = self.docker.clone();
        let hub = self.event_hub.clone();
        let tracker = self.tracker.clone();
        let cid = container_id.clone();

        // Spawn and forget - this task runs independently
        tokio::spawn(async move {
            hub.broadcast(&cid, ContainerEvent::PowerActionStarted {
                action: "start".to_string(),
            }).await;

            let result = Self::do_start(&docker, &cid).await;
            
            hub.broadcast(&cid, ContainerEvent::PowerActionCompleted {
                action: "start".to_string(),
                success: result.is_ok(),
                message: result.as_ref().map(|_| "started".to_string())
                    .unwrap_or_else(|e| e.clone()),
            }).await;

            if result.is_ok() {
                hub.broadcast_state(&cid, "running").await;
            }

            // Release action lock
            tracker.write().await.finish(&cid);

            match result {
                Ok(_) => info!("Container {} started successfully", cid),
                Err(e) => error!("Failed to start container {}: {}", cid, e),
            }
        });

        Ok(())
    }

    /// Stop a container - fire and forget
    pub async fn stop(&self, container_id: String, uuid: String) -> Result<(), String> {
        {
            let mut tracker = self.tracker.write().await;
            if !tracker.try_start(&container_id, PowerAction::Stop) {
                return Err(format!("Container {} already has an action in progress", container_id));
            }
        }

        let docker = self.docker.clone();
        let hub = self.event_hub.clone();
        let tracker = self.tracker.clone();
        let cid = container_id.clone();

        tokio::spawn(async move {
            hub.broadcast(&cid, ContainerEvent::PowerActionStarted {
                action: "stop".to_string(),
            }).await;

            let result = Self::do_stop(&docker, &cid).await;
            
            hub.broadcast(&cid, ContainerEvent::PowerActionCompleted {
                action: "stop".to_string(),
                success: result.is_ok(),
                message: result.as_ref().map(|_| "stopped".to_string())
                    .unwrap_or_else(|e| e.clone()),
            }).await;

            hub.broadcast_state(&cid, "stopped").await;
            tracker.write().await.finish(&cid);

            match result {
                Ok(_) => info!("Container {} stopped successfully", cid),
                Err(e) => warn!("Stop container {} returned error (may be normal): {}", cid, e),
            }
        });

        Ok(())
    }

    /// Kill a container - fire and forget
    pub async fn kill(&self, container_id: String, uuid: String) -> Result<(), String> {
        {
            let mut tracker = self.tracker.write().await;
            if !tracker.try_start(&container_id, PowerAction::Kill) {
                return Err(format!("Container {} already has an action in progress", container_id));
            }
        }

        let docker = self.docker.clone();
        let hub = self.event_hub.clone();
        let tracker = self.tracker.clone();
        let cid = container_id.clone();

        tokio::spawn(async move {
            hub.broadcast(&cid, ContainerEvent::PowerActionStarted {
                action: "kill".to_string(),
            }).await;

            let result = Self::do_kill(&docker, &cid).await;
            
            hub.broadcast(&cid, ContainerEvent::PowerActionCompleted {
                action: "kill".to_string(),
                success: result.is_ok(),
                message: result.as_ref().map(|_| "killed".to_string())
                    .unwrap_or_else(|e| e.clone()),
            }).await;

            hub.broadcast_state(&cid, "stopped").await;
            tracker.write().await.finish(&cid);

            match result {
                Ok(_) => info!("Container {} killed successfully", cid),
                Err(e) => warn!("Kill container {} returned error: {}", cid, e),
            }
        });

        Ok(())
    }

    /// Restart a container - fire and forget
    pub async fn restart(&self, container_id: String, uuid: String) -> Result<(), String> {
        {
            let mut tracker = self.tracker.write().await;
            if !tracker.try_start(&container_id, PowerAction::Restart) {
                return Err(format!("Container {} already has an action in progress", container_id));
            }
        }

        let docker = self.docker.clone();
        let hub = self.event_hub.clone();
        let tracker = self.tracker.clone();
        let cid = container_id.clone();

        tokio::spawn(async move {
            hub.broadcast(&cid, ContainerEvent::PowerActionStarted {
                action: "restart".to_string(),
            }).await;

            hub.broadcast_message(&cid, &format!("{}@pkg.lat: Stopping container...", &cid)).await;
            
            // Stop first (ignore errors - container might not be running)
            let _ = Self::do_stop(&docker, &cid).await;
            
            // Small delay to ensure container is fully stopped
            tokio::time::sleep(Duration::from_millis(500)).await;
            
            hub.broadcast_message(&cid, &format!("{}@pkg.lat: Starting container...", &cid)).await;
            
            // Start
            let result = Self::do_start(&docker, &cid).await;
            
            hub.broadcast(&cid, ContainerEvent::PowerActionCompleted {
                action: "restart".to_string(),
                success: result.is_ok(),
                message: result.as_ref().map(|_| "restarted".to_string())
                    .unwrap_or_else(|e| e.clone()),
            }).await;

            if result.is_ok() {
                hub.broadcast_state(&cid, "running").await;
            } else {
                hub.broadcast_state(&cid, "stopped").await;
            }

            tracker.write().await.finish(&cid);

            match result {
                Ok(_) => info!("Container {} restarted successfully", cid),
                Err(e) => error!("Failed to restart container {}: {}", cid, e),
            }
        });

        Ok(())
    }

    // Internal Docker operations with timeouts

    async fn do_start(docker: &Docker, container_id: &str) -> Result<(), String> {
        match tokio::time::timeout(
            Duration::from_secs(30),
            docker.start_container::<String>(container_id, None)
        ).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => {
                let err_str = e.to_string();
                if err_str.contains("already started") || err_str.contains("is already running") {
                    Ok(()) // Already running is fine
                } else {
                    Err(err_str)
                }
            }
            Err(_) => Err("Timeout starting container".to_string()),
        }
    }

    async fn do_stop(docker: &Docker, container_id: &str) -> Result<(), String> {
        let opts = StopContainerOptions { t: 10 };
        
        match tokio::time::timeout(
            Duration::from_secs(15),
            docker.stop_container(container_id, Some(opts))
        ).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => {
                let err_str = e.to_string();
                if err_str.contains("is not running") || err_str.contains("No such container") {
                    Ok(()) // Already stopped is fine
                } else {
                    Err(err_str)
                }
            }
            Err(_) => {
                // Timeout - try to kill
                warn!("Stop timed out for {}, attempting kill", container_id);
                Self::do_kill(docker, container_id).await
            }
        }
    }

    async fn do_kill(docker: &Docker, container_id: &str) -> Result<(), String> {
        let opts = KillContainerOptions { signal: "SIGKILL" };
        
        match tokio::time::timeout(
            Duration::from_secs(5),
            docker.kill_container(container_id, Some(opts))
        ).await {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => {
                let err_str = e.to_string();
                if err_str.contains("is not running") || err_str.contains("No such container") {
                    Ok(())
                } else {
                    Err(err_str)
                }
            }
            Err(_) => Err("Timeout killing container".to_string()),
        }
    }
}
