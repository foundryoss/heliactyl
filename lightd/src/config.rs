use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;
use crate::network_config::NetworkConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_version")]
    pub version: String,
    pub server: ServerConfig,
    pub docker: DockerConfig,
    pub storage: StorageConfig,
    pub monitoring: Option<MonitoringConfig>,
    #[serde(skip)]
    pub network: Option<NetworkConfig>,
}

fn default_version() -> String {
    "0.1.0".to_string()
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
pub struct StorageConfig {
    pub base_path: String,
    pub containers_path: String,
    pub volumes_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub enabled: bool,
    pub interval_ms: u64,
    pub ru_config: RUConfigSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RUConfigSettings {
    pub cpu_weight: f64,
    pub memory_weight: f64,
    pub io_weight: f64,
    pub network_weight: f64,
    pub storage_weight: f64,
    pub base_ru: f64,
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_ms: 1000,
            ru_config: RUConfigSettings::default(),
        }
    }
}

impl Default for RUConfigSettings {
    fn default() -> Self {
        Self {
            cpu_weight: 1.0,
            memory_weight: 0.5,
            io_weight: 2.0,
            network_weight: 1.5,
            storage_weight: 0.8,
            base_ru: 0.1,
        }
    }
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
