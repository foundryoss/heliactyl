use axum::{
    extract::Extension,
    http::StatusCode,
    response::Json,
};
use serde_json::{json, Value};

use crate::middleware::auth::AuthUser;

pub async fn get_me(
    Extension(auth_user): Extension<AuthUser>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let user = auth_user.user;
    
    // Return user data wrapped in "user" property to match client expectations
    Ok(Json(json!({
        "user": {
            "id": user.id.map(|id| id.to_hex()),
            "email": user.email,
            "username": user.username,
            "discordId": user.discord_id,
            "isAdmin": user.is_admin,
            "createdAt": user.created_at,
            "updatedAt": user.updated_at,
            "data": user.data,
        }
    })))
}
