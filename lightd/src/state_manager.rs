//! Lock-free state management for lightd daemon
//!
//! Uses DashMap for concurrent access without global locks.
//! State updates are batched and written to disk asynchronously.

use crate::models::ContainerTracker;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::Notify;
use tracing::{info, warn, error};
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

/// Lock-free state manager using DashMap
/// 
/// All operations are non-blocking and thread-safe.
/// State is persisted asynchronously via a background task.
pub struct StateManager {
    state_file_path: String,
    /// Lock-free concurrent map: UUID -> ContainerState
    containers: Arc<DashMap<String, ContainerState>>,
    /// Reverse index: Container ID -> UUID (for fast lookups)
    container_id_index: Arc<DashMap<String, String>>,
    /// Flag to indicate state needs saving
    dirty: Arc<AtomicBool>,
    /// Notify for save task
    save_notify: Arc<Notify>,
    /// Daemon version
    daemon_version: String,
}

impl StateManager {
    pub fn new(storage_path: &str) -> Self {
        let state_file_path = format!("{}/states.json", storage_path);

        Self {
            state_file_path,
            containers: Arc::new(DashMap::new()),
            container_id_index: Arc::new(DashMap::new()),
            dirty: Arc::new(AtomicBool::new(false)),
            save_notify: Arc::new(Notify::new()),
            daemon_version: "0.1.0".to_string(),
        }
    }
    
