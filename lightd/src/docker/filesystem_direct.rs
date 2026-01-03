use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use std::io::{Read, Write};
use tracing::info;

#[derive(Debug, Serialize, Deserialize)]
pub struct FileInfo {
    pub name: String,
    pub path: String,
    #[serde(rename = "isDirectory")]
    pub is_directory: bool,
    pub size: u64,
    #[serde(rename = "modifiedAt")]
    pub modified: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DirectoryListing {
    pub path: String,
    #[serde(rename = "items")]
    pub files: Vec<FileInfo>,
    #[serde(rename = "totalFiles")]
    pub total_files: usize,
    #[serde(rename = "totalDirectories")]
    pub total_directories: usize,
}

pub struct FilesystemManagerDirect {
    volumes_base_path: String,
}

impl FilesystemManagerDirect {
    pub fn new(volumes_base_path: String) -> Self {
        Self { volumes_base_path }
    }

    /// Get the host path for a container's volume
    /// /home/container is mounted to {volumes_path}/{container_id} (user data)
    /// /app/data is mounted to {volumes_path}/{container_id}_data (entrypoint, read-only)
    /// We only work with /home/container mount
    fn get_volume_path(&self, container_id: &str, container_path: &str) -> anyhow::Result<PathBuf> {
        // Container's /home/container is stored at {volumes_path}/{container_id} (NOT _data!)
        let volume_root = PathBuf::from(&self.volumes_base_path)
            .join(container_id);
        
        // Remove /home/container prefix if present to get relative path
        let relative_path = container_path
            .trim_start_matches("/home/container")
            .trim_start_matches('/');
        
        let full_path = if relative_path.is_empty() {
            volume_root.clone()
        } else {
            volume_root.join(relative_path)
        };
        
        // Security check: ensure the resolved path is within the volume root
        // For paths that don't exist yet, we need to check the parent directory
        let path_to_check = if full_path.exists() {
            full_path.clone()
        } else {
            // For non-existent paths, check the deepest existing parent
            let mut current = full_path.clone();
            while !current.exists() && current.parent().is_some() {
                current = current.parent().unwrap().to_path_buf();
            }
            current
        };
        
        // Canonicalize both paths for comparison
        let canonical = path_to_check.canonicalize().unwrap_or_else(|_| path_to_check.clone());
        let canonical_root = volume_root.canonicalize().unwrap_or(volume_root.clone());
        
        if !canonical.starts_with(&canonical_root) {
            return Err(anyhow::anyhow!("Path traversal attempt detected"));
        }
        
        Ok(full_path)
    }

    /// List files and directories
    pub fn list_directory(&self, container_id: &str, path: &str) -> anyhow::Result<DirectoryListing> {
        info!("Listing directory {} in container {}", path, container_id);
        
        let host_path = self.get_volume_path(container_id, path)?;
        
        if !host_path.exists() {
            return Err(anyhow::anyhow!("Path does not exist"));
        }
        
        if !host_path.is_dir() {
            return Err(anyhow::anyhow!("Path is not a directory"));
        }
        
        let mut files = Vec::new();
        let mut total_files = 0;
        let mut total_directories = 0;
        
        let entries = fs::read_dir(&host_path)?;
        
        for entry in entries {
            let entry = entry?;
            let metadata = entry.metadata()?;
            let file_name = entry.file_name().to_string_lossy().to_string();
            
            // Skip hidden files starting with .
            if file_name.starts_with('.') {
                continue;
            }
            
            let is_directory = metadata.is_dir();
            let size = if is_directory {
                0
            } else {
                metadata.len()
            };
            
            let modified = metadata.modified()
                .ok()
                .and_then(|time| {
                    use std::time::SystemTime;
                    time.duration_since(SystemTime::UNIX_EPOCH)
                        .ok()
                        .map(|d| {
                            let datetime = chrono::DateTime::<chrono::Utc>::from_timestamp(d.as_secs() as i64, 0);
                            datetime.map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                        })
                        .flatten()
                });
            
            // Normalize path to always show /home/container prefix
            // Strip the /home/container prefix from input path to get relative path
            let relative_input = path
                .trim_start_matches("/home/container")
                .trim_start_matches('/')
                .trim_end_matches('/');
            
            let normalized_path = if relative_input.is_empty() {
                // We're at root, so files are directly under /home/container
                format!("/home/container/{}", file_name)
            } else {
                // We're in a subdirectory
                format!("/home/container/{}/{}", relative_input, file_name)
            };
            
            if is_directory {
                total_directories += 1;
            } else {
                total_files += 1;
            }
            
            files.push(FileInfo {
                name: file_name,
                path: normalized_path,
                is_directory,
                size,
                modified,
            });
        }
        
        // Sort: directories first, then alphabetically
        files.sort_by(|a, b| {
            match (a.is_directory, b.is_directory) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            }
        });
        
