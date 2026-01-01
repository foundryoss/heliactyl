use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::auth::oauth::AppState;
use crate::middleware::auth::AuthUser;

// Middleware to check if user is admin
fn require_admin(auth_user: &AuthUser) -> Result<(), (StatusCode, Json<Value>)> {
    if !auth_user.user.is_admin {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Admin access required" })),
        ));
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct AdminUserResponse {
    pub id: String,
    pub email: String,
    pub username: String,
    pub is_admin: bool,
    pub created_at: String,
}

// GET /api/admin/users - List all users
pub async fn list_users(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_admin(&auth_user)?;

    let users = state.mongo.get_all_users().await.map_err(|e| {
        tracing::error!("Failed to fetch users: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to fetch users" })),
        )
    })?;

    let items: Vec<AdminUserResponse> = users
        .into_iter()
        .map(|u| AdminUserResponse {
            id: u.id.map(|id| id.to_hex()).unwrap_or_default(),
            email: u.email,
            username: u.username,
            is_admin: u.is_admin,
            created_at: u.created_at.to_string(),
        })
        .collect();

    Ok(Json(json!({ "items": items })))
}

#[derive(Debug, Deserialize)]
pub struct SetAdminRequest {
    #[serde(rename = "isAdmin")]
    pub is_admin: bool,
}

// POST /api/admin/users/:id/admin - Set user admin status
pub async fn set_user_admin(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
    Json(payload): Json<SetAdminRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_admin(&auth_user)?;

    // Prevent self-demotion
    let current_user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    if current_user_id == user_id && !payload.is_admin {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Cannot remove your own admin status" })),
        ));
    }

    state
        .mongo
        .set_user_admin(&user_id, payload.is_admin)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update user admin status: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to update user" })),
            )
        })?;

    // Log audit event
    state
        .audit
        .log(
            current_user_id.clone(),
            "admin.set_user_admin".to_string(),
            json!({
                "target_user_id": user_id,
                "is_admin": payload.is_admin
            }),
        )
        .await
        .ok();

    Ok(Json(json!({ "ok": true })))
}

// GET /api/admin/users/:id - Get single user details
pub async fn get_user(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_admin(&auth_user)?;

    let user = state.mongo.get_user_by_id(&user_id).await.map_err(|e| {
        tracing::error!("Failed to fetch user: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to fetch user" })),
        )
    })?;

    match user {
        Some(u) => Ok(Json(json!({
            "user": {
                "id": u.id.map(|id| id.to_hex()),
                "email": u.email,
                "username": u.username,
                "discordId": u.discord_id,
                "isAdmin": u.is_admin,
                "createdAt": u.created_at.to_string(),
                "updatedAt": u.updated_at.to_string(),
            }
        }))),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "User not found" })),
        )),
    }
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub email: Option<String>,
    pub username: Option<String>,
}

// PATCH /api/admin/users/:id - Update user details
pub async fn update_user(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
    Json(payload): Json<UpdateUserRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_admin(&auth_user)?;

    state
        .mongo
        .update_user(&user_id, payload.email.as_deref(), payload.username.as_deref())
        .await
        .map_err(|e| {
            tracing::error!("Failed to update user: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to update user" })),
            )
        })?;

    // Log audit event
    let current_user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    state
        .audit
        .log(
            current_user_id,
            "admin.update_user".to_string(),
            json!({
                "target_user_id": user_id,
                "email": payload.email,
                "username": payload.username
            }),
        )
        .await
        .ok();

    Ok(Json(json!({ "ok": true })))
}

// DELETE /api/admin/users/:id - Delete user
pub async fn delete_user(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_admin(&auth_user)?;

    // Prevent self-deletion
    let current_user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    if current_user_id == user_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Cannot delete your own account from admin panel" })),
        ));
    }

    state.mongo.delete_user(&user_id).await.map_err(|e| {
        tracing::error!("Failed to delete user: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to delete user" })),
        )
    })?;

    // Log audit event
    state
        .audit
        .log(
            current_user_id,
            "admin.delete_user".to_string(),
            json!({ "target_user_id": user_id }),
        )
        .await
        .ok();

    Ok(Json(json!({ "ok": true })))
}

