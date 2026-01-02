use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use mongodb::bson::doc;
use serde::Deserialize;

use crate::middleware::auth::AuthUser;
use crate::auth::oauth::AppState;
use crate::models::user::User;
use crate::models::server::Server;
use crate::models::tenant::Tenant;
use crate::daemon::DaemonClient;

/// Resource Unit transaction record for wallet operations
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RUTransaction {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<mongodb::bson::oid::ObjectId>,
    pub user_id: String,
    pub amount: f64,
    pub balance_after: f64,
    pub transaction_type: String,
    pub description: String,
    pub reference_id: Option<String>,
    pub reference_type: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub created_by: Option<String>,
}

/// Get user wallet balance
pub async fn get_wallet(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let db = state.mongo.database();
    let users_collection = db.collection::<User>("users");
    
    let user = users_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&user_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "User not found" }))))?;
    
    Ok(Json(json!({
        "ruBalance": user.wallet.ru_balance,
        "usedResourceUnits": user.wallet.used_resource_units
    })))
}

/// Get user RU transactions with pagination
pub async fn get_ru_transactions(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<crate::utils::pagination::PaginationQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let (page, page_size) = crate::utils::pagination::parse_page_params(Some(params.page), Some(params.page_size));
    let skip = (page - 1) * page_size;
    
    let db = state.mongo.database();
    let transactions_collection = db.collection::<RUTransaction>("ru_transactions");
    
    // Get total count
    let total = transactions_collection
        .count_documents(doc! { "user_id": &user_id })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
    
    let mut cursor = transactions_collection
        .find(doc! { "user_id": &user_id })
        .sort(doc! { "created_at": -1 })
        .skip(skip as u64)
        .limit(page_size)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
    
    let mut transactions = Vec::new();
    use futures_util::StreamExt;
    while let Some(result) = cursor.next().await {
        if let Ok(tx) = result {
            transactions.push(json!({
                "id": tx.id.map(|id| id.to_hex()),
                "amount": tx.amount,
                "balanceAfter": tx.balance_after,
                "type": tx.transaction_type,
                "description": tx.description,
                "referenceId": tx.reference_id,
                "referenceType": tx.reference_type,
                "createdAt": tx.created_at.to_rfc3339(),
                "createdBy": tx.created_by,
            }));
        }
    }
    
    let meta = crate::utils::pagination::build_page_meta(page, page_size, total as i64);
    
    Ok(Json(json!({ 
        "items": transactions,
        "meta": meta
    })))
}

#[derive(Debug, Deserialize)]
pub struct AddRURequest {
    pub amount: f64,
}

/// Add RU to wallet (admin or payment integration)
pub async fn add_ru_credits(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AddRURequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    if payload.amount <= 0.0 {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "Amount must be positive" }))));
    }
    
    let db = state.mongo.database();
    let users_collection = db.collection::<User>("users");
    
    // Update user's RU balance
    let result = users_collection
        .update_one(
            doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&user_id).ok() },
            doc! { "$inc": { "wallet.ru_balance": payload.amount } },
        )
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
    
    if result.matched_count == 0 {
        return Err((StatusCode::NOT_FOUND, Json(json!({ "error": "User not found" }))));
    }
    
    // Get updated balance
    let user = users_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&user_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "User not found" }))))?;
    
    // Audit log
    let _ = state.audit.log(
        user_id.clone(),
        "wallet:add_ru".to_string(),
        json!({ "amount": payload.amount, "newBalance": user.wallet.ru_balance })
    ).await;
    
    Ok(Json(json!({
        "ruBalance": user.wallet.ru_balance,
        "added": payload.amount
    })))
}

#[derive(Debug, Deserialize)]
pub struct DeductRURequest {
    pub amount: f64,
    pub reason: Option<String>,
}

