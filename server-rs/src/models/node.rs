use serde::{Deserialize, Serialize};
use mongodb::bson::oid::ObjectId;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Node {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    pub limits: NodeLimits,
    pub network: NodeNetwork,
    pub node_config: NodeConfig,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeLimits {
    pub cpu: String,
    pub memory: String,
    pub disk: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeNetwork {
    pub uri: String,
    pub scheme: String,
    #[serde(rename = "authType")]
    pub auth_type: String,
    pub ip: String,
    pub fqdn: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _note: Option<String>,
    pub version: String,
    pub server: NodeServerConfig,
    pub docker: NodeDockerConfig,
    pub storage: NodeStorageConfig,
    pub monitoring: NodeMonitoringConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeDockerConfig {
    pub socket_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeStorageConfig {
    pub base_path: String,
    pub containers_path: String,
    pub volumes_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NodeMonitoringConfig {
    pub enabled: bool,
    pub interval_ms: u64,
    pub ru_config: RuConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RuConfig {
    pub cpu_weight: f64,
    pub memory_weight: f64,
    pub io_weight: f64,
    pub network_weight: f64,
    pub storage_weight: f64,
    pub base_ru: f64,
}

impl Node {
    pub fn new(
        name: String,
        icon_url: Option<String>,
        limits: NodeLimits,
        network: NodeNetwork,
        node_config: NodeConfig,
    ) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: None,
            name,
            icon_url,
            limits,
            network,
            node_config,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateNodeRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    pub limits: NodeLimits,
    pub network: NodeNetwork,
    pub node_config: NodeConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateNodeRequest {
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    pub limits: Option<NodeLimits>,
    pub network: Option<NodeNetwork>,
    pub node_config: Option<NodeConfig>,
}