// GET /api/admin/diagnostics - System diagnostics
pub async fn diagnostics(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_admin(&auth_user)?;

    // Check MongoDB
    let db_ok = state.mongo.ping().await.is_ok();

    // Check Redis (via session manager)
    let redis_ok = state.session_manager.ping().is_ok();

    Ok(Json(json!({
        "db": { "ok": db_ok },
        "redis": { "ok": redis_ok },
        "daemon": { "url": state.daemon_client.base_url() }
    })))
}

#[derive(Debug, Deserialize)]
pub struct ResetPasswordRequest {
    pub password: String,
}

// POST /api/admin/users/:id/reset-password - Reset user password
pub async fn reset_user_password(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Path(user_id): Path<String>,
    Json(payload): Json<ResetPasswordRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_admin(&auth_user)?;

    // Hash the new password
    let password_hash = bcrypt::hash(&payload.password, bcrypt::DEFAULT_COST).map_err(|e| {
        tracing::error!("Failed to hash password: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to hash password" })),
        )
    })?;

    state
        .mongo
        .reset_user_password(&user_id, &password_hash)
        .await
        .map_err(|e| {
            tracing::error!("Failed to reset password: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to reset password" })),
            )
        })?;

    // Log audit event
    let current_user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    state
        .audit
        .log(
            current_user_id,
            "admin.reset_password".to_string(),
            json!({ "target_user_id": user_id }),
        )
        .await
        .ok();

    Ok(Json(json!({ "ok": true })))
}

// GET /api/admin/nodes - List all nodes
pub async fn list_nodes(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_admin(&auth_user)?;

    let nodes = state.mongo.get_all_nodes().await.map_err(|e| {
        tracing::error!("Failed to fetch nodes: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to fetch nodes" })),
        )
    })?;

    let items: Vec<Value> = nodes
        .into_iter()
        .map(|n| json!({
            "id": n.id.map(|id| id.to_hex()),
            "name": n.name,
            "icon_url": n.icon_url,
            "limits": n.limits,
            "network": n.network,
            "node_config": n.node_config,
            "created_at": n.created_at.to_string(),
            "updated_at": n.updated_at.to_string(),
        }))
        .collect();

    Ok(Json(json!({ "items": items })))
}

// POST /api/admin/nodes - Create a new node
pub async fn create_node(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<crate::models::node::CreateNodeRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_admin(&auth_user)?;

    let node = crate::models::node::Node::new(
        payload.name,
        payload.icon_url,
        payload.limits,
        payload.network,
        payload.node_config,
    );

    let node_id = state.mongo.create_node(&node).await.map_err(|e| {
        tracing::error!("Failed to create node: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to create node" })),
        )
    })?;

    // Log audit event
    let current_user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    state
        .audit
        .log(
            current_user_id,
            "admin.create_node".to_string(),
            json!({ "node_id": node_id, "name": node.name }),
        )
        .await
        .ok();

    Ok(Json(json!({ "ok": true, "id": node_id })))
}

// GET /api/admin/nodes/:id - Get single node
pub async fn get_node(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_admin(&auth_user)?;

    let node = state.mongo.get_node_by_id(&node_id).await.map_err(|e| {
        tracing::error!("Failed to fetch node: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to fetch node" })),
        )
    })?;

    match node {
        Some(n) => Ok(Json(json!({
            "node": {
                "id": n.id.map(|id| id.to_hex()),
                "name": n.name,
                "icon_url": n.icon_url,
                "limits": n.limits,
                "network": n.network,
                "node_config": n.node_config,
                "created_at": n.created_at.to_string(),
                "updated_at": n.updated_at.to_string(),
            }
        }))),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "Node not found" })),
        )),
    }
}