/// Deduct RU from wallet (internal use for billing)
pub async fn deduct_ru_credits(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<DeductRURequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    if payload.amount <= 0.0 {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "Amount must be positive" }))));
    }
    
    let db = state.mongo.database();
    let users_collection = db.collection::<User>("users");
    
    // Check current balance
    let user = users_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&user_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "User not found" }))))?;
    
    if user.wallet.ru_balance < payload.amount {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ 
            "error": "Insufficient RU balance",
            "required": payload.amount,
            "available": user.wallet.ru_balance
        }))));
    }
    
    // Deduct from balance and track usage
    users_collection
        .update_one(
            doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&user_id).ok() },
            doc! { 
                "$inc": { 
                    "wallet.ru_balance": -payload.amount,
                    "wallet.used_resource_units": payload.amount
                } 
            },
        )
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
    
    let new_balance = user.wallet.ru_balance - payload.amount;
    let new_used = user.wallet.used_resource_units + payload.amount;
    
    // Audit log
    let _ = state.audit.log(
        user_id,
        "wallet:deduct_ru".to_string(),
        json!({ 
            "amount": payload.amount, 
            "reason": payload.reason,
            "newBalance": new_balance 
        })
    ).await;
    
    Ok(Json(json!({
        "ruBalance": new_balance,
        "deducted": payload.amount
    })))
}

/// Admin: Give RU to a user
#[derive(Debug, Deserialize)]
pub struct AdminGiveRURequest {
    pub user_id: String,
    pub amount: f64,
    pub reason: Option<String>,
}

pub async fn admin_give_credits(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AdminGiveRURequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Check if admin
    if !auth_user.user.is_admin {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Admin access required" }))));
    }
    
    let admin_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    if payload.amount <= 0.0 {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "Amount must be positive" }))));
    }
    
    let db = state.mongo.database();
    let users_collection = db.collection::<User>("users");
    let transactions_collection = db.collection::<RUTransaction>("ru_transactions");
    
    // Get target user
    let target_user = users_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&payload.user_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "User not found" }))))?;
    
    let new_balance = target_user.wallet.ru_balance + payload.amount;
    
    // Update balance
    users_collection
        .update_one(
            doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&payload.user_id).ok() },
            doc! { "$inc": { "wallet.ru_balance": payload.amount } },
        )
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
    
    // Create transaction record
    let transaction = RUTransaction {
        id: None,
        user_id: payload.user_id.clone(),
        amount: payload.amount,
        balance_after: new_balance,
        transaction_type: "admin_credit".to_string(),
        description: payload.reason.clone().unwrap_or_else(|| "Admin credit".to_string()),
        reference_id: None,
        reference_type: None,
        created_at: chrono::Utc::now(),
        created_by: Some(admin_id.clone()),
    };
    
    transactions_collection
        .insert_one(&transaction)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
    
    // Audit log
    let _ = state.audit.log(
        admin_id,
        "admin:give_credits".to_string(),
        json!({
            "targetUserId": &payload.user_id,
            "amount": payload.amount,
            "reason": payload.reason,
            "newBalance": new_balance
        })
    ).await;
    
    Ok(Json(json!({
        "userId": payload.user_id,
        "amount": payload.amount,
        "newBalance": new_balance
    })))
}

/// Admin: Set RU balance for a user (to any value)
#[derive(Debug, Deserialize)]
pub struct AdminSetRURequest {
    pub user_id: String,
    pub balance: f64,
    pub reason: Option<String>,
}

pub async fn admin_set_credits(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<AdminSetRURequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Check if admin
    if !auth_user.user.is_admin {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Admin access required" }))));
    }
    
    let admin_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    if payload.balance < 0.0 {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "Balance cannot be negative" }))));
    }
    
    let db = state.mongo.database();
    let users_collection = db.collection::<User>("users");
    let transactions_collection = db.collection::<RUTransaction>("ru_transactions");
    
    // Get target user
    let target_user = users_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&payload.user_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "User not found" }))))?;
    
    let old_balance = target_user.wallet.ru_balance;
    let difference = payload.balance - old_balance;
    
    // Set balance directly
    users_collection
        .update_one(
            doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&payload.user_id).ok() },
            doc! { "$set": { "wallet.ru_balance": payload.balance } },
        )
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
    
    // Create transaction record
    let transaction = RUTransaction {
        id: None,
        user_id: payload.user_id.clone(),
        amount: difference,
        balance_after: payload.balance,
        transaction_type: "admin_set".to_string(),
        description: payload.reason.clone().unwrap_or_else(|| format!("Admin set balance from {:.2} to {:.2}", old_balance, payload.balance)),
        reference_id: None,
        reference_type: None,
        created_at: chrono::Utc::now(),
        created_by: Some(admin_id.clone()),
    };
    
    transactions_collection
        .insert_one(&transaction)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
    
    // Audit log
    let _ = state.audit.log(
        admin_id,
        "admin:set_credits".to_string(),
        json!({
            "targetUserId": &payload.user_id,
            "oldBalance": old_balance,
            "newBalance": payload.balance,
            "difference": difference,
            "reason": payload.reason
        })
    ).await;
    
    Ok(Json(json!({
        "userId": payload.user_id,
        "oldBalance": old_balance,
        "newBalance": payload.balance,
        "difference": difference
    })))
}

