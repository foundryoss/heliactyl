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

    /// List files and directories in a container path
    pub async fn list_directory(&self, container_id: &str, path: &str) -> anyhow::Result<DirectoryListing> {
        info!("Listing directory {} in container {}", path, container_id);
        
        // Sanitize path - ensure it starts with / and doesn't have dangerous patterns
        let clean_path = self.sanitize_path(path);
        
        // Use ls -la to get detailed file information
        let cmd = vec!["ls", "-la", "--time-style=iso", &clean_path];
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/workspace".to_string()),
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
        
        let clean_path = self.sanitize_path(path);
        
        // Use stat to check if it's a file
        let stat_cmd = vec!["stat", "-c", "%F", &clean_path];
        
        let exec_options = CreateExecOptions {
            cmd: Some(stat_cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/workspace".to_string()),
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
            working_dir: Some("/workspace".to_string()),
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
        
        let clean_path = self.sanitize_path(path);
        
        // Escape content for shell
        let escaped_content = content.replace("'", "'\"'\"'");
        let write_cmd = format!("echo '{}' > '{}'", escaped_content, clean_path);
        
        let cmd = vec!["sh", "-c", &write_cmd];
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/workspace".to_string()),
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
        
        let clean_path = self.sanitize_path(path);
        let cmd = vec!["mkdir", "-p", &clean_path];
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/workspace".to_string()),
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
        
        let clean_path = self.sanitize_path(path);
        let cmd = vec!["rm", "-rf", &clean_path];
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            working_dir: Some("/workspace".to_string()),
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

    /// Check if path is a directory
    async fn is_directory(&self, container_id: &str, path: &str) -> anyhow::Result<bool> {
        let cmd = vec!["test", "-d", path];
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(false),
            attach_stderr: Some(false),
            working_dir: Some("/workspace".to_string()),
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
    fn sanitize_path(&self, path: &str) -> String {
        let mut clean_path = path.trim().to_string();
        
        // Remove dangerous patterns
        clean_path = clean_path.replace("..", "");
        clean_path = clean_path.replace("//", "/");
        
        // Ensure path starts with /
        if !clean_path.starts_with('/') {
            clean_path = format!("/{}", clean_path);
        }
        
        // Convert API paths to actual container paths
        // "/" maps to "/workspace" (the working directory)
        // "/something" maps to "/workspace/something"
        if clean_path == "/" {
            "/workspace".to_string()
        } else {
            format!("/workspace{}", clean_path)
        }
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