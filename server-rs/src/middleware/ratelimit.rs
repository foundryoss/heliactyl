use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::net::SocketAddr;
use std::sync::Arc;
use crate::database::redis::RedisClient;

#[derive(Clone)]
pub struct RateLimiter {
    redis: Arc<RedisClient>,
    per_sec: u32,
    per_min: u32,
}

impl RateLimiter {
    pub fn new(redis_url: &str, per_sec: u32, per_min: u32) -> Result<Self, redis::RedisError> {
        let redis = RedisClient::new(redis_url)?;
        Ok(Self {
            redis: Arc::new(redis),
            per_sec,
            per_min,
        })
    }

    pub async fn check(&self, ip: &str) -> Result<(), StatusCode> {
        match self.redis.check_rate_limit(ip, self.per_sec, self.per_min) {
            Ok(true) => Ok(()),
            Ok(false) => Err(StatusCode::TOO_MANY_REQUESTS),
            Err(_) => {
                // If Redis fails, allow the request (fail open)
                eprintln!("Redis error during rate limit check");
                Ok(())
            }
        }
    }
}

pub async fn rate_limit_middleware(
    limiter: Arc<RateLimiter>,
    req: Request,
    next: Next,
) -> Response {
    // Skip rate limiting for daemon/lightd routes (they use API key auth)
    let path = req.uri().path();
    if path.starts_with("/api/lightd/") {
        return next.run(req).await;
    }

    // Extract IP from connection info or headers
    let ip = req
        .extensions()
        .get::<SocketAddr>()
        .map(|addr| addr.ip().to_string())
        .or_else(|| {
            req.headers()
                .get("x-forwarded-for")
                .and_then(|h| h.to_str().ok())
                .map(|s| s.split(',').next().unwrap_or("unknown").trim().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string());

    match limiter.check(&ip).await {
        Ok(_) => next.run(req).await,
        Err(status) => (
            status,
            "Rate limit exceeded. Please slow down your requests.",
        )
            .into_response(),
    }
}
