use redis::{Commands, RedisError};
use crate::database::redis::RedisClient;
use uuid::Uuid;

pub struct SessionManager {
    redis: RedisClient,
}

impl SessionManager {
    pub fn new(redis_url: &str) -> Result<Self, RedisError> {
        let redis = RedisClient::new(redis_url)?;
        Ok(Self { redis })
    }

    /// Create a new session token for a user
    pub fn create_session(&self, user_id: &str, expiry_days: u64) -> Result<String, RedisError> {
    let token = Uuid::new_v4().to_string();
    let key = format!("session:{}", token);
    let expiry_secs = expiry_days * 24 * 60 * 60;
    
    let mut conn = self.redis.client.get_connection()?;
    
    // Store user_id with the token
    let _: () = conn.set_ex(&key, user_id, expiry_secs)?;
    
    Ok(token)
}

    /// Get user_id from session token
    pub fn get_session(&self, token: &str) -> Result<Option<String>, RedisError> {
        let key = format!("session:{}", token);
        let mut conn = self.redis.client.get_connection()?;
        
        let user_id: Option<String> = conn.get(&key)?;
        Ok(user_id)
    }

    /// Delete a session
    pub fn delete_session(&self, token: &str) -> Result<(), RedisError> {
        let key = format!("session:{}", token);
        let mut conn = self.redis.client.get_connection()?;
        
        let _: () = conn.del(&key)?;
        Ok(())
    }

    /// Extend session expiry
    pub fn extend_session(&self, token: &str, expiry_days: u64) -> Result<bool, RedisError> {
        let key = format!("session:{}", token);
        let expiry_secs = expiry_days * 24 * 60 * 60;
        let mut conn = self.redis.client.get_connection()?;
        
        let result: bool = conn.expire(&key, expiry_secs as i64)?;
        Ok(result)
    }
}