/// Admin: Get user wallet info
pub async fn admin_get_user_wallet(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(user_id): axum::extract::Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Check if admin
    if !auth_user.user.is_admin {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Admin access required" }))));
    }
    
    let db = state.mongo.database();
    let users_collection = db.collection::<User>("users");
    
    let user = users_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&user_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "User not found" }))))?;
    
    Ok(Json(json!({
        "userId": user_id,
        "ruBalance": user.wallet.ru_balance,
        "usedResourceUnits": user.wallet.used_resource_units
    })))
}

/// Daemon: Deduct RU for container usage (special auth)
/// The daemon sends container_id, and the panel looks up which user owns it
#[derive(Debug, Deserialize)]
pub struct DaemonDeductRequest {
    pub container_id: String,  // Docker container ID or UUID from lightd
    pub amount: f64,
    pub description: String,
}

pub async fn daemon_deduct_credits(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<DaemonDeductRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Verify lightd API key from Authorization header or config
    let lightd_api_key = state.config
        .get_string("lightd_api_key")
        .or_else(|| std::env::var("LIGHTD_API_KEY").ok())
        .unwrap_or_else(|| "lightd-secret-key-2024-ru-deduct".to_string());
    
    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    
    let token = auth_header.strip_prefix("Bearer ").unwrap_or(auth_header);
    
    if token != lightd_api_key {
        tracing::warn!("Invalid lightd API key attempt from: {:?}", headers.get("x-forwarded-for"));
        return Err((StatusCode::UNAUTHORIZED, Json(json!({ "error": "Invalid lightd credentials" }))));
    }
    
    if payload.amount <= 0.0 {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "Amount must be positive" }))));
    }
    
    let db = state.mongo.database();
    let servers_collection = db.collection::<crate::models::server::Server>("servers");
    let tenants_collection = db.collection::<crate::models::tenant::Tenant>("tenants");
    let users_collection = db.collection::<User>("users");
    let transactions_collection = db.collection::<RUTransaction>("ru_transactions");
    
    // Find server by container_id (UUID from lightd)
    let server = servers_collection
        .find_one(doc! { "container_id": &payload.container_id })
        .await
        .map_err(|e| {
            tracing::error!("Database error finding server: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" })))
        })?
        .ok_or_else(|| {
            tracing::warn!("Server not found for container_id: {}", payload.container_id);
            (StatusCode::NOT_FOUND, Json(json!({ "error": "Server not found for container" })))
        })?;
    
    // Get tenant to find owner
    let tenant = tenants_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&server.tenant_id).ok() })
        .await
        .map_err(|e| {
            tracing::error!("Database error finding tenant: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" })))
        })?
        .ok_or_else(|| {
            tracing::warn!("Tenant not found: {}", server.tenant_id);
            (StatusCode::NOT_FOUND, Json(json!({ "error": "Tenant not found" })))
        })?;
    
    let user_id = tenant.owner_user_id.clone();
    
    // Get user
    let user = users_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&user_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "User not found" }))))?;
    
    // Check balance (allow negative for now, but flag it)
    let insufficient = user.wallet.ru_balance < payload.amount;
    let new_balance = user.wallet.ru_balance - payload.amount;
    let new_used = user.wallet.used_resource_units + payload.amount;
    
    // If user is already deeply negative, tell lightd to stop billing this container
    // This prevents repeated stop calls and unnecessary billing
    let already_suspended = user.wallet.ru_balance <= -10.0;
    
    // Deduct from balance and track usage
    users_collection
        .update_one(
            doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&user_id).ok() },
            doc! { 
                "$inc": { 
                    "wallet.ru_balance": -payload.amount,
                    "wallet.used_resource_units": payload.amount
                } 
            },
        )
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
    
    // Create transaction record
    let server_id = server.id.map(|id| id.to_hex());
    let transaction = RUTransaction {
        id: None,
        user_id: user_id.clone(),
        amount: -payload.amount, // Negative for deduction
        balance_after: new_balance,
        transaction_type: "usage_deduct".to_string(),
        description: payload.description.clone(),
        reference_id: Some(payload.container_id.clone()),
        reference_type: Some("container".to_string()),
        created_at: chrono::Utc::now(),
        created_by: Some("lightd".to_string()),
    };
    
    transactions_collection
        .insert_one(&transaction)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
    
    tracing::info!(
        "Lightd deducted {} RU from user {} for container {} (server: {:?}). New balance: {}, used: {}",
        payload.amount, user_id, payload.container_id, server_id, new_balance, new_used
    );
    
    // Check if we need to suspend containers due to negative balance
    // Only suspend if user JUST crossed the -10 threshold (wasn't already suspended)
    if new_balance <= -10.0 && !already_suspended {
        tracing::warn!(
            "User {} just crossed suspension threshold (balance: {} -> {}). Suspending containers...",
            user_id, user.wallet.ru_balance, new_balance
        );
        tokio::spawn({
            let state = state.clone();
            let user_id = user_id.clone();
            async move {
                if let Err(e) = suspend_user_containers_if_negative(&state, &user_id, new_balance).await {
                    tracing::error!("Failed to suspend containers for user {}: {}", user_id, e);
                }
            }
        });
    }
    
    Ok(Json(json!({
        "success": true,
        "userId": user_id,
        "deducted": payload.amount,
        "newBalance": new_balance,
        "usedResourceUnits": new_used,
        "insufficient": insufficient,
        "suspended": new_balance <= -10.0
    })))
}

