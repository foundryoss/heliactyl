use std::collections::HashMap;
use std::net::{TcpListener, SocketAddr};
use std::sync::Arc;
use tracing::info;
use serde::{Serialize, Deserialize};
use crate::network_config::NetworkConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortAllocation {
    pub container_port: String,
    pub host_port: String,
    pub host_ip: String,
    pub protocol: String,
}

/// Network and port management for containers
pub struct NetworkManager {
    allocated_ports: HashMap<u16, String>, // port -> container_id
    container_ports: HashMap<String, Vec<PortAllocation>>, // container_id -> port allocations
    port_range: (u16, u16), // (start, end) port range for allocation
    network_config: Arc<NetworkConfig>,
}

impl NetworkManager {
    pub fn new(start_port: u16, end_port: u16) -> Self {
        Self {
            allocated_ports: HashMap::new(),
            container_ports: HashMap::new(),
            port_range: (start_port, end_port),
            network_config: Arc::new(NetworkConfig::default()),
        }
    }

    pub fn with_config(network_config: Arc<NetworkConfig>) -> Self {
        Self {
            allocated_ports: HashMap::new(),
            container_ports: HashMap::new(),
            port_range: (network_config.ports.range_start, network_config.ports.range_end),
            network_config,
        }
    }

    /// Find an available port from the available_ports array first, then fallback to range
    pub fn find_available_port(&mut self) -> Option<u16> {
        // First try to use ports from the available_ports array
        let available_ports = self.network_config.get_available_ports();
        for &port in available_ports {
            if !self.allocated_ports.contains_key(&port) && self.is_port_available(port) {
                return Some(port);
            }
        }

        // Fallback to range-based allocation if no ports in array are available
        for port in self.port_range.0..=self.port_range.1 {
            if !self.allocated_ports.contains_key(&port) && self.is_port_available(port) {
                return Some(port);
            }
        }
        
        None
    }

    /// Check if a specific port is available on the system
    pub fn is_port_available(&self, port: u16) -> bool {
        match TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    /// Allocate a specific port to a container
    pub fn allocate_port(&mut self, port: u16, container_id: String) -> Result<(), String> {
        if self.allocated_ports.contains_key(&port) {
            return Err(format!("Port {} already allocated to container {}", port, self.allocated_ports[&port]));
        }

        if !self.is_port_available(port) {
            return Err(format!("Port {} is not available on the system", port));
        }

        self.allocated_ports.insert(port, container_id.clone());
        info!("Allocated port {} to container {}", port, container_id);
        Ok(())
    }

    /// Release a port from a container
    pub fn release_port(&mut self, port: u16) -> Option<String> {
        if let Some(container_id) = self.allocated_ports.remove(&port) {
            info!("Released port {} from container {}", port, container_id);
            Some(container_id)
        } else {
            None
        }
    }

    /// Release all ports for a container
    pub fn release_container_ports(&mut self, container_id: &str) {
        // Remove from container_ports tracking
        if let Some(allocations) = self.container_ports.remove(container_id) {
            for allocation in allocations {
                if let Ok(port) = allocation.host_port.parse::<u16>() {
                    self.allocated_ports.remove(&port);
                    info!("Released port {} for container {}", port, container_id);
                }
            }
        }

        // Also clean up any ports allocated to this container in the legacy way
        let ports_to_remove: Vec<u16> = self.allocated_ports
            .iter()
            .filter(|(_, id)| *id == container_id)
            .map(|(port, _)| *port)
            .collect();

        for port in ports_to_remove {
            self.release_port(port);
        }
    }

    /// Get all allocated ports
    pub fn get_allocated_ports(&self) -> &HashMap<u16, String> {
        &self.allocated_ports
    }

    /// Auto-allocate ports with host IP configuration support
    pub fn auto_allocate_ports_with_config(
        &mut self,
        container_id: &str,
        port_configs: &[crate::models::PortConfig],
    ) -> Result<Vec<PortAllocation>, String> {
        let mut allocations = Vec::new();

        for config in port_configs {
            let host_ip = config.host_ip.as_deref().unwrap_or("0.0.0.0").to_string();
            let protocol = config.protocol.as_deref().unwrap_or("tcp").to_string();

            let host_port = if let Some(ref port_str) = config.host_port {
                if port_str == "auto" || port_str.is_empty() {
                    // Auto-allocate a port
                    match self.find_available_port() {
                        Some(port) => {
                            self.allocate_port(port, container_id.to_string())?;
                            port.to_string()
                        }
                        None => {
                            return Err("No available ports in the configured range".to_string());
                        }
                    }
                } else {
                    // Use specific port
                    let port: u16 = port_str.parse()
                        .map_err(|_| format!("Invalid port number: {}", port_str))?;
                    
                    self.allocate_port(port, container_id.to_string())?;
                    port.to_string()
                }
            } else {
                // Auto-allocate if no host port specified
                match self.find_available_port() {
                    Some(port) => {
                        self.allocate_port(port, container_id.to_string())?;
                        port.to_string()
                    }
                    None => {
                        return Err("No available ports in the configured range".to_string());
                    }
                }
            };

            let allocation = PortAllocation {
                container_port: config.container_port.clone(),
                host_port,
                host_ip,
                protocol,
            };

            allocations.push(allocation);
        }

        // Store allocations for this container
        self.container_ports.insert(container_id.to_string(), allocations.clone());
        
        info!("Allocated ports for container '{}': {:?}", container_id, allocations);
        Ok(allocations)
    }

    /// Auto-allocate ports for a container based on requested mappings (legacy method)
    pub fn auto_allocate_ports(&mut self, container_id: &str, requested_ports: &HashMap<String, String>) -> Result<HashMap<String, String>, String> {
        let mut allocated_mappings = HashMap::new();

        for (container_port, host_port) in requested_ports {
            let final_host_port = if host_port.is_empty() || host_port == "auto" {
                // Auto-allocate a port
                match self.find_available_port() {
                    Some(port) => {
                        self.allocate_port(port, container_id.to_string())?;
                        port.to_string()
                    }
                    None => {
                        return Err("No available ports in the configured range".to_string());
                    }
                }
            } else {
                // Use specific port
                let port: u16 = host_port.parse()
                    .map_err(|_| format!("Invalid port number: {}", host_port))?;
                
                self.allocate_port(port, container_id.to_string())?;
                port.to_string()
            };

            allocated_mappings.insert(container_port.clone(), final_host_port);
        }

        Ok(allocated_mappings)
    }

    /// Get ports for a specific container
    pub fn get_container_ports(&self, container_id: &str) -> Option<&Vec<PortAllocation>> {
        self.container_ports.get(container_id)
    }

    /// Check if a specific port is available
    pub fn check_port_availability(&self, port: u16) -> bool {
        !self.allocated_ports.contains_key(&port) && self.is_port_available(port)
    }
}