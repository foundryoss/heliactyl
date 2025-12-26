use crate::database::mongo::MongoClient;
use crate::models::audit::AuditLog;
use std::sync::Arc;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

pub struct AuditService {
    mongo: Arc<MongoClient>,
}

impl AuditService {
    pub fn new(mongo: Arc<MongoClient>) -> Self {
        // Ensure data directory exists
        std::fs::create_dir_all("data/logs").ok();
        Self { mongo }
    }

    /// Record an audit log entry
    pub async fn log(&self, user: String, action: String, data: serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
        let audit_log = AuditLog::new(user.clone(), action.clone(), data.clone());

        // Save to MongoDB (in background, don't block)
        let mongo = self.mongo.clone();
        let log_clone = audit_log.clone();
        tokio::spawn(async move {
            let db = mongo.database();
            let collection = db.collection::<AuditLog>("audit_logs");
            if let Err(e) = collection.insert_one(&log_clone).await {
                tracing::error!("Failed to save audit log to MongoDB: {}", e);
            }
        });

        // Save to file (compact format to save space)
        let log_entry = format!(
            "{},{},{},{}\n",
            chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::from(audit_log.timestamp)).format("%Y-%m-%d %H:%M:%S"),
            user,
            action,
            serde_json::to_string(&data).unwrap_or_default()
        );

        // Append to daily log file
        let date = chrono::Utc::now().format("%Y-%m-%d");
        let log_file = format!("data/logs/audit-{}.log", date);
        
        tokio::spawn(async move {
            if let Ok(mut file) = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_file)
                .await
            {
                if let Err(e) = file.write_all(log_entry.as_bytes()).await {
                    tracing::error!("Failed to write audit log to file: {}", e);
                }
            }
        });

        Ok(())
    }

    /// Get audit logs for a specific user
    pub async fn get_user_logs(&self, user: &str, limit: i64) -> Result<Vec<AuditLog>, Box<dyn std::error::Error>> {
        let db = self.mongo.database();
        let collection = db.collection::<AuditLog>("audit_logs");
        
        let mut cursor = collection
            .find(mongodb::bson::doc! { "user": user })
            .sort(mongodb::bson::doc! { "timestamp": -1 })
            .limit(limit)
            .await?;

        let mut logs = Vec::new();
        use futures_util::StreamExt;
        while let Some(result) = cursor.next().await {
            if let Ok(log) = result {
                logs.push(log);
            }
        }

        Ok(logs)
    }

    /// Get audit logs for a specific tenant with pagination
    pub async fn get_tenant_logs(&self, tenant_id: &str, page: i64, page_size: i64) -> Result<(Vec<AuditLog>, i64), Box<dyn std::error::Error>> {
        let db = self.mongo.database();
        let collection = db.collection::<AuditLog>("audit_logs");
        
        let skip = (page - 1) * page_size;
        
        // Get total count
        let total = collection
            .count_documents(mongodb::bson::doc! {})
            .await? as i64;
        
        // Get logs (filter by tenant_id in data field)
        let mut cursor = collection
            .find(mongodb::bson::doc! {})
            .sort(mongodb::bson::doc! { "timestamp": -1 })
            .skip(skip as u64)
            .limit(page_size)
            .await?;

        let mut logs = Vec::new();
        use futures_util::StreamExt;
        while let Some(result) = cursor.next().await {
            if let Ok(log) = result {
                // Filter by tenant_id in data
                if let Some(data_tenant_id) = log.data.get("tenantId").and_then(|v| v.as_str()) {
                    if data_tenant_id == tenant_id {
                        logs.push(log);
                    }
                }
            }
        }

        Ok((logs, total))
    }
}