    /// Mark state as dirty (needs saving)
    fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::SeqCst);
        self.save_notify.notify_one();
    }
    
    /// Get the dirty flag and notify for spawning save task
    pub fn get_save_handles(&self) -> (Arc<AtomicBool>, Arc<Notify>, String) {
        (self.dirty.clone(), self.save_notify.clone(), self.state_file_path.clone())
    }
    
    /// Get containers map for background save task
    pub fn get_containers_ref(&self) -> Arc<DashMap<String, ContainerState>> {
        self.containers.clone()
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
        let state: DaemonState = serde_json::from_str(&content)?;
        
        // Populate DashMap from loaded state
        for (uuid, container_state) in state.containers {
            // Build reverse index
            if let Some(cid) = &container_state.container_id {
                self.container_id_index.insert(cid.clone(), uuid.clone());
            }
            self.containers.insert(uuid, container_state);
        }
        
        self.daemon_version = state.daemon_version;
        Ok(())
    }

    pub async fn save_state(&self) -> anyhow::Result<()> {
        let state = self.get_state_snapshot();
        let content = serde_json::to_string_pretty(&state)?;
        fs::write(&self.state_file_path, content).await?;
        Ok(())
    }
    
    /// Get a snapshot of state for async saving (lock-free)
    pub fn get_state_snapshot(&self) -> DaemonState {
        let containers: HashMap<String, ContainerState> = self.containers
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect();
            
        DaemonState {
            containers,
            last_updated: Utc::now(),
            daemon_version: self.daemon_version.clone(),
        }
    }

    /// Add a container (lock-free)
    pub async fn add_container(&self, uuid: &str, container_state: ContainerState) -> anyhow::Result<()> {
        // Update reverse index
        if let Some(cid) = &container_state.container_id {
            self.container_id_index.insert(cid.clone(), uuid.to_string());
        }
        
        self.containers.insert(uuid.to_string(), container_state);
        self.mark_dirty();
        info!("Added container {} to daemon state", uuid);
        Ok(())
    }

    /// Update container state (lock-free)
    pub async fn update_container_state(&self, uuid: &str, new_state: &str) -> anyhow::Result<()> {
        if let Some(mut entry) = self.containers.get_mut(uuid) {
            entry.state = new_state.to_string();
            entry.updated_at = Utc::now();
            self.mark_dirty();
            info!("Updated container {} state to: {}", uuid, new_state);
        } else {
            warn!("Attempted to update state for non-existent container: {}", uuid);
        }
        Ok(())
    }

    /// Update container ID (lock-free)
    pub async fn update_container_id(&self, uuid: &str, container_id: &str) -> anyhow::Result<()> {
        if let Some(mut entry) = self.containers.get_mut(uuid) {
            // Remove old index entry if exists
            if let Some(old_cid) = &entry.container_id {
                self.container_id_index.remove(old_cid);
            }
            
            // Update container ID and index
            entry.container_id = Some(container_id.to_string());
            entry.updated_at = Utc::now();
            self.container_id_index.insert(container_id.to_string(), uuid.to_string());
            
            self.mark_dirty();
            info!("Updated container {} Docker ID to: {}", uuid, container_id);
        } else {
            warn!("Attempted to update container ID for non-existent container: {}", uuid);
        }
        Ok(())
    }

    /// Remove a container (lock-free)
    pub async fn remove_container(&self, uuid: &str) -> anyhow::Result<()> {
        if let Some((_, container_state)) = self.containers.remove(uuid) {
            // Remove from reverse index
            if let Some(cid) = &container_state.container_id {
                self.container_id_index.remove(cid);
            }
            self.mark_dirty();
            info!("Removed container {} from daemon state", uuid);
        } else {
            warn!("Attempted to remove non-existent container: {}", uuid);
        }
        Ok(())
    }

    /// Get container by UUID (lock-free, returns clone)
    pub fn get_container(&self, uuid: &str) -> Option<ContainerState> {
        self.containers.get(uuid).map(|entry| entry.value().clone())
    }

    /// Get all containers (lock-free, returns clone)
    pub fn get_all_containers(&self) -> HashMap<String, ContainerState> {
        self.containers
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// Find by container ID using reverse index (lock-free, O(1))
    pub fn find_by_container_id(&self, container_id: &str) -> Option<(String, ContainerState)> {
        // Use reverse index for O(1) lookup
        if let Some(uuid_ref) = self.container_id_index.get(container_id) {
            let uuid = uuid_ref.value().clone();
            if let Some(state) = self.containers.get(&uuid) {
                return Some((uuid, state.value().clone()));
            }
        }
        None
    }
    
    /// Get UUID for a container ID (lock-free, O(1))
    pub fn get_uuid_for_container_id(&self, container_id: &str) -> Option<String> {
        self.container_id_index.get(container_id).map(|r| r.value().clone())
    }

    /// Reconcile with Docker (lock-free iteration)
    pub async fn reconcile_with_docker(&self, active_container_ids: &[String]) -> anyhow::Result<()> {
        let mut containers_to_update = Vec::new();
        
        // Collect updates without holding any locks
        for entry in self.containers.iter() {
            let uuid = entry.key().clone();
            let container_state = entry.value();
            
            if let Some(container_id) = &container_state.container_id {
                if !active_container_ids.contains(container_id) {
                    containers_to_update.push((uuid, "stopped".to_string()));
                }
            }
        }

        // Apply updates
        for (uuid, new_state) in containers_to_update {
            self.update_container_state(&uuid, &new_state).await?;
        }

        info!("Reconciled daemon state with Docker - checked {} containers", self.containers.len());
        Ok(())
    }

    /// Lock a container (lock-free)
    pub async fn lock_container(&self, uuid: &str, reason: &str) -> anyhow::Result<()> {
        if let Some(mut entry) = self.containers.get_mut(uuid) {
            entry.locked = Some(true);
            entry.lock_reason = Some(reason.to_string());
            entry.locked_at = Some(Utc::now().timestamp());
            entry.updated_at = Utc::now();
            self.mark_dirty();
            info!("Locked container {} with reason: {}", uuid, reason);
        } else {
            warn!("Attempted to lock non-existent container: {}", uuid);
        }
        Ok(())
    }

    /// Unlock a container (lock-free)
    pub async fn unlock_container(&self, uuid: &str) -> anyhow::Result<()> {
        if let Some(mut entry) = self.containers.get_mut(uuid) {
            entry.locked = Some(false);
            entry.lock_reason = None;
            entry.locked_at = None;
            entry.updated_at = Utc::now();
            self.mark_dirty();
            info!("Unlocked container {}", uuid);
        } else {
            warn!("Attempted to unlock non-existent container: {}", uuid);
        }
        Ok(())
    }

    /// Suspend a container (lock-free)
    pub async fn suspend_container(&self, uuid: &str, reason: &str) -> anyhow::Result<()> {
        if let Some(mut entry) = self.containers.get_mut(uuid) {
            entry.state = "suspended".to_string();
            entry.locked = Some(true);
            entry.lock_reason = Some(reason.to_string());
            entry.locked_at = Some(Utc::now().timestamp());
            entry.updated_at = Utc::now();
            self.mark_dirty();
            info!("Suspended container {} with reason: {}", uuid, reason);
        } else {
            warn!("Attempted to suspend non-existent container: {}", uuid);
        }
        Ok(())
    }

    /// Unsuspend a container (lock-free)
    pub async fn unsuspend_container(&self, uuid: &str) -> anyhow::Result<()> {
        if let Some(mut entry) = self.containers.get_mut(uuid) {
            entry.state = "stopped".to_string();
            entry.locked = Some(false);
            entry.lock_reason = None;
            entry.locked_at = None;
            entry.updated_at = Utc::now();
            self.mark_dirty();
            info!("Unsuspended container {}", uuid);
        } else {
            warn!("Attempted to unsuspend non-existent container: {}", uuid);
        }
        Ok(())
    }

    /// Check if container is suspended (lock-free)
    pub fn is_container_suspended(&self, uuid: &str) -> bool {
        self.containers.get(uuid)
            .map(|entry| entry.state == "suspended")
            .unwrap_or(false)
    }

    /// Update container limits (lock-free)
    pub async fn update_container_limits(&self, uuid: &str, limits: &crate::models::ResourceLimits) -> anyhow::Result<()> {
        if let Some(mut entry) = self.containers.get_mut(uuid) {
            entry.memory_limit = limits.memory.clone();
            entry.cpu_limit = limits.cpu.clone();
            entry.disk_limit = limits.disk.clone();
            entry.swap_limit = limits.swap.clone();
            entry.pids_limit = limits.pids;
            entry.threads_limit = limits.threads;
            entry.updated_at = Utc::now();
            self.mark_dirty();
            info!("Updated limits for container {}", uuid);
        } else {
            warn!("Attempted to update limits for non-existent container: {}", uuid);
        }
        Ok(())
    }

    /// Check if container is locked (lock-free)
    pub fn is_container_locked(&self, uuid: &str) -> bool {
        self.containers.get(uuid)
            .and_then(|entry| entry.locked)
            .unwrap_or(false)
    }

    /// Get recovery info (lock-free)
    pub async fn get_recovery_info(&self) -> Vec<(String, ContainerState)> {
        self.containers.iter()
            .filter(|entry| {
                matches!(entry.value().state.as_str(), "running" | "created" | "restarting")
            })
            .map(|entry| (entry.key().clone(), entry.value().clone()))
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
