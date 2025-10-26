use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct BackendServer {
    pub name: String,
    pub host: String,
    pub port: u16,
    pub active_connections: Arc<RwLock<u32>>,
    pub healthy: Arc<RwLock<bool>>,
    pub total_requests: Arc<RwLock<u64>>,
}

impl BackendServer {
    pub fn new(name: String, host: String, port: u16) -> Self {
        Self {
            name,
            host,
            port,
            active_connections: Arc::new(RwLock::new(0)),
            healthy: Arc::new(RwLock::new(true)),
            total_requests: Arc::new(RwLock::new(0)),
        }
    }

    pub fn url(&self) -> String {
        format!("http://{}:{}", self.host, self.port)
    }

    pub async fn increment_connections(&self) {
        let mut conns = self.active_connections.write().await;
        *conns += 1;
        let mut total = self.total_requests.write().await;
        *total += 1;
    }

    pub async fn decrement_connections(&self) {
        let mut conns = self.active_connections.write().await;
        if *conns > 0 {
            *conns -= 1;
        }
    }

    pub async fn get_load(&self) -> u32 {
        *self.active_connections.read().await
    }

    pub async fn is_healthy(&self) -> bool {
        *self.healthy.read().await
    }

    pub async fn set_healthy(&self, healthy: bool) {
        let mut h = self.healthy.write().await;
        *h = healthy;
    }
}

#[derive(Clone)]
pub struct LoadBalancer {
    servers: Arc<RwLock<Vec<BackendServer>>>,
    strategy: LoadBalancingStrategy,
}

#[derive(Clone, Debug)]
pub enum LoadBalancingStrategy {
    LeastConnections,
    RoundRobin,
}

impl LoadBalancer {
    pub fn new(servers: Vec<BackendServer>, strategy: LoadBalancingStrategy) -> Self {
        Self {
            servers: Arc::new(RwLock::new(servers)),
            strategy,
        }
    }

    pub async fn get_next_server(&self) -> Option<BackendServer> {
        let servers = self.servers.read().await;
        
        // Filter healthy servers
        let mut healthy_servers: Vec<_> = Vec::new();
        for server in servers.iter() {
            if server.is_healthy().await {
                healthy_servers.push(server.clone());
            }
        }

        if healthy_servers.is_empty() {
            return None;
        }

        match self.strategy {
            LoadBalancingStrategy::LeastConnections => {
                self.least_connections_server(healthy_servers).await
            }
            LoadBalancingStrategy::RoundRobin => {
                // For simplicity, just pick the first healthy server
                // In production, you'd maintain a counter
                healthy_servers.first().cloned()
            }
        }
    }

    async fn least_connections_server(&self, servers: Vec<BackendServer>) -> Option<BackendServer> {
        let mut min_load = u32::MAX;
        let mut selected = None;

        for server in servers {
            let load = server.get_load().await;
            if load < min_load {
                min_load = load;
                selected = Some(server);
            }
        }

        selected
    }

    pub async fn get_stats(&self) -> Vec<ServerStats> {
        let servers = self.servers.read().await;
        let mut stats = Vec::new();

        for server in servers.iter() {
            stats.push(ServerStats {
                name: server.name.clone(),
                url: server.url(),
                active_connections: server.get_load().await,
                healthy: server.is_healthy().await,
                total_requests: *server.total_requests.read().await,
            });
        }

        stats
    }

    pub async fn update_server_health(&self, name: &str, healthy: bool) {
        let servers = self.servers.read().await;
        if let Some(server) = servers.iter().find(|s| s.name == name) {
            server.set_healthy(healthy).await;
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub struct ServerStats {
    pub name: String,
    pub url: String,
    pub active_connections: u32,
    pub healthy: bool,
    pub total_requests: u64,
}

// Do not remove -- used in config parsing
pub struct LoadBalancerConfig {
    pub servers: HashMap<String, ServerConfig>,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}