        // Normalize the returned path to always show /home/container
        // Ensure the path always starts with /home/container
        let normalized_list_path = if path.is_empty() || path == "/" {
            "/home/container".to_string()
        } else if path.starts_with("/home/container") {
            path.to_string()
        } else {
            format!("/home/container/{}", path.trim_start_matches('/'))
        };
        
        Ok(DirectoryListing {
            path: normalized_list_path,
            files,
            total_files,
            total_directories,
        })
    }

    /// Read file content
    pub fn get_file_content(&self, container_id: &str, path: &str) -> anyhow::Result<String> {
        info!("Reading file {} in container {}", path, container_id);
        
        let host_path = self.get_volume_path(container_id, path)?;
        
        if !host_path.exists() {
            return Err(anyhow::anyhow!("File does not exist"));
        }
        
        if !host_path.is_file() {
            return Err(anyhow::anyhow!("Path is not a file"));
        }
        
        let mut file = fs::File::open(&host_path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        
        Ok(content)
    }

    /// Write file content
    pub fn write_file(&self, container_id: &str, path: &str, content: &str) -> anyhow::Result<()> {
        info!("Writing file {} in container {}", path, container_id);
        
        let host_path = self.get_volume_path(container_id, path)?;
        
        // Create parent directories if they don't exist
        if let Some(parent) = host_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let mut file = fs::File::create(&host_path)?;
        file.write_all(content.as_bytes())?;
        
        Ok(())
    }

    /// Write file content from bytes (for binary uploads)
    pub fn write_file_bytes(&self, container_id: &str, path: &str, content: &[u8]) -> anyhow::Result<()> {
        info!("Writing file (bytes) {} in container {}", path, container_id);
        
        let host_path = self.get_volume_path(container_id, path)?;
        
        // Create parent directories if they don't exist
        if let Some(parent) = host_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let mut file = fs::File::create(&host_path)?;
        file.write_all(content)?;
        
        Ok(())
    }

    /// Create directory
    pub fn create_directory(&self, container_id: &str, path: &str) -> anyhow::Result<()> {
        info!("Creating directory {} in container {}", path, container_id);
        
        let host_path = self.get_volume_path(container_id, path)?;
        
        fs::create_dir_all(&host_path)?;
        
        Ok(())
    }

    /// Delete file or directory
    pub fn delete(&self, container_id: &str, path: &str) -> anyhow::Result<()> {
        info!("Deleting {} in container {}", path, container_id);
        
        let host_path = self.get_volume_path(container_id, path)?;
        
        if !host_path.exists() {
            return Err(anyhow::anyhow!("Path does not exist"));
        }
        
        if host_path.is_dir() {
            fs::remove_dir_all(&host_path)?;
        } else {
            fs::remove_file(&host_path)?;
        }
        
        Ok(())
    }

    /// Rename/move file
    pub fn rename(&self, container_id: &str, old_path: &str, new_path: &str) -> anyhow::Result<()> {
        info!("Renaming {} to {} in container {}", old_path, new_path, container_id);
        
        let old_host_path = self.get_volume_path(container_id, old_path)?;
        let new_host_path = self.get_volume_path(container_id, new_path)?;
        
        if !old_host_path.exists() {
            return Err(anyhow::anyhow!("Source path does not exist"));
        }
        
        // Create parent directories for destination if needed
        if let Some(parent) = new_host_path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        fs::rename(&old_host_path, &new_host_path)?;
        
        Ok(())
    }

    /// Create tar archive from multiple sources
    pub fn create_archive(&self, container_id: &str, source_paths: &[String], archive_path: &str, compression: Option<&str>) -> anyhow::Result<String> {
        if source_paths.is_empty() {
            return Err(anyhow::anyhow!("No source paths provided"));
        }
        
        let archive_host = self.get_volume_path(container_id, archive_path)?;
        
        // Validate all source paths exist
        let source_hosts: Vec<PathBuf> = source_paths.iter()
            .map(|p| self.get_volume_path(container_id, p))
            .collect::<Result<Vec<_>, _>>()?;
        
        for (idx, source_host) in source_hosts.iter().enumerate() {
            if !source_host.exists() {
                return Err(anyhow::anyhow!("Source path does not exist: {}", source_paths[idx]));
            }
        }
        
        // Create parent directory for archive
        if let Some(parent) = archive_host.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let file = fs::File::create(&archive_host)?;
        
        // Apply compression wrapper based on type
        let encoder: Box<dyn Write> = match compression {
            Some("gzip") => {
                use flate2::write::GzEncoder;
                use flate2::Compression;
                Box::new(GzEncoder::new(file, Compression::default()))
            }
            Some("bzip2") => {
                use bzip2::write::BzEncoder;
                use bzip2::Compression;
                Box::new(BzEncoder::new(file, Compression::default()))
            }
            _ => Box::new(file),
        };
        
        let mut tar = tar::Builder::new(encoder);
        
        // Add each source to the archive
        for source_host in source_hosts {
            if source_host.is_dir() {
                let dir_name = source_host.file_name()
                    .ok_or_else(|| anyhow::anyhow!("Invalid directory name"))?;
                tar.append_dir_all(dir_name, &source_host)?;
            } else {
                let mut file = fs::File::open(&source_host)?;
                let name = source_host.file_name()
                    .ok_or_else(|| anyhow::anyhow!("Invalid source file name"))?;
                tar.append_file(name, &mut file)?;
            }
        }
        
        tar.finish()?;
        
        Ok(format!("Archive created: {} ({} items)", archive_path, source_paths.len()))
    }

    /// Extract tar archive
    pub fn extract_archive(&self, container_id: &str, archive_path: &str, dest_path: &str) -> anyhow::Result<String> {
        let archive_host = self.get_volume_path(container_id, archive_path)?;
        let dest_host = self.get_volume_path(container_id, dest_path)?;
        
        if !archive_host.exists() {
            return Err(anyhow::anyhow!("Archive does not exist"));
        }
        
        // Create destination directory
        fs::create_dir_all(&dest_host)?;
        
        let file = fs::File::open(&archive_host)?;
        
        // Try to detect compression by file extension
        let archive_name = archive_path.to_lowercase();
        
        if archive_name.ends_with(".tar.gz") || archive_name.ends_with(".tgz") {
            use flate2::read::GzDecoder;
            let decoder = GzDecoder::new(file);
            let mut archive = tar::Archive::new(decoder);
            archive.unpack(&dest_host)?;
        } else if archive_name.ends_with(".tar.bz2") || archive_name.ends_with(".tbz2") {
            use bzip2::read::BzDecoder;
            let decoder = BzDecoder::new(file);
            let mut archive = tar::Archive::new(decoder);
            archive.unpack(&dest_host)?;
        } else {
            // Assume uncompressed tar
            let mut archive = tar::Archive::new(file);
            archive.unpack(&dest_host)?;
        }
        
        Ok(format!("Archive extracted to: {}", dest_path))
    }

    /// Create zip archive from multiple sources
    pub fn create_zip(&self, container_id: &str, source_paths: &[String], zip_path: &str) -> anyhow::Result<String> {
        if source_paths.is_empty() {
            return Err(anyhow::anyhow!("No source paths provided"));
        }
        
        let zip_host = self.get_volume_path(container_id, zip_path)?;
        
        // Validate all source paths exist
        let source_hosts: Vec<PathBuf> = source_paths.iter()
            .map(|p| self.get_volume_path(container_id, p))
            .collect::<Result<Vec<_>, _>>()?;
        
        for (idx, source_host) in source_hosts.iter().enumerate() {
            if !source_host.exists() {
                return Err(anyhow::anyhow!("Source path does not exist: {}", source_paths[idx]));
            }
        }
        
        // Create parent directory for zip
        if let Some(parent) = zip_host.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let file = fs::File::create(&zip_host)?;
        let mut zip = zip::ZipWriter::new(file);
        
        let options = zip::write::FileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        
        // Add each source to the zip
        for source_host in source_hosts {
            if source_host.is_dir() {
                let dir_name = source_host.file_name()
                    .ok_or_else(|| anyhow::anyhow!("Invalid directory name"))?
                    .to_string_lossy();
                zip.add_directory(dir_name.as_ref(), options)?;
                self.zip_dir_with_prefix(&mut zip, &source_host, &source_host, &dir_name, &options)?;
            } else {
                let name = source_host.file_name()
                    .ok_or_else(|| anyhow::anyhow!("Invalid source file name"))?;
                zip.start_file(name.to_string_lossy(), options)?;
                let mut f = fs::File::open(&source_host)?;
                std::io::copy(&mut f, &mut zip)?;
            }
        }
        
        zip.finish()?;
        
        Ok(format!("Zip created: {} ({} items)", zip_path, source_paths.len()))
    }
    
    fn zip_dir_with_prefix(&self, zip: &mut zip::ZipWriter<fs::File>, base: &PathBuf, current: &PathBuf, prefix: &str, options: &zip::write::FileOptions) -> anyhow::Result<()> {
        for entry in fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(base)
                .map_err(|_| anyhow::anyhow!("Invalid path"))?;
            let full_path = format!("{}/{}", prefix, relative.to_string_lossy());
            
            if path.is_dir() {
                zip.add_directory(&full_path, *options)?;
                self.zip_dir_with_prefix(zip, base, &path, prefix, options)?;
            } else {
                zip.start_file(&full_path, *options)?;
                let mut f = fs::File::open(&path)?;
                std::io::copy(&mut f, zip)?;
            }
        }
        
        Ok(())
    }

