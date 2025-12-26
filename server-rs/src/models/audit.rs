use serde::{Deserialize, Serialize};
use mongodb::bson::{oid::ObjectId, DateTime as BsonDateTime};
use chrono::Utc;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AuditLog {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub user: String,
    pub timestamp: BsonDateTime,
    pub action: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Serialize, Clone)]
pub struct AuditLogResponse {
    pub id: String,
    #[serde(rename = "userId")]
    pub user_id: String,
    pub action: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(flatten)]
    pub data: serde_json::Value,
}

impl AuditLog {
    pub fn new(user: String, action: String, data: serde_json::Value) -> Self {
        Self {
            id: None,
            user,
            timestamp: BsonDateTime::from_millis(Utc::now().timestamp_millis()),
            action,
            data,
        }
    }

    pub fn to_response(&self) -> AuditLogResponse {
        AuditLogResponse {
            id: self.id.map(|id| id.to_hex()).unwrap_or_default(),
            user_id: self.user.clone(),
            action: self.action.clone(),
            created_at: chrono::DateTime::<chrono::Utc>::from(
                std::time::SystemTime::from(self.timestamp)
            ).to_rfc3339(),
            data: self.data.clone(),
        }
    }
}
