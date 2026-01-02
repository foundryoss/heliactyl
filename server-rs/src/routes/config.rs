use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::auth::oauth::AppState;
use crate::middleware::auth::AuthUser;
use axum::extract::Extension;

/// Get config value (admin only)
pub async fn get_config(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Check if admin
    if !auth_user.user.is_admin {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Admin access required" }))));
    }
    
    // Get lightd API key from config
    let lightd_api_key = state.config
        .get_string("lightd_api_key")
        .unwrap_or_else(|| "not-configured".to_string());
    
    let node_name = state.config
        .get_string("node")
        .unwrap_or_else(|| "unknown".to_string());
    
    Ok(Json(json!({
        "node": node_name,
        "lightdApiKey": lightd_api_key,
        "port": state.config.get_int("port").unwrap_or(3500),
    })))
}
