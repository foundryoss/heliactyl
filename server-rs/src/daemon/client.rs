use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Clone)]
pub struct DaemonClient {
    base_url: String,
    client: Client,
}

/// Request format matching lightd's CreateContainerRequest
#[derive(Debug, Serialize)]
pub struct CreateContainerRequest {
    pub image: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_command: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ports: Option<HashMap<String, String>>, // "80": "auto" or "80": "8080"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limits: Option<ResourceLimits>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_content: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ResourceLimits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub swap: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pids: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threads: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct DaemonResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub message: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Container {
    pub uuid: String,
    pub container_id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    pub allocated_ports: Vec<PortAllocation>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PortAllocation {
    pub container_port: u16,
    pub host_port: u16,
    pub host_ip: String,
    pub protocol: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ContainerLookup {
    pub uuid: String,
    pub container_id: String,
    pub name: String,
    pub state: String,
    pub image: String,
}

impl DaemonClient {
    pub fn new(base_url: String, _api_key: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(300))
            .build()
            .unwrap_or_else(|_| Client::new());
        
        Self { base_url, client }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Create a new container
    pub async fn create_container(&self, request: CreateContainerRequest) -> Result<Container, String> {
        let url = format!("{}/containers", self.base_url);
        
        tracing::info!("Creating container on lightd: {}", url);
        tracing::debug!("Request: {:?}", request);
        
        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        let status = response.status();
        tracing::info!("lightd response status: {}", status);

        // Read the response body
        let text = response.text().await.map_err(|e| format!("Failed to read response: {}", e))?;
        tracing::info!("lightd response body: {}", text);
        
        // Parse JSON
        let json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("Failed to parse JSON: {} - body was: {}", e, text))?;
        
        // Check success field
        let success = json["success"].as_bool().unwrap_or(false);
        if !success {
            let error_msg = json["message"].as_str().unwrap_or("Unknown error from lightd");
            return Err(error_msg.to_string());
        }
        
        // Extract data from response
        let data = json.get("data").ok_or_else(|| format!("No data in response: {}", text))?;
        
        let uuid = data["custom_uuid"].as_str().unwrap_or("");
        let container_id = data["container_id"].as_str().unwrap_or("");
        
        tracing::info!("Parsed from response: custom_uuid={}, container_id={}", uuid, container_id);
        
        if uuid.is_empty() {
            return Err(format!("lightd returned empty UUID. Response: {}", text));
        }
        
        let container = Container {
            uuid: uuid.to_string(),
            container_id: container_id.to_string(),
            name: data["name"].as_str().or_else(|| request.name.as_deref()).unwrap_or("").to_string(),
            image: data["image"].as_str().unwrap_or(&request.image).to_string(),
            state: data["state"].as_str().unwrap_or("created").to_string(),
            allocated_ports: data["allocated_ports"].as_array()
                .map(|arr| arr.iter().filter_map(|p| {
                    Some(PortAllocation {
                        container_port: p["container_port"].as_u64()? as u16,
                        host_port: p["host_port"].as_u64()? as u16,
                        host_ip: p["host_ip"].as_str()?.to_string(),
                        protocol: p["protocol"].as_str().unwrap_or("tcp").to_string(),
                    })
                }).collect())
                .unwrap_or_default(),
        };
        
        tracing::info!("Container created successfully: uuid={}", container.uuid);
        
        Ok(container)
    }

    /// Get container by UUID
    pub async fn get_container(&self, uuid: &str) -> Result<ContainerLookup, String> {
        let url = format!("{}/containers/uuid/{}", self.base_url, uuid);
        
        tracing::debug!("Getting container by UUID: {}", url);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Container not found ({}): {}", status, error_text));
        }

        let text = response.text().await.map_err(|e| format!("Failed to read response: {}", e))?;
        tracing::debug!("lightd get_container response: {}", text);
        
        let json: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("Failed to parse response: {}", e))?;
        
        // Check if success
        let success = json["success"].as_bool().unwrap_or(false);
        if !success {
            let msg = json["message"].as_str().unwrap_or("Unknown error");
            return Err(msg.to_string());
        }
        
