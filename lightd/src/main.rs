use axum::{
    extract::State,
    response::Json,
    routing::{delete, get, post, put},
    Router,
};
use clap::{Parser, Subcommand};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;
use tracing::info;
use bollard::{
    network::{CreateNetworkOptions, ListNetworksOptions},
    container::ListContainersOptions,
    Docker,
};

mod config;
mod docker;
mod handlers;
mod models;
mod types;
mod network_config;
mod container_tracker;
mod state_manager;
mod monitoring;
mod websocket;
mod services;

use container_tracker::ContainerTrackingManager;
use state_manager::StateManager;
use config::Config;
use docker::{DockerClient, NetworkManager};
use types::AppState;
use services::{PowerActionService, PowerExecutor, ContainerEventHub, AsyncPowerManager};

async fn perform_daemon_recovery(docker: &DockerClient, state_manager: &StateManager) -> anyhow::Result<()> {
    info!("Starting daemon recovery process...");

    // Get all active Docker containers
    let options = ListContainersOptions::<String> {
        all: true,
        ..Default::default()
    };

    let containers = docker.client.list_containers(Some(options)).await?;
    let active_container_ids: Vec<String> = containers
        .iter()
        .filter_map(|c| c.id.as_ref().map(|id| id.clone()))
        .collect();

    // Reconcile state with Docker (lock-free)
    state_manager.reconcile_with_docker(&active_container_ids).await?;

    // Get containers that should be recovered
    let recovery_containers = state_manager.get_recovery_info().await;
    
    if recovery_containers.is_empty() {
        info!(" No containers need recovery");
    } else {
        info!("Found {} containers that may need recovery", recovery_containers.len());
        
        for (uuid, container_state) in recovery_containers {
            if let Some(container_id) = &container_state.container_id {
                // Check if container still exists in Docker
                if active_container_ids.contains(container_id) {
                    // Container exists, check its actual state
                    if let Some(docker_container) = containers.iter().find(|c| c.id.as_ref() == Some(container_id)) {
                        let docker_state = docker_container.state.as_deref().unwrap_or("unknown");
                        
                        if docker_state != container_state.state {
                            info!("Updating container {} state from '{}' to '{}'", uuid, container_state.state, docker_state);
                            state_manager.update_container_state(&uuid, docker_state).await?;
                        }
                    }
                } else {
                    // Container no longer exists in Docker
                    info!("Container {} no longer exists in Docker, marking as stopped", uuid);
                    state_manager.update_container_state(&uuid, "stopped").await?;
                }
            } else {
                // Container has no Docker ID, likely failed during creation
                info!("Container {} has no Docker ID, marking as failed", uuid);
                state_manager.update_container_state(&uuid, "failed").await?;
            }
        }
    }

    info!("Daemon recovery completed successfully");
    Ok(())
}

#[derive(Parser)]
#[command(name = "lightd")]
#[command(about = "Lightweight Docker Container Management Daemon")]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Network management commands
    Network {
        #[command(subcommand)]
        network_cmd: NetworkCommands,
    },
    /// Start the daemon (default if no command specified)
    Serve,
}

#[derive(Subcommand)]
enum NetworkCommands {
    /// Create the lightd network
    Create {
        #[arg(short, long, default_value = "lightd-network")]
        name: String,
        #[arg(short, long, default_value = "bridge")]
        driver: String,
        #[arg(short, long)]
        subnet: Option<String>,
        #[arg(short, long)]
        gateway: Option<String>,
    },
    /// List all networks
    List,
    /// Remove the lightd network
    Remove {
        #[arg(short, long, default_value = "lightd-network")]
        name: String,
    },
    /// Check if lightd network exists
    Check {
        #[arg(short, long, default_value = "lightd-network")]
        name: String,
    },
    /// Setup complete lightd networking
    Setup {
        #[arg(short, long, default_value = "lightd-network")]
        name: String,
        #[arg(short, long, default_value = "172.20.0.0/16")]
        subnet: String,
        #[arg(short, long, default_value = "172.20.0.1")]
        gateway: String,
    },
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Network { network_cmd }) => {
            let config = Config::load("config.json").await?;
            execute_network_command(network_cmd, &config.docker.socket_path).await?;
        }
        Some(Commands::Serve) | None => {
            start_daemon().await?;
        }
    }

    Ok(())
}

