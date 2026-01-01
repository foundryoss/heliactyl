use bollard::Docker;
use tracing::{error, info};

/// Docker client wrapper for managing Docker daemon connections
#[derive(Clone)]
pub struct DockerClient {
    pub client: Docker,
}

impl DockerClient {
    /// Create a new Docker client connection
    pub async fn new(socket_path: &str) -> anyhow::Result<Self> {
        // Use a longer timeout (600 seconds = 10 minutes) for image pulls and long operations
        let timeout_seconds = 600;
        
        let client = if socket_path.starts_with("unix://") {
            Docker::connect_with_socket(socket_path, timeout_seconds, bollard::API_DEFAULT_VERSION)?
        } else {
            Docker::connect_with_unix(socket_path, timeout_seconds, bollard::API_DEFAULT_VERSION)?
        };

        // Test connection
        match client.ping().await {
            Ok(_) => info!("Successfully connected to Docker daemon"),
            Err(e) => {
                error!("Failed to connect to Docker daemon: {}", e);
                return Err(e.into());
            }
        }

        Ok(Self { client })
    }

    /// Get a reference to the underlying Docker client
    pub fn client(&self) -> &Docker {
        &self.client
    }
}