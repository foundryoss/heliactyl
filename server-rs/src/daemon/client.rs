use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone)]
pub struct DaemonClient {
    base_url: String,
    api_key: String,
    client: Client,
}

#[derive(Debug, Serialize)]
pub struct CreateContainerRequest {
    pub name: String,
    pub image: String,
    pub env: HashMap<String, String>,
    pub limits: ResourceLimits,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResourceLimits {
    pub cpu_limit: f64,
    pub memory_limit: i64,
    pub disk_limit: i64,
}

#[derive(Debug, Deserialize)]
pub struct DaemonResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub ports: Vec<Port>,
    pub limits: ResourceLimits,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Port {
    pub container_port: i32,
    pub host_port: i32,
    pub protocol: String,
    pub primary: bool,
    pub active: bool,
}

#[derive(Debug, Deserialize)]
pub struct ContainerStats {
    pub cpu_usage: f64,
    pub memory_usage: i64,
    pub disk_usage: i64,
    pub network_usage: i64,
    pub container_id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub created: String,
    pub updated: String,
    pub ports: Vec<Port>,
}

impl DaemonClient {
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            base_url,
            api_key,
            client: Client::new(),
        }
    }

    /// Create a new container (returns container info immediately)
    pub async fn create_container(
        &self,
        request: CreateContainerRequest,
    ) -> Result<Container, Box<dyn std::error::Error>> {
        let url = format!("{}/api/containers", self.base_url);
        
        tracing::info!("Sending container creation request to daemon: {}", url);
        
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Failed to send request to daemon: {}", e);
                format!("Daemon request failed: {}", e)
            })?;

        let status = response.status();
        tracing::info!("Daemon response status: {}", status);

        if !status.is_success() {
            let error_text = response.text().await?;
            tracing::error!("Daemon returned error. Status: {}, Body: {}", status, error_text);
            return Err(format!("Daemon error ({}): {}", status, error_text).into());
        }

        let daemon_response: DaemonResponse<Container> = response.json().await
            .map_err(|e| {
                tracing::error!("Failed to parse response: {}", e);
                format!("Failed to parse response: {}", e)
            })?;
        
        if !daemon_response.success {
            let error = daemon_response.error.unwrap_or_else(|| "Unknown error".to_string());
            tracing::error!("Daemon returned error: {}", error);
            return Err(error.into());
        }

        let container = daemon_response.data.ok_or("No container data returned")?;
        
        tracing::info!("Container created with ID: {} (state: {})", container.id, container.state);
        
        Ok(container)
    }

    /// List all containers
    pub async fn list_containers(&self) -> Result<Vec<Container>, Box<dyn std::error::Error>> {
        let url = format!("{}/api/containers", self.base_url);
        
        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        let daemon_response: DaemonResponse<Vec<Container>> = response.json().await?;
        
        if daemon_response.success {
            Ok(daemon_response.data.unwrap_or_default())
        } else {
            Err(daemon_response.error.unwrap_or_else(|| "Unknown error".to_string()).into())
        }
    }

    /// Get container by ID
    pub async fn get_container(&self, container_id: &str) -> Result<Container, Box<dyn std::error::Error>> {
        let containers = self.list_containers().await?;
        containers
            .into_iter()
            .find(|c| c.id == container_id)
            .ok_or_else(|| "Container not found".into())
    }

    /// Start a container
    pub async fn start_container(&self, container_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/api/container/{}/power", self.base_url, container_id);
        
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&serde_json::json!({"action": "start"}))
            .send()
            .await?;

        let daemon_response: DaemonResponse<serde_json::Value> = response.json().await?;
        
        if daemon_response.success {
            Ok(())
        } else {
            Err(daemon_response.error.unwrap_or_else(|| "Unknown error".to_string()).into())
        }
    }

    /// Stop a container
    pub async fn stop_container(&self, container_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/api/container/{}/power", self.base_url, container_id);
        
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&serde_json::json!({"action": "stop"}))
            .send()
            .await?;

        let daemon_response: DaemonResponse<serde_json::Value> = response.json().await?;
        
        if daemon_response.success {
            Ok(())
        } else {
            Err(daemon_response.error.unwrap_or_else(|| "Unknown error".to_string()).into())
        }
    }

    /// Restart a container
    pub async fn restart_container(&self, container_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/api/container/{}/power", self.base_url, container_id);
        
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&serde_json::json!({"action": "restart"}))
            .send()
            .await?;

        let daemon_response: DaemonResponse<serde_json::Value> = response.json().await?;
        
        if daemon_response.success {
            Ok(())
        } else {
            Err(daemon_response.error.unwrap_or_else(|| "Unknown error".to_string()).into())
        }
    }

    /// Delete a container
    pub async fn delete_container(&self, container_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/api/container/{}/delete", self.base_url, container_id);
        
        let response = self
            .client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        let daemon_response: DaemonResponse<serde_json::Value> = response.json().await?;
        
        if daemon_response.success {
            Ok(())
        } else {
            Err(daemon_response.error.unwrap_or_else(|| "Unknown error".to_string()).into())
        }
    }

    /// Get container by ID using stats endpoint (includes full container info)
    pub async fn get_container_info(&self, container_id: &str) -> Result<ContainerStats, Box<dyn std::error::Error>> {
        let url = format!("{}/api/container/{}/stats", self.base_url, container_id);
        
        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        let daemon_response: DaemonResponse<ContainerStats> = response.json().await?;
        
        if daemon_response.success {
            daemon_response.data.ok_or_else(|| "No container info returned".into())
        } else {
            Err(daemon_response.error.unwrap_or_else(|| "Unknown error".to_string()).into())
        }
    }

    /// Update container resource limits
    pub async fn update_limits(
        &self,
        container_id: &str,
        limits: ResourceLimits,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("{}/api/container/{}/limits", self.base_url, container_id);
        
        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&limits)
            .send()
            .await?;

        let daemon_response: DaemonResponse<serde_json::Value> = response.json().await?;
        
        if daemon_response.success {
            Ok(())
        } else {
            Err(daemon_response.error.unwrap_or_else(|| "Unknown error".to_string()).into())
        }
    }

    /// Get billing information for a container
    pub async fn get_container_billing(&self, container_id: &str) -> Result<crate::models::billing::ContainerBilling, Box<dyn std::error::Error>> {
        let url = format!("{}/api/billing/container/{}", self.base_url, container_id);
        
        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        let daemon_response: DaemonResponse<crate::models::billing::ContainerBilling> = response.json().await?;
        
        if daemon_response.success {
            daemon_response.data.ok_or_else(|| "No billing data returned".into())
        } else {
            Err(daemon_response.error.unwrap_or_else(|| "Unknown error".to_string()).into())
        }
    }

    /// Get billing information for a tenant
    pub async fn get_tenant_billing(&self, tenant_id: &str) -> Result<crate::models::billing::TenantBilling, Box<dyn std::error::Error>> {
        let url = format!("{}/api/billing/tenant/{}", self.base_url, tenant_id);
        
        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        let daemon_response: DaemonResponse<crate::models::billing::TenantBilling> = response.json().await?;
        
        if daemon_response.success {
            daemon_response.data.ok_or_else(|| "No billing data returned".into())
        } else {
            Err(daemon_response.error.unwrap_or_else(|| "Unknown error".to_string()).into())
        }
    }

    /// Get billing configuration
    pub async fn get_billing_config(&self) -> Result<crate::models::billing::BillingConfig, Box<dyn std::error::Error>> {
        let url = format!("{}/api/billing/config", self.base_url);
        
        let response = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await?;

        let daemon_response: DaemonResponse<crate::models::billing::BillingConfig> = response.json().await?;
        
        if daemon_response.success {
            daemon_response.data.ok_or_else(|| "No billing config returned".into())
        } else {
            Err(daemon_response.error.unwrap_or_else(|| "Unknown error".to_string()).into())
        }
    }
}