async fn execute_network_command(cmd: NetworkCommands, docker_socket: &str) -> anyhow::Result<()> {
    let docker = if docker_socket.starts_with("unix://") {
        Docker::connect_with_socket(docker_socket, 120, bollard::API_DEFAULT_VERSION)?
    } else {
        Docker::connect_with_unix(docker_socket, 120, bollard::API_DEFAULT_VERSION)?
    };

    match cmd {
        NetworkCommands::Create { name, driver, subnet, gateway } => {
            create_network(&docker, &name, &driver, subnet.as_deref(), gateway.as_deref()).await
        }
        NetworkCommands::List => list_networks(&docker).await,
        NetworkCommands::Remove { name } => remove_network(&docker, &name).await,
        NetworkCommands::Check { name } => check_network(&docker, &name).await,
        NetworkCommands::Setup { name, subnet, gateway } => {
            setup_network(&docker, &name, &subnet, &gateway).await
        }
    }
}

async fn create_network(
    docker: &Docker,
    name: &str,
    driver: &str,
    subnet: Option<&str>,
    gateway: Option<&str>,
) -> anyhow::Result<()> {
    if network_exists(docker, name).await? {
        info!("Network '{}' already exists", name);
        return Ok(());
    }

    let mut ipam_config = Vec::new();
    if let (Some(subnet), Some(gateway)) = (subnet, gateway) {
        let config = bollard::models::IpamConfig {
            subnet: Some(subnet.to_string()),
            gateway: Some(gateway.to_string()),
            ..Default::default()
        };
        ipam_config.push(config);
    }

    let options = CreateNetworkOptions {
        name: name.to_string(),
        driver: driver.to_string(),
        ipam: if !ipam_config.is_empty() {
            bollard::models::Ipam {
                driver: Some("default".to_string()),
                config: Some(ipam_config),
                options: None,
            }
        } else {
            Default::default()
        },
        ..Default::default()
    };

    match docker.create_network(options).await {
        Ok(response) => {
            info!("Created network '{}' with ID: {}", name, response.id.unwrap_or_default());
            Ok(())
        }
        Err(e) => {
            tracing::error!("Failed to create network '{}': {}", name, e);
            Err(e.into())
        }
    }
}

async fn list_networks(docker: &Docker) -> anyhow::Result<()> {
    let options = ListNetworksOptions::<String>::default();

    match docker.list_networks(Some(options)).await {
        Ok(networks) => {
            println!("Docker Networks:");
            println!("{:<20} {:<15} {:<15} {:<30}", "NAME", "DRIVER", "SCOPE", "SUBNET");
            println!("{}", "-".repeat(80));

            for network in networks {
                let name = network.name.unwrap_or_default();
                let driver = network.driver.unwrap_or_default();
                let scope = network.scope.unwrap_or_default();
                
                let subnet = network.ipam
                    .and_then(|ipam| ipam.config)
                    .and_then(|config| config.first().cloned())
                    .and_then(|c| c.subnet)
                    .unwrap_or_default();

                println!("{:<20} {:<15} {:<15} {:<30}", name, driver, scope, subnet);
            }
            Ok(())
        }
        Err(e) => {
            tracing::error!("Failed to list networks: {}", e);
            Err(e.into())
        }
    }
}

async fn remove_network(docker: &Docker, name: &str) -> anyhow::Result<()> {
    if !network_exists(docker, name).await? {
        info!("Network '{}' does not exist", name);
        return Ok(());
    }

    match docker.remove_network(name).await {
        Ok(_) => {
            info!("Removed network '{}'", name);
            Ok(())
        }
        Err(e) => {
            tracing::error!("Failed to remove network '{}': {}", name, e);
            Err(e.into())
        }
    }
}

