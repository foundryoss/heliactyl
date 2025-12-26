use serde::{Deserialize, Serialize};
use mongodb::bson::oid::ObjectId;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Server {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub tenant_id: String,
    pub container_id: String,  // UUID from daemon
    pub name: String,
    pub docker_image: String,
    pub logs_url: Option<String>,  // Streaming logs URL from daemon
    pub created_at: chrono::DateTime<chrono::Utc>,
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
    #[serde(alias = "dockerImage")]
    pub docker_image: String,
    #[serde(alias = "memoryMb")]
    pub memory_mb: i64,
    #[serde(alias = "diskMb")]
    pub disk_mb: i64,
    #[serde(alias = "cpuPercent")]
    pub cpu_percent: i64,
    #[serde(default)]
    pub location: String,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct ServerResponse {
    pub id: String,
    pub container_id: String,
    pub name: String,
    pub state: String,
    pub ports: Vec<ServerPort>,
    pub logs_url: Option<String>,
}