    /// Extract zip archive
    pub fn extract_zip(&self, container_id: &str, zip_path: &str, dest_path: &str) -> anyhow::Result<String> {
        let zip_host = self.get_volume_path(container_id, zip_path)?;
        let dest_host = self.get_volume_path(container_id, dest_path)?;
        
        if !zip_host.exists() {
            return Err(anyhow::anyhow!("Zip file does not exist"));
        }
        
        // Create destination directory
        fs::create_dir_all(&dest_host)?;
        
        let file = fs::File::open(&zip_host)?;
        let mut archive = zip::ZipArchive::new(file)?;
        
        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let outpath = dest_host.join(file.name());
            
            if file.is_dir() {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    fs::create_dir_all(p)?;
                }
                let mut outfile = fs::File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }
        }
        
        Ok(format!("Zip extracted to: {}", dest_path))
    }

    /// Change file permissions (chmod)
    pub fn chmod(&self, container_id: &str, path: &str, permissions: &str) -> anyhow::Result<()> {
        use std::process::Command;
        
        let host_path = self.get_volume_path(container_id, path)?;
        
        if !host_path.exists() {
            return Err(anyhow::anyhow!("Path does not exist"));
        }
        
        let output = Command::new("chmod")
            .arg(permissions)
            .arg(&host_path)
            .output()?;
        
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to change permissions: {}", error));
        }
        
        Ok(())
    }

    /// Change file ownership (chown)
    pub fn chown(&self, container_id: &str, path: &str, owner: &str, group: Option<&str>) -> anyhow::Result<()> {
        use std::process::Command;
        
        let host_path = self.get_volume_path(container_id, path)?;
        
        if !host_path.exists() {
            return Err(anyhow::anyhow!("Path does not exist"));
        }
        
        let owner_group = if let Some(g) = group {
            format!("{}:{}", owner, g)
        } else {
            owner.to_string()
        };
        
        let output = Command::new("chown")
            .arg(&owner_group)
            .arg(&host_path)
            .output()?;
        
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Failed to change ownership: {}", error));
        }
        
        Ok(())
    }
}
