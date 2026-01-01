use axum::{
    extract::{Extension, State, Path},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use mongodb::bson::doc;

use crate::{heli, middleware::auth::AuthUser};
use crate::auth::oauth::AppState;
use crate::models::tenant::{Tenant, TenantMember, CreateTenantRequest, TenantResponse, ResourcesResponse};
use crate::models::server::Server;
use crate::utils::resources::{get_package_resources, sum_used_resources, remaining_resources, parse_memory_to_mb, parse_disk_to_mb, parse_cpu_to_percent};
use futures_util::StreamExt;

#[derive(Debug, Deserialize)]
pub struct AddMemberRequest {
    #[serde(rename = "userId")]
    pub user_id: String,
}

#[derive(Debug, Deserialize)]
pub struct AddMemberByEmailRequest {
    pub email: String,
}

#[derive(Debug, Deserialize)]
pub struct TransferOwnershipRequest {
    #[serde(rename = "userId")]
    pub user_id: String,
}

/// List tenants for the current user
pub async fn list_tenants(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    
    // Find all tenants where user is a member
    let mut cursor = tenants_collection
        .find(doc! { "members.user_id": &user_id })
        .await
        .map_err(|e| {
            tracing::error!("Failed to query tenants: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" })))
        })?;
    
    let mut tenants = Vec::new();
    while let Some(result) = cursor.next().await {
        if let Ok(tenant) = result {
            // Find user's role in this tenant
            let role = tenant.members.iter()
                .find(|m| m.user_id == user_id)
                .map(|m| m.role.clone())
                .unwrap_or_else(|| "user".to_string());
            
            tenants.push(json!({
                "id": tenant.id.map(|id| id.to_hex()),
                "name": tenant.name,
                "ownerUserId": tenant.owner_user_id,
                "packageId": tenant.package_id,
                "role": role,
                "createdAt": tenant.created_at.to_rfc3339(),
                "updatedAt": tenant.updated_at.to_rfc3339(),
            }));
        }
    }
    
    Ok(Json(json!({ "items": tenants })))
}

/// Create a new tenant
pub async fn create_tenant(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateTenantRequest>,
) -> Result<Json<TenantResponse>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let package_id = payload.package_id.unwrap_or_else(|| "free".to_string());
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");

    let heli_config = heli::parse::HeliConfig::parse("./config.heli")
        .expect(&format!("Failed to parse config file: {}", "./config.heli"));
    
    // Check tenant limit (default 100, can be configured later)
    // TODO: status - done : Implemented
    let max_tenants = heli_config.get_int(".max_tenant").unwrap_or(100);
    let tenant_count = tenants_collection
        .count_documents(doc! { "members.user_id": &user_id })
        .await
        .unwrap_or(0);

    if tenant_count >= max_tenants as u64 {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "Tenant limit reached" })),
        ));
    }
    
    // Create tenant with owner as first member
    let tenant = Tenant {
        id: None,
        name: payload.name.clone(),
        owner_user_id: user_id.clone(),
        package_id: package_id.clone(),
        extra_memory_mb: 0,
        extra_disk_mb: 0,
        extra_cpu_percent: 0,
        extra_server_slots: 0,
        members: vec![TenantMember {
            user_id: user_id.clone(),
            role: "owner".to_string(),
        }],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    let result = tenants_collection.insert_one(&tenant).await
        .map_err(|e| {
            tracing::error!("Failed to create tenant: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Failed to create tenant" })))
        })?;
    
    let tenant_id = result.inserted_id.as_object_id().unwrap().to_hex();
    
    // Audit log
    let _ = state.audit.log(
        user_id,
        "tenant:create".to_string(),
        json!({ "tenantId": &tenant_id, "name": payload.name })
    ).await;
    
    Ok(Json(TenantResponse {
        id: tenant_id,
        name: payload.name,
        package_id,
    }))
}

/// Get tenant members
pub async fn get_tenant_members(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    let users_collection = db.collection::<crate::models::user::User>("users");
    
    // Get tenant and check membership
    let tenant = tenants_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Tenant not found" }))))?;
    
    // Check if user is a member
    if !tenant.members.iter().any(|m| m.user_id == user_id) {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Forbidden" }))));
    }
    
    // Get user info for all members
    let mut members_data = Vec::new();
    for member in &tenant.members {
        let user = users_collection
            .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&member.user_id).ok() })
            .await
            .ok()
            .flatten();
        
        members_data.push(json!({
            "userId": member.user_id,
            "role": member.role,
            "email": user.as_ref().map(|u| u.email.clone()),
            "username": user.as_ref().map(|u| u.username.clone()),
        }));
    }
    
    Ok(Json(json!({ "items": members_data })))
}

