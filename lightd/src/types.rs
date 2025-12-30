use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use std::collections::HashMap;
use tokio::sync::broadcast;
use crate::{
    config::Config, 
    docker::{DockerClient, NetworkManager}, 
    network_config::NetworkConfig, 
    container_tracker::ContainerTrackingManager, 
    state_manager::StateManager, 
    monitoring::ResourceMonitor,
    websocket::TokenManager,
    models::ContainerStats,
};

#[derive(Clone)]
pub struct AppState {
    pub docker: Arc<DockerClient>,
    pub config: Arc<Config>,
    pub network: Arc<Mutex<NetworkManager>>,
    pub network_config: Arc<NetworkConfig>,
    pub container_tracker: Arc<ContainerTrackingManager>,
    pub state_manager: Arc<Mutex<StateManager>>,
    pub resource_monitor: Option<Arc<ResourceMonitor>>,
    pub websocket_tokens: Arc<TokenManager>,
    pub websocket_broadcasters: Arc<RwLock<HashMap<String, (
        broadcast::Sender<ContainerStats>,
        broadcast::Sender<String>,
        broadcast::Sender<String>,
    )>>>,
}