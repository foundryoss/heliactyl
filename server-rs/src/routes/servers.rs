use axum::{
    extract::{Extension, State, Multipart},
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

/// List servers for a tenant
pub async fn list_servers(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    let servers_collection = db.collection::<Server>("servers");
    let nodes_collection = db.collection::<crate::models::node::Node>("nodes");
    
    // Check membership
    let tenant = tenants_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Tenant not found" }))))?;
    
    if !tenant.members.iter().any(|m| m.user_id == user_id) {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Forbidden" }))));
    }
    
    // Get server records from MongoDB
    let mut cursor = servers_collection
        .find(doc! { "tenant_id": &tenant_id })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
    
    let mut server_records = Vec::new();
    
    while let Some(result) = cursor.next().await {
        if let Ok(server) = result {
            // Skip servers with empty container_id (failed creations)
            if server.container_id.is_empty() {
                tracing::warn!("Server {} has empty container_id, skipping", server.name);
                continue;
            }
            server_records.push(server);
        }
    }
    
    // Fetch all container states concurrently
    let state_futures: Vec<_> = server_records.iter().map(|server| {
        let server_node_id = server.node_id.clone();
        let server_container_id = server.container_id.clone();
        let nodes_collection = nodes_collection.clone();
        
        async move {
            // Get the node for this server
            let node = nodes_collection
                .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&server_node_id).ok() })
                .await
                .ok()
                .flatten();
            
            let mut state_str = "unknown".to_string();
            
            // Query daemon for live state
            if let Some(node) = &node {
                let daemon_url = format!("{}://{}", node.network.scheme, node.network.uri);
                let daemon = crate::daemon::client::DaemonClient::new(daemon_url.clone(), "".to_string());
                
                tracing::debug!("Querying daemon {} for container {}", daemon_url, server_container_id);
                
                match daemon.get_container(&server_container_id).await {
                    Ok(container) => {
                        tracing::debug!("Got container state: {}", container.state);
                        state_str = container.state;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to get container {} from daemon: {}", server_container_id, e);
                    }
                }
            } else {
                tracing::warn!("Node {} not found for server", server_node_id);
            }
            
            state_str
        }
    }).collect();
    
    // Wait for all state queries to complete concurrently
    let states = futures_util::future::join_all(state_futures).await;
    
    // Build response with fetched states
    let servers: Vec<_> = server_records.iter().zip(states.iter()).map(|(server, state_str)| {
        json!({
            "id": server.id.map(|id| id.to_hex()),
            "containerId": server.container_id,
            "name": server.name,
            "description": server.description,
            "dockerImage": server.docker_image,
            "state": state_str,
            "limits": server.limits,
            "network": {
                "ports": server.network.ports,
                "allocatedPorts": server.network.allocated_ports
            },
            "startup": server.startup,
            "serverSoftwareId": server.server_software_id,
            "nodeId": server.node_id,
            "createdAt": server.created_at.to_rfc3339(),
            "updatedAt": server.updated_at.to_rfc3339()
        })
    }).collect();
    
    Ok(Json(json!({ "items": servers })))
}

