use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use crate::network_config::NetworkConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub docker: DockerConfig,
    pub volumes: VolumeConfig,
    pub storage: StorageConfig,
    pub containers: ContainerConfig,
    pub logging: LoggingConfig,
    #[serde(skip)]
    pub network: Option<NetworkConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerConfig {
    pub socket_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeConfig {
    pub base_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub base_path: String,
    pub containers_path: String,
    pub networks_path: String,
    pub logs_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerConfig {
    pub default_network: String,
    pub cleanup_timeout: u64,
    pub max_containers: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub file: String,
}

impl Config {
    pub async fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let content = fs::read_to_string(path).await?;
        let mut config: Config = serde_json::from_str(&content)?;
        
        // Load network configuration
        config.network = Some(NetworkConfig::load("network.json").await?);
        
        Ok(config)
    }
}