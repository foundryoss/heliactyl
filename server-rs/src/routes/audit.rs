use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    response::Json,
};
use mongodb::bson::doc;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::middleware::auth::AuthUser;
use crate::auth::oauth::AppState;
use crate::utils::pagination::{PaginationQuery, build_page_meta};

#[derive(Debug, serde::Deserialize)]
pub struct LimitQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

/// Get audit logs for the current user
pub async fn get_my_audit_logs(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Query(params): Query<LimitQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let logs = state.audit.get_user_logs(&user_id, params.limit).await
        .map_err(|e| {
            tracing::error!("Failed to fetch audit logs: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to fetch audit logs" })),
            )
        })?;

    let response_logs: Vec<_> = logs.iter().map(|log| log.to_response()).collect();

    Ok(Json(json!({
        "items": response_logs
    })))
}

/// Get audit logs for a specific tenant
pub async fn get_tenant_audit_logs(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
    Query(params): Query<PaginationQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    // Check if user is a member of the tenant
    let db = state.mongo.database();
    let tenants_collection = db.collection::<crate::models::tenant::Tenant>("tenants");
    
    let tenant = tenants_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Tenant not found" }))))?;
    
    if !tenant.members.iter().any(|m| m.user_id == user_id) {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Forbidden" }))));
    }
    
    let (logs, total) = state.audit.get_tenant_logs(&tenant_id, params.page, params.page_size).await
        .map_err(|e| {
            tracing::error!("Failed to fetch audit logs: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to fetch audit logs" })),
            )
        })?;

    let response_logs: Vec<_> = logs.iter().map(|log| log.to_response()).collect();

    Ok(Json(json!({
        "items": response_logs,
        "meta": build_page_meta(params.page, params.page_size, total)
    })))
}
