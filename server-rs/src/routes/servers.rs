use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};
use std::sync::Arc;
use mongodb::bson::doc;
use futures_util::StreamExt;

use crate::middleware::auth::AuthUser;
use crate::auth::oauth::AppState;
use crate::models::server::{Server, CreateServerRequest};
use crate::models::tenant::Tenant;
use crate::daemon::client::{CreateContainerRequest, ResourceLimits};

/// List servers for a tenant - get specific containers by UUID from daemon
pub async fn list_servers(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    let servers_collection = db.collection::<Server>("servers");
    
    // Check membership
    let tenant = tenants_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Tenant not found" }))))?;
    
    if !tenant.members.iter().any(|m| m.user_id == user_id) {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Forbidden" }))));
    }
    
    // Get our server records
    let mut cursor = servers_collection
        .find(doc! { "tenant_id": &tenant_id })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
    
    let mut servers = Vec::new();
    
    while let Some(result) = cursor.next().await {
        if let Ok(server) = result {
            // Get live container data from daemon for this specific container
            match state.daemon_client.get_container(&server.container_id).await {
                Ok(container) => {
                    servers.push(json!({
                        "id": server.id.map(|id| id.to_hex()),
                        "containerId": container.id,
                        "name": container.name,
                        "dockerImage": container.image,
                        "state": container.state,
                        "ports": container.ports,
                        "limits": container.limits,
                        "logsUrl": server.logs_url,
                        "createdAt": server.created_at.to_rfc3339()
                    }));
                }
                Err(e) => {
                    tracing::warn!("Failed to get container {} from daemon: {}", server.container_id, e);
                    // Include server with unknown state if daemon can't find it
                    servers.push(json!({
                        "id": server.id.map(|id| id.to_hex()),
                        "containerId": server.container_id,
                        "name": server.name,
                        "dockerImage": server.docker_image,
                        "state": "unknown",
                        "ports": [],
                        "limits": null,
                        "logsUrl": server.logs_url,
                        "createdAt": server.created_at.to_rfc3339()
                    }));
                }
            }
        }
    }
    
    Ok(Json(json!({ "items": servers })))
}

/// Create a server using the daemon
pub async fn create_server(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
    Json(payload): Json<CreateServerRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    tracing::info!("CREATE SERVER: user={}, tenant={}, name={}", user_id, tenant_id, payload.name);
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    let servers_collection = db.collection::<Server>("servers");
    
    // Validate tenant membership
    let tenant = tenants_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() })
        .await
        .map_err(|e| {
            tracing::error!("Database error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" })))
        })?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Tenant not found" }))))?;
    
    if !tenant.members.iter().any(|m| m.user_id == user_id) {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Forbidden" }))));
    }
    
    // Prepare daemon request
    let mut env = payload.env.clone();
    env.insert("TENANT_ID".to_string(), tenant_id.clone());
    
    let container_request = CreateContainerRequest {
        name: payload.name.clone(),
        image: payload.docker_image.clone(),
        env,
        limits: ResourceLimits {
            cpu_limit: payload.cpu_percent as f64 / 100.0,
            memory_limit: payload.memory_mb * 1024 * 1024,
            disk_limit: payload.disk_mb * 1024 * 1024,
        },
    };
    
    // Create container - daemon returns container info immediately
    let container = state.daemon_client.create_container(container_request).await
        .map_err(|e| {
            tracing::error!("Failed to create container: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": format!("Failed to create container: {}", e) })))
        })?;
    
    tracing::info!("Container created: {} (state: {})", container.id, container.state);
    
    // Generate logs URL for this container
    let logs_url = format!("/api/containers/{}/logs", container.id);
    
    // Store server record with real container UUID
    let server = Server {
        id: None,
        tenant_id: tenant_id.clone(),
        container_id: container.id.clone(),
        name: payload.name.clone(),
        docker_image: payload.docker_image.clone(),
        logs_url: Some(logs_url.clone()),
        created_at: chrono::Utc::now(),
    };
    
    let result = servers_collection.insert_one(&server).await
        .map_err(|e| {
            tracing::error!("Failed to create server record: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Failed to create server" })))
        })?;
    
    let server_id = result.inserted_id.as_object_id().unwrap().to_hex();
    
    tracing::info!("Server record created: {}", server_id);
    
    // Audit log
    let _ = state.audit.log(
        user_id,
        "server:create".to_string(),
        json!({
            "tenantId": &tenant_id,
            "serverId": &server_id,
            "containerId": &container.id,
            "name": &payload.name,
            "dockerImage": &payload.docker_image,
        })
    ).await;
    
    // Return container info to frontend
    Ok(Json(json!({
        "id": server_id,
        "containerId": container.id,
        "name": container.name,
        "dockerImage": container.image,
        "state": container.state,
        "ports": container.ports,
        "limits": container.limits,
        "logsUrl": logs_url
    })))
}

/// Delete a server and its container
pub async fn delete_server(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    let servers_collection = db.collection::<Server>("servers");
    
    // Check membership
    let tenant = tenants_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Tenant not found" }))))?;
    
    if !tenant.members.iter().any(|m| m.user_id == user_id) {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Forbidden" }))));
    }
    
    // Get server to find container ID
    let server = servers_collection
        .find_one(doc! {
            "_id": mongodb::bson::oid::ObjectId::parse_str(&server_id).ok(),
            "tenant_id": &tenant_id
        })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Server not found" }))))?;
    
    // Delete container from daemon
    if let Err(e) = state.daemon_client.delete_container(&server.container_id).await {
        tracing::warn!("Failed to delete container from daemon: {}", e);
        // Continue with database deletion even if daemon deletion fails
    }
    
    // Delete server from database
    servers_collection
        .delete_one(doc! {
            "_id": mongodb::bson::oid::ObjectId::parse_str(&server_id).ok(),
            "tenant_id": &tenant_id
        })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
    
    // Audit log
    let _ = state.audit.log(
        user_id,
        "server:delete".to_string(),
        json!({ 
            "tenantId": &tenant_id, 
            "serverId": &server_id,
            "containerId": &server.container_id,
            "name": &server.name
        })
    ).await;
    
    Ok(Json(json!({ "ok": true })))
}