/// Get tenant resources
pub async fn get_tenant_resources(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    axum::extract::Path(tenant_id): axum::extract::Path<String>,
) -> Result<Json<ResourcesResponse>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    let servers_collection = db.collection::<Server>("servers");
    
    // Get tenant and check membership
    let tenant = tenants_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Tenant not found" }))))?;
    
    // Check if user is a member
    if !tenant.members.iter().any(|m| m.user_id == user_id) {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Forbidden" }))));
    }
    
    // Get resource usage from MongoDB servers
    let servers_collection = db.collection::<crate::models::server::Server>("servers");
    let mut cursor = servers_collection
        .find(doc! { "tenant_id": &tenant_id })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?;
    
    // Calculate used resources from our servers
    let mut servers_data = Vec::new();
    while let Some(result) = cursor.next().await {
        if let Ok(server) = result {
            // Parse limits from server record
            let memory_mb = parse_memory_to_mb(&server.limits.memory);
            let disk_mb = parse_disk_to_mb(&server.limits.disk);
            let cpu_percent = parse_cpu_to_percent(&server.limits.cpu);
            servers_data.push((memory_mb, disk_mb, cpu_percent));
        }
    }
    
    let package = get_package_resources(&tenant.package_id);
    let extra = crate::models::tenant::Resources {
        memory_mb: tenant.extra_memory_mb,
        disk_mb: tenant.extra_disk_mb,
        cpu_percent: tenant.extra_cpu_percent,
        server_slots: tenant.extra_server_slots,
    };
    let used = sum_used_resources(&servers_data);
    let remaining = remaining_resources(&package, &extra, &used);
    
    Ok(Json(ResourcesResponse {
        package,
        extra,
        used,
        remaining,
    }))
}

/// Add a member to a tenant (owner only)
pub async fn add_tenant_member(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Path(tenant_id): Path<String>,
    Json(payload): Json<AddMemberRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    
    // Get tenant and check if user is owner
    let tenant = tenants_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Tenant not found" }))))?;
    
    // Check if user is owner
    let is_owner = tenant.members.iter().any(|m| m.user_id == user_id && m.role == "owner");
    if !is_owner {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Forbidden" }))));
    }
    
    // Check if member already exists
    if tenant.members.iter().any(|m| m.user_id == payload.user_id) {
        return Ok(Json(json!({ "ok": true })));
    }
    
    // Add member
    let new_member = TenantMember {
        user_id: payload.user_id.clone(),
        role: "user".to_string(),
    };
    
    tenants_collection
        .update_one(
            doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() },
            doc! { "$push": { "members": mongodb::bson::to_bson(&new_member).unwrap() } }
        )
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Failed to add member" }))))?;
    
    // Audit log
    let _ = state.audit.log(
        user_id,
        "tenant:member:add".to_string(),
        json!({ "tenantId": &tenant_id, "userId": payload.user_id })
    ).await;
    
    Ok(Json(json!({ "ok": true })))
}

/// Add a member to a tenant by email (owner only)
pub async fn add_tenant_member_by_email(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Path(tenant_id): Path<String>,
    Json(payload): Json<AddMemberByEmailRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    let users_collection = db.collection::<crate::models::user::User>("users");
    
    // Get tenant and check if user is owner
    let tenant = tenants_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Tenant not found" }))))?;
    
    // Check if user is owner
    let is_owner = tenant.members.iter().any(|m| m.user_id == user_id && m.role == "owner");
    if !is_owner {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Forbidden" }))));
    }
    
    // Find user by email
    let target_user = users_collection
        .find_one(doc! { "email": payload.email.to_lowercase() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "User not found" }))))?;
    
    let target_user_id = target_user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    // Check if member already exists
    if tenant.members.iter().any(|m| m.user_id == target_user_id) {
        return Ok(Json(json!({ "ok": true })));
    }
    
    // Add member
    let new_member = TenantMember {
        user_id: target_user_id.clone(),
        role: "user".to_string(),
    };
    
    tenants_collection
        .update_one(
            doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() },
            doc! { "$push": { "members": mongodb::bson::to_bson(&new_member).unwrap() } }
        )
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Failed to add member" }))))?;
    
    // Audit log
    let _ = state.audit.log(
        user_id,
        "tenant:member:add".to_string(),
        json!({ "tenantId": &tenant_id, "userId": target_user_id })
    ).await;
    
    Ok(Json(json!({ "ok": true })))
}

