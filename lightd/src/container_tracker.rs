use crate::models::ContainerTracker;
use serde_json;
use tokio::fs;
use tracing::{info, error};
use uuid::Uuid;

pub struct ContainerTrackingManager {
    storage_dir: String,
}

impl ContainerTrackingManager {
    pub fn new(storage_dir: &str) -> Self {
        Self {
            storage_dir: storage_dir.to_string(),
        }
    }

    pub async fn init(&self) -> anyhow::Result<()> {
        // Create storage directory if it doesn't exist
        fs::create_dir_all(&self.storage_dir).await?;
        info!("Container tracking storage initialized at: {}", self.storage_dir);
        Ok(())
    }

    pub fn generate_uuid() -> String {
        Uuid::new_v4().to_string()
    }

    pub async fn save_container(&self, tracker: &ContainerTracker) -> anyhow::Result<()> {
        let file_path = format!("{}/{}.json", self.storage_dir, tracker.custom_uuid);
        let content = serde_json::to_string_pretty(tracker)?;
        fs::write(&file_path, content).await?;
        info!("Saved container tracking data: {}", file_path);
        Ok(())
    }

    pub async fn load_container(&self, uuid: &str) -> anyhow::Result<ContainerTracker> {
        let file_path = format!("{}/{}.json", self.storage_dir, uuid);
        let content = fs::read_to_string(&file_path).await?;
        let tracker: ContainerTracker = serde_json::from_str(&content)?;
        Ok(tracker)
    }

    pub async fn update_container_status(&self, uuid: &str, status: &str) -> anyhow::Result<()> {
        let mut tracker = self.load_container(uuid).await?;
        tracker.status = status.to_string();
        self.save_container(&tracker).await?;
        Ok(())
    }

    pub async fn remove_container(&self, uuid: &str) -> anyhow::Result<()> {
        let file_path = format!("{}/{}.json", self.storage_dir, uuid);
        match fs::remove_file(&file_path).await {
            Ok(_) => {
                info!("Removed container tracking data: {}", file_path);
                Ok(())
            }
            Err(e) => {
                error!("Failed to remove container tracking data {}: {}", file_path, e);
                Err(e.into())
            }
        }
    }

    pub async fn list_containers(&self) -> anyhow::Result<Vec<ContainerTracker>> {
        let mut containers = Vec::new();
        let mut dir = fs::read_dir(&self.storage_dir).await?;
        
        while let Some(entry) = dir.next_entry().await? {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(file_name) = path.file_stem().and_then(|s| s.to_str()) {
                    match self.load_container(file_name).await {
                        Ok(tracker) => containers.push(tracker),
                        Err(e) => error!("Failed to load container {}: {}", file_name, e),
                    }
                }
            }
        }
        
        Ok(containers)
    }

    pub async fn find_by_container_id(&self, container_id: &str) -> anyhow::Result<Option<ContainerTracker>> {
        let containers = self.list_containers().await?;
        Ok(containers.into_iter().find(|c| c.container_id == container_id))
    }

    pub async fn cleanup_orphaned(&self, active_container_ids: &[String]) -> anyhow::Result<()> {
        let containers = self.list_containers().await?;
        
        for tracker in containers {
            if !active_container_ids.contains(&tracker.container_id) {
                info!("Cleaning up orphaned container tracking: {}", tracker.custom_uuid);
                self.remove_container(&tracker.custom_uuid).await?;
            }
        }
        
        Ok(())
    }
}