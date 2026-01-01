use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use mongodb::bson::doc;

use crate::middleware::auth::AuthUser;
use crate::auth::oauth::AppState;
use crate::models::tenant::Tenant;
use crate::models::billing::{TenantBalance, BillingTransaction};

/// Get billing information for a tenant
pub async fn get_tenant_billing(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    
    // Check membership
    let tenant = tenants_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Tenant not found" }))))?;
    
    // Check if user is a member
    if !tenant.members.iter().any(|m| m.user_id == user_id) {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Forbidden" }))));
    }
    
    // Get tenant balance from database
    let balance_collection = db.collection::<TenantBalance>("tenant_balances");
    let balance = balance_collection
        .find_one(doc! { "tenant_id": &tenant_id })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
    
    // TODO: Calculate usage from container metrics when monitoring is implemented
    let usage = json!({
        "cpu": 0.0,
        "memory": 0,
        "disk": 0,
        "network": 0,
        "estimated_cost": 0.0
    });
    
    Ok(Json(json!({
        "tenantId": tenant_id,
        "billing": {
            "usage": usage,
            "period": {
                "start": chrono::Utc::now().format("%Y-%m-01T00:00:00Z").to_string(),
                "end": chrono::Utc::now().to_rfc3339(),
            }
        },
        "balance": balance.map(|b| json!({
            "balance": b.balance,
            "currency": b.currency,
            "lastUpdated": b.last_updated.to_rfc3339(),
        })),
    })))
}

/// Get billing configuration
pub async fn get_billing_config(
    Extension(_auth_user): Extension<AuthUser>,
    State(_state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Return default billing configuration
    // TODO: Store this in database for admin configuration
    Ok(Json(json!({
        "enabled": true,
        "currency": "USD",
        "rates": {
            "cpu_per_hour": 0.01,
            "memory_gb_per_hour": 0.005,
            "disk_gb_per_month": 0.10,
            "network_gb": 0.01
        },
        "billing_cycle": "monthly",
        "payment_methods": ["credit_card", "paypal"]
    })))
}

/// Get billing transactions for a tenant
pub async fn get_billing_transactions(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    
    // Check membership
    let tenant = tenants_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Tenant not found" }))))?;
    
    // Check if user is a member
    if !tenant.members.iter().any(|m| m.user_id == user_id) {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Forbidden" }))));
    }
    
    // Get transactions from database
    let transactions_collection = db.collection::<BillingTransaction>("billing_transactions");
    let mut cursor = transactions_collection
        .find(doc! { "tenant_id": &tenant_id })
        .sort(doc! { "created_at": -1 })
        .limit(100)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
    
    let mut transactions = Vec::new();
    use futures_util::StreamExt;
    while let Some(result) = cursor.next().await {
        if let Ok(transaction) = result {
            transactions.push(json!({
                "id": transaction.id.map(|id| id.to_hex()),
                "amount": transaction.amount,
                "currency": transaction.currency,
                "description": transaction.description,
                "type": transaction.transaction_type,
                "status": transaction.status,
                "createdAt": transaction.created_at.to_rfc3339(),
                "completedAt": transaction.completed_at.map(|dt| dt.to_rfc3339()),
            }));
        }
    }
    
    Ok(Json(json!({ "items": transactions })))
}

/// Add funds to tenant balance (admin only or payment integration)
pub async fn add_funds(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let amount = payload.get("amount")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(json!({ "error": "Invalid amount" }))))?;
    
    if amount <= 0.0 {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "Amount must be positive" }))));
    }
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    
    // Check membership
    let tenant = tenants_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Tenant not found" }))))?;
    
    // Check if user is owner
    if tenant.owner_user_id != user_id {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Only owner can add funds" }))));
    }
    
    // Update or create balance
    let balance_collection = db.collection::<TenantBalance>("tenant_balances");
    let existing_balance = balance_collection
        .find_one(doc! { "tenant_id": &tenant_id })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
    
    let new_balance = if let Some(mut balance) = existing_balance {
        balance.balance += amount;
        balance.last_updated = chrono::Utc::now();
        
        balance_collection
            .replace_one(
                doc! { "tenant_id": &tenant_id },
                &balance,
            )
            .await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
        
        balance
    } else {
        let balance = TenantBalance {
            id: None,
            tenant_id: tenant_id.clone(),
            balance: amount,
            currency: "USD".to_string(),
            last_updated: chrono::Utc::now(),
        };
        
        balance_collection
            .insert_one(&balance)
            .await
            .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
        
        balance
    };
    
    // Create transaction record
    let transactions_collection = db.collection::<BillingTransaction>("billing_transactions");
    let transaction = BillingTransaction {
        id: None,
        tenant_id: tenant_id.clone(),
        amount,
        currency: "USD".to_string(),
        description: "Funds added".to_string(),
        transaction_type: "credit".to_string(),
        status: "completed".to_string(),
        created_at: chrono::Utc::now(),
        completed_at: Some(chrono::Utc::now()),
    };
    
    transactions_collection
        .insert_one(&transaction)
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
    
    // Audit log
    let _ = state.audit.log(
        user_id,
        "billing:add_funds".to_string(),
        json!({
            "tenantId": &tenant_id,
            "amount": amount,
        })
    ).await;
    
    Ok(Json(json!({
        "balance": new_balance.balance,
        "currency": new_balance.currency,
    })))
}