/// Create a server
pub async fn create_server(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
    Json(payload): Json<CreateServerRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    tracing::info!("Creating server '{}' for tenant {}", payload.name, tenant_id);
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    let servers_collection = db.collection::<Server>("servers");
    let software_collection = db.collection::<crate::models::software::ServerSoftware>("server_software");
    let nodes_collection = db.collection::<crate::models::node::Node>("nodes");
    
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
    
    // Get server software
    let software = software_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&payload.server_software_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Server software not found" }))))?;
    
    // Get node
    let node = nodes_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&payload.node_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Node not found" }))))?;
    
    // Get docker image (first one or specified)
    let docker_image = software.docker_images.values().next()
        .ok_or_else(|| (StatusCode::BAD_REQUEST, Json(json!({ "error": "No docker images configured" }))))?
        .clone();
    
    // Build environment variables from software defaults + user overrides
    let mut env = std::collections::HashMap::new();
    for var in &software.variables {
        env.insert(var.env_variable.clone(), var.default_value.clone());
    }
    for (key, value) in &payload.env {
        env.insert(key.clone(), value.clone());
    }
    env.insert("TENANT_ID".to_string(), tenant_id.clone());
    env.insert("SERVER_NAME".to_string(), payload.name.clone());
    
    // Get ports from software networking config
    let ports = software.networking
        .as_ref()
        .map(|n| n.port_binds.clone())
        .unwrap_or_default();
    
    // Build limits from node defaults
    let limits = ResourceLimits {
        cpu: Some(node.limits.cpu.clone()),
        memory: Some(node.limits.memory.clone()),
        disk: Some(node.limits.disk.clone()),
        swap: None,
        pids: Some(256),
        threads: Some(512),
    };
    
    // Parse startup command
    let startup_command_str = if !software.startup_cmd.is_empty() {
        // Replace variables in startup command
        let mut cmd = software.startup_cmd.clone();
        for (key, value) in &env {
            cmd = cmd.replace(&format!("${{{}}}", key), value);
            cmd = cmd.replace(&format!("${}", key), value);
        }
        cmd
    } else {
        "sleep infinity".to_string()
    };
    
    let startup_command = Some(vec!["sh".to_string(), "-c".to_string(), startup_command_str.clone()]);
    
    // Replace {{VAR}} with $VAR in install/update content so shell can use env vars
    let install_content = if software.install_content.is_empty() { 
        None 
    } else { 
        let mut content = software.install_content.clone();
        // Replace {{VAR}} with $VAR for shell variable expansion
        let re = regex::Regex::new(r"\{\{(\w+)\}\}").unwrap();
        content = re.replace_all(&content, r"$$$1").to_string();
        Some(content)
    };
    
    let update_content = if software.update_content.is_empty() { 
        None 
    } else { 
        let mut content = software.update_content.clone();
        let re = regex::Regex::new(r"\{\{(\w+)\}\}").unwrap();
        content = re.replace_all(&content, r"$$$1").to_string();
        Some(content)
    };
    
    // Build daemon request
    let container_request = CreateContainerRequest {
        image: docker_image.clone(),
        name: Some(payload.name.clone()),
        description: payload.description.clone(),
        startup_command,
        env: Some(env.clone()),
        ports: if ports.is_empty() { None } else { Some(ports.clone()) },
        limits: Some(limits.clone()),
        install_content,
        update_content,
    };
    
    // Create container on daemon
    let daemon_url = format!("{}://{}", node.network.scheme, node.network.uri);
    let daemon = crate::daemon::client::DaemonClient::new(daemon_url, "".to_string());
    
    tracing::info!("Sending create request to daemon");
    
    let container = daemon.create_container(container_request).await
        .map_err(|e| {
            tracing::error!("Daemon error: {}", e);
            (StatusCode::BAD_GATEWAY, Json(json!({ "error": format!("Failed to create container: {}", e) })))
        })?;
    
    tracing::info!("Container created: uuid={}, state={}", container.uuid, container.state);
    
    // Convert software port_binds to ServerPortConfig format
    let mut server_ports = std::collections::HashMap::new();
    for (port, _) in &ports {
        server_ports.insert(port.clone(), crate::models::server::ServerPortConfig {
            protocol: "tcp".to_string(),
            primary: port == "3000" || port == "8080", // Mark common ports as primary
        });
    }
    
    // Convert allocated ports from daemon response
    let allocated_ports: Vec<crate::models::server::AllocatedPort> = container.allocated_ports.clone()
        .into_iter()
        .map(|p| crate::models::server::AllocatedPort {
            container_port: p.container_port.to_string(),
            host_port: p.host_port.to_string(),
            host_ip: p.host_ip,
            protocol: p.protocol,
        })
        .collect();
    
    // Save to MongoDB
    let now = chrono::Utc::now();
    let server = Server {
        id: None,
        tenant_id: tenant_id.clone(),
        container_id: container.uuid.clone(),
        name: payload.name.clone(),
        description: payload.description.clone(),
        server_software_id: payload.server_software_id.clone(),
        node_id: payload.node_id.clone(),
        docker_image: docker_image.clone(),
        volumes: payload.volumes.clone(),
        network: crate::models::server::ServerNetwork {
            ports: server_ports,
            allocated_ports: Some(allocated_ports),
        },
        limits: crate::models::server::ServerLimits {
            cpu: limits.cpu.clone().unwrap_or_default(),
            memory: limits.memory.clone().unwrap_or_default(),
            disk: limits.disk.clone().unwrap_or_default(),
            pids: limits.pids.map(|p| p as i64),
            threads: limits.threads.map(|t| t as i64),
            ru_limit: None,  // Optional RU limit
        },
        env,
        startup: crate::models::server::ServerStartup {
            skip_install: payload.startup.as_ref().map(|s| s.skip_install).unwrap_or(false),
            skip_update: payload.startup.as_ref().map(|s| s.skip_update).unwrap_or(false),
            command: Some(startup_command_str),
        },
        logs_url: Some(format!("/api/containers/{}/logs", container.uuid)),
        created_at: now,
        updated_at: now,
    };
    
    let result = servers_collection.insert_one(&server).await
        .map_err(|e| {
            tracing::error!("MongoDB error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Failed to save server" })))
        })?;
    
    let server_id = result.inserted_id.as_object_id().unwrap().to_hex();
    
    tracing::info!("Server saved: id={}", server_id);
    
    // Audit log
    let _ = state.audit.log(
        user_id,
        "server:create".to_string(),
        json!({
            "tenantId": &tenant_id,
            "serverId": &server_id,
            "containerId": &container.uuid,
            "name": &payload.name,
        })
    ).await;
    
    // Get RU estimate for the server
    let ru_estimate = daemon.calculate_ru_estimate(
        &limits.cpu.clone().unwrap_or_default(),
        &limits.memory.clone().unwrap_or_default(),
        &limits.disk.clone().unwrap_or_default()
    ).await.ok();
    
    Ok(Json(json!({
        "id": server_id,
        "containerId": container.uuid,
        "name": payload.name,
        "dockerImage": docker_image,
        "state": container.state,
        "allocatedPorts": container.allocated_ports,
        "ruEstimate": ru_estimate.map(|e| json!({
            "ruPerHour": e.ru_per_hour,
            "ruPerDay": e.ru_per_day,
            "ruPerMonth": e.ru_per_month,
            "pricePerHour": e.price_per_hour,
            "pricePerMonth": e.price_per_month
        }))
    })))
}

