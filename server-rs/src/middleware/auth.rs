use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use mongodb::bson::{doc, oid::ObjectId};
use serde_json::json;
use std::sync::Arc;

use crate::database::session::SessionManager;
use crate::database::mongo::MongoClient;
use crate::models::user::User;

#[derive(Clone)]
pub struct AuthUser {
    pub user: User,
}

pub async fn auth_middleware(
    State(state): State<Arc<AuthState>>,
    headers: HeaderMap,
    mut req: Request,
    next: Next,
) -> Result<Response, (StatusCode, axum::Json<serde_json::Value>)> {
    // Extract Bearer token from Authorization header
    let auth_header = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                axum::Json(json!({ "error": "Missing authorization header" })),
            )
        })?;

    // Check if it starts with "Bearer "
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                axum::Json(json!({ "error": "Invalid authorization format" })),
            )
        })?;

    // Get user_id from Redis session
    let user_id = state
        .session_manager
        .get_session(token)
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(json!({ "error": "Session lookup failed" })),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                axum::Json(json!({ "error": "Invalid or expired token" })),
            )
        })?;

    // Get user from MongoDB
    let db = state.mongo.database();
    let users_collection = db.collection::<User>("users");
    
    let object_id = ObjectId::parse_str(&user_id).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(json!({ "error": "Invalid user ID" })),
        )
    })?;

    let user = users_collection
        .find_one(doc! { "_id": object_id })
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(json!({ "error": "Database error" })),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                axum::Json(json!({ "error": "User not found" })),
            )
        })?;

    // Insert user into request extensions
    req.extensions_mut().insert(AuthUser { user });

    Ok(next.run(req).await)
}

#[derive(Clone)]
pub struct AuthState {
    pub session_manager: Arc<SessionManager>,
    pub mongo: Arc<MongoClient>,
}
