use serde::{Deserialize, Serialize};
use mongodb::bson::oid::ObjectId;
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Server {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub tenant_id: String,
    pub container_id: String,  // UUID from lightd daemon
    pub name: String,
    pub description: Option<String>,
    pub server_software_id: String,  // Reference to ServerSoftware collection
    pub node_id: String,  // Reference to Node collection
    pub docker_image: String,  // Resolved from server_software
    pub volumes: Vec<ServerVolume>,
    pub network: ServerNetwork,
    pub limits: ServerLimits,
    pub env: HashMap<String, String>,
    pub startup: ServerStartup,
    pub logs_url: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerVolume {
    pub name: String,
    pub mount_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerNetwork {
    pub ports: HashMap<String, ServerPortConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allocated_ports: Option<Vec<AllocatedPort>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerPortConfig {
    pub protocol: String,
    pub primary: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerLimits {
    pub cpu: String,
    pub memory: String,
    pub disk: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pids: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threads: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ru_limit: Option<f64>,  // Optional RU limit per hour
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerStartup {
    #[serde(default)]
    pub skip_install: bool,
    #[serde(default)]
    pub skip_update: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,  // The actual startup command
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AllocatedPort {
    pub container_port: String,
    pub host_port: String,
    pub host_ip: String,
    pub protocol: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ServerPort {
    pub container_port: i32,
    pub host_port: i32,
    pub protocol: String,
    pub primary: bool,
}

#[derive(Debug, Deserialize)]
pub struct CreateServerRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(alias = "serverSoftwareId")]
    pub server_software_id: String,
    #[serde(alias = "nodeId")]
    pub node_id: String,
    #[serde(default)]
    pub volumes: Vec<ServerVolume>,
    #[serde(default)]
    pub network: Option<ServerNetwork>,
    #[serde(default)]
    pub limits: Option<ServerLimits>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub startup: Option<ServerStartup>,
}

#[derive(Debug, Serialize)]
pub struct ServerResponse {
    pub id: String,
    pub container_id: String,
    pub name: String,
    pub description: Option<String>,
    pub state: String,
    pub ports: Vec<ServerPort>,
    pub limits: ServerLimits,
    pub logs_url: Option<String>,
}
