use mongodb::bson::{doc, oid::ObjectId, DateTime, Document};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct User {
    #[serde(rename = "_id", skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    pub email: String,
    pub username: String,
    #[serde(rename = "passwordHash")]
    pub password_hash: String,
    #[serde(rename = "discordId", skip_serializing_if = "Option::is_none")]
    pub discord_id: Option<String>,
    #[serde(rename = "isAdmin")]
    pub is_admin: bool,
    #[serde(rename = "createdAt")]
    pub created_at: DateTime,
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime,
    #[serde(default)]
    pub data: Vec<Value>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub identifier: String, // Changed from email to identifier
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub token: String,
    #[serde(rename = "userId")]
    pub user_id: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    #[serde(rename = "userId")]
    pub user_id: String,
}

impl User {
    pub fn new(email: String, username: String, password_hash: String) -> Self {
        let now = DateTime::now();
        Self {
            id: None,
            email,
            username,
            password_hash,
            discord_id: None,
            is_admin: false,
            created_at: now,
            updated_at: now,
            data: Vec::new(),
        }
    }
}
