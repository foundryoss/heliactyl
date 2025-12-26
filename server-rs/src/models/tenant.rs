use serde::{Deserialize, Serialize};
use mongodb::bson::oid::ObjectId;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TenantMember {
    pub user_id: String,
    pub role: String, // "owner" or "user"
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Tenant {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub name: String,
    pub owner_user_id: String,
    pub package_id: String,
    pub extra_memory_mb: i64,
    pub extra_disk_mb: i64,
    pub extra_cpu_percent: i64,
    pub extra_server_slots: i64,
    #[serde(default)]
    pub members: Vec<TenantMember>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTenantRequest {
    pub name: String,
    #[serde(default)]
    pub package_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TenantResponse {
    pub id: String,
    pub name: String,
    pub package_id: String,
}

#[derive(Debug, Serialize)]
pub struct Resources {
    pub memory_mb: i64,
    pub disk_mb: i64,
    pub cpu_percent: i64,
    pub server_slots: i64,
}

#[derive(Debug, Serialize)]
pub struct ResourcesResponse {
    pub package: Resources,
    pub extra: Resources,
    pub used: UsedResources,
    pub remaining: Resources,
}

#[derive(Debug, Serialize)]
pub struct UsedResources {
    pub memory_mb: i64,
    pub disk_mb: i64,
    pub cpu_percent: i64,
    pub servers: usize,
}