/// Get RU estimate for server creation
pub async fn get_ru_estimate(
    Extension(_auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(node_id): axum::extract::Path<String>,
    axum::extract::Query(params): axum::extract::Query<RUEstimateParams>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    tracing::info!("Getting RU estimate for node: {}", node_id);
    
    let db = state.mongo.database();
    let nodes_collection = db.collection::<crate::models::node::Node>("nodes");
    
    // Get node
    let node = nodes_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&node_id).ok() })
        .await
        .map_err(|e| {
            tracing::error!("Database error getting node: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" })))
        })?
        .ok_or_else(|| {
            tracing::warn!("Node not found: {}", node_id);
            (StatusCode::NOT_FOUND, Json(json!({ "error": "Node not found" })))
        })?;
    
    tracing::info!("Found node: {}, daemon: {}://{}", node.name, node.network.scheme, node.network.uri);
    
    // Get daemon client
    let daemon_url = format!("{}://{}", node.network.scheme, node.network.uri);
    let daemon = crate::daemon::client::DaemonClient::new(daemon_url.clone(), "".to_string());
    
    // Use provided limits or node defaults
    let cpu = params.cpu.unwrap_or_else(|| node.limits.cpu.clone());
    let memory = params.memory.unwrap_or_else(|| node.limits.memory.clone());
    let disk = params.disk.unwrap_or_else(|| node.limits.disk.clone());
    
    tracing::info!("Calculating RU estimate for cpu={}, memory={}, disk={}", cpu, memory, disk);
    
    // Get RU estimate from daemon
    match daemon.calculate_ru_estimate(&cpu, &memory, &disk).await {
        Ok(estimate) => {
            tracing::info!("Got RU estimate: {} RU/hour", estimate.ru_per_hour);
            Ok(Json(json!({
                "ruPerHour": estimate.ru_per_hour,
                "ruPerDay": estimate.ru_per_day,
                "ruPerMonth": estimate.ru_per_month,
                "pricePerHour": estimate.price_per_hour,
                "pricePerDay": estimate.price_per_day,
                "pricePerMonth": estimate.price_per_month,
                "limits": {
                    "cpu": cpu,
                    "memory": memory,
                    "disk": disk
                }
            })))
        }
        Err(e) => {
            tracing::warn!("Failed to get RU estimate from daemon {}: {}", daemon_url, e);
            // Return a fallback estimate based on limits
            let cpu_val: f64 = cpu.parse().unwrap_or(1.0);
            let mem_mb = if memory.to_lowercase().ends_with("g") {
                memory.trim_end_matches(|c| c == 'g' || c == 'G').parse::<f64>().unwrap_or(1.0) * 1024.0
            } else {
                memory.trim_end_matches(|c| c == 'm' || c == 'M').parse::<f64>().unwrap_or(1024.0)
            };
            let mem_gb = mem_mb / 1024.0;
            
            let ru_per_hour = 0.1 + (cpu_val * 1.0) + (mem_gb * 0.5);
            let ru_per_day = ru_per_hour * 24.0;
            let ru_per_month = ru_per_day * 30.0;
            
            Ok(Json(json!({
                "ruPerHour": ru_per_hour,
                "ruPerDay": ru_per_day,
                "ruPerMonth": ru_per_month,
                "pricePerHour": ru_per_hour * 0.001,
                "pricePerDay": ru_per_day * 0.001,
                "pricePerMonth": ru_per_month * 0.001,
                "limits": {
                    "cpu": cpu,
                    "memory": memory,
                    "disk": disk
                },
                "fallback": true
            })))
        }
    }
}