        let data = json.get("data").ok_or("No data in response")?;
        
        Ok(ContainerLookup {
            uuid: data["uuid"].as_str().unwrap_or(uuid).to_string(),
            container_id: data["container_id"].as_str().unwrap_or("").to_string(),
            name: data["name"].as_str().unwrap_or("").to_string(),
            state: data["state"].as_str().unwrap_or("unknown").to_string(),
            image: data["image"].as_str().unwrap_or("").to_string(),
        })
    }

    /// Start container by UUID
    pub async fn start_container(&self, uuid: &str) -> Result<(), String> {
        let url = format!("{}/containers/uuid/{}/start", self.base_url, uuid);
        
        let response = self.client.post(&url).send().await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Failed to start: {}", error_text));
        }
        
        Ok(())
    }

    /// Stop container by UUID
    pub async fn stop_container(&self, uuid: &str) -> Result<(), String> {
        let url = format!("{}/containers/uuid/{}/stop", self.base_url, uuid);
        
        let response = self.client.post(&url).send().await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Failed to stop: {}", error_text));
        }
        
        Ok(())
    }

    /// Delete container by UUID
    pub async fn delete_container(&self, uuid: &str) -> Result<(), String> {
        let url = format!("{}/containers/{}", self.base_url, uuid);
        
        let response = self.client.delete(&url).send().await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Failed to delete: {}", error_text));
        }
        
        Ok(())
    }

    /// List all containers
    pub async fn list_containers(&self) -> Result<Vec<ContainerLookup>, String> {
        let url = format!("{}/containers", self.base_url);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            return Ok(vec![]);
        }

        let json: serde_json::Value = response.json().await
            .map_err(|e| format!("Failed to parse response: {}", e))?;
        
        let data = json.get("data").and_then(|d| d.as_array());
        
        Ok(data.map(|arr| {
            arr.iter().filter_map(|c| {
                Some(ContainerLookup {
                    uuid: c["id"].as_str()?.to_string(),
                    container_id: c["id"].as_str()?.to_string(),
                    name: c["name"].as_str()?.to_string(),
                    state: c["state"].as_str().unwrap_or("unknown").to_string(),
                    image: c["image"].as_str()?.to_string(),
                })
            }).collect()
        }).unwrap_or_default())
    }

    /// Get RU configuration from daemon
    pub async fn get_ru_config(&self) -> Result<RUConfig, String> {
        let url = format!("{}/monitoring/ru/config", self.base_url);
        
        let response = self.client.get(&url).send().await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            return Err("Failed to get RU config".to_string());
        }

        let json: serde_json::Value = response.json().await
            .map_err(|e| format!("Failed to parse response: {}", e))?;
        
        let data = json.get("data").ok_or("No data in response")?;
        
        Ok(RUConfig {
            cpu_weight: data["cpu_weight"].as_f64().unwrap_or(1.0),
            memory_weight: data["memory_weight"].as_f64().unwrap_or(0.5),
            io_weight: data["io_weight"].as_f64().unwrap_or(2.0),
            network_weight: data["network_weight"].as_f64().unwrap_or(1.5),
            storage_weight: data["storage_weight"].as_f64().unwrap_or(0.8),
            base_ru: data["base_ru"].as_f64().unwrap_or(0.1),
            ru_price_per_hour: data["ru_price_per_hour"].as_f64().unwrap_or(0.001),
        })
    }

    /// Calculate RU estimate for given limits
    pub async fn calculate_ru_estimate(&self, cpu: &str, memory: &str, disk: &str) -> Result<RUEstimate, String> {
        let url = format!("{}/monitoring/ru/estimate", self.base_url);
        
        let body = serde_json::json!({
            "cpu": cpu,
            "memory": memory,
            "disk": disk
        });
        
        let response = self.client.post(&url).json(&body).send().await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            return Err("Failed to calculate RU estimate".to_string());
        }

        let json: serde_json::Value = response.json().await
            .map_err(|e| format!("Failed to parse response: {}", e))?;
        
        let data = json.get("data").ok_or("No data in response")?;
        
        Ok(RUEstimate {
            ru_per_hour: data["ru_per_hour"].as_f64().unwrap_or(0.0),
            ru_per_day: data["ru_per_day"].as_f64().unwrap_or(0.0),
            ru_per_month: data["ru_per_month"].as_f64().unwrap_or(0.0),
            price_per_hour: data["price_per_hour"].as_f64().unwrap_or(0.0),
            price_per_day: data["price_per_day"].as_f64().unwrap_or(0.0),
            price_per_month: data["price_per_month"].as_f64().unwrap_or(0.0),
        })
    }

    /// Get container logs by UUID
    pub async fn get_container_logs(&self, uuid: &str, tail: Option<&str>) -> Result<String, String> {
        // First, resolve UUID to Docker container ID
        let container = self.get_container(uuid).await?;
        let docker_id = container.container_id;
        
        if docker_id.is_empty() {
            return Err("Container has no Docker ID".to_string());
        }
        
        let url = format!("{}/containers/{}/logs", self.base_url, docker_id);
        
        let body = serde_json::json!({
            "follow": false,
            "tail": tail.unwrap_or("50")
        });
        
        tracing::debug!("Getting logs for container: {} (docker_id: {})", uuid, docker_id);
        
        let response = self.client.post(&url).json(&body).send().await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Failed to get logs: {}", error_text));
        }

        let json: serde_json::Value = response.json().await
            .map_err(|e| format!("Failed to parse response: {}", e))?;
        
        let success = json["success"].as_bool().unwrap_or(false);
        if !success {
            let msg = json["message"].as_str().unwrap_or("Unknown error");
            return Err(msg.to_string());
        }
        
        let logs = json["data"].as_str().unwrap_or("").to_string();
        Ok(logs)
    }

    /// Get RU summary for all containers from daemon
    pub async fn get_ru_summary(&self) -> Result<HashMap<String, f64>, String> {
        let url = format!("{}/monitoring/ru/summary", self.base_url);
        
        let response = self.client.get(&url).send().await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            return Err("Failed to get RU summary".to_string());
        }

        let json: serde_json::Value = response.json().await
            .map_err(|e| format!("Failed to parse response: {}", e))?;
        
        let data = json.get("data").ok_or("No data in response")?;
        
        let mut summary = HashMap::new();
        if let Some(obj) = data.as_object() {
            for (container_id, ru_value) in obj {
                if let Some(ru) = ru_value.as_f64() {
                    summary.insert(container_id.clone(), ru);
                }
            }
        }
        
        Ok(summary)
    }

    /// Get container metrics from daemon
    pub async fn get_container_metrics(&self, container_id: &str) -> Result<ContainerMetrics, String> {
        let url = format!("{}/monitoring/containers/{}", self.base_url, container_id);
        
        let response = self.client.get(&url).send().await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            return Err("Failed to get container metrics".to_string());
        }

        let json: serde_json::Value = response.json().await
            .map_err(|e| format!("Failed to parse response: {}", e))?;
        
        let data = json.get("data").ok_or("No data in response")?;
        
        Ok(ContainerMetrics {
            container_id: data["container_id"].as_str().unwrap_or("").to_string(),
            cpu_percent: data["cpu_percent"].as_f64().unwrap_or(0.0),
            memory_usage_bytes: data["memory_usage_bytes"].as_u64().unwrap_or(0),
            memory_percent: data["memory_percent"].as_f64().unwrap_or(0.0),
            is_running: data["is_running"].as_bool().unwrap_or(false),
        })
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RUConfig {
    pub cpu_weight: f64,
    pub memory_weight: f64,
    pub io_weight: f64,
    pub network_weight: f64,
    pub storage_weight: f64,
    pub base_ru: f64,
    pub ru_price_per_hour: f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RUEstimate {
    pub ru_per_hour: f64,
    pub ru_per_day: f64,
    pub ru_per_month: f64,
    pub price_per_hour: f64,
    pub price_per_day: f64,
    pub price_per_month: f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ContainerMetrics {
    pub container_id: String,
    pub cpu_percent: f64,
    pub memory_usage_bytes: u64,
    pub memory_percent: f64,
    pub is_running: bool,
}