async fn check_network(docker: &Docker, name: &str) -> anyhow::Result<()> {
    if network_exists(docker, name).await? {
        println!("Network '{}' exists", name);
    } else {
        println!("Network '{}' does not exist", name);
    }
    Ok(())
}

async fn setup_network(docker: &Docker, name: &str, subnet: &str, gateway: &str) -> anyhow::Result<()> {
    println!("Setting up lightd networking...");
    create_network(docker, name, "bridge", Some(subnet), Some(gateway)).await?;
    
    if network_exists(docker, name).await? {
        println!("lightd network setup complete!");
        println!("   Network: {}", name);
        println!("   Subnet: {}", subnet);
        println!("   Gateway: {}", gateway);
    }
    
    Ok(())
}

async fn network_exists(docker: &Docker, name: &str) -> anyhow::Result<bool> {
    let options = ListNetworksOptions::<String>::default();

    match docker.list_networks(Some(options)).await {
        Ok(networks) => Ok(networks.iter().any(|n| n.name.as_ref() == Some(&name.to_string()))),
        Err(e) => {
            tracing::error!("Failed to check if network exists: {}", e);
            Err(e.into())
        }
    }
}

async fn start_daemon() -> anyhow::Result<()> {
    let config = Config::load("config.json").await?;
    
    info!("Tokio runtime configured with {} worker threads", config.server.worker_threads);
    
    let docker = DockerClient::new(&config.docker.socket_path).await?;
    
    let network_config = config.network.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Network configuration not loaded"))?;
    
    let mut network = NetworkManager::with_config(Arc::new(network_config.clone()))
        .with_storage(&config.storage.base_path);

    let container_tracker = ContainerTrackingManager::new(&config.storage.containers_path);
    container_tracker.init().await?;

    // Restore network state from container tracker data
    if let Ok(containers) = container_tracker.list_containers().await {
        network.restore_from_containers(&containers);
    }

    // Initialize lock-free state manager
    let mut state_manager = StateManager::new(&config.storage.base_path);
    state_manager.init().await?;

    // Perform daemon recovery
    perform_daemon_recovery(&docker, &state_manager).await?;

    // Wrap in Arc (no RwLock needed - StateManager is internally lock-free)
    let state_manager = Arc::new(state_manager);

    // Initialize resource monitor if enabled
    let resource_monitor = if config.monitoring.as_ref().map(|m| m.enabled).unwrap_or(true) {
        let monitoring_config = config.monitoring.as_ref().cloned().unwrap_or_default();
        let ru_config = crate::monitoring::ru_calculator::RUConfig {
            cpu_weight: monitoring_config.ru_config.cpu_weight,
            memory_weight: monitoring_config.ru_config.memory_weight,
            io_weight: monitoring_config.ru_config.io_weight,
            network_weight: monitoring_config.ru_config.network_weight,
            storage_weight: monitoring_config.ru_config.storage_weight,
            base_ru: monitoring_config.ru_config.base_ru,
        };
        
        let monitor = Arc::new(crate::monitoring::ResourceMonitor::new(
            Arc::new(docker.client.clone()),
            state_manager.clone(),
            ru_config,
            monitoring_config.interval_ms,
        ).with_remote(monitoring_config.remote.clone()));
        
        if let Some(ref remote) = monitoring_config.remote {
            if remote.enabled {
                info!("Remote panel RU posting enabled: {}", remote.url);
            } else {
                info!("Remote panel RU posting disabled");
            }
        }
        
        // Start monitoring in background
        let monitor_clone = monitor.clone();
        tokio::spawn(async move {
            monitor_clone.start_monitoring().await;
        });
        
        info!("Resource monitoring started with interval: {}ms", monitoring_config.interval_ms);
        Some(monitor)
    } else {
        info!("Resource monitoring is disabled");
        None
    };
    
    // Start background state save task
    {
        let state_manager_for_save = state_manager.clone();
        let (dirty, _notify, path) = state_manager_for_save.get_save_handles();
        
        tokio::spawn(async move {
            info!("Background state save task started");
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));
            
            loop {
                interval.tick().await;
                
                // Check if dirty and save
                if dirty.swap(false, std::sync::atomic::Ordering::SeqCst) {
                    let state = state_manager_for_save.get_state_snapshot();
                    let content = serde_json::to_string_pretty(&state).unwrap_or_default();
                    if let Err(e) = tokio::fs::write(&path, content).await {
                        tracing::error!("Failed to save state: {}", e);
                    }
                }
            }
        });
    }

    // Initialize WebSocket token manager
    let websocket_tokens = Arc::new(crate::websocket::TokenManager::new());
    
    let token_manager_clone = websocket_tokens.clone();
    tokio::spawn(async move {
        token_manager_clone.start_cleanup_task().await;
    });

    // Initialize services
    let docker_arc = Arc::new(docker);
    let container_tracker_arc = Arc::new(container_tracker);
    
    let power_actions = Arc::new(PowerActionService::new(
        Arc::new(docker_arc.client.clone()),
        state_manager.clone(),
        container_tracker_arc.clone(),
    ));
    
    // Initialize PowerExecutor with state update callback
    let state_manager_for_callback = state_manager.clone();
    let tracker_for_callback = container_tracker_arc.clone();
    
    let state_callback: services::power_executor::StateCallback = Arc::new(move |uuid, action, status| {
        let new_state = match (action.as_str(), status.as_str()) {
            ("start", "completed") => Some("running"),
            ("start", "failed") => Some("stopped"),
            ("stop", "completed") => Some("stopped"),
            ("stop", "failed") => Some("stopped"),
            ("kill", "completed") => Some("stopped"),
            ("kill", "failed") => Some("stopped"),
            ("restart", "completed") => Some("running"),
            ("restart", "failed") => Some("stopped"),
            ("suspend", "completed") => Some("suspended"),
            ("suspend", "failed") => Some("stopped"),
            _ => None,
        };
        
        if let Some(state) = new_state {
            let sm = state_manager_for_callback.clone();
            let tracker = tracker_for_callback.clone();
            let uuid_clone = uuid.clone();
            let state_str = state.to_string();
            
            // Spawn update task - never blocks the callback
            tokio::spawn(async move {
                let _ = sm.update_container_state(&uuid_clone, &state_str).await;
                let _ = tracker.update_container_status(&uuid_clone, &state_str).await;
            });
        }
    });
    
    let power_executor = Arc::new(PowerExecutor::new(
        config.docker.socket_path.clone(),
        state_callback,
    ));
    
    info!("Power executor initialized on dedicated thread");

    // Initialize event hub and async power manager
    let event_hub = Arc::new(ContainerEventHub::new());
    
    let async_power = Arc::new(AsyncPowerManager::new(
        Arc::new(docker_arc.client.clone()),
        event_hub.clone(),
    ));
    
    info!("Container event hub and async power manager initialized");

    let state = AppState {
        docker: docker_arc,
        config: Arc::new(config.clone()),
        network: Arc::new(RwLock::new(network)),
        network_config: Arc::new(network_config.clone()),
        container_tracker: container_tracker_arc,
        state_manager,
        resource_monitor,
        websocket_tokens,
        power_actions,
        power_executor,
        event_hub,
        async_power,
    };

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/containers", post(handlers::container::create_container))
        .route("/containers", get(handlers::container::list_containers))
        .route("/containers/:id", delete(handlers::container::remove_container))
        .route("/containers/:id/start", post(handlers::container::start_container))
        .route("/containers/:id/stop", post(handlers::container::stop_container))
        .route("/containers/:id/kill", post(handlers::container::kill_container))
        .route("/containers/:id/restart", post(handlers::container::restart_container))
        .route("/containers/:id/suspend", post(handlers::container::suspend_container))
        .route("/containers/:id/unsuspend", post(handlers::container::unsuspend_container))
        .route("/containers/:id/attach", post(handlers::container::attach_container))
        .route("/containers/:id/exec", post(handlers::container::exec_container))
        .route("/containers/:id/logs", post(handlers::container::get_container_logs))
        .route("/containers/:id/stats", get(handlers::container::get_container_stats))
        .route("/containers/:id/debug", get(handlers::container::debug_container))
        .route("/containers/:id/update", post(handlers::container::update_container))
        .route("/containers/:id/limits", put(handlers::container::update_container_limits))
        .route("/containers/:id/status", get(handlers::container::get_container_status))
        .route("/containers/:id/pending-action", get(handlers::container::get_container_pending_action))
        .route("/containers/uuid/:uuid", get(handlers::container::get_container_by_uuid))
        .route("/containers/uuid/:uuid/start", post(handlers::container::start_container_by_uuid))
        .route("/containers/uuid/:uuid/stop", post(handlers::container::stop_container_by_uuid))
        .route("/containers/uuid/:uuid/restart", post(handlers::container::restart_container_by_uuid))
        .route("/containers/uuid/:uuid/suspend", post(handlers::container::suspend_container_by_uuid))
        .route("/containers/uuid/:uuid/unsuspend", post(handlers::container::unsuspend_container_by_uuid))
        .route("/power-actions/:action_id", get(handlers::container::get_power_action_status))
        .route("/websocket/generate", get(handlers::websocket::generate_websocket_token))
        .route("/websocket", get(handlers::websocket::websocket_handler))
        .route("/containers/:id/files", get(handlers::filesystem::list_directory))
        .route("/containers/:id/files/content/*path", get(handlers::filesystem::get_file_content))
        .route("/containers/:id/files/write", post(handlers::filesystem::write_file))
        .route("/containers/:id/files/mkdir", post(handlers::filesystem::create_directory))
        .route("/containers/:id/files/delete", post(handlers::filesystem::delete_path))
        .route("/containers/:id/files/chmod", post(handlers::filesystem::chmod_path))
        .route("/containers/:id/files/chown", post(handlers::filesystem::chown_path))
        .route("/containers/:id/files/archive", post(handlers::filesystem::create_archive))
        .route("/containers/:id/files/extract", post(handlers::filesystem::extract_archive))
        .route("/containers/:id/files/zip", post(handlers::filesystem::create_zip))
        .route("/containers/:id/files/unzip", post(handlers::filesystem::extract_zip))
        .route("/containers/:id/files/copy", post(handlers::filesystem::copy_file))
        .route("/containers/:id/files/upload", post(handlers::filesystem::upload_file))
        .route("/monitoring/system", get(handlers::monitoring::get_system_metrics))
        .route("/monitoring/system/history", get(handlers::monitoring::get_system_metrics_history))
        .route("/monitoring/containers/:id", get(handlers::monitoring::get_container_metrics))
        .route("/monitoring/containers/:id/history", get(handlers::monitoring::get_container_metrics_history))
        .route("/monitoring/ru/summary", get(handlers::monitoring::get_ru_summary))
        .route("/monitoring/ru/containers/:id", get(handlers::monitoring::get_container_ru_breakdown))
        .route("/monitoring/ru/config", get(handlers::monitoring::get_ru_config))
        .route("/monitoring/ru/estimate", post(handlers::monitoring::calculate_ru_estimate))
        .route("/snapshots", get(handlers::snapshot::list_snapshots))
        .route("/snapshots/:id", get(handlers::snapshot::get_snapshot_info))
        .route("/snapshots/:id", delete(handlers::snapshot::delete_snapshot))
        .route("/snapshots/:id/restore", post(handlers::snapshot::restore_snapshot))
        .route("/containers/:id/snapshots", get(handlers::snapshot::list_container_snapshots))
        .route("/containers/:id/snapshots", post(handlers::snapshot::create_snapshot))
        .route("/volumes", post(handlers::volume::create_volume))
        .route("/volumes", get(handlers::volume::list_volumes))
        .route("/volumes/:name", delete(handlers::volume::remove_volume))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("{}:{}", config.server.host, config.server.port);
    info!("Starting lightd server on {}", addr);

    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health_check(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "lightd",
        "version": state.config.version
    }))
}
