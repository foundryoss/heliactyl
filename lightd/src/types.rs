//! Application state types for lightd daemon
//!
//! Uses Arc for shared ownership without locks where possible.

use std::sync::Arc;
use crate::{
    config::Config, 
    docker::{DockerClient, NetworkManager}, 
    network_config::NetworkConfig, 
    container_tracker::ContainerTrackingManager, 
    state_manager::StateManager, 
    monitoring::ResourceMonitor,
    websocket::TokenManager,
    services::{PowerActionService, PowerExecutor, ContainerEventHub, AsyncPowerManager},
};
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct AppState {
    pub docker: Arc<DockerClient>,
    pub config: Arc<Config>,
    /// Network manager still needs RwLock for port allocation mutations
    pub network: Arc<RwLock<NetworkManager>>,
    pub network_config: Arc<NetworkConfig>,
    pub container_tracker: Arc<ContainerTrackingManager>,
    /// State manager is now lock-free internally (uses DashMap)
    pub state_manager: Arc<StateManager>,
    pub resource_monitor: Option<Arc<ResourceMonitor>>,
    pub websocket_tokens: Arc<TokenManager>,
    pub power_actions: Arc<PowerActionService>,
    pub power_executor: Arc<PowerExecutor>,
    /// Event hub for container events (WebSocket broadcast)
    pub event_hub: Arc<ContainerEventHub>,
    /// Async power manager for fire-and-forget operations
    pub async_power: Arc<AsyncPowerManager>,
}