/// Delete a server
pub async fn delete_server(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    let servers_collection = db.collection::<Server>("servers");
    let nodes_collection = db.collection::<crate::models::node::Node>("nodes");
    
    // Check membership
    let tenant = tenants_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Tenant not found" }))))?;
    
    if !tenant.members.iter().any(|m| m.user_id == user_id) {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Forbidden" }))));
    }
    
    // Get server
    let server = servers_collection
        .find_one(doc! {
            "_id": mongodb::bson::oid::ObjectId::parse_str(&server_id).ok(),
            "tenant_id": &tenant_id
        })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Server not found" }))))?;
    
    // Delete from daemon
    if let Ok(Some(node)) = nodes_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&server.node_id).ok() })
        .await
    {
        let daemon_url = format!("{}://{}", node.network.scheme, node.network.uri);
        let daemon = crate::daemon::client::DaemonClient::new(daemon_url, "".to_string());
        
        if let Err(e) = daemon.delete_container(&server.container_id).await {
            tracing::warn!("Failed to delete container from daemon: {}", e);
        }
    }
    
    // Delete from MongoDB
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
        json!({ "tenantId": &tenant_id, "serverId": &server_id })
    ).await;
    
    Ok(Json(json!({ "ok": true })))
}


/// Helper to get server and daemon client
async fn get_server_and_daemon(
    state: &Arc<AppState>,
    tenant_id: &str,
    server_id: &str,
    user_id: &str,
) -> Result<(Server, crate::daemon::client::DaemonClient), (StatusCode, Json<Value>)> {
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    let servers_collection = db.collection::<Server>("servers");
    let nodes_collection = db.collection::<crate::models::node::Node>("nodes");
    
    // Check membership
    let tenant = tenants_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(tenant_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Tenant not found" }))))?;
    
    if !tenant.members.iter().any(|m| m.user_id == user_id) {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Forbidden" }))));
    }
    
    // Get server
    let server = servers_collection
        .find_one(doc! {
            "_id": mongodb::bson::oid::ObjectId::parse_str(server_id).ok(),
            "tenant_id": tenant_id
        })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Server not found" }))))?;
    
    // Get node and create daemon client
    let node = nodes_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&server.node_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Node not found" }))))?;
    
    let daemon_url = format!("{}://{}", node.network.scheme, node.network.uri);
    let daemon = crate::daemon::client::DaemonClient::new(daemon_url, "".to_string());
    
    Ok((server, daemon))
}

/// Start a server
pub async fn start_server(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let (server, daemon) = get_server_and_daemon(&state, &tenant_id, &server_id, &user_id).await?;
    
    daemon.start_container(&server.container_id).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({ "error": e }))))?;
    
    Ok(Json(json!({ "ok": true })))
}

