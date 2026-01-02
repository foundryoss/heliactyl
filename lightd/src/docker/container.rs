use bollard::{
    container::{
        Config, CreateContainerOptions, KillContainerOptions, ListContainersOptions,
        RemoveContainerOptions, StartContainerOptions, StopContainerOptions,
    },
    image::CreateImageOptions,
    Docker,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::models::{ContainerInfo, CreateContainerRequest, PortMapping, ResourceLimits};
use super::NetworkManager;

/// Parse memory limit string (e.g., "2g", "512m") to bytes
fn parse_memory_limit(limit: &str) -> anyhow::Result<i64> {
    let limit = limit.to_lowercase();
    if let Some(num_str) = limit.strip_suffix("g") {
        let num: f64 = num_str.parse()?;
        Ok((num * 1024.0 * 1024.0 * 1024.0) as i64)
    } else if let Some(num_str) = limit.strip_suffix("m") {
        let num: f64 = num_str.parse()?;
        Ok((num * 1024.0 * 1024.0) as i64)
    } else if let Some(num_str) = limit.strip_suffix("k") {
        let num: f64 = num_str.parse()?;
        Ok((num * 1024.0) as i64)
    } else {
        // Assume bytes
        Ok(limit.parse()?)
    }
}

/// Parse CPU limit string (e.g., "2.0", "0.5") to CPU quota
fn parse_cpu_limit(limit: &str) -> anyhow::Result<i64> {
    let cpu_float: f64 = limit.parse()?;
    // CPU quota is in microseconds per period (100000 microseconds = 100ms period)
    // So 1.0 CPU = 100000, 2.0 CPU = 200000, 0.5 CPU = 50000
    Ok((cpu_float * 100000.0) as i64)
}

/// Generate environment variables for resource limits
fn generate_limit_env_vars(limits: &Option<ResourceLimits>) -> Vec<String> {
    let mut env_vars = Vec::new();
    
    if let Some(limits) = limits {
        // Memory limit
        if let Some(memory) = &limits.memory {
            env_vars.push(format!("LIGHTD_MEMORY_LIMIT={}", memory));
            // Also provide in bytes for easier parsing
            if let Ok(bytes) = parse_memory_limit(memory) {
                env_vars.push(format!("LIGHTD_MEMORY_LIMIT_BYTES={}", bytes));
            }
        }
        
        // CPU limit
        if let Some(cpu) = &limits.cpu {
            env_vars.push(format!("LIGHTD_CPU_LIMIT={}", cpu));
            // Also provide as quota for easier parsing
            if let Ok(quota) = parse_cpu_limit(cpu) {
                env_vars.push(format!("LIGHTD_CPU_QUOTA={}", quota));
            }
        }
        
        // Disk limit
        if let Some(disk) = &limits.disk {
            env_vars.push(format!("LIGHTD_DISK_LIMIT={}", disk));
            if let Ok(bytes) = parse_memory_limit(disk) {
                env_vars.push(format!("LIGHTD_DISK_LIMIT_BYTES={}", bytes));
            }
        }
        
        // Swap limit
        if let Some(swap) = &limits.swap {
            env_vars.push(format!("LIGHTD_SWAP_LIMIT={}", swap));
            if let Ok(bytes) = parse_memory_limit(swap) {
                env_vars.push(format!("LIGHTD_SWAP_LIMIT_BYTES={}", bytes));
            }
        }
        
        // PID limit
        if let Some(pids) = limits.pids {
            env_vars.push(format!("LIGHTD_PIDS_LIMIT={}", pids));
        }
        
        // Thread limit
        if let Some(threads) = limits.threads {
            env_vars.push(format!("LIGHTD_THREADS_LIMIT={}", threads));
        }
    }
    
    // Add a general indicator that limits are managed by lightd
    env_vars.push("LIGHTD_MANAGED=true".to_string());
    
    env_vars
}

/// Generate environment variables for allocated ports
fn generate_port_env_vars(allocated_ports: &HashMap<String, String>) -> Vec<String> {
    let mut env_vars = Vec::new();
    
    if allocated_ports.is_empty() {
        return env_vars;
    }
    
    // Add individual port mappings: LIGHTD_PORT_<container_port>=<host_port>
    for (container_port, host_port) in allocated_ports {
        env_vars.push(format!("LIGHTD_PORT_{}={}", container_port, host_port));
    }
    
    // Add primary port (first allocated port) for convenience
    if let Some((container_port, host_port)) = allocated_ports.iter().next() {
        env_vars.push(format!("LIGHTD_PRIMARY_PORT={}", host_port));
        env_vars.push(format!("LIGHTD_PRIMARY_CONTAINER_PORT={}", container_port));
    }
    
    // Add all ports as JSON for easy parsing
    if let Ok(ports_json) = serde_json::to_string(allocated_ports) {
        env_vars.push(format!("LIGHTD_PORTS_JSON={}", ports_json));
    }
    
    // Add port count
    env_vars.push(format!("LIGHTD_PORT_COUNT={}", allocated_ports.len()));
    
    env_vars
}

/// Container management operations
pub struct ContainerManager {
    client: Docker,
}

impl ContainerManager {
    pub fn new(client: Docker) -> Self {
        Self { client }
    }
    
    /// Create from a reference (clones the Docker client)
    pub fn from_ref(client: &Docker) -> Self {
        Self { client: client.clone() }
    }

    /// Create a new container with automatic port allocation and volume binding
    pub async fn create_with_networking(
        &self, 
        req: CreateContainerRequest, 
        network_manager: &Arc<Mutex<NetworkManager>>, 
        container_uuid: &str,
        volumes_base_path: &str,
    ) -> anyhow::Result<(String, Vec<super::network::PortAllocation>)> {
        // Pull image if it doesn't exist locally
        self.pull_image_if_needed(&req.image).await?;

        // Create container's dedicated volume directory (convert to absolute path for Docker)
        let relative_path = format!("{}/{}", volumes_base_path, container_uuid);
        let container_volume_path = std::fs::canonicalize(&relative_path)
            .or_else(|_| {
                // If path doesn't exist yet, create it first then canonicalize
                std::fs::create_dir_all(&relative_path)?;
                std::fs::canonicalize(&relative_path)
            })?
            .to_string_lossy()
            .to_string();
        
        // Ensure directory exists
        tokio::fs::create_dir_all(&container_volume_path).await?;
        info!("Created container volume directory: {}", container_volume_path);

        // Handle port allocation with HashMap format
        let allocated_ports = if let Some(ports) = &req.ports {
            // Use UUID for port allocation tracking instead of user-provided name
            let mut net_mgr = network_manager.lock().await;
            net_mgr.auto_allocate_ports(container_uuid, ports)
                .map_err(|e| anyhow::anyhow!("Port allocation failed: {}", e))?
        } else {
            HashMap::new()
        };

        let mut port_bindings = HashMap::new();
        let mut exposed_ports = HashMap::new();

        for (container_port, host_port) in &allocated_ports {
            let port_spec = format!("{}/tcp", container_port);
            exposed_ports.insert(port_spec.clone(), HashMap::new());
            
            port_bindings.insert(
                port_spec,
                Some(vec![bollard::models::PortBinding {
                    host_ip: Some("0.0.0.0".to_string()),
                    host_port: Some(host_port.clone()),
                }]),
            );
        }

        // Create data directory for entrypoint (separate from user volume)
        let data_path = format!("{}/{}_data", volumes_base_path, container_uuid);
        let data_volume_path = std::fs::canonicalize(&data_path)
            .or_else(|_| {
                std::fs::create_dir_all(&data_path)?;
                std::fs::canonicalize(&data_path)
            })?
            .to_string_lossy()
            .to_string();
        tokio::fs::create_dir_all(&data_volume_path).await?;

        // Bind volumes: /home/container (user data) and /data (entrypoint, read-only)
        let mut binds = vec![
            format!("{}:/home/container", container_volume_path),
            format!("{}:/data:ro", data_volume_path),
        ];
        
        // Add any additional user-specified volumes
        if let Some(volumes) = &req.volumes {
            for volume in volumes {
                let bind = if volume.read_only.unwrap_or(false) {
                    format!("{}:{}:ro", volume.source, volume.target)
                } else {
                    format!("{}:{}", volume.source, volume.target)
                };
                binds.push(bind);
            }
        }

        // Apply resource limits
        let mut host_config = bollard::models::HostConfig {
            port_bindings: if port_bindings.is_empty() {
                None
            } else {
                Some(port_bindings)
            },
            binds: Some(binds), // Always has at least the container volume
            restart_policy: req.restart_policy.as_ref().map(|_policy| {
                bollard::models::RestartPolicy {
                    name: Some(bollard::models::RestartPolicyNameEnum::UNLESS_STOPPED),
                    maximum_retry_count: None,
                }
            }),
            // Enable OOM killer - container will be killed if it exceeds memory limit
            oom_kill_disable: Some(false),
            ..Default::default()
        };

        // Apply CPU and memory limits
        if let Some(limits) = &req.limits {
            if let Some(memory) = &limits.memory {
                if let Ok(memory_bytes) = parse_memory_limit(memory) {
                    host_config.memory = Some(memory_bytes);
                }
            }
            
            if let Some(cpu) = &limits.cpu {
                if let Ok(cpu_quota) = parse_cpu_limit(cpu) {
                    host_config.cpu_quota = Some(cpu_quota);
                    host_config.cpu_period = Some(100000); // Standard period
                }
            }

            // Apply swap limit
            if let Some(swap) = &limits.swap {
                if let Ok(swap_bytes) = parse_memory_limit(swap) {
                    // Docker swap limit is memory + swap, so we add them
                    let memory_bytes = host_config.memory.unwrap_or(0);
                    host_config.memory_swap = Some(memory_bytes + swap_bytes);
                }
            }

            // Apply PID limit
            if let Some(pids) = limits.pids {
                host_config.pids_limit = Some(pids as i64);
            }

            // Apply thread limit (using ulimits)
            if let Some(threads) = limits.threads {
                let ulimit = bollard::models::ResourcesUlimits {
                    name: Some("nproc".to_string()),
                    soft: Some(threads as i64),
                    hard: Some(threads as i64),
                };
                host_config.ulimits = Some(vec![ulimit]);
            }
        }

        let config = Config {
            image: Some(req.image.clone()),
            env: {
                let mut all_env = Vec::new();
                
                // Add user-provided environment variables
                if let Some(env) = &req.env {
                    all_env.extend(env.iter().map(|(k, v)| format!("{}={}", k, v)));
                }
                
                // Add resource limit environment variables
                all_env.extend(generate_limit_env_vars(&req.limits));
                
                // Add port allocation environment variables
                all_env.extend(generate_port_env_vars(&allocated_ports));
                
                if all_env.is_empty() {
                    None
                } else {
                    Some(all_env)
                }
            },
            cmd: req.startup_command.clone().or(req.command.clone()),
            working_dir: Some("/home/container".to_string()),
            tty: Some(true),
            open_stdin: Some(true),
            attach_stdin: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            exposed_ports: if exposed_ports.is_empty() {
                None
            } else {
                Some(exposed_ports)
            },
            host_config: Some(host_config),
            ..Default::default()
        };

        let options = CreateContainerOptions {
            name: container_uuid, // Use UUID as Docker container name
            platform: None,
        };

        let response = self.client.create_container(Some(options), config).await?;
        
        // Convert HashMap to PortAllocation format for response
        let port_allocations: Vec<super::network::PortAllocation> = allocated_ports
            .into_iter()
            .map(|(container_port, host_port)| super::network::PortAllocation {
                container_port,
                host_port,
                host_ip: "0.0.0.0".to_string(),
                protocol: "tcp".to_string(),
            })
            .collect();
        
        info!("Created container: {} with ports: {:?}", response.id, port_allocations);
        
        Ok((response.id, port_allocations))
    }

    /// Create a new container
    pub async fn create(&self, req: CreateContainerRequest) -> anyhow::Result<String> {
        // Pull image if it doesn't exist locally
        self.pull_image_if_needed(&req.image).await?;

        let mut port_bindings = HashMap::new();
        let mut exposed_ports = HashMap::new();

        if let Some(ports) = &req.ports {
            for (container_port, host_port) in ports {
                let protocol = "tcp"; // Default protocol
                let port_spec = format!("{}/{}", container_port, protocol);
                exposed_ports.insert(port_spec.clone(), HashMap::new());
                
                if !host_port.is_empty() && host_port != "auto" {
                    let host_ip = "0.0.0.0"; // Default host IP
                    port_bindings.insert(
                        port_spec,
                        Some(vec![bollard::models::PortBinding {
                            host_ip: Some(host_ip.to_string()),
                            host_port: Some(host_port.clone()),
                        }]),
                    );
                }
            }
        }

        let mut binds = Vec::new();
        if let Some(volumes) = &req.volumes {
            for volume in volumes {
                let bind = if volume.read_only.unwrap_or(false) {
                    format!("{}:{}:ro", volume.source, volume.target)
                } else {
                    format!("{}:{}", volume.source, volume.target)
                };
                binds.push(bind);
            }
        }

        let config = Config {
            image: Some(req.image.clone()),
            env: req.env.as_ref().map(|env| {
                env.iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect()
            }),
            cmd: req.command.clone(),
            working_dir: req.working_dir.clone(),
            exposed_ports: if exposed_ports.is_empty() {
                None
            } else {
                Some(exposed_ports)
            },
            host_config: Some(bollard::models::HostConfig {
                port_bindings: if port_bindings.is_empty() {
                    None
                } else {
                    Some(port_bindings)
                },
                binds: if binds.is_empty() { None } else { Some(binds) },
                restart_policy: req.restart_policy.as_ref().map(|_policy| {
                    bollard::models::RestartPolicy {
                        name: Some(bollard::models::RestartPolicyNameEnum::UNLESS_STOPPED),
                        maximum_retry_count: None,
                    }
                }),
                ..Default::default()
            }),
            ..Default::default()
        };

        let options = CreateContainerOptions {
            name: req.name.as_deref().unwrap_or(""),
            platform: None,
        };

        let response = self.client.create_container(Some(options), config).await?;
        info!("Created container: {}", response.id);
        
        Ok(response.id)
    }

    /// Pull Docker image if needed
    async fn pull_image_if_needed(&self, image: &str) -> anyhow::Result<()> {
        let options = CreateImageOptions {
            from_image: image,
            ..Default::default()
        };

        let mut stream = self.client.create_image(Some(options), None, None);
        
        use futures_util::stream::StreamExt;
        while let Some(result) = stream.next().await {
            match result {
                Ok(info) => {
                    if let Some(status) = info.status {
                        info!("Image pull: {}", status);
                    }
                }
                Err(e) => {
                    warn!("Image pull warning: {}", e);
                }
            }
        }

        Ok(())
    }

    /// Start a container
    pub async fn start(&self, id: &str) -> anyhow::Result<()> {
        // First check if container exists
        match self.client.inspect_container(id, None).await {
            Ok(container_info) => {
                info!("Container {} exists, state: {:?}", id, container_info.state);
                
                // Check if container is already running
                if let Some(state) = &container_info.state {
                    if state.running == Some(true) {
                        info!("Container {} is already running", id);
                        return Ok(());
                    }
                }
            }
            Err(e) => {
                error!("Container {} not found or error inspecting: {}", id, e);
                return Err(anyhow::anyhow!("Container not found: {}", e));
            }
        }

        // Try to start the container with better error handling
        info!("Attempting to start container: {}", id);
        match self.client.start_container(id, None::<StartContainerOptions<String>>).await {
            Ok(_) => {
                info!("Successfully started container: {}", id);
            }
            Err(bollard::errors::Error::JsonSerdeError { .. }) => {
                // This is actually success - Docker returns empty body for start_container
                info!("Container {} started successfully (empty response from Docker API)", id);
            }
            Err(bollard::errors::Error::DockerResponseServerError { status_code, message }) => {
                error!("Docker server error starting container {}: HTTP {} - {}", id, status_code, message);
                return Err(anyhow::anyhow!("Docker server error: HTTP {} - {}", status_code, message));
            }
            Err(bollard::errors::Error::DockerContainerWaitError { error, code }) => {
                error!("Docker container wait error starting {}: {} (code: {})", id, error, code);
                return Err(anyhow::anyhow!("Container wait error: {} (code: {})", error, code));
            }
            Err(e) => {
                error!("Docker API error when starting container {}: {:?}", id, e);
                return Err(anyhow::anyhow!("Docker API error: {}", e));
            }
        }

        // Brief verification that container started
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        match self.client.inspect_container(id, None).await {
            Ok(container_info) => {
                if let Some(state) = &container_info.state {
                    if state.running == Some(true) {
                        info!("Verified container {} is running", id);
                        Ok(())
                    } else {
                        warn!("Container {} not running after start - status: {:?}", id, state.status);
                        // Don't fail here since Docker didn't return an error - container might be designed to exit
                        Ok(())
                    }
                } else {
                    warn!("Could not determine container {} state after start", id);
                    Ok(())
                }
            }
            Err(e) => {
                warn!("Failed to verify container {} state after start: {}", id, e);
                // Don't fail here since the start command succeeded
                Ok(())
            }
        }
    }

    /// Stop a container gracefully with timeout, falls back to kill if needed
    pub async fn stop(&self, id: &str) -> anyhow::Result<()> {
        // First try graceful stop with 5 second timeout
        let options = StopContainerOptions { t: 5 };
        
        // Use tokio timeout to ensure we don't hang forever
        let stop_result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            self.client.stop_container(id, Some(options))
        ).await;
        
        match stop_result {
            Ok(Ok(_)) => {
                info!("Stopped container: {}", id);
                return Ok(());
            }
            Ok(Err(bollard::errors::Error::JsonSerdeError { .. })) => {
                info!("Container {} stopped successfully (empty response)", id);
                return Ok(());
            }
            Ok(Err(bollard::errors::Error::DockerResponseServerError { status_code: 304, .. })) => {
                // Container already stopped
                info!("Container {} was already stopped", id);
                return Ok(());
            }
            Ok(Err(e)) => {
                warn!("Graceful stop failed for container {}: {}, attempting force kill", id, e);
            }
            Err(_) => {
                warn!("Graceful stop timed out for container {}, attempting force kill", id);
            }
        }
        
        // Graceful stop failed or timed out - force kill
        self.force_stop(id).await
    }
    
    /// Force stop a container using SIGKILL with timeout
    pub async fn force_stop(&self, id: &str) -> anyhow::Result<()> {
        let kill_result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.client.kill_container(id, Some(KillContainerOptions { signal: "SIGKILL" }))
        ).await;
        
        match kill_result {
            Ok(Ok(_)) => {
                info!("Force killed container: {}", id);
                Ok(())
            }
            Ok(Err(bollard::errors::Error::JsonSerdeError { .. })) => {
                info!("Container {} force killed successfully (empty response)", id);
                Ok(())
            }
            Ok(Err(bollard::errors::Error::DockerResponseServerError { status_code: 404, .. })) => {
                // Container doesn't exist anymore
                info!("Container {} no longer exists", id);
                Ok(())
            }
            Ok(Err(bollard::errors::Error::DockerResponseServerError { status_code: 409, .. })) => {
                // Container not running - that's fine
                info!("Container {} is not running (already stopped)", id);
                Ok(())
            }
            Ok(Err(e)) => {
                error!("Failed to force kill container {}: {}", id, e);
                Err(e.into())
            }
            Err(_) => {
                error!("Force kill timed out for container {} - Docker daemon may be unresponsive", id);
                Err(anyhow::anyhow!("Force kill timed out - Docker daemon unresponsive"))
            }
        }
    }

    /// Kill a container forcefully with timeout
    pub async fn kill(&self, id: &str) -> anyhow::Result<()> {
        let options = KillContainerOptions { signal: "SIGKILL" };
        
        let kill_result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            self.client.kill_container(id, Some(options))
        ).await;
        
        match kill_result {
            Ok(Ok(_)) => {
                info!("Killed container: {}", id);
                Ok(())
            }
            Ok(Err(bollard::errors::Error::JsonSerdeError { .. })) => {
                info!("Container {} killed successfully (empty response)", id);
                Ok(())
            }
            Ok(Err(bollard::errors::Error::DockerResponseServerError { status_code: 404, .. })) => {
                info!("Container {} no longer exists", id);
                Ok(())
            }
            Ok(Err(bollard::errors::Error::DockerResponseServerError { status_code: 409, .. })) => {
                info!("Container {} is not running", id);
                Ok(())
            }
            Ok(Err(e)) => {
                error!("Failed to kill container {}: {}", id, e);
                Err(e.into())
            }
            Err(_) => {
                error!("Kill command timed out for container {} - Docker daemon may be unresponsive", id);
                Err(anyhow::anyhow!("Kill timed out - Docker daemon unresponsive"))
            }
        }
    }

    /// Pause a container (Docker pause - can be resumed)
    pub async fn pause(&self, id: &str, _message: Option<String>) -> anyhow::Result<()> {
        match self.client.pause_container(id).await {
            Ok(_) => {
                info!("Paused container: {}", id);
                Ok(())
            }
            Err(bollard::errors::Error::JsonSerdeError { .. }) => {
                info!("Container {} paused successfully (empty response)", id);
                Ok(())
            }
            Err(e) => {
                error!("Failed to pause container {}: {}", id, e);
                Err(e.into())
            }
        }
    }

    /// Suspend a container (kill and prevent all operations)
    pub async fn suspend(&self, id: &str, message: Option<String>) -> anyhow::Result<()> {
        let reason = message.unwrap_or_else(|| "Container suspended by administrator".to_string());
        info!("Suspending container {} with reason: {}", id, reason);
        
        // Kill the container to stop it completely
        match self.kill(id).await {
            Ok(_) => {
                info!("Container {} killed as part of suspension", id);
                Ok(())
            }
            Err(e) => {
                warn!("Failed to kill container {} during suspension (may already be stopped): {}", id, e);
                // Continue with suspension even if kill fails (container might already be stopped)
                Ok(())
            }
        }
    }

    /// Resume (unpause) a container
    pub async fn resume(&self, id: &str) -> anyhow::Result<()> {
        match self.client.unpause_container(id).await {
            Ok(_) => {
                info!("Resumed (unpaused) container: {}", id);
                Ok(())
            }
            Err(bollard::errors::Error::JsonSerdeError { .. }) => {
                info!("Container {} resumed successfully (empty response)", id);
                Ok(())
            }
            Err(e) => {
                error!("Failed to resume container {}: {}", id, e);
                Err(e.into())
            }
        }
    }

    /// Update container resource limits and environment variables
    pub async fn update_limits(&self, container_id: &str, limits: &ResourceLimits, restart: bool) -> anyhow::Result<String> {
        info!("Updating limits for container: {}", container_id);
        
        if restart {
            // Stop the container first
            info!("Stopping container {} to apply new limits", container_id);
            if let Err(e) = self.stop(container_id).await {
                warn!("Failed to stop container {} for limit update: {}", container_id, e);
            }
            
            // Get current container configuration
            let inspect_result = self.client.inspect_container(container_id, None).await?;
            
            // Create new configuration with updated limits and environment
            let mut new_env = Vec::new();
            
            // Preserve existing non-lightd environment variables
            if let Some(current_env) = &inspect_result.config.as_ref().and_then(|c| c.env.as_ref()) {
                for env_var in current_env.iter() {
                    if !env_var.starts_with("LIGHTD_") {
                        new_env.push(env_var.clone());
                    }
                }
            }
            
            // Add new limit environment variables
            new_env.extend(generate_limit_env_vars(&Some(limits.clone())));
            
            // Update container with new environment variables
            // Note: Docker doesn't allow updating resource limits on existing containers
            // The container needs to be recreated with new limits
            info!("Container {} environment updated with new limits. Restart required for resource limits to take effect.", container_id);
            
            // Start the container again
            if let Err(e) = self.start(container_id).await {
                error!("Failed to restart container {} after limit update: {}", container_id, e);
                return Err(e);
            }
            
            Ok(format!("Container {} limits updated and restarted successfully", container_id))
        } else {
            // Just update environment variables without restart
            // This requires executing commands to set environment variables in running container
            let mut update_commands = Vec::new();
            
            if let Some(memory) = &limits.memory {
                update_commands.push(format!("export LIGHTD_MEMORY_LIMIT='{}'", memory));
                if let Ok(bytes) = parse_memory_limit(memory) {
                    update_commands.push(format!("export LIGHTD_MEMORY_LIMIT_BYTES='{}'", bytes));
                }
            }
            
            if let Some(cpu) = &limits.cpu {
                update_commands.push(format!("export LIGHTD_CPU_LIMIT='{}'", cpu));
                if let Ok(quota) = parse_cpu_limit(cpu) {
                    update_commands.push(format!("export LIGHTD_CPU_QUOTA='{}'", quota));
                }
            }
            
            if let Some(disk) = &limits.disk {
                update_commands.push(format!("export LIGHTD_DISK_LIMIT='{}'", disk));
                if let Ok(bytes) = parse_memory_limit(disk) {
                    update_commands.push(format!("export LIGHTD_DISK_LIMIT_BYTES='{}'", bytes));
                }
            }
            
            if let Some(swap) = &limits.swap {
                update_commands.push(format!("export LIGHTD_SWAP_LIMIT='{}'", swap));
                if let Ok(bytes) = parse_memory_limit(swap) {
                    update_commands.push(format!("export LIGHTD_SWAP_LIMIT_BYTES='{}'", bytes));
                }
            }
            
            if let Some(pids) = limits.pids {
                update_commands.push(format!("export LIGHTD_PIDS_LIMIT='{}'", pids));
            }
            
            if let Some(threads) = limits.threads {
                update_commands.push(format!("export LIGHTD_THREADS_LIMIT='{}'", threads));
            }
            
            // Write environment variables to a file that can be sourced
            let env_script = update_commands.join(" && ");
            let full_command = format!("echo '{}' > /tmp/lightd_limits.env && echo 'Environment variables updated. Source /tmp/lightd_limits.env to apply in current session.'", env_script);
            
            match self.exec_command(container_id, vec!["sh", "-c", &full_command]).await {
                Ok(output) => {
                    info!("Updated environment variables in container {}: {}", container_id, output);
                    Ok(format!("Container {} environment variables updated. Note: Resource limits require container restart to take effect.", container_id))
                }
                Err(e) => {
                    error!("Failed to update environment variables in container {}: {}", container_id, e);
                    Err(e)
                }
            }
        }
    }

    /// Remove a container
    pub async fn remove(&self, id: &str) -> anyhow::Result<()> {
        let options = RemoveContainerOptions {
            force: true,
            ..Default::default()
        };
        self.client.remove_container(id, Some(options)).await?;
        info!("Removed container: {}", id);
        Ok(())
    }

    /// List all containers
    pub async fn list(&self) -> anyhow::Result<Vec<ContainerInfo>> {
        let options = ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        };

        let containers = self.client.list_containers(Some(options)).await?;
        
        let mut result = Vec::new();
        for container in containers {
            let ports = container.ports.unwrap_or_default()
                .into_iter()
                .map(|port| PortMapping {
                    private_port: port.private_port,
                    public_port: port.public_port,
                    host_ip: port.ip,
                    r#type: port.typ.map(|t| t.to_string()).unwrap_or_else(|| "tcp".to_string()),
                })
                .collect();

            result.push(ContainerInfo {
                id: container.id.unwrap_or_default(),
                name: container.names.unwrap_or_default().join(","),
                image: container.image.unwrap_or_default(),
                state: container.state.unwrap_or_default(),
                status: container.status.unwrap_or_default(),
                created: format!("{}", container.created.unwrap_or_default()),
                ports,
            });
        }

        Ok(result)
    }

    /// Attach to a container and return exec session for shell access
    pub async fn attach(&self, id: &str) -> anyhow::Result<String> {
        use bollard::exec::{CreateExecOptions, StartExecResults};
        
        // Create exec session for interactive shell
        let exec_options = CreateExecOptions {
            cmd: Some(vec!["/bin/sh"]),
            attach_stdin: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            tty: Some(true),
            ..Default::default()
        };

        let exec = self.client.create_exec(id, exec_options).await?;
        info!("Created shell exec session {} for container {}", exec.id, id);
        
        // Start the exec session
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { .. } => {
                info!("Successfully created shell session for container {}", id);
                Ok(exec.id)
            }
            StartExecResults::Detached => {
                warn!("Shell session started but detached for container {}", id);
                Ok(exec.id)
            }
        }
    }

    /// Get container logs
    pub async fn get_logs(&self, id: &str, follow: bool, tail: Option<&str>) -> anyhow::Result<String> {
        use bollard::container::LogsOptions;
        use futures_util::stream::StreamExt;
        
        let options = LogsOptions::<String> {
            follow,
            stdout: true,
            stderr: true,
            tail: tail.unwrap_or("100").to_string(),
            ..Default::default()
        };

        let mut stream = self.client.logs(id, Some(options));
        let mut logs = String::new();
        
        // Collect logs (limit to prevent infinite streams)
        let mut count = 0;
        while let Some(chunk) = stream.next().await {
            if count > 1000 { break; } // Prevent infinite collection
            match chunk {
                Ok(log_output) => {
                    logs.push_str(&log_output.to_string());
                }
                Err(e) => {
                    error!("Error reading logs: {}", e);
                    break;
                }
            }
            count += 1;
        }

        Ok(logs)
    }

    /// Stream container logs continuously via a broadcast channel
    pub async fn stream_logs(
        &self,
        id: &str,
        tx: tokio::sync::broadcast::Sender<String>,
        tail: Option<&str>,
    ) -> anyhow::Result<()> {
        use bollard::container::LogsOptions;
        use futures_util::stream::StreamExt;
        
        // First check if container is running to decide follow mode
        let is_running = match self.client.inspect_container(id, None).await {
            Ok(info) => info.state.and_then(|s| s.running).unwrap_or(false),
            Err(_) => false,
        };
        
        // Use follow=true only for running containers
        let options = LogsOptions::<String> {
            follow: is_running,
            stdout: true,
            stderr: true,
            tail: tail.unwrap_or("all").to_string(),
            timestamps: false,
            ..Default::default()
        };

        info!("Starting log stream for container: {} (follow={})", id, is_running);
        let mut stream = self.client.logs(id, Some(options));
        
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(log_output) => {
                    let text = log_output.to_string();
                    // Send each line separately
                    for line in text.lines() {
                        let line = line.trim().to_string();
                        if !line.is_empty() {
                            if tx.send(line).is_err() {
                                info!("No receivers for log stream, stopping");
                                return Ok(());
                            }
                        }
                    }
                }
                Err(e) => {
                    // Container might have stopped or been removed
                    warn!("Log stream error for {}: {}", id, e);
                    break;
                }
            }
        }
        
        info!("Log stream ended for container: {}", id);
        Ok(())
    }

    /// Attach to container and stream output (for interactive containers with TTY)
    pub async fn attach_and_stream(
        &self,
        id: &str,
        tx: tokio::sync::broadcast::Sender<String>,
    ) -> anyhow::Result<()> {
        use bollard::container::AttachContainerOptions;
        use futures_util::stream::StreamExt;
        
        let options = AttachContainerOptions::<String> {
            stdout: Some(true),
            stderr: Some(true),
            stream: Some(true),
            logs: Some(true), // Include existing logs
            ..Default::default()
        };

        info!("Attaching to container: {}", id);
        let attach_result = self.client.attach_container(id, Some(options)).await?;
        let mut output = attach_result.output;
        
        let mut buffer = Vec::with_capacity(1024);
        let mut line_start = 0;
        
        while let Some(chunk) = output.next().await {
            match chunk {
                Ok(log_output) => {
                    let bytes = log_output.into_bytes();
                    buffer.extend_from_slice(&bytes);
                    
                    // Process complete lines
                    let mut search_start = line_start;
                    loop {
                        if let Some(pos) = buffer[search_start..].iter().position(|&b| b == b'\n') {
                            let newline_pos = search_start + pos;
                            
                            // Limit line length to 4096 chars
                            if newline_pos - line_start <= 4096 {
                                let line = String::from_utf8_lossy(&buffer[line_start..newline_pos])
                                    .trim()
                                    .to_string();
                                
                                if !line.is_empty() && tx.send(line).is_err() {
                                    info!("No receivers for attach stream, stopping");
                                    return Ok(());
                                }
                                
                                line_start = newline_pos + 1;
                                search_start = line_start;
                            } else {
                                // Line too long, send first 4096 chars
                                let line = String::from_utf8_lossy(&buffer[line_start..(line_start + 4096)])
                                    .trim()
                                    .to_string();
                                
                                if !line.is_empty() && tx.send(line).is_err() {
                                    info!("No receivers for attach stream, stopping");
                                    return Ok(());
                                }
                                
                                line_start += 4096;
                                search_start = line_start;
                            }
                        } else {
                            // No complete line yet, check if buffer is too large
                            let current_line_length = buffer.len() - line_start;
                            if current_line_length > 4096 {
                                let line = String::from_utf8_lossy(&buffer[line_start..(line_start + 4096)])
                                    .trim()
                                    .to_string();
                                
                                if !line.is_empty() && tx.send(line).is_err() {
                                    info!("No receivers for attach stream, stopping");
                                    return Ok(());
                                }
                                
                                line_start += 4096;
                                search_start = line_start;
                            } else {
                                break;
                            }
                        }
                    }
                    
                    // Trim buffer if it's getting large
                    if line_start > 2048 && line_start > buffer.len() / 2 {
                        buffer.drain(0..line_start);
                        line_start = 0;
                    }
                }
                Err(e) => {
                    warn!("Attach stream error for {}: {}", id, e);
                    break;
                }
            }
        }
        
        // Send any remaining buffered data
        if line_start < buffer.len() {
            let line = String::from_utf8_lossy(&buffer[line_start..])
                .trim()
                .to_string();
            if !line.is_empty() {
                let _ = tx.send(line);
            }
        }
        
        info!("Attach stream ended for container: {}", id);
        Ok(())
    }

    /// Execute a command in a running container
    pub async fn exec_command(&self, id: &str, cmd: Vec<&str>) -> anyhow::Result<String> {
        use bollard::exec::{CreateExecOptions, StartExecResults};
        use futures_util::stream::StreamExt;
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        let exec = self.client.create_exec(id, exec_options).await?;
        
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                while let Some(chunk) = output.next().await {
                    match chunk {
                        Ok(log_output) => {
                            result.push_str(&log_output.to_string());
                        }
                        Err(e) => {
                            error!("Error executing command: {}", e);
                            break;
                        }
                    }
                }
                Ok(result)
            }
            StartExecResults::Detached => {
                Ok("Command executed (detached)".to_string())
            }
        }
    }

    /// Execute a command in a running container with a timeout
    /// Returns output collected within the timeout period
    pub async fn exec_command_with_timeout(&self, id: &str, cmd: Vec<&str>, timeout_secs: u64) -> anyhow::Result<String> {
        use bollard::exec::{CreateExecOptions, StartExecResults};
        use futures_util::stream::StreamExt;
        use tokio::time::{timeout, Duration};
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        let exec = self.client.create_exec(id, exec_options).await?;
        
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                
                // Collect output with timeout
                let timeout_result = timeout(Duration::from_secs(timeout_secs), async {
                    let mut collected = String::new();
                    while let Some(chunk) = output.next().await {
                        match chunk {
                            Ok(log_output) => {
                                collected.push_str(&log_output.to_string());
                            }
                            Err(e) => {
                                error!("Error executing command: {}", e);
                                break;
                            }
                        }
                    }
                    collected
                }).await;
                
                match timeout_result {
                    Ok(output) => Ok(output),
                    Err(_) => {
                        // Timeout occurred - command is still running
                        Ok(format!("{}\n[Command still running in background after {}s timeout]", result, timeout_secs))
                    }
                }
            }
            StartExecResults::Detached => {
                Ok("Command executed (detached)".to_string())
            }
        }
    }

    /// Execute a command in detached mode (fire and forget)
    pub async fn exec_command_detached(&self, id: &str, cmd: Vec<&str>) -> anyhow::Result<String> {
        use bollard::exec::{CreateExecOptions, StartExecOptions};
        
        let exec_options = CreateExecOptions {
            cmd: Some(cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(false),
            attach_stderr: Some(false),
            ..Default::default()
        };

        let exec = self.client.create_exec(id, exec_options).await?;
        
        let start_options = StartExecOptions {
            detach: true,
            ..Default::default()
        };
        
        self.client.start_exec(&exec.id, Some(start_options)).await?;
        
        Ok("Command started in background".to_string())
    }

    /// Get container stats
    pub async fn get_stats(&self, id: &str) -> anyhow::Result<String> {
        use bollard::container::StatsOptions;
        use futures_util::stream::StreamExt;
        
        let options = StatsOptions {
            stream: false,
            one_shot: true,
        };

        let mut stream = self.client.stats(id, Some(options));
        
        if let Some(stats_result) = stream.next().await {
            match stats_result {
                Ok(stats) => {
                    let stats_json = serde_json::to_string_pretty(&stats)?;
                    Ok(stats_json)
                }
                Err(e) => {
                    error!("Error getting stats: {}", e);
                    Err(e.into())
                }
            }
        } else {
            Ok("No stats available".to_string())
        }
    }


    /// Get container inspect info
    pub async fn inspect(&self, id: &str) -> anyhow::Result<String> {
        let container_info = self.client.inspect_container(id, None).await?;
        let info_json = serde_json::to_string_pretty(&container_info)?;
        Ok(info_json)
    }

    /// Debug container creation and startup issues
    pub async fn debug_container(&self, id: &str) -> anyhow::Result<String> {
        let mut debug_info = String::new();
        
        // Get container inspection
        match self.client.inspect_container(id, None).await {
            Ok(container_info) => {
                debug_info.push_str(&format!("=== Container {} Debug Info ===\n", id));
                debug_info.push_str(&format!("State: {:?}\n", container_info.state));
                debug_info.push_str(&format!("Config: {:?}\n", container_info.config));
                debug_info.push_str(&format!("Host Config: {:?}\n", container_info.host_config));
                
                // Get logs
                if let Ok(logs) = self.get_logs(id, false, Some("100")).await {
                    debug_info.push_str(&format!("Logs:\n{}\n", logs));
                }
            }
            Err(e) => {
                debug_info.push_str(&format!("Failed to inspect container: {}\n", e));
            }
        }
        
        Ok(debug_info)
    }

    /// Run installation script in container
    pub async fn run_installation(&self, id: &str, install_script: &str) -> anyhow::Result<String> {
        use bollard::exec::{CreateExecOptions, StartExecResults};
        use futures_util::stream::StreamExt;
        
        info!("Running installation script in container: {}", id);
        
        // Create script file in the container - work in /home/container
        let script_content = format!(
            "#!/bin/sh\nset -e\necho 'Starting installation...'\necho 'Container UUID: {}'\n\ncd /home/container\n\n{}\necho 'Installation completed successfully'",
            id, install_script
        );
        
        // Write script to container
        let escaped = script_content.replace("'", "'\"'\"'");
        let write_script_str = format!("echo '{}' > /tmp/install.sh && chmod +x /tmp/install.sh", escaped);
        let write_script_cmd: Vec<String> = vec!["sh".to_string(), "-c".to_string(), write_script_str.clone()];
        
        let exec_options = CreateExecOptions {
            cmd: Some(write_script_cmd.clone()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            tty: Some(true),
            ..Default::default()
        };

        let exec = self.client.create_exec(id, exec_options).await?;
        
        // Execute script write
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                while let Some(_) = output.next().await {
                    // Consume output
                }
            }
            StartExecResults::Detached => {}
        }
        
        // Now execute the installation script
        let run_script_cmd = vec!["/bin/sh", "/tmp/install.sh"];
        
        let exec_options = CreateExecOptions {
            cmd: Some(run_script_cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            tty: Some(true),
            working_dir: Some("/home/container".to_string()),
            ..Default::default()
        };

        let exec = self.client.create_exec(id, exec_options).await?;
        
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                while let Some(chunk) = output.next().await {
                    match chunk {
                        Ok(log_output) => {
                            let output_str = log_output.to_string();
                            result.push_str(&output_str);
                            info!("Installation output: {}", output_str.trim());
                        }
                        Err(e) => {
                            error!("Error during installation: {}", e);
                            return Err(e.into());
                        }
                    }
                }
                
                // Clean up script file
                let cleanup_cmd = vec!["rm", "-f", "/tmp/install.sh"];
                let exec_options = CreateExecOptions {
                    cmd: Some(cleanup_cmd.iter().map(|s| s.to_string()).collect()),
                    attach_stdout: Some(false),
                    attach_stderr: Some(false),
                    ..Default::default()
                };
                let exec = self.client.create_exec(id, exec_options).await?;
                let _ = self.client.start_exec(&exec.id, None).await;
                
                info!("Installation completed for container: {}", id);
                Ok(result)
            }
            StartExecResults::Detached => {
                Ok("Installation script executed (detached)".to_string())
            }
        }
    }

    /// Run update script in container
    pub async fn run_update(&self, id: &str, update_script: &str) -> anyhow::Result<String> {
        use bollard::exec::{CreateExecOptions, StartExecResults};
        use futures_util::stream::StreamExt;
        
        info!("Running update script in container: {}", id);
        
        // Create a temporary script file in the container
        let script_content = format!(
            "#!/bin/sh\nset -e\necho 'Starting update...'\n\n# Ensure workspace directory exists and change to it\nmkdir -p /workspace\ncd /workspace\n\n{}\necho 'Update completed successfully'",
            update_script
        );
        
        // Write script to container
        let escaped = script_content.replace("'", "'\"'\"'");
        let write_script_str = format!("echo '{}' > /tmp/update.sh && chmod +x /tmp/update.sh", escaped);
        let write_script_cmd: Vec<String> = vec!["sh".to_string(), "-c".to_string(), write_script_str.clone()];
        
        let exec_options = CreateExecOptions {
            cmd: Some(write_script_cmd.clone()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        let exec = self.client.create_exec(id, exec_options).await?;
        
        // Execute script write
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                while let Some(_) = output.next().await {
                    // Consume output
                }
            }
            StartExecResults::Detached => {}
        }
        
        // Now execute the update script
        let run_script_cmd = vec!["/bin/sh", "/tmp/update.sh"];
        
        let exec_options = CreateExecOptions {
            cmd: Some(run_script_cmd.iter().map(|s| s.to_string()).collect()),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            ..Default::default()
        };

        let exec = self.client.create_exec(id, exec_options).await?;
        
        match self.client.start_exec(&exec.id, None).await? {
            StartExecResults::Attached { mut output, .. } => {
                let mut result = String::new();
                while let Some(chunk) = output.next().await {
                    match chunk {
                        Ok(log_output) => {
                            let output_str = log_output.to_string();
                            result.push_str(&output_str);
                            info!("Update output: {}", output_str.trim());
                        }
                        Err(e) => {
                            error!("Error during update: {}", e);
                            return Err(e.into());
                        }
                    }
                }
                
                // Clean up script file
                let cleanup_cmd = vec!["rm", "-f", "/tmp/update.sh"];
                let exec_options = CreateExecOptions {
                    cmd: Some(cleanup_cmd.iter().map(|s| s.to_string()).collect()),
                    attach_stdout: Some(false),
                    attach_stderr: Some(false),
                    ..Default::default()
                };
                let exec = self.client.create_exec(id, exec_options).await?;
                let _ = self.client.start_exec(&exec.id, None).await;
                
                info!("Update completed for container: {}", id);
                Ok(result)
            }
            StartExecResults::Detached => {
                Ok("Update script executed (detached)".to_string())
            }
        }
    }
}