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
        "currencyBalance": user.wallet.currency_balance,
        "currency": user.wallet.currency
    })))
}

#[derive(Debug, Deserialize)]
pub struct AddRURequest {
    pub amount: f64,
}

/// Add RU credits to wallet (admin or payment integration)
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

/// Deduct RU credits from wallet (internal use for billing)
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
    
    // Deduct from balance
    users_collection
        .update_one(
            doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&user_id).ok() },
            doc! { "$inc": { "wallet.ru_balance": -payload.amount } },
        )
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
    
    let new_balance = user.wallet.ru_balance - payload.amount;
    
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

#[derive(Debug, Deserialize)]
pub struct RUEstimateParams {
    pub cpu: Option<String>,
    pub memory: Option<String>,
    pub disk: Option<String>,
}
