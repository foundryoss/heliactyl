use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
};
use mongodb::bson::doc;
use serde_json::{json, Value};
use std::sync::Arc;

use crate::auth::password;
use crate::database::mongo::MongoClient;
use crate::database::session::SessionManager;
use crate::models::user::{RegisterRequest, RegisterResponse, LoginRequest, LoginResponse, User};
use crate::models::tenant::{Tenant, TenantMember};

pub struct AppState {
    pub mongo: Arc<MongoClient>,
    pub jwt_secret: String,
    pub session_manager: Arc<SessionManager>,
    pub audit: Arc<crate::database::audit::AuditService>,
    pub daemon_client: Arc<crate::daemon::DaemonClient>,
    pub config: Arc<crate::heli::parse::HeliConfig>,
}

pub async fn register(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, (StatusCode, Json<Value>)> {
    // Validate input
    if payload.email.is_empty() || payload.username.is_empty() || payload.password.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Email, username, and password are required" })),
        ));
    }

    if payload.password.len() < 6 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Password must be at least 6 characters" })),
        ));
    }

    let db = state.mongo.database();
    let users_collection = db.collection::<User>("users");

    // Check if user already exists
    let existing_user = users_collection
        .find_one(doc! {
            "$or": [
                { "email": &payload.email },
                { "username": &payload.username }
            ]
        })
        .await;

    match existing_user {
        Ok(Some(_)) => {
            return Err((
                StatusCode::CONFLICT,
                Json(json!({ "error": "User with this email or username already exists" })),
            ));
        }
        Ok(None) => {}
        Err(e) => {
            eprintln!("Database error: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Database error" })),
            ));
        }
    }

    // Hash password
    let password_hash = match password::hash_password(&payload.password) {
        Ok(hash) => hash,
        Err(e) => {
            eprintln!("Password hashing error: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to hash password" })),
            ));
        }
    };

    // Check if this is the first user (make them admin)
    let user_count = users_collection.count_documents(doc! {}).await.unwrap_or(0);
    let is_first_user = user_count == 0;

    // Create user
    let mut user = User::new(payload.email.clone(), payload.username.clone(), password_hash);
    if is_first_user {
        user.is_admin = true;
        tracing::info!("First user registered - granting admin privileges");
    }

    // Insert user into database
    let insert_result = users_collection.insert_one(&user).await;

    match insert_result {
        Ok(result) => {
            let user_id = result.inserted_id.as_object_id().unwrap().to_hex();

            // Create session token
            let token = match state.session_manager.create_session(&user_id, 7) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("Session creation error: {}", e);
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": "Failed to create session" })),
                    ));
                }
            };

            // Auto-create default tenant for the user (best-effort)
            let tenants_collection = db.collection::<Tenant>("tenants");
            
            // Get default package from config
            let default_package = if let Ok(heli_config) = crate::heli::parse::HeliConfig::parse("./config.heli") {
                heli_config.get_string("packages.defaultPackage").unwrap_or_else(|| "free".to_string())
            } else {
                "free".to_string()
            };
            
            let default_tenant = Tenant {
                id: None,
                name: "Default".to_string(),
                owner_user_id: user_id.clone(),
                package_id: default_package,
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
            
            // Best-effort tenant creation (don't fail registration if this fails)
            if let Ok(tenant_result) = tenants_collection.insert_one(&default_tenant).await {
                let tenant_id = tenant_result.inserted_id.as_object_id().unwrap().to_hex();
                
                // Audit log tenant creation
                let _ = state.audit.log(
                    user_id.clone(),
                    "tenant:create:auto".to_string(),
                    json!({
                        "tenantId": tenant_id,
                        "name": default_tenant.name,
                        "packageId": default_tenant.package_id
                    })
                ).await;
            }

            // Audit log the registration
            let _ = state.audit.log(
                user_id.clone(),
                "user:register".to_string(),
                json!({
                    "email": payload.email,
                    "username": payload.username
                })
            ).await;

            Ok(Json(RegisterResponse { token, user_id }))
        }
        Err(e) => {
            eprintln!("Database insert error: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to create user" })),
            ))
        }
    }
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<Value>)> {
    // Validate input
    if payload.identifier.is_empty() || payload.password.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "Email and password are required" })),
        ));
    }

    let db = state.mongo.database();
    let users_collection = db.collection::<User>("users");

    // Find user by email
    let user = users_collection
        .find_one(doc! { "email": &payload.identifier })
        .await;

    let user = match user {
        Ok(Some(u)) => u,
        Ok(None) => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "Invalid credentials" })),
            ));
        }
        Err(e) => {
            eprintln!("Database error: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Database error" })),
            ));
        }
    };

    // Verify password
    let password_valid = match password::verify_password(&payload.password, &user.password_hash) {
        Ok(valid) => valid,
        Err(e) => {
            eprintln!("Password verification error: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Authentication error" })),
            ));
        }
    };

    if !password_valid {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "Invalid credentials" })),
        ));
    }

    // Create session token
    let user_id = user.id.unwrap().to_hex();
    let token = match state.session_manager.create_session(&user_id, 7) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Session creation error: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "Failed to create session" })),
            ));
        }
    };

    // Audit log the login
    let _ = state.audit.log(
        user_id.clone(),
        "user:login".to_string(),
        json!({
            "email": payload.identifier,
            "ip": "unknown", // Can be extracted from headers if needed
            "success": true
        })
    ).await;

    Ok(Json(LoginResponse { token, user_id }))
}