/// Stop a server
pub async fn stop_server(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let (server, daemon) = get_server_and_daemon(&state, &tenant_id, &server_id, &user_id).await?;
    
    daemon.stop_container(&server.container_id).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({ "error": e }))))?;
    
    Ok(Json(json!({ "ok": true })))
}

/// Restart a server (stop then start)
pub async fn restart_server(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let (server, daemon) = get_server_and_daemon(&state, &tenant_id, &server_id, &user_id).await?;
    
    // Stop first (ignore errors in case it's already stopped)
    let _ = daemon.stop_container(&server.container_id).await;
    
    // Wait a moment
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    // Start
    daemon.start_container(&server.container_id).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({ "error": e }))))?;
    
    Ok(Json(json!({ "ok": true })))
}

/// Kill a server (force stop)
pub async fn kill_server(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let (server, daemon) = get_server_and_daemon(&state, &tenant_id, &server_id, &user_id).await?;
    
    // Use stop with force - lightd doesn't have a separate kill endpoint via UUID
    daemon.stop_container(&server.container_id).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({ "error": e }))))?;
    
    Ok(Json(json!({ "ok": true })))
}

/// Get server logs
pub async fn get_server_logs(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let (server, daemon) = get_server_and_daemon(&state, &tenant_id, &server_id, &user_id).await?;
    
    let tail = params.get("tail").map(|s| s.as_str());
    
    let logs = daemon.get_container_logs(&server.container_id, tail).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({ "error": e }))))?;
    
    Ok(Json(json!({ "logs": logs })))
}


/// Get WebSocket credentials for a server
/// This generates a token from lightd and returns the WebSocket URL
pub async fn get_server_websocket(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    let servers_collection = db.collection::<Server>("servers");
    let nodes_collection = db.collection::<crate::models::node::Node>("nodes");
    
    // Check membership
    let tenant = tenants_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Tenant not found" }))))?;
    
    if !tenant.members.iter().any(|m| m.user_id == user_id) {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Forbidden" }))));
    }
    
    // Get server
    let server = servers_collection
        .find_one(doc! {
            "_id": mongodb::bson::oid::ObjectId::parse_str(&server_id).ok(),
            "tenant_id": &tenant_id
        })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Server not found" }))))?;
    
    // Get node
    let node = nodes_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&server.node_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Node not found" }))))?;
    
    // Generate token from lightd
    let daemon_url = format!("{}://{}", node.network.scheme, node.network.uri);
    let token_url = format!("{}/websocket/generate?container_id={}", daemon_url, server.container_id);
    
    let client = reqwest::Client::new();
    let response = client.get(&token_url).send().await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({ "error": format!("Failed to connect to daemon: {}", e) }))))?;
    
    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err((StatusCode::BAD_GATEWAY, Json(json!({ "error": format!("Daemon error: {}", error_text) }))));
    }
    
    let json: serde_json::Value = response.json().await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({ "error": format!("Invalid response: {}", e) }))))?;
    
    let success = json["success"].as_bool().unwrap_or(false);
    if !success {
        let msg = json["message"].as_str().unwrap_or("Unknown error");
        return Err((StatusCode::BAD_GATEWAY, Json(json!({ "error": msg }))));
    }
    
    let data = json.get("data").ok_or_else(|| (StatusCode::BAD_GATEWAY, Json(json!({ "error": "No data in response" }))))?;
    let token = data["token"].as_str().ok_or_else(|| (StatusCode::BAD_GATEWAY, Json(json!({ "error": "No token in response" }))))?;
    
    // Build WebSocket URL
    let ws_scheme = if node.network.scheme == "https" { "wss" } else { "ws" };
    let socket_url = format!("{}://{}/websocket?token={}", ws_scheme, node.network.uri, token);
    
    Ok(Json(json!({
        "token": token,
        "socket": socket_url,
        "expiresAt": data["expires_at"]
    })))
}


// ============================================================================
// Filesystem Routes
// ============================================================================

