use crate::models::ContainerTracker;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::fs;
use tracing::{info, warn};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerState {
    pub state: String,
    pub container_id: Option<String>,
    pub image: String,
    pub name: String,
    pub description: Option<String>,
    pub startup_command: Option<String>,
    pub disk_limit: Option<String>,
    pub memory_limit: Option<String>,
    pub cpu_limit: Option<String>,
    pub swap_limit: Option<String>,
    pub pids_limit: Option<u64>,
    pub threads_limit: Option<u64>,
    pub attached_volumes: Vec<String>,
    pub ports: HashMap<String, String>,
    pub env: Option<HashMap<String, String>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub locked: Option<bool>,
    pub lock_reason: Option<String>,
    pub locked_at: Option<i64>,
    pub restart_policy: Option<String>,
    pub install_content: Option<String>,
    pub update_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonState {
    pub containers: HashMap<String, ContainerState>,
    pub last_updated: DateTime<Utc>,
    pub daemon_version: String,
}

pub struct StateManager {
    state_file_path: String,
    pub state: DaemonState,
}

impl StateManager {
    pub fn new(storage_path: &str) -> Self {
        let state_file_path = format!("{}/states.json", storage_path);
        let state = DaemonState {
            containers: HashMap::new(),
            last_updated: Utc::now(),
            daemon_version: "0.1.0".to_string(),
        };

        Self {
            state_file_path,
            state,
        }
    }

    pub async fn init(&mut self) -> anyhow::Result<()> {
        // Create storage directory if it doesn't exist
        if let Some(parent) = std::path::Path::new(&self.state_file_path).parent() {
            fs::create_dir_all(parent).await?;
        }

        // Load existing state if file exists
        if fs::metadata(&self.state_file_path).await.is_ok() {
            match self.load_state().await {
                Ok(_) => info!("Loaded existing daemon state from: {}", self.state_file_path),
                Err(e) => {
                    warn!("Failed to load existing state, starting fresh: {}", e);
                    self.save_state().await?;
                }
            }
        } else {
            // Create new state file
            self.save_state().await?;
            info!("Created new daemon state file: {}", self.state_file_path);
        }

        Ok(())
    }

    async fn load_state(&mut self) -> anyhow::Result<()> {
        let content = fs::read_to_string(&self.state_file_path).await?;
        self.state = serde_json::from_str(&content)?;
        Ok(())
    }

    pub async fn save_state(&self) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(&self.state)?;
        fs::write(&self.state_file_path, content).await?;
        Ok(())
    }

    pub async fn add_container(&mut self, uuid: &str, container_state: ContainerState) -> anyhow::Result<()> {
        self.state.containers.insert(uuid.to_string(), container_state);
        self.state.last_updated = Utc::now();
        self.save_state().await?;
        info!("Added container {} to daemon state", uuid);
        Ok(())
    }

    pub async fn update_container_state(&mut self, uuid: &str, new_state: &str) -> anyhow::Result<()> {
        if let Some(container) = self.state.containers.get_mut(uuid) {
            container.state = new_state.to_string();
            container.updated_at = Utc::now();
            self.state.last_updated = Utc::now();
            self.save_state().await?;
            info!("Updated container {} state to: {}", uuid, new_state);
        } else {
            warn!("Attempted to update state for non-existent container: {}", uuid);
        }
        Ok(())
    }

    pub async fn update_container_id(&mut self, uuid: &str, container_id: &str) -> anyhow::Result<()> {
        if let Some(container) = self.state.containers.get_mut(uuid) {
            container.container_id = Some(container_id.to_string());
            container.updated_at = Utc::now();
            self.state.last_updated = Utc::now();
            self.save_state().await?;
            info!("Updated container {} Docker ID to: {}", uuid, container_id);
        } else {
            warn!("Attempted to update container ID for non-existent container: {}", uuid);
        }
        Ok(())
    }

    pub async fn remove_container(&mut self, uuid: &str) -> anyhow::Result<()> {
        if self.state.containers.remove(uuid).is_some() {
            self.state.last_updated = Utc::now();
            self.save_state().await?;
            info!("Removed container {} from daemon state", uuid);
        } else {
            warn!("Attempted to remove non-existent container: {}", uuid);
        }
        Ok(())
    }

    pub fn get_container(&self, uuid: &str) -> Option<&ContainerState> {
        self.state.containers.get(uuid)
    }

    pub fn get_all_containers(&self) -> &HashMap<String, ContainerState> {
        &self.state.containers
    }

    pub fn find_by_container_id(&self, container_id: &str) -> Option<(&String, &ContainerState)> {
        self.state.containers.iter()
            .find(|(_, state)| state.container_id.as_ref() == Some(&container_id.to_string()))
    }

    pub async fn reconcile_with_docker(&mut self, active_container_ids: &[String]) -> anyhow::Result<()> {
        let mut containers_to_update = Vec::new();
        
        for (uuid, container_state) in &self.state.containers {
            if let Some(container_id) = &container_state.container_id {
                if !active_container_ids.contains(container_id) {
                    // Container exists in state but not in Docker - mark as stopped
                    containers_to_update.push((uuid.clone(), "stopped".to_string()));
                }
            }
        }

        // Update container states
        for (uuid, new_state) in containers_to_update {
            self.update_container_state(&uuid, &new_state).await?;
        }

        info!("Reconciled daemon state with Docker - checked {} containers", self.state.containers.len());
        Ok(())
    }

    pub async fn lock_container(&mut self, uuid: &str, reason: &str) -> anyhow::Result<()> {
        if let Some(container) = self.state.containers.get_mut(uuid) {
            container.locked = Some(true);
            container.lock_reason = Some(reason.to_string());
            container.locked_at = Some(Utc::now().timestamp());
            container.updated_at = Utc::now();
            self.state.last_updated = Utc::now();
            self.save_state().await?;
            info!("Locked container {} with reason: {}", uuid, reason);
        } else {
            warn!("Attempted to lock non-existent container: {}", uuid);
        }
        Ok(())
    }

    pub async fn unlock_container(&mut self, uuid: &str) -> anyhow::Result<()> {
        if let Some(container) = self.state.containers.get_mut(uuid) {
            container.locked = Some(false);
            container.lock_reason = None;
            container.locked_at = None;
            container.updated_at = Utc::now();
            self.state.last_updated = Utc::now();
            self.save_state().await?;
            info!("Unlocked container {}", uuid);
        } else {
            warn!("Attempted to unlock non-existent container: {}", uuid);
        }
        Ok(())
    }

    pub async fn suspend_container(&mut self, uuid: &str, reason: &str) -> anyhow::Result<()> {
        if let Some(container) = self.state.containers.get_mut(uuid) {
            container.state = "suspended".to_string();
            container.locked = Some(true);
            container.lock_reason = Some(reason.to_string());
            container.locked_at = Some(Utc::now().timestamp());
            container.updated_at = Utc::now();
            self.state.last_updated = Utc::now();
            self.save_state().await?;
            info!("Suspended container {} with reason: {}", uuid, reason);
        } else {
            warn!("Attempted to suspend non-existent container: {}", uuid);
        }
        Ok(())
    }

    pub async fn unsuspend_container(&mut self, uuid: &str) -> anyhow::Result<()> {
        if let Some(container) = self.state.containers.get_mut(uuid) {
            container.state = "stopped".to_string(); // Set to stopped, user can start it manually
            container.locked = Some(false);
            container.lock_reason = None;
            container.locked_at = None;
            container.updated_at = Utc::now();
            self.state.last_updated = Utc::now();
            self.save_state().await?;
            info!("Unsuspended container {}", uuid);
        } else {
            warn!("Attempted to unsuspend non-existent container: {}", uuid);
        }
        Ok(())
    }

    pub fn is_container_suspended(&self, uuid: &str) -> bool {
        self.state.containers.get(uuid)
            .map(|c| c.state == "suspended")
            .unwrap_or(false)
    }

    pub async fn update_container_limits(&mut self, uuid: &str, limits: &crate::models::ResourceLimits) -> anyhow::Result<()> {
        if let Some(container) = self.state.containers.get_mut(uuid) {
            container.memory_limit = limits.memory.clone();
            container.cpu_limit = limits.cpu.clone();
            container.disk_limit = limits.disk.clone();
            container.swap_limit = limits.swap.clone();
            container.pids_limit = limits.pids;
            container.threads_limit = limits.threads;
            container.updated_at = Utc::now();
            self.state.last_updated = Utc::now();
            self.save_state().await?;
            info!("Updated limits for container {}", uuid);
        } else {
            warn!("Attempted to update limits for non-existent container: {}", uuid);
        }
        Ok(())
    }

    pub fn is_container_locked(&self, uuid: &str) -> bool {
        self.state.containers.get(uuid)
            .and_then(|c| c.locked)
            .unwrap_or(false)
    }

    pub async fn get_recovery_info(&self) -> Vec<(String, ContainerState)> {
        self.state.containers.iter()
            .filter(|(_, state)| {
                // Return containers that should be recovered (running, created, etc.)
                matches!(state.state.as_str(), "running" | "created" | "restarting")
            })
            .map(|(uuid, state)| (uuid.clone(), state.clone()))
            .collect()
    }
}

impl From<&ContainerTracker> for ContainerState {
    fn from(tracker: &ContainerTracker) -> Self {
        ContainerState {
            state: tracker.status.clone(),
            container_id: Some(tracker.container_id.clone()),
            image: tracker.image.clone(),
            name: tracker.name.clone(),
            description: tracker.description.clone(),
            startup_command: tracker.startup_command.as_ref().map(|cmd| cmd.join(" ")),
            disk_limit: tracker.limits.disk.clone(),
            memory_limit: tracker.limits.memory.clone(),
            cpu_limit: tracker.limits.cpu.clone(),
            swap_limit: tracker.limits.swap.clone(),
            pids_limit: tracker.limits.pids,
            threads_limit: tracker.limits.threads,
            attached_volumes: tracker.attached_volumes.iter().map(|vm| vm.source.clone()).collect(),
            ports: tracker.ports.clone(),
            env: tracker.env.clone(),
            created_at: tracker.created_at,
            updated_at: Utc::now(),
            locked: None,
            lock_reason: None,
            locked_at: None,
            restart_policy: None,
            install_content: tracker.install_content.clone(),
            update_content: tracker.update_content.clone(),
        }
    }
}