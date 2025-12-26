use redis::{Client, Commands, RedisError};

pub struct RedisClient {
    pub client: Client,
}

impl RedisClient {
    pub fn new(url: &str) -> Result<Self, RedisError> {
        let client = Client::open(url)?;
        Ok(Self { client })
    }

    /// Increment a counter with expiration
    pub fn increment_with_expiry(&self, key: &str, expiry_secs: u64) -> Result<i64, RedisError> {
        let mut conn = self.client.get_connection()?;
        
        // Increment the key
        let count: i64 = conn.incr(key, 1)?;
        
        // Set expiry only if this is the first increment (count == 1)
        if count == 1 {
            let _: () = conn.expire(key, expiry_secs as i64)?;
        }
        
        Ok(count)
    }

    /// Check rate limit for an IP
    pub fn check_rate_limit(&self, ip: &str, per_sec: u32, per_min: u32) -> Result<bool, RedisError> {
        let sec_key = format!("ratelimit:sec:{}", ip);
        let min_key = format!("ratelimit:min:{}", ip);
        
        // Check per-second limit
        let sec_count = self.increment_with_expiry(&sec_key, 1)?;
        if sec_count > per_sec as i64 {
            return Ok(false);
        }
        
        // Check per-minute limit
        let min_count = self.increment_with_expiry(&min_key, 60)?;
        if min_count > per_min as i64 {
            return Ok(false);
        }
        
        Ok(true)
    }

    /// Test connection
    pub fn ping(&self) -> Result<String, RedisError> {
        let mut conn = self.client.get_connection()?;
        redis::cmd("PING").query(&mut conn)
    }
}