/// List files in server directory
pub async fn list_server_files(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let (server, daemon) = get_server_and_daemon(&state, &tenant_id, &server_id, &user_id).await?;
    
    let path = params.get("path").map(|s| s.as_str()).unwrap_or("/home/container");
    
    let result = daemon.list_files(&server.container_id, path).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({ "error": e }))))?;
    
    // The daemon already returns the data in the correct format (items with isDirectory, modifiedAt, etc.)
    // Just extract the data.items array and pass it through
    if let Some(data) = result.get("data") {
        // Check for new format first (items)
        if let Some(items) = data.get("items") {
            return Ok(Json(json!({ "items": items })));
        }
        // Fallback to old format (files) and transform
        if let Some(files) = data.get("files").and_then(|e| e.as_array()) {
            let items: Vec<serde_json::Value> = files.iter().map(|entry| {
                json!({
                    "name": entry.get("name").and_then(|n| n.as_str()).unwrap_or(""),
                    "isDirectory": entry.get("is_directory").and_then(|d| d.as_bool()).unwrap_or(false),
                    "size": entry.get("size").and_then(|s| s.as_u64()).unwrap_or(0),
                    "modifiedAt": entry.get("modified").and_then(|m| m.as_str()).unwrap_or("")
                })
            }).collect();
            
            return Ok(Json(json!({ "items": items })));
        }
    }
    
    Ok(Json(json!({ "items": [] })))
}

/// Read file content from server
pub async fn read_server_file(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let (server, daemon) = get_server_and_daemon(&state, &tenant_id, &server_id, &user_id).await?;
    
    let path = params.get("path").ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing path parameter" })))
    })?;
    
    let content = daemon.read_file(&server.container_id, path).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({ "error": e }))))?;
    
    Ok(Json(json!({ "content": content })))
}

/// Write file content to server
pub async fn write_server_file(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let (server, daemon) = get_server_and_daemon(&state, &tenant_id, &server_id, &user_id).await?;
    
    let path = payload.get("path").and_then(|p| p.as_str()).ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing path" })))
    })?;
    
    let content = payload.get("content").and_then(|c| c.as_str()).ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing content" })))
    })?;
    
    daemon.write_file(&server.container_id, path, content).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({ "error": e }))))?;
    
    Ok(Json(json!({ "ok": true })))
}

/// Delete file or directory from server
pub async fn delete_server_file(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let (server, daemon) = get_server_and_daemon(&state, &tenant_id, &server_id, &user_id).await?;
    
    let path = payload.get("path").and_then(|p| p.as_str()).ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing path" })))
    })?;
    
    daemon.delete_file(&server.container_id, path).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({ "error": e }))))?;
    
    Ok(Json(json!({ "ok": true })))
}

/// Create folder in server
pub async fn create_server_folder(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let (server, daemon) = get_server_and_daemon(&state, &tenant_id, &server_id, &user_id).await?;
    
    let path = payload.get("path").and_then(|p| p.as_str()).ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing path" })))
    })?;
    
    daemon.create_folder(&server.container_id, path).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({ "error": e }))))?;
    
    Ok(Json(json!({ "ok": true })))
}

/// Rename file in server
pub async fn rename_server_file(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let (server, daemon) = get_server_and_daemon(&state, &tenant_id, &server_id, &user_id).await?;
    
    let old_path = payload.get("oldPath").and_then(|p| p.as_str()).ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing oldPath" })))
    })?;
    
    let new_path = payload.get("newPath").and_then(|p| p.as_str()).ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing newPath" })))
    })?;
    
    daemon.rename_file(&server.container_id, old_path, new_path).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({ "error": e }))))?;
    
    Ok(Json(json!({ "ok": true })))
}

/// Copy file in server
pub async fn copy_server_file(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let (server, daemon) = get_server_and_daemon(&state, &tenant_id, &server_id, &user_id).await?;
    
    let source_path = payload.get("sourcePath").and_then(|p| p.as_str()).ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing sourcePath" })))
    })?;
    
    let destination_path = payload.get("destinationPath").and_then(|p| p.as_str()).ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing destinationPath" })))
    })?;
    
    daemon.copy_file(&server.container_id, source_path, destination_path).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({ "error": e }))))?;
    
    Ok(Json(json!({ "ok": true })))
}

