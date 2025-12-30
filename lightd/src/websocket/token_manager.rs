use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};
use sha2::{Sha256, Digest};
use base64::{Engine as _, engine::general_purpose};
use rand::Rng;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketToken {
    pub token: String,
    pub container_id: String,
    pub container_uuid: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub is_active: bool,
}

#[derive(Debug, Clone)]
pub struct TokenManager {
    tokens: Arc<RwLock<HashMap<String, WebSocketToken>>>,
}

impl TokenManager {
    pub fn new() -> Self {
        Self {
            tokens: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Generate a new WebSocket token for a container
    pub async fn generate_token(&self, container_id: String, container_uuid: String) -> anyhow::Result<WebSocketToken> {
        // Generate token synchronously before any await points
        let ws_token = {
            let random_bytes: [u8; 32] = rand::thread_rng().gen();
            
            // Create token payload
            let payload = format!("{}:{}:{}", container_id, container_uuid, Utc::now().timestamp());
            let mut hasher = Sha256::new();
            hasher.update(payload.as_bytes());
            hasher.update(&random_bytes);
            let hash = hasher.finalize();
            
            let token = general_purpose::URL_SAFE_NO_PAD.encode(hash);
            let now = Utc::now();
            let expires_at = now + Duration::hours(24); // Token valid for 24 hours
            
            WebSocketToken {
                token,
                container_id,
                container_uuid,
                created_at: now,
                expires_at,
                is_active: true,
            }
        };
        
        // Store token (await happens after RNG is dropped)
        let mut tokens = self.tokens.write().await;
        tokens.insert(ws_token.token.clone(), ws_token.clone());
        
        // Clean up expired tokens
        self.cleanup_expired_tokens(&mut tokens).await;
        
        Ok(ws_token)
    }

    /// Validate a WebSocket token
    pub async fn validate_token(&self, token: &str) -> Option<WebSocketToken> {
        let tokens = self.tokens.read().await;
        if let Some(ws_token) = tokens.get(token) {
            if ws_token.is_active && ws_token.expires_at > Utc::now() {
                return Some(ws_token.clone());
            }
        }
        None
    }

    /// Revoke a token
    pub async fn revoke_token(&self, token: &str) -> bool {
        let mut tokens = self.tokens.write().await;
        if let Some(ws_token) = tokens.get_mut(token) {
            ws_token.is_active = false;
            return true;
        }
        false
    }

    /// Get all active tokens for a container
    pub async fn get_container_tokens(&self, container_id: &str) -> Vec<WebSocketToken> {
        let tokens = self.tokens.read().await;
        tokens
            .values()
            .filter(|token| {
                (token.container_id == container_id || token.container_uuid == container_id) 
                && token.is_active 
                && token.expires_at > Utc::now()
            })
            .cloned()
            .collect()
    }

    /// Clean up expired tokens
    async fn cleanup_expired_tokens(&self, tokens: &mut HashMap<String, WebSocketToken>) {
        let now = Utc::now();
        tokens.retain(|_, token| token.expires_at > now);
    }

    /// Periodic cleanup task
    pub async fn start_cleanup_task(self: Arc<Self>) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(3600)); // Clean every hour
        
        loop {
            interval.tick().await;
            let mut tokens = self.tokens.write().await;
            self.cleanup_expired_tokens(&mut tokens).await;
            tracing::info!("Cleaned up expired WebSocket tokens. Active tokens: {}", tokens.len());
        }
    }
}

impl Default for TokenManager {
    fn default() -> Self {
        Self::new()
    }
}