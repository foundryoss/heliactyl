use serde::{Deserialize, Serialize};
use mongodb::bson::oid::ObjectId;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UsageRecord {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub cpu_usage: f64,
    pub memory_usage: i64,
    pub disk_usage: i64,
    pub network_usage: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContainerBilling {
    pub container_id: String,
    pub container_name: String,
    pub tenant_id: String,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub last_update: chrono::DateTime<chrono::Utc>,
    pub total_runtime_hours: f64,
    pub usage_records: Vec<UsageRecord>,
    pub total_cost: f64,
    pub resource_units: f64,
    pub free_quota_used_hours: f64,
    pub billable_hours: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TenantBilling {
    pub tenant_id: String,
    pub containers: Vec<ContainerBilling>,
    pub total_cost: f64,
    pub currency: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BillingConfig {
    pub enabled: bool,
    pub currency: String,
    pub free_quota_hours: f64,
    pub tracking_interval: i32,
    pub price_per_gb_ram: f64,
    pub price_per_cpu_core: f64,
    pub price_per_gb_disk: f64,
    pub price_per_gb_network: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BillingTransaction {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub tenant_id: String,
    pub amount: f64,
    pub currency: String,
    pub description: String,
    pub transaction_type: String, // "charge", "credit", "refund"
    pub status: String, // "pending", "completed", "failed"
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TenantBalance {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub tenant_id: String,
    pub balance: f64,
    pub currency: String,
    pub last_updated: chrono::DateTime<chrono::Utc>,
}
