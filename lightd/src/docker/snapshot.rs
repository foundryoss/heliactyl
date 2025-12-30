use bollard::Docker;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use tokio::process::Command;
use tracing::{info, warn, debug};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::state_manager::{StateManager, ContainerState};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub snapshot_id: String,
    pub container_uuid: String,
    pub container_id: String,
    pub container_name: String,
    pub created_at: DateTime<Utc>,
    pub size_bytes: u64,
    pub file_count: u64,
    pub container_config: ContainerState,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub snapshot_id: String,
    pub container_uuid: String,
    pub container_name: String,
    pub created_at: DateTime<Utc>,
    pub size_bytes: u64,
    pub file_count: u64,
    pub status: String,
}

pub struct SnapshotManager {
    docker: Docker,
    snapshots_path: String,
}

impl SnapshotManager {
    pub fn new(docker: Docker, storage_path: &str) -> Self {
        let snapshots_path = format!("{}/snapshots", storage_path);
        Self {
            docker,
            snapshots_path,
        }
    }

    pub async fn init(&self) -> anyhow::Result<()> {
        fs::create_dir_all(&self.snapshots_path).await?;
        info!("Initialized snapshot storage at: {}", self.snapshots_path);
        Ok(())
    }

    /// Create a simple file-based snapshot of container workspace
    pub async fn create_snapshot(
        &self,
        container_id: &str,
        container_uuid: &str,
        state_manager: &StateManager,
    ) -> anyhow::Result<String> {
        let snapshot_id = Uuid::new_v4().to_string();
        let snapshot_dir = format!("{}/{}", self.snapshots_path, snapshot_id);
        
        info!("Creating workspace snapshot {} for container {}", snapshot_id, container_id);
        
        // Create snapshot directory
        fs::create_dir_all(&snapshot_dir).await?;
        
        // Get container state
        let container_state = state_manager.get_container(container_uuid)
            .ok_or_else(|| anyhow::anyhow!("Container not found in state manager"))?;
        
        // 1. Create tar archive of workspace files
        info!("Creating tar archive of workspace files...");
        let archive_path = format!("{}/workspace.tar.gz", snapshot_dir);
        let file_count = self.create_workspace_archive(container_id, &archive_path).await?;
        
        // 2. Calculate archive size
        let size_bytes = fs::metadata(&archive_path).await?.len();
        
        // 3. Create metadata
        let metadata = SnapshotMetadata {
            snapshot_id: snapshot_id.clone(),
            container_uuid: container_uuid.to_string(),
            container_id: container_id.to_string(),
            container_name: container_state.name.clone(),
            created_at: Utc::now(),
            size_bytes,
            file_count,
            container_config: container_state.clone(),
        };
        
        // 4. Save metadata
        let metadata_path = format!("{}/metadata.json", snapshot_dir);
        let metadata_json = serde_json::to_string_pretty(&metadata)?;
        fs::write(&metadata_path, metadata_json).await?;
        
        info!("Snapshot {} created successfully ({} bytes, {} files)", 
              snapshot_id, size_bytes, file_count);
        Ok(snapshot_id)
    }

    /// Restore workspace files from snapshot
    pub async fn restore_snapshot(
        &self,
        snapshot_id: &str,
        container_id: &str,
    ) -> anyhow::Result<()> {
        let snapshot_dir = format!("{}/{}", self.snapshots_path, snapshot_id);
        let metadata_path = format!("{}/metadata.json", snapshot_dir);
        let archive_path = format!("{}/workspace.tar.gz", snapshot_dir);
        
        info!("Restoring workspace snapshot {} to container {}", snapshot_id, container_id);
        
        // Load metadata
        let metadata_content = fs::read_to_string(&metadata_path).await?;
        let metadata: SnapshotMetadata = serde_json::from_str(&metadata_content)?;
        
        // Check if archive exists
        if !Path::new(&archive_path).exists() {
            return Err(anyhow::anyhow!("Snapshot archive not found: {}", archive_path));
        }
        
        // 1. Clear workspace directory
        info!("Clearing workspace directory...");
        self.clear_workspace(container_id).await?;
        
        // 2. Extract archive to workspace
        info!("Extracting archive to workspace...");
        self.extract_workspace_archive(container_id, &archive_path).await?;
        
        info!("Snapshot {} restored successfully ({} files)", 
              snapshot_id, metadata.file_count);
        Ok(())
    }

    /// List all available snapshots
    pub async fn list_snapshots(&self) -> anyhow::Result<Vec<SnapshotInfo>> {
        let mut snapshots = Vec::new();
        let mut entries = fs::read_dir(&self.snapshots_path).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let snapshot_id = entry.file_name().to_string_lossy().to_string();
                let metadata_path = format!("{}/{}/metadata.json", self.snapshots_path, snapshot_id);
                
                if let Ok(metadata_content) = fs::read_to_string(&metadata_path).await {
                    if let Ok(metadata) = serde_json::from_str::<SnapshotMetadata>(&metadata_content) {
                        snapshots.push(SnapshotInfo {
                            snapshot_id: metadata.snapshot_id,
                            container_uuid: metadata.container_uuid,
                            container_name: metadata.container_name,
                            created_at: metadata.created_at,
                            size_bytes: metadata.size_bytes,
                            file_count: metadata.file_count,
                            status: "available".to_string(),
                        });
                    }
                }
            }
        }
        
        snapshots.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(snapshots)
    }

    /// Delete a snapshot
    pub async fn delete_snapshot(&self, snapshot_id: &str) -> anyhow::Result<()> {
        let snapshot_dir = format!("{}/{}", self.snapshots_path, snapshot_id);
        
        if !Path::new(&snapshot_dir).exists() {
            return Err(anyhow::anyhow!("Snapshot {} not found", snapshot_id));
        }
        
        info!("Deleting snapshot {}", snapshot_id);
        fs::remove_dir_all(&snapshot_dir).await?;
        info!("Snapshot {} deleted successfully", snapshot_id);
        Ok(())
    }

    /// Get snapshot metadata
    pub async fn get_snapshot_metadata(&self, snapshot_id: &str) -> anyhow::Result<SnapshotMetadata> {
        let metadata_path = format!("{}/{}/metadata.json", self.snapshots_path, snapshot_id);
        let metadata_content = fs::read_to_string(&metadata_path).await?;
        let metadata: SnapshotMetadata = serde_json::from_str(&metadata_content)?;
        Ok(metadata)
    }

    // Private helper methods

    async fn create_workspace_archive(&self, container_id: &str, archive_path: &str) -> anyhow::Result<u64> {
        // Use docker exec to create tar archive of workspace
        let output = Command::new("docker")
            .args(&[
                "exec",
                container_id,
                "sh", "-c",
                "cd /workspace && find . -type f | wc -l && tar -czf - . 2>/dev/null || echo 'No files to archive'"
            ])
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to create workspace archive: {}", 
                String::from_utf8_lossy(&output.stderr)));
        }

        // Get file count from first line of output
        let output_str = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<&str> = output_str.lines().collect();
        let file_count = if !lines.is_empty() {
            lines[0].trim().parse::<u64>().unwrap_or(0)
        } else {
            0
        };

        // Create the actual archive
        let output = Command::new("docker")
            .args(&[
                "exec",
                container_id,
                "sh", "-c",
                "cd /workspace && tar -czf - . 2>/dev/null || true"
            ])
            .output()
            .await?;

        if !output.status.success() {
            warn!("Archive creation had warnings: {}", String::from_utf8_lossy(&output.stderr));
        }

        // Write archive data to file
        fs::write(archive_path, &output.stdout).await?;
        
        debug!("Created workspace archive with {} files", file_count);
        Ok(file_count)
    }

    async fn clear_workspace(&self, container_id: &str) -> anyhow::Result<()> {
        // Clear all files in workspace directory
        let output = Command::new("docker")
            .args(&[
                "exec",
                container_id,
                "sh", "-c",
                "cd /workspace && rm -rf * .* 2>/dev/null || true"
            ])
            .output()
            .await?;

        if !output.status.success() {
            warn!("Workspace clear had warnings: {}", String::from_utf8_lossy(&output.stderr));
        }

        debug!("Cleared workspace directory");
        Ok(())
    }

    async fn extract_workspace_archive(&self, container_id: &str, archive_path: &str) -> anyhow::Result<()> {
        // Read archive file
        let archive_data = fs::read(archive_path).await?;
        
        if archive_data.is_empty() {
            debug!("Archive is empty, nothing to extract");
            return Ok(());
        }

        // Create temporary file in container and extract
        let temp_archive = "/tmp/restore_archive.tar.gz";
        
        // Copy archive to container
        let mut child = Command::new("docker")
            .args(&[
                "exec", "-i",
                container_id,
                "sh", "-c",
                &format!("cat > {}", temp_archive)
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;

        if let Some(stdin) = child.stdin.as_mut() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(&archive_data).await?;
            stdin.flush().await?;
        }

        let output = child.wait_with_output().await?;
        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to copy archive to container: {}", 
                String::from_utf8_lossy(&output.stderr)));
        }

        // Extract archive in workspace
        let output = Command::new("docker")
            .args(&[
                "exec",
                container_id,
                "sh", "-c",
                &format!("cd /workspace && tar -xzf {} && rm {}", temp_archive, temp_archive)
            ])
            .output()
            .await?;

        if !output.status.success() {
            return Err(anyhow::anyhow!("Failed to extract archive: {}", 
                String::from_utf8_lossy(&output.stderr)));
        }

        debug!("Extracted workspace archive");
        Ok(())
    }
}