// PATCH /api/admin/nodes/:id - Update node
pub async fn update_node(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
    Json(payload): Json<crate::models::node::UpdateNodeRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_admin(&auth_user)?;

    state
        .mongo
        .update_node(&node_id, &payload)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update node: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to update node" })),
            )
        })?;

    // Log audit event
    let current_user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    state
        .audit
        .log(
            current_user_id,
            "admin.update_node".to_string(),
            json!({ "node_id": node_id }),
        )
        .await
        .ok();

    Ok(Json(json!({ "ok": true })))
}

// DELETE /api/admin/nodes/:id - Delete node
pub async fn delete_node(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Path(node_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_admin(&auth_user)?;

    state.mongo.delete_node(&node_id).await.map_err(|e| {
        tracing::error!("Failed to delete node: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to delete node" })),
        )
    })?;

    // Log audit event
    let current_user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    state
        .audit
        .log(
            current_user_id,
            "admin.delete_node".to_string(),
            json!({ "node_id": node_id }),
        )
        .await
        .ok();

    Ok(Json(json!({ "ok": true })))
}


// GET /api/admin/software - List all server software
pub async fn list_software(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_admin(&auth_user)?;

    let software = state.mongo.get_all_software().await.map_err(|e| {
        tracing::error!("Failed to fetch software: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to fetch software" })),
        )
    })?;

    let items: Vec<Value> = software
        .into_iter()
        .map(|s| json!({
            "id": s.id.map(|id| id.to_hex()),
            "name": s.name,
            "icon_url": s.icon_url,
            "docker_images": s.docker_images,
            "startup_cmd": s.startup_cmd,
            "install_content": s.install_content,
            "update_content": s.update_content,
            "runtime": s.runtime,
            "variables": s.variables,
            "created_at": s.created_at.to_string(),
        }))
        .collect();

    Ok(Json(json!({ "items": items })))
}

// POST /api/admin/software - Create server software
pub async fn create_software(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<crate::models::software::CreateSoftwareRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_admin(&auth_user)?;

    let software = crate::models::software::ServerSoftware::new(
        payload.name,
        payload.icon_url,
        payload.docker_images,
        payload.startup_cmd,
        payload.install_content,
        payload.update_content,
        payload.runtime,
        payload.networking,
        payload.variables,
    );

    let software_id = state.mongo.create_software(&software).await.map_err(|e| {
        tracing::error!("Failed to create software: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to create software" })),
        )
    })?;

    let current_user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    state
        .audit
        .log(
            current_user_id,
            "admin.create_software".to_string(),
            json!({ "software_id": software_id, "name": software.name }),
        )
        .await
        .ok();

    Ok(Json(json!({ "ok": true, "id": software_id })))
}

// PATCH /api/admin/software/:id - Update software
pub async fn update_software(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Path(software_id): Path<String>,
    Json(payload): Json<crate::models::software::UpdateSoftwareRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_admin(&auth_user)?;

    state
        .mongo
        .update_software(&software_id, &payload)
        .await
        .map_err(|e| {
            tracing::error!("Failed to update software: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to update software" })),
            )
        })?;

    let current_user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    state
        .audit
        .log(
            current_user_id,
            "admin.update_software".to_string(),
            json!({ "software_id": software_id }),
        )
        .await
        .ok();

    Ok(Json(json!({ "ok": true })))
}

// DELETE /api/admin/software/:id - Delete software
pub async fn delete_software(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Path(software_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    require_admin(&auth_user)?;

    state.mongo.delete_software(&software_id).await.map_err(|e| {
        tracing::error!("Failed to delete software: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "Failed to delete software" })),
        )
    })?;

    let current_user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    state
        .audit
        .log(
            current_user_id,
            "admin.delete_software".to_string(),
            json!({ "software_id": software_id }),
        )
        .await
        .ok();

    Ok(Json(json!({ "ok": true })))
}