/// Helper: Suspend all containers for a user when balance is <= -10 RU
async fn suspend_user_containers_if_negative(
    state: &Arc<AppState>,
    user_id: &str,
    new_balance: f64,
) -> Result<(), String> {
    const SUSPENSION_THRESHOLD: f64 = -10.0;
    
    if new_balance > SUSPENSION_THRESHOLD {
        return Ok(()); // Balance is fine, no action needed
    }
    
    tracing::warn!(
        "User {} balance is {} RU (threshold: {}). Suspending all containers...",
        user_id, new_balance, SUSPENSION_THRESHOLD
    );
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    let servers_collection = db.collection::<Server>("servers");
    let nodes_collection = db.collection::<crate::models::node::Node>("nodes");
    
    // Find all tenants owned by this user
    let _user_oid = mongodb::bson::oid::ObjectId::parse_str(user_id)
        .map_err(|e| format!("Invalid user ID: {}", e))?;
    
    let mut tenant_cursor = tenants_collection
        .find(doc! { "owner_user_id": user_id })
        .await
        .map_err(|e| format!("DB error finding tenants: {}", e))?;
    
    use futures_util::StreamExt;
    
    let mut tenant_ids = Vec::new();
    while let Some(result) = tenant_cursor.next().await {
        if let Ok(tenant) = result {
            if let Some(id) = tenant.id {
                tenant_ids.push(id.to_hex());
            }
        }
    }
    
    if tenant_ids.is_empty() {
        tracing::info!("User {} has no tenants, nothing to suspend", user_id);
        return Ok(());
    }
    
    // Find all servers for these tenants
    let mut server_cursor = servers_collection
        .find(doc! { "tenant_id": { "$in": &tenant_ids } })
        .await
        .map_err(|e| format!("DB error finding servers: {}", e))?;
    
    let mut suspended_count = 0;
    let mut failed_count = 0;
    
    while let Some(result) = server_cursor.next().await {
        if let Ok(server) = result {
            // Get node info to create daemon client
            let node = match nodes_collection
                .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&server.node_id).ok() })
                .await
            {
                Ok(Some(n)) => n,
                Ok(None) => {
                    tracing::error!("Node {} not found for server {}", server.node_id, server.name);
                    failed_count += 1;
                    continue;
                }
                Err(e) => {
                    tracing::error!("DB error finding node: {}", e);
                    failed_count += 1;
                    continue;
                }
            };
            
            // Create daemon client for this node
            let daemon_url = format!("{}://{}", node.network.scheme, node.network.uri);
            let daemon = DaemonClient::new(daemon_url, "".to_string());
            
            // Stop the container
            match daemon.stop_container(&server.container_id).await {
                Ok(_) => {
                    tracing::info!(
                        "Suspended container {} (server: {}) for user {} due to negative balance",
                        server.container_id, server.name, user_id
                    );
                    suspended_count += 1;
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to stop container {} for user {}: {}",
                        server.container_id, user_id, e
                    );
                    failed_count += 1;
                }
            }
        }
    }
    
    tracing::warn!(
        "Suspended {} containers for user {} (failed: {})",
        suspended_count, user_id, failed_count
    );
    
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct RUEstimateParams {
    pub cpu: Option<String>,
    pub memory: Option<String>,
    pub disk: Option<String>,
}
