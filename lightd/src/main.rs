use axum::{
    response::Json,
    routing::{delete, get, post},
    Router,
};
use clap::{Parser, Subcommand};
use std::sync::Arc;
use tokio::sync::Mutex;
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

use container_tracker::ContainerTrackingManager;
use state_manager::StateManager;
use config::Config;
use docker::{DockerClient, NetworkManager};
use types::AppState;

async fn perform_daemon_recovery(docker: &DockerClient, state_manager: &mut StateManager) -> anyhow::Result<()> {
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

    // Reconcile state with Docker
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
        /// Network name (default: lightd-network)
        #[arg(short, long, default_value = "lightd-network")]
        name: String,
        /// Network driver (default: bridge)
        #[arg(short, long, default_value = "bridge")]
        driver: String,
        /// Network subnet (e.g., 172.20.0.0/16)
        #[arg(short, long)]
        subnet: Option<String>,
        /// Network gateway (e.g., 172.20.0.1)
        #[arg(short, long)]
        gateway: Option<String>,
    },
    /// List all networks
    List,
    /// Remove the lightd network
    Remove {
        /// Network name to remove
        #[arg(short, long, default_value = "lightd-network")]
        name: String,
    },
    /// Check if lightd network exists
    Check {
        /// Network name to check
        #[arg(short, long, default_value = "lightd-network")]
        name: String,
    },
    /// Setup complete lightd networking (create network + configure)
    Setup {
        /// Network name (default: lightd-network)
        #[arg(short, long, default_value = "lightd-network")]
        name: String,
        /// Network subnet (default: 172.20.0.0/16)
        #[arg(short, long, default_value = "172.20.0.0/16")]
        subnet: String,
        /// Network gateway (default: 172.20.0.1)
        #[arg(short, long, default_value = "172.20.0.1")]
        gateway: String,
    },
}



#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Network { network_cmd }) => {
            // Handle network commands
            let config = Config::load("config.json").await?;
            execute_network_command(network_cmd, &config.docker.socket_path).await?;
        }
        Some(Commands::Serve) | None => {
            // Start the daemon (default behavior)
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
        NetworkCommands::List => {
            list_networks(&docker).await
        }
        NetworkCommands::Remove { name } => {
            remove_network(&docker, &name).await
        }
        NetworkCommands::Check { name } => {
            check_network(&docker, &name).await
        }
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
    // Check if network already exists
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
            if let Some(subnet) = subnet {
                info!("   Subnet: {}", subnet);
            }
            if let Some(gateway) = gateway {
                info!("   Gateway: {}", gateway);
            }
            Ok(())
        }
        Err(e) => {
            tracing::error!("Failed to create network '{}': {}", name, e);
            Err(e.into())
        }
    }
}

async fn list_networks(docker: &Docker) -> anyhow::Result<()> {
    let options = ListNetworksOptions::<String> {
        ..Default::default()
    };

    match docker.list_networks(Some(options)).await {
        Ok(networks) => {
            println!("Docker Networks:");
            println!("{:<20} {:<15} {:<15} {:<30}", "NAME", "DRIVER", "SCOPE", "SUBNET");
            println!("{}", "-".repeat(80));

            for network in networks {
                let name = network.name.unwrap_or_default();
                let driver = network.driver.unwrap_or_default();
                let scope = network.scope.unwrap_or_default();
                
                let subnet = if let Some(ipam) = &network.ipam {
                    if let Some(config) = &ipam.config {
                        config.first()
                            .and_then(|c| c.subnet.as_ref())
                            .map(|s| s.to_string())
                            .unwrap_or_default()
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

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
    
    // Create the network
    create_network(docker, name, "bridge", Some(subnet), Some(gateway)).await?;
    
    // Verify it was created
    if network_exists(docker, name).await? {
        println!("lightd network setup complete!");
        println!("   Network: {}", name);
        println!("   Subnet: {}", subnet);
        println!("   Gateway: {}", gateway);
        println!("\nYou can now create containers that will use this network automatically.");
    } else {
        tracing::error!("Network setup failed - network not found after creation");
    }
    
    Ok(())
}

async fn network_exists(docker: &Docker, name: &str) -> anyhow::Result<bool> {
    let options = ListNetworksOptions::<String> {
        ..Default::default()
    };

    match docker.list_networks(Some(options)).await {
        Ok(networks) => {
            Ok(networks.iter().any(|n| n.name.as_ref() == Some(&name.to_string())))
        }
        Err(e) => {
            tracing::error!("Failed to check if network exists: {}", e);
            Err(e.into())
        }
    }
}

async fn start_daemon() -> anyhow::Result<()> {

    let config = Config::load("config.json").await?;
    let docker = DockerClient::new(&config.docker.socket_path).await?;
    
    // Get network config from the loaded config
    let network_config = config.network.as_ref()
        .ok_or_else(|| anyhow::anyhow!("Network configuration not loaded"))?;
    
    // Use network config for port range and create network manager with config
    let network = NetworkManager::with_config(Arc::new(network_config.clone()));

    // Initialize container tracker with configurable path
    let container_tracker = ContainerTrackingManager::new(&config.storage.containers_path);
    container_tracker.init().await?;

    // Initialize state manager
    let mut state_manager = StateManager::new(&config.storage.base_path);
    state_manager.init().await?;

    // Perform daemon recovery
    perform_daemon_recovery(&docker, &mut state_manager).await?;

    let state = AppState {
        docker: Arc::new(docker),
        config: Arc::new(config.clone()),
        network: Arc::new(Mutex::new(network)),
        network_config: Arc::new(network_config.clone()),
        container_tracker: Arc::new(container_tracker),
        state_manager: Arc::new(Mutex::new(state_manager)),
    };

    let app = Router::new()
        .route("/health", get(health_check))
        .route("/containers", post(handlers::container::create_container))
        .route("/containers", get(handlers::container::list_containers))
        .route("/containers/:id", get(handlers::container::inspect_container))
        .route("/containers/:id", delete(handlers::container::remove_container))
        .route("/containers/:id/start", post(handlers::container::start_container))
        .route("/containers/:id/stop", post(handlers::container::stop_container))
        .route("/containers/:id/kill", post(handlers::container::kill_container))
        .route("/containers/:id/suspend", post(handlers::container::suspend_container))
        .route("/containers/:id/attach", post(handlers::container::attach_container))
        .route("/containers/:id/exec", post(handlers::container::exec_container))
        .route("/containers/:id/logs", post(handlers::container::get_container_logs))
        .route("/containers/:id/stats", get(handlers::container::get_container_stats))
        .route("/containers/:id/debug", get(handlers::container::debug_container))
        .route("/containers/:id/update", post(handlers::container::update_container))
        .route("/containers/:id/status", get(handlers::container::get_container_status))
        // Filesystem routes
        .route("/containers/:id/files", get(handlers::filesystem::list_directory))
        .route("/containers/:id/files/content/*path", get(handlers::filesystem::get_file_content))
        .route("/containers/:id/files/write", post(handlers::filesystem::write_file))
        .route("/containers/:id/files/mkdir", post(handlers::filesystem::create_directory))
        .route("/containers/:id/files/delete", post(handlers::filesystem::delete_path))
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

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "lightd",
        "version": "0.1.0"
    }))
}