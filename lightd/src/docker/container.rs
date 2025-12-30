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

use crate::models::{ContainerInfo, CreateContainerRequest, PortMapping};
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

/// Container management operations
pub struct ContainerManager<'a> {
    client: &'a Docker,
}

impl<'a> ContainerManager<'a> {
    pub fn new(client: &'a Docker) -> Self {
        Self { client }
    }

    /// Create a new container with automatic port allocation
    pub async fn create_with_networking(&self, req: CreateContainerRequest, network_manager: &Arc<Mutex<NetworkManager>>, container_uuid: &str) -> anyhow::Result<(String, Vec<super::network::PortAllocation>)> {
        // Pull image if it doesn't exist locally
        self.pull_image_if_needed(&req.image).await?;

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

        // Apply resource limits
        let mut host_config = bollard::models::HostConfig {
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
            env: req.env.as_ref().map(|env| {
                env.iter()
                    .map(|(k, v)| format!("{}={}", k, v))
                    .collect()
            }),
            cmd: req.startup_command.clone().or(req.command.clone()),
            working_dir: req.working_dir.clone().or(Some("/workspace".to_string())),
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

    /// Stop a container gracefully
    pub async fn stop(&self, id: &str) -> anyhow::Result<()> {
        let options = StopContainerOptions { t: 10 };
        match self.client.stop_container(id, Some(options)).await {
            Ok(_) => {
                info!("Stopped container: {}", id);
                Ok(())
            }
            Err(bollard::errors::Error::JsonSerdeError { .. }) => {
                info!("Container {} stopped successfully (empty response)", id);
                Ok(())
            }
            Err(e) => {
                error!("Failed to stop container {}: {}", id, e);
                Err(e.into())
            }
        }
    }

    /// Kill a container forcefully
    pub async fn kill(&self, id: &str) -> anyhow::Result<()> {
        let options = KillContainerOptions { signal: "SIGKILL" };
        match self.client.kill_container(id, Some(options)).await {
            Ok(_) => {
                info!("Killed container: {}", id);
                Ok(())
            }
            Err(bollard::errors::Error::JsonSerdeError { .. }) => {
                info!("Container {} killed successfully (empty response)", id);
                Ok(())
            }
            Err(e) => {
                error!("Failed to kill container {}: {}", id, e);
                Err(e.into())
            }
        }
    }

    /// Suspend (pause) a container
    pub async fn suspend(&self, id: &str, _message: Option<String>) -> anyhow::Result<()> {
        match self.client.pause_container(id).await {
            Ok(_) => {
                info!("Suspended (paused) container: {}", id);
                Ok(())
            }
            Err(bollard::errors::Error::JsonSerdeError { .. }) => {
                info!("Container {} suspended successfully (empty response)", id);
                Ok(())
            }
            Err(e) => {
                error!("Failed to suspend container {}: {}", id, e);
                Err(e.into())
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
        
        // Create workspace directory and script file in the container
        let script_content = format!(
            "#!/bin/sh\nset -e\necho 'Starting installation...'\necho 'Container UUID: {}'\n\n# Create workspace directory\nmkdir -p /workspace\ncd /workspace\n\n{}\necho 'Installation completed successfully'",
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