/// Remove a member from a tenant (owner only)
pub async fn remove_tenant_member(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Path((tenant_id, member_user_id)): Path<(String, String)>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    
    // Get tenant and check if user is owner
    let tenant = tenants_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Tenant not found" }))))?;
    
    // Check if user is owner
    let is_owner = tenant.members.iter().any(|m| m.user_id == user_id && m.role == "owner");
    if !is_owner {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Forbidden" }))));
    }
    
    // Remove member
    tenants_collection
        .update_one(
            doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() },
            doc! { "$pull": { "members": { "user_id": &member_user_id } } }
        )
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Failed to remove member" }))))?;
    
    // Audit log
    let _ = state.audit.log(
        user_id,
        "tenant:member:remove".to_string(),
        json!({ "tenantId": &tenant_id, "userId": member_user_id })
    ).await;
    
    Ok(Json(json!({ "ok": true })))
}

/// Transfer tenant ownership to another member (owner only)
pub async fn transfer_tenant_ownership(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Path(tenant_id): Path<String>,
    Json(payload): Json<TransferOwnershipRequest>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    
    // Get tenant and check if user is owner
    let mut tenant = tenants_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Tenant not found" }))))?;
    
    // Check if user is owner
    let is_owner = tenant.members.iter().any(|m| m.user_id == user_id && m.role == "owner");
    if !is_owner {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Forbidden" }))));
    }
    
    // Check if new owner is a member
    if !tenant.members.iter().any(|m| m.user_id == payload.user_id) {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": "New owner must be a member" }))));
    }
    
    // Update roles
    for member in &mut tenant.members {
        if member.user_id == user_id {
            member.role = "user".to_string();
        } else if member.user_id == payload.user_id {
            member.role = "owner".to_string();
        }
    }
    
    // Update tenant
    let now = mongodb::bson::DateTime::now();
    tenants_collection
        .update_one(
            doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() },
            doc! { 
                "$set": { 
                    "owner_user_id": &payload.user_id,
                    "members": mongodb::bson::to_bson(&tenant.members).unwrap(),
                    "updated_at": now
                } 
            }
        )
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Failed to transfer ownership" }))))?;
    
    // Audit log
    let _ = state.audit.log(
        user_id,
        "tenant:ownership:transfer".to_string(),
        json!({ "tenantId": &tenant_id, "newOwnerId": payload.user_id })
    ).await;
    
    Ok(Json(json!({ "ok": true })))
}

/// Delete a tenant and all its servers (owner only)
pub async fn delete_tenant(
    Extension(auth_user): Extension<AuthUser>,
    State(state): State<Arc<AppState>>,
    Path(tenant_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user_id = auth_user.user.id.map(|id| id.to_hex()).unwrap_or_default();
    
    let db = state.mongo.database();
    let tenants_collection = db.collection::<Tenant>("tenants");
    let servers_collection = db.collection::<Server>("servers");
    
    // Get tenant and check if user is owner
    let tenant = tenants_collection
        .find_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Database error" }))))?
        .ok_or_else(|| (StatusCode::NOT_FOUND, Json(json!({ "error": "Tenant not found" }))))?;
    
    // Check if user is owner
    let is_owner = tenant.members.iter().any(|m| m.user_id == user_id && m.role == "owner");
    if !is_owner {
        return Err((StatusCode::FORBIDDEN, Json(json!({ "error": "Forbidden" }))));
    }
    
    // Delete all servers for this tenant
    // Note: In production, you'd also want to delete servers from Pterodactyl
    servers_collection
        .delete_many(doc! { "tenant_id": &tenant_id })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Failed to delete servers" }))))?;
    
    // Delete tenant
    tenants_collection
        .delete_one(doc! { "_id": mongodb::bson::oid::ObjectId::parse_str(&tenant_id).ok() })
        .await
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": "Failed to delete tenant" }))))?;
    
    // Audit log
    let _ = state.audit.log(
        user_id,
        "tenant:delete".to_string(),
        json!({ "tenantId": &tenant_id })
    ).await;
    
    Ok(Json(json!({ "ok": true })))
}
