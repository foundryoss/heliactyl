use bollard::Docker;
use bollard::exec::{CreateExecOptions, StartExecResults};
use futures_util::stream::StreamExt;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

#[derive(Debug, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    pub size: Option<u64>,
    pub permissions: String,
    pub modified: Option<String>,
    pub owner: Option<String>,
    pub group: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DirectoryListing {
    pub path: String,
    pub files: Vec<FileInfo>,
    pub total_files: usize,
    pub total_directories: usize,
}

pub struct FilesystemManager<'a> {
    client: &'a Docker,
}

impl<'a> FilesystemManager<'a> {
    pub fn new(client: &'a Docker) -> Self {
        Self { client }
    }

    /// Validate path without performing the full sanitization (for pre-validation)
    pub fn validate_path(&self, path: &str) -> anyhow::Result<()> {
        // Just call sanitize_path and ignore the result, we only care about validation
        self.sanitize_path(path).map(|_| ())
    }

    /// List files and directories in a container path
    pub async fn list_directory(&self, container_id: &str, path: &str) -> anyhow::Result<DirectoryListing> {
        info!("Listing directory {} in container {}", path, container_id);
        
        // Sanitize path - ensure it starts with / and doesn't have dangerous patterns
        let clean_path = self.sanitize_path(path)?;
        
        // Use ls -la to get detailed file information
        let cmd = vec!["ls", "-la", "--time-style=iso", &clean_path];
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/home/container".to_string()),
            ..Default::default()
        };

        let exec = self.client.create_exec(container_id, exec_options).await?;
        
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                while let Some(chunk) = output.next().await {
                    match chunk {
                        Ok(log_output) => {
                            result.push_str(&log_output.to_string());
                        }
                        Err(e) => {
                            error!("Error reading directory listing: {}", e);
                            return Err(e.into());
                        }
                    }
                }
                