/// Compress files/folders into archive
pub async fn compress_server_files(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let (server, daemon) = get_server_and_daemon(&state, &tenant_id, &server_id, &user_id).await?;
    
    // Support both sourcePaths (array) and sourcePath (single string) for backward compatibility
    let source_paths = if let Some(paths_array) = payload.get("sourcePaths").and_then(|p| p.as_array()) {
        paths_array.iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<String>>()
    } else if let Some(single_path) = payload.get("sourcePath").and_then(|p| p.as_str()) {
        vec![single_path.to_string()]
    } else {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing sourcePath or sourcePaths" }))));
    };
    
    if source_paths.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "sourcePaths cannot be empty" }))));
    }
    
    let archive_path = payload.get("archivePath").and_then(|p| p.as_str()).ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing archivePath" })))
    })?;
    
    let compression = payload.get("compression").and_then(|c| c.as_str()).unwrap_or("gzip");
    
    let message = daemon.compress_files(&server.container_id, &source_paths, archive_path, compression).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({ "error": e }))))?;
    
    Ok(Json(json!({ "ok": true, "message": message })))
}

/// Decompress archive
pub async fn decompress_server_file(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let (server, daemon) = get_server_and_daemon(&state, &tenant_id, &server_id, &user_id).await?;
    
    let archive_path = payload.get("archivePath").and_then(|p| p.as_str()).ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing archivePath" })))
    })?;
    
    let destination_path = payload.get("destinationPath").and_then(|p| p.as_str()).unwrap_or("/");
    
    let message = daemon.extract_archive(&server.container_id, archive_path, destination_path).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({ "error": e }))))?;
    
    Ok(Json(json!({ "ok": true, "message": message })))
}

/// Change file permissions
pub async fn chmod_server_file(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let (server, daemon) = get_server_and_daemon(&state, &tenant_id, &server_id, &user_id).await?;
    
    let path = payload.get("path").and_then(|p| p.as_str()).ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing path" })))
    })?;
    
    let permissions = payload.get("permissions").and_then(|p| p.as_str()).ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing permissions" })))
    })?;
    
    daemon.chmod(&server.container_id, path, permissions).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({ "error": e }))))?;
    
    Ok(Json(json!({ "ok": true })))
}

/// Change file ownership
pub async fn chown_server_file(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let (server, daemon) = get_server_and_daemon(&state, &tenant_id, &server_id, &user_id).await?;
    
    let path = payload.get("path").and_then(|p| p.as_str()).ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing path" })))
    })?;
    
    let owner = payload.get("owner").and_then(|o| o.as_str()).ok_or_else(|| {
        (StatusCode::BAD_REQUEST, Json(json!({ "error": "Missing owner" })))
    })?;
    
    daemon.chown(&server.container_id, path, owner).await
        .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({ "error": e }))))?;
    
    Ok(Json(json!({ "ok": true })))
}

/// Upload file to server
pub async fn upload_server_file(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path((tenant_id, server_id)): axum::extract::Path<(String, String)>,
    mut multipart: Multipart,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let (server, daemon) = get_server_and_daemon(&state, &tenant_id, &server_id, &user_id).await?;
    
    let mut uploaded_files = Vec::new();
    let mut target_path = "/".to_string();
    
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("Failed to read multipart: {}", e) })))
    })? {
        let name = field.name().unwrap_or("").to_string();
        
        if name == "path" {
            target_path = field.text().await.map_err(|e| {
                (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("Failed to read path: {}", e) })))
            })?;
            continue;
        }
        
        if name == "file" || name == "files" {
            let file_name = field.file_name().unwrap_or("uploaded_file").to_string();
            let data = field.bytes().await.map_err(|e| {
                (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("Failed to read file: {}", e) })))
            })?;
            
            // Build the full path
            let file_path = if target_path == "/" || target_path.is_empty() {
                format!("/{}", file_name)
            } else {
                format!("{}/{}", target_path.trim_end_matches('/'), file_name)
            };
            
            // Upload to daemon
            daemon.upload_file(&server.container_id, &file_path, &data).await
                .map_err(|e| (StatusCode::BAD_GATEWAY, Json(json!({ "error": e }))))?;
            
            uploaded_files.push(file_name);
        }
    }
    
    if uploaded_files.is_empty() {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "No files uploaded" }))));
    }
    
    Ok(Json(json!({ 
        "ok": true, 
        "message": format!("Uploaded {} file(s)", uploaded_files.len()),
        "files": uploaded_files
    })))
}
