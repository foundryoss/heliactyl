use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::{info, warn};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NetworkConfig {
    pub network: NetworkSettings,
    pub ports: PortSettings,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NetworkSettings {
    pub name: String,
    pub subnet: String,
    pub gateway: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PortSettings {
    pub available_ports: Vec<u16>,
    pub range_start: u16,
    pub range_end: u16,
    pub default_host_ip: String,
    pub reserved_ports: Vec<u16>,
}

impl NetworkConfig {
    pub async fn load(path: &str) -> anyhow::Result<Self> {
        match fs::read_to_string(path).await {
            Ok(content) => {
                let config: NetworkConfig = serde_json::from_str(&content)?;
                info!("Loaded network configuration from {}", path);
                Ok(config)
            }
            Err(_) => {
                warn!("Network config file not found, creating default config at {}", path);
                let default_config = Self::default();
                default_config.save(path).await?;
                Ok(default_config)
            }
        }
    }

    pub async fn save(&self, path: &str) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content).await?;
        info!("Saved network configuration to {}", path);
        Ok(())
    }

    pub fn is_port_reserved(&self, port: u16) -> bool {
        self.ports.reserved_ports.contains(&port)
    }

    pub fn get_available_ports(&self) -> &Vec<u16> {
        &self.ports.available_ports
    }

    pub async fn allocate_port(&mut self, port: u16) -> anyhow::Result<()> {
        if let Some(index) = self.ports.available_ports.iter().position(|&p| p == port) {
            self.ports.available_ports.remove(index);
            self.save("network.json").await?;
            Ok(())
        } else {
            Err(anyhow::anyhow!("Port {} not available", port))
        }
    }

    pub async fn release_port(&mut self, port: u16) -> anyhow::Result<()> {
        if !self.ports.available_ports.contains(&port) {
            self.ports.available_ports.push(port);
            self.ports.available_ports.sort();
            self.save("network.json").await?;
        }
        Ok(())
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            network: NetworkSettings {
                name: "lightd-network".to_string(),
                subnet: "172.20.0.0/16".to_string(),
                gateway: "172.20.0.1".to_string(),
            },
            ports: PortSettings {
                available_ports: (9001..=9050).collect(),
                range_start: 9000,
                range_end: 19999,
                default_host_ip: "0.0.0.0".to_string(),
                reserved_ports: vec![22, 80, 443, 3000, 8080],
            },
        }
    }
}