                self.parse_ls_output(&result, &clean_path)
            }
            StartExecResults::Detached => {
                Err(anyhow::anyhow!("Directory listing command detached unexpectedly"))
            }
        }
    }

    /// Get file content
    pub async fn get_file_content(&self, container_id: &str, path: &str) -> anyhow::Result<String> {
        info!("Getting file content {} in container {}", path, container_id);
        
        let clean_path = self.sanitize_path(path)?;
        
        // Use stat to check if it's a file
        let stat_cmd = vec!["stat", "-c", "%F", &clean_path];
        
        let exec_options = CreateExecOptions {
            cmd: Some(stat_cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/home/container".to_string()),
            ..Default::default()
        };

        let exec = self.client.create_exec(container_id, exec_options).await?;
        
        let file_type = match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                while let Some(chunk) = output.next().await {
                    match chunk {
                        Ok(log_output) => {
                            result.push_str(&log_output.to_string());
                        }
                        Err(e) => {
                            error!("Error checking file type: {}", e);
                            return Err(e.into());
                        }
                    }
                }
                result.trim().to_string()
            }
            StartExecResults::Detached => {
                return Err(anyhow::anyhow!("Stat command detached unexpectedly"));
            }
        };
        
        if file_type.contains("directory") {
            return Err(anyhow::anyhow!("Path is a directory, not a file"));
        }
        
        let cmd = vec!["cat", &clean_path];
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/home/container".to_string()),
            ..Default::default()
        };

        let exec = self.client.create_exec(container_id, exec_options).await?;
        
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                while let Some(chunk) = output.next().await {
                    match chunk {
                        Ok(log_output) => {
                            result.push_str(&log_output.to_string());
                        }
                        Err(e) => {
                            error!("Error reading file content: {}", e);
                            return Err(e.into());
                        }
                    }
                }
                Ok(result)
            }
            StartExecResults::Detached => {
                Err(anyhow::anyhow!("File read command detached unexpectedly"))
            }
        }
    }

    /// Write file content
    pub async fn write_file(&self, container_id: &str, path: &str, content: &str) -> anyhow::Result<()> {
        info!("Writing file {} in container {}", path, container_id);
        
        let clean_path = self.sanitize_path(path)?;
        
        // Escape content for shell
        let escaped_content = content.replace("'", "'\"'\"'");
        let write_cmd = format!("echo '{}' > '{}'", escaped_content, clean_path);
        
        let cmd = vec!["sh", "-c", &write_cmd];
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/home/container".to_string()),
            ..Default::default()
        };

        let exec = self.client.create_exec(container_id, exec_options).await?;
        
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                while let Some(chunk) = output.next().await {
                    match chunk {
                        Ok(log_output) => {
                            result.push_str(&log_output.to_string());
                        }
                        Err(e) => {
                            error!("Error writing file: {}", e);
                            return Err(e.into());
                        }
                    }
                }
                
                if !result.trim().is_empty() {
                    error!("File write produced output: {}", result);
                    return Err(anyhow::anyhow!("File write failed: {}", result));
                }
                
                Ok(())
            }
            StartExecResults::Detached => {
                Err(anyhow::anyhow!("File write command detached unexpectedly"))
            }
        }
    }

    /// Create directory
    pub async fn create_directory(&self, container_id: &str, path: &str) -> anyhow::Result<()> {
        info!("Creating directory {} in container {}", path, container_id);
        
        let clean_path = self.sanitize_path(path)?;
        let cmd = vec!["mkdir", "-p", &clean_path];
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/home/container".to_string()),
            ..Default::default()
        };

        let exec = self.client.create_exec(container_id, exec_options).await?;
        
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                while let Some(chunk) = output.next().await {
                    match chunk {
                        Ok(log_output) => {
                            result.push_str(&log_output.to_string());
                        }
                        Err(e) => {
                            error!("Error creating directory: {}", e);
                            return Err(e.into());
                        }
                    }
                }
                
                if !result.trim().is_empty() {
                    error!("Directory creation produced output: {}", result);
                    return Err(anyhow::anyhow!("Directory creation failed: {}", result));
                }
                
                Ok(())
            }
            StartExecResults::Detached => {
                Err(anyhow::anyhow!("Directory creation command detached unexpectedly"))
            }
        }
    }

    /// Delete file or directory
    pub async fn delete(&self, container_id: &str, path: &str) -> anyhow::Result<()> {
        info!("Deleting {} in container {}", path, container_id);
        
        let clean_path = self.sanitize_path(path)?;
        let cmd = vec!["rm", "-rf", &clean_path];
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/home/container".to_string()),
            ..Default::default()
        };

        let exec = self.client.create_exec(container_id, exec_options).await?;
        
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                while let Some(chunk) = output.next().await {
                    match chunk {
                        Ok(log_output) => {
                            result.push_str(&log_output.to_string());
                        }
                        Err(e) => {
                            error!("Error deleting: {}", e);
                            return Err(e.into());
                        }
                    }
                }
                
                if !result.trim().is_empty() {
                    error!("Delete produced output: {}", result);
                    return Err(anyhow::anyhow!("Delete failed: {}", result));
                }
                
                Ok(())
            }
            StartExecResults::Detached => {
                Err(anyhow::anyhow!("Delete command detached unexpectedly"))
            }
        }
    }

    /// Change file permissions (chmod)
    pub async fn chmod(&self, container_id: &str, path: &str, permissions: &str) -> anyhow::Result<()> {
        info!("Changing permissions of {} to {} in container {}", path, permissions, container_id);
        
        let clean_path = self.sanitize_path(path)?;
        
        // Validate permissions format (octal or symbolic)
        if !self.is_valid_permission(permissions) {
            return Err(anyhow::anyhow!("Invalid permission format: {}", permissions));
        }
        
        let cmd = vec!["chmod", permissions, &clean_path];
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/home/container".to_string()),
            ..Default::default()
        };

        let exec = self.client.create_exec(container_id, exec_options).await?;
        
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                while let Some(chunk) = output.next().await {
                    match chunk {
                        Ok(log_output) => {
                            result.push_str(&log_output.to_string());
                        }
                        Err(e) => {
                            error!("Error changing permissions: {}", e);
                            return Err(e.into());
                        }
                    }
                }
                
                if !result.trim().is_empty() {
                    error!("Chmod produced output: {}", result);
                    return Err(anyhow::anyhow!("Chmod failed: {}", result));
                }
                
                Ok(())
            }
            StartExecResults::Detached => {
                Err(anyhow::anyhow!("Chmod command detached unexpectedly"))
            }
        }
    }

    /// Change file ownership (chown)
    pub async fn chown(&self, container_id: &str, path: &str, owner: &str, group: Option<&str>) -> anyhow::Result<()> {
        let ownership = if let Some(g) = group {
            format!("{}:{}", owner, g)
        } else {
            owner.to_string()
        };
        
        info!("Changing ownership of {} to {} in container {}", path, ownership, container_id);
        
        let clean_path = self.sanitize_path(path)?;
        let cmd = vec!["chown", &ownership, &clean_path];
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/home/container".to_string()),
            ..Default::default()
        };

        let exec = self.client.create_exec(container_id, exec_options).await?;
        
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                while let Some(chunk) = output.next().await {
                    match chunk {
                        Ok(log_output) => {
                            result.push_str(&log_output.to_string());
                        }
                        Err(e) => {
                            error!("Error changing ownership: {}", e);
                            return Err(e.into());
                        }
                    }
                }
                
                if !result.trim().is_empty() {
                    error!("Chown produced output: {}", result);
                    return Err(anyhow::anyhow!("Chown failed: {}", result));
                }
                
                Ok(())
            }
            StartExecResults::Detached => {
                Err(anyhow::anyhow!("Chown command detached unexpectedly"))
            }
        }
    }

    /// Create a tar archive of a directory or file
    pub async fn create_archive(&self, container_id: &str, source_path: &str, archive_path: &str, compression: Option<&str>) -> anyhow::Result<String> {
        info!("Creating archive {} from {} in container {}", archive_path, source_path, container_id);
        
        let clean_source = self.sanitize_path(source_path)?;
        let clean_archive = self.sanitize_path(archive_path)?;
        
        // Determine compression flag
        let compression_flag = match compression {
            Some("gzip") | Some("gz") => "z",
            Some("bzip2") | Some("bz2") => "j",
            Some("xz") => "J",
            _ => "",
        };
        
        // Build tar command
        let tar_cmd = if compression_flag.is_empty() {
            format!("tar -cf '{}' -C '{}' .", clean_archive, clean_source)
        } else {
            format!("tar -c{}f '{}' -C '{}' .", compression_flag, clean_archive, clean_source)
        };
        
        let cmd = vec!["sh", "-c", &tar_cmd];
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/home/container".to_string()),
            ..Default::default()
        };

        let exec = self.client.create_exec(container_id, exec_options).await?;
        
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                while let Some(chunk) = output.next().await {
                    match chunk {
                        Ok(log_output) => {
                            result.push_str(&log_output.to_string());
                        }
                        Err(e) => {
                            error!("Error creating archive: {}", e);
                            return Err(e.into());
                        }
                    }
                }
                
                // Get archive size
                let size_info = self.get_file_size(container_id, &clean_archive).await
                    .unwrap_or_else(|_| "unknown".to_string());
                
                Ok(format!("Archive created successfully. Size: {}", size_info))
            }
            StartExecResults::Detached => {
                Err(anyhow::anyhow!("Archive creation command detached unexpectedly"))
            }
        }
    }

    /// Extract a tar archive
    pub async fn extract_archive(&self, container_id: &str, archive_path: &str, destination_path: &str) -> anyhow::Result<String> {
        info!("Extracting archive {} to {} in container {}", archive_path, destination_path, container_id);
        
        let clean_archive = self.sanitize_path(archive_path)?;
        let clean_dest = self.sanitize_path(destination_path)?;
        
        // Create destination directory if it doesn't exist
        self.create_directory(container_id, destination_path).await?;
        
        // Auto-detect compression and extract
        let tar_cmd = format!("tar -xaf '{}' -C '{}'", clean_archive, clean_dest);
        let cmd = vec!["sh", "-c", &tar_cmd];
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/home/container".to_string()),
            ..Default::default()
        };

        let exec = self.client.create_exec(container_id, exec_options).await?;
        
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                while let Some(chunk) = output.next().await {
                    match chunk {
                        Ok(log_output) => {
                            result.push_str(&log_output.to_string());
                        }
                        Err(e) => {
                            error!("Error extracting archive: {}", e);
                            return Err(e.into());
                        }
                    }
                }
                
                Ok("Archive extracted successfully".to_string())
            }
            StartExecResults::Detached => {
                Err(anyhow::anyhow!("Archive extraction command detached unexpectedly"))
            }
        }
    }

    /// Create a zip archive
    pub async fn create_zip(&self, container_id: &str, source_path: &str, zip_path: &str) -> anyhow::Result<String> {
        info!("Creating zip {} from {} in container {}", zip_path, source_path, container_id);
        
        let clean_source = self.sanitize_path(source_path)?;
        let clean_zip = self.sanitize_path(zip_path)?;
        
        // Check if zip is available, if not try to install it
        let check_zip_cmd = vec!["which", "zip"];
        let exec_options = CreateExecOptions {
            cmd: Some(check_zip_cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        let exec = self.client.create_exec(container_id, exec_options).await?;
        let zip_available = match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                while let Some(chunk) = output.next().await {
                    if let Ok(log_output) = chunk {
                        result.push_str(&log_output.to_string());
                    }
                }
                !result.trim().is_empty()
            }
            StartExecResults::Detached => false,
        };

        if !zip_available {
            // Try to install zip (works on Alpine and Debian-based systems)
            let install_cmd = vec!["sh", "-c", "apk add zip 2>/dev/null || apt-get update && apt-get install -y zip 2>/dev/null || yum install -y zip 2>/dev/null || true"];
            let exec_options = CreateExecOptions {
                cmd: Some(install_cmd.iter().map(|s| s.to_string()).collect()),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                ..Default::default()
            };

            let exec = self.client.create_exec(container_id, exec_options).await?;
            let _ = self.client.start_exec(&exec.id, None).await?;
        }
        
        // Create zip archive
        let zip_cmd = format!("cd '{}' && zip -r '{}' .", clean_source, clean_zip);
        let cmd = vec!["sh", "-c", &zip_cmd];
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/home/container".to_string()),
            ..Default::default()
        };

        let exec = self.client.create_exec(container_id, exec_options).await?;
        
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                while let Some(chunk) = output.next().await {
                    match chunk {
                        Ok(log_output) => {
                            result.push_str(&log_output.to_string());
                        }
                        Err(e) => {
                            error!("Error creating zip: {}", e);
                            return Err(e.into());
                        }
                    }
                }
                
                // Get zip size
                let size_info = self.get_file_size(container_id, &clean_zip).await
                    .unwrap_or_else(|_| "unknown".to_string());
                
                Ok(format!("Zip archive created successfully. Size: {}", size_info))
            }
            StartExecResults::Detached => {
                Err(anyhow::anyhow!("Zip creation command detached unexpectedly"))
            }
        }
    }

    /// Extract a zip archive
    pub async fn extract_zip(&self, container_id: &str, zip_path: &str, destination_path: &str) -> anyhow::Result<String> {
        info!("Extracting zip {} to {} in container {}", zip_path, destination_path, container_id);
        
        let clean_zip = self.sanitize_path(zip_path)?;
        let clean_dest = self.sanitize_path(destination_path)?;
        
        // Create destination directory if it doesn't exist
        self.create_directory(container_id, destination_path).await?;
        
        // Check if unzip is available
        let check_unzip_cmd = vec!["which", "unzip"];
        let exec_options = CreateExecOptions {
            cmd: Some(check_unzip_cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        let exec = self.client.create_exec(container_id, exec_options).await?;
        let unzip_available = match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                while let Some(chunk) = output.next().await {
                    if let Ok(log_output) = chunk {
                        result.push_str(&log_output.to_string());
                    }
                }
                !result.trim().is_empty()
            }
            StartExecResults::Detached => false,
        };

        if !unzip_available {
            // Try to install unzip
            let install_cmd = vec!["sh", "-c", "apk add unzip 2>/dev/null || apt-get update && apt-get install -y unzip 2>/dev/null || yum install -y unzip 2>/dev/null || true"];
            let exec_options = CreateExecOptions {
                cmd: Some(install_cmd.iter().map(|s| s.to_string()).collect()),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                ..Default::default()
            };

            let exec = self.client.create_exec(container_id, exec_options).await?;
            let _ = self.client.start_exec(&exec.id, None).await?;
        }
        
        // Extract zip
        let unzip_cmd = format!("unzip -o '{}' -d '{}'", clean_zip, clean_dest);
        let cmd = vec!["sh", "-c", &unzip_cmd];
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/home/container".to_string()),
            ..Default::default()
        };

        let exec = self.client.create_exec(container_id, exec_options).await?;
        
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                while let Some(chunk) = output.next().await {
                    match chunk {
                        Ok(log_output) => {
                            result.push_str(&log_output.to_string());
                        }
                        Err(e) => {
                            error!("Error extracting zip: {}", e);
                            return Err(e.into());
                        }
                    }
                }
                
                Ok("Zip archive extracted successfully".to_string())
            }
            StartExecResults::Detached => {
                Err(anyhow::anyhow!("Zip extraction command detached unexpectedly"))
            }
        }
    }

    /// Copy/move files
    pub async fn copy_file(&self, container_id: &str, source_path: &str, destination_path: &str, move_file: bool) -> anyhow::Result<()> {
        let operation = if move_file { "mv" } else { "cp" };
        info!("{}ing {} to {} in container {}", operation, source_path, destination_path, container_id);
        
        let clean_source = self.sanitize_path(source_path)?;
        let clean_dest = self.sanitize_path(destination_path)?;
        
        let cmd = if move_file {
            vec!["mv", &clean_source, &clean_dest]
        } else {
            vec!["cp", "-r", &clean_source, &clean_dest]
        };
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/home/container".to_string()),
            ..Default::default()
        };

        let exec = self.client.create_exec(container_id, exec_options).await?;
        
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                while let Some(chunk) = output.next().await {
                    match chunk {
                        Ok(log_output) => {
                            result.push_str(&log_output.to_string());
                        }
                        Err(e) => {
                            error!("Error {}ing file: {}", operation, e);
                            return Err(e.into());
                        }
                    }
                }
                
                if !result.trim().is_empty() {
                    error!("{} produced output: {}", operation, result);
                    return Err(anyhow::anyhow!("{} failed: {}", operation, result));
                }
                
                Ok(())
            }
            StartExecResults::Detached => {
                Err(anyhow::anyhow!("{} command detached unexpectedly", operation))
            }
        }
    }

    /// Get file size
    async fn get_file_size(&self, container_id: &str, path: &str) -> anyhow::Result<String> {
        let cmd = vec!["stat", "-c", "%s", path];
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/home/container".to_string()),
            ..Default::default()
        };

        let exec = self.client.create_exec(container_id, exec_options).await?;
        
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                while let Some(chunk) = output.next().await {
                    match chunk {
                        Ok(log_output) => {
                            result.push_str(&log_output.to_string());
                        }
                        Err(e) => {
                            return Err(e.into());
                        }
                    }
                }
                
                let size_bytes: u64 = result.trim().parse().unwrap_or(0);
                Ok(self.format_file_size(size_bytes))
            }
            StartExecResults::Detached => {
                Err(anyhow::anyhow!("Stat command detached unexpectedly"))
            }
        }
    }

    /// Format file size in human readable format
    fn format_file_size(&self, bytes: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = bytes as f64;
        let mut unit_index = 0;
        
        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }
        
        if unit_index == 0 {
            format!("{} {}", bytes, UNITS[unit_index])
        } else {
            format!("{:.2} {}", size, UNITS[unit_index])
        }
    }

    /// Validate permission format
    fn is_valid_permission(&self, permissions: &str) -> bool {
        // Check octal format (e.g., 755, 644)
        if permissions.chars().all(|c| c.is_ascii_digit()) && permissions.len() <= 4 {
            if let Ok(octal) = u32::from_str_radix(permissions, 8) {
                return octal <= 0o7777;
            }
        }
        
        // Check symbolic format (e.g., u+x, go-w, a=r)
        // This is a simplified check - real chmod supports more complex patterns
        let symbolic_chars = "ugoa+-=rwxXst";
        permissions.chars().all(|c| symbolic_chars.contains(c))
    }

    /// Check if path is a directory
    async fn is_directory(&self, container_id: &str, path: &str) -> anyhow::Result<bool> {
        let cmd = vec!["test", "-d", path];
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(false),
            attach_stderr: Some(false),
            working_dir: Some("/home/container".to_string()),
            ..Default::default()
        };

        let exec = self.client.create_exec(container_id, exec_options).await?;
        
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { .. } => {
                // Check the exit code by inspecting the exec
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                match self.client.inspect_exec(&exec.id).await {
                    Ok(exec_info) => {
                        if let Some(exit_code) = exec_info.exit_code {
                            Ok(exit_code == 0)
                        } else {
                            // If no exit code yet, assume it's still running or failed
                            Ok(false)
                        }
                    }
                    Err(_) => Ok(false),
                }
            }
            StartExecResults::Detached => Ok(false),
        }
    }

    /// Sanitize path to prevent directory traversal attacks
    fn sanitize_path(&self, path: &str) -> anyhow::Result<String> {
        let mut clean_path = path.trim().to_string();
        
        // Remove any null bytes or control characters first
        clean_path = clean_path.chars()
            .filter(|c| !c.is_control() && *c != '\0')
            .collect();
        
        // Check for dangerous patterns that could lead to directory traversal
        let dangerous_patterns = [
            "..", "~", "$", "`", "|", "&", ";", "(", ")", "{", "}", "[", "]",
            "\\", "/../", "/./", "../", "./", "..\\", ".\\",
        ];
        
        for pattern in &dangerous_patterns {
            if clean_path.contains(pattern) {
                return Err(anyhow::anyhow!("Path contains forbidden pattern: {}", pattern));
            }
        }
        
        // Normalize path separators and remove duplicates
        clean_path = clean_path.replace("\\", "/");
        while clean_path.contains("//") {
            clean_path = clean_path.replace("//", "/");
        }
        
        // Ensure path starts with /
        if !clean_path.starts_with('/') {
            clean_path = format!("/{}", clean_path);
        }
        
        // Remove trailing slash unless it's root
        if clean_path.len() > 1 && clean_path.ends_with('/') {
            clean_path.pop();
        }
        
        // Split path into components and validate each one
        let components: Vec<&str> = clean_path.split('/').collect();
        let mut safe_components = Vec::new();
        
        for component in components {
            if component.is_empty() {
                continue; // Skip empty components (from leading slash)
            }
            
            // Reject dangerous component names
            if component == "." || component == ".." {
                return Err(anyhow::anyhow!("Path component not allowed: {}", component));
            }
            
            // Reject hidden files that start with dot (except legitimate file extensions)
            if component.starts_with('.') && component.len() > 1 && !component[1..].contains('.') {
                return Err(anyhow::anyhow!("Hidden files not allowed: {}", component));
            }
            
            // Allow alphanumeric, dash, underscore, and dot for file extensions
            // Also allow common safe characters for filenames
            if !component.chars().all(|c| {
                c.is_alphanumeric() || 
                c == '-' || c == '_' || c == '.' || 
                c == ' ' || c == '+' || c == '@'
            }) {
                return Err(anyhow::anyhow!("Path component contains invalid characters: {}", component));
            }
            
            // Reject component names that are too long
            if component.len() > 255 {
                return Err(anyhow::anyhow!("Path component too long: {}", component));
            }
            
            safe_components.push(component);
        }
        
        // Reconstruct the safe path
        let safe_path = if safe_components.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", safe_components.join("/"))
        };
        
        // Final length check
        if safe_path.len() > 4096 {
            return Err(anyhow::anyhow!("Path too long"));
        }
        
        // Convert API paths to actual container paths
        // "/" maps to "/home/container" (the container's data directory)
        // "/something" maps to "/home/container/something"
        let final_path = if safe_path == "/" {
            "/home/container".to_string()
        } else {
            format!("/home/container{}", safe_path)
        };
        
        Ok(final_path)
    }

    /// Parse ls -la output into FileInfo structs
    fn parse_ls_output(&self, output: &str, path: &str) -> anyhow::Result<DirectoryListing> {
        let mut files = Vec::new();
        let mut total_files = 0;
        let mut total_directories = 0;
        
        for line in output.lines() {
            if line.trim().is_empty() || line.starts_with("total ") {
                continue;
            }
            
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 9 {
                continue;
            }
            
            let permissions = parts[0].to_string();
            let owner = Some(parts[2].to_string());
            let group = Some(parts[3].to_string());
            let size_str = parts[4];
            let modified = Some(format!("{} {}", parts[5], parts[6]));
            let name = parts[8..].join(" ");
            
            // Skip . and .. entries
            if name == "." || name == ".." {
                continue;
            }
            
            let is_directory = permissions.starts_with('d');
            let size = if is_directory {
                None
            } else {
                size_str.parse().ok()
            };
            
            let file_path = if path.ends_with('/') {
                format!("{}{}", path, name)
            } else {
                format!("{}/{}", path, name)
            };
            
            if is_directory {
                total_directories += 1;
            } else {
                total_files += 1;
            }
            
            files.push(FileInfo {
                name,
                path: file_path,
                is_directory,
                size,
                permissions,
                modified,
                owner,
                group,
            });
        }
        
        Ok(DirectoryListing {
            path: path.to_string(),
            files,
            total_files,
            total_directories,
        })
    }
}