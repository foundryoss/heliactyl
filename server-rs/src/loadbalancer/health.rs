use super::balancer::LoadBalancer;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;

pub struct HealthChecker {
    load_balancer: Arc<LoadBalancer>,
    check_interval: Duration,
}

impl HealthChecker {
    pub fn new(load_balancer: Arc<LoadBalancer>, check_interval_secs: u64) -> Self {
        Self {
            load_balancer,
            check_interval: Duration::from_secs(check_interval_secs),
        }
    }

    pub async fn start(self) {
        let mut ticker = interval(self.check_interval);
        
        loop {
            ticker.tick().await;
            self.check_health().await;
        }
    }

    async fn check_health(&self) {
        let stats = self.load_balancer.get_stats().await;
        
        for stat in stats {
            let url = format!("{}/status", stat.url);
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();

            match client.get(&url).send().await {
                Ok(response) => {
                    let is_healthy = response.status().is_success();
                    if !is_healthy && stat.healthy {
                        tracing::warn!("Backend server {} is now unhealthy", stat.name);
                    } else if is_healthy && !stat.healthy {
                        tracing::info!("Backend server {} is now healthy", stat.name);
                    }
                    
                   
                   // self.load_balancer.update_server_health(&stat.name, is_healthy).await;
                }
                Err(e) => {
                    if stat.healthy {
                        tracing::warn!("Backend server {} health check failed: {}", stat.name, e);
                    }
                    self.load_balancer.update_server_health(&stat.name, false).await;
                }
            }
        }
    }
}
