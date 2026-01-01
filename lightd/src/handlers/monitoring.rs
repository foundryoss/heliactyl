use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{
    models::ApiResponse,
    types::AppState,
    monitoring::resource_monitor::{ContainerMetrics, SystemMetrics},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct MetricsQuery {
    pub limit: Option<usize>,
    pub since_minutes: Option<i64>,
}

/// Get current system metrics and RU summary
pub async fn get_system_metrics(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<SystemMetricsResponse>>, StatusCode> {
    info!("Getting system metrics");
    
    if let Some(monitor) = &state.resource_monitor {
        let system_metrics = monitor.get_system_metrics().await;
        let ru_summary = monitor.get_ru_summary().await;
        
        let response = SystemMetricsResponse {
            system_metrics,
            container_ru_summary: ru_summary,
        };
        
        Ok(Json(ApiResponse::success(response)))
    } else {
        Ok(Json(ApiResponse::error("Resource monitoring is not enabled".to_string())))
    }
}

/// Get system metrics history
pub async fn get_system_metrics_history(
    State(state): State<AppState>,
    Query(query): Query<MetricsQuery>,
) -> Result<Json<ApiResponse<Vec<SystemMetrics>>>, StatusCode> {
    info!("Getting system metrics history");
    
    if let Some(monitor) = &state.resource_monitor {
        let mut history = monitor.get_system_metrics_history().await;
        
        // Apply limit if specified
        if let Some(limit) = query.limit {
            let start_index = if history.len() > limit {
                history.len() - limit
            } else {
                0
            };
            history = history[start_index..].to_vec();
        }
        
        // Apply time filter if specified
        if let Some(since_minutes) = query.since_minutes {
            let cutoff_time = chrono::Utc::now() - chrono::Duration::minutes(since_minutes);
            history.retain(|metrics| metrics.timestamp >= cutoff_time);
        }
        
        Ok(Json(ApiResponse::success(history)))
    } else {
        Ok(Json(ApiResponse::error("Resource monitoring is not enabled".to_string())))
    }
}

/// Get current metrics for a specific container
pub async fn get_container_metrics(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
) -> Result<Json<ApiResponse<ContainerMetricsResponse>>, StatusCode> {
    info!("Getting metrics for container: {}", container_id);
    
    if let Some(monitor) = &state.resource_monitor {
        let metrics = monitor.get_container_metrics(&container_id).await;
        let ru_history = monitor.get_container_ru_history(&container_id).await;
        
        let response = ContainerMetricsResponse {
            current_metrics: metrics,
            ru_history,
        };
        
        Ok(Json(ApiResponse::success(response)))
    } else {
        Ok(Json(ApiResponse::error("Resource monitoring is not enabled".to_string())))
    }
}

/// Get metrics history for a specific container
pub async fn get_container_metrics_history(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
    Query(query): Query<MetricsQuery>,
) -> Result<Json<ApiResponse<Vec<ContainerMetrics>>>, StatusCode> {
    info!("Getting metrics history for container: {}", container_id);
    
    if let Some(monitor) = &state.resource_monitor {
        if let Some(mut history) = monitor.get_container_metrics_history(&container_id).await {
            // Apply limit if specified
            if let Some(limit) = query.limit {
                let start_index = if history.len() > limit {
                    history.len() - limit
                } else {
                    0
                };
                history = history[start_index..].to_vec();
            }
            
            // Apply time filter if specified
            if let Some(since_minutes) = query.since_minutes {
                let cutoff_time = chrono::Utc::now() - chrono::Duration::minutes(since_minutes);
                history.retain(|metrics| metrics.timestamp >= cutoff_time);
            }
            
            Ok(Json(ApiResponse::success(history)))
        } else {
            Ok(Json(ApiResponse::error(format!("No metrics found for container: {}", container_id))))
        }
    } else {
        Ok(Json(ApiResponse::error("Resource monitoring is not enabled".to_string())))
    }
}

/// Get RU summary for all containers
pub async fn get_ru_summary(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<RUSummaryResponse>>, StatusCode> {
    info!("Getting RU summary for all containers");
    
    if let Some(monitor) = &state.resource_monitor {
        let current_ru = monitor.get_ru_summary().await;
        let system_metrics = monitor.get_system_metrics().await;
        
        let total_ru = current_ru.values().sum::<f64>();
        let container_count = current_ru.len();
        
        let response = RUSummaryResponse {
            total_system_ru: total_ru,
            container_count,
            average_ru_per_container: if container_count > 0 {
                total_ru / container_count as f64
            } else {
                0.0
            },
            container_ru: current_ru,
            peak_ru_container: system_metrics.as_ref().and_then(|m| m.peak_ru_container.clone()),
            peak_ru_value: system_metrics.as_ref().map(|m| m.peak_ru_value).unwrap_or(0.0),
        };
        
        Ok(Json(ApiResponse::success(response)))
    } else {
        Ok(Json(ApiResponse::error("Resource monitoring is not enabled".to_string())))
    }
}

/// Get detailed RU breakdown for a specific container
pub async fn get_container_ru_breakdown(
    State(state): State<AppState>,
    Path(container_id): Path<String>,
) -> Result<Json<ApiResponse<crate::monitoring::ru_calculator::ContainerRUHistory>>, StatusCode> {
    info!("Getting RU breakdown for container: {}", container_id);
    
    if let Some(monitor) = &state.resource_monitor {
        if let Some(ru_history) = monitor.get_container_ru_history(&container_id).await {
            Ok(Json(ApiResponse::success(ru_history)))
        } else {
            Ok(Json(ApiResponse::error(format!("No RU history found for container: {}", container_id))))
        }
    } else {
        Ok(Json(ApiResponse::error("Resource monitoring is not enabled".to_string())))
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SystemMetricsResponse {
    pub system_metrics: Option<SystemMetrics>,
    pub container_ru_summary: std::collections::HashMap<String, f64>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ContainerMetricsResponse {
    pub current_metrics: Option<ContainerMetrics>,
    pub ru_history: Option<crate::monitoring::ru_calculator::ContainerRUHistory>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RUSummaryResponse {
    pub total_system_ru: f64,
    pub container_count: usize,
    pub average_ru_per_container: f64,
    pub container_ru: std::collections::HashMap<String, f64>,
    pub peak_ru_container: Option<String>,
    pub peak_ru_value: f64,
}


/// Get RU pricing configuration
pub async fn get_ru_config(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<RUConfigResponse>>, StatusCode> {
    info!("Getting RU configuration");
    
    let monitoring = state.config.monitoring.as_ref()
        .ok_or_else(|| StatusCode::SERVICE_UNAVAILABLE)?;
    let ru_config = &monitoring.ru_config;
    
    let response = RUConfigResponse {
        cpu_weight: ru_config.cpu_weight,
        memory_weight: ru_config.memory_weight,
        io_weight: ru_config.io_weight,
        network_weight: ru_config.network_weight,
        storage_weight: ru_config.storage_weight,
        base_ru: ru_config.base_ru,
        // Pricing: 1 RU = $0.001 per hour (configurable)
        ru_price_per_hour: 0.001,
    };
    
    Ok(Json(ApiResponse::success(response)))
}

/// Calculate estimated RU for given resource limits
pub async fn calculate_ru_estimate(
    State(state): State<AppState>,
    Json(req): Json<RUEstimateRequest>,
) -> Result<Json<ApiResponse<RUEstimateResponse>>, StatusCode> {
    info!("Calculating RU estimate for cpu={}, memory={}, disk={}", req.cpu, req.memory, req.disk);
    
    let monitoring = state.config.monitoring.as_ref()
        .ok_or_else(|| StatusCode::SERVICE_UNAVAILABLE)?;
    let ru_config = &monitoring.ru_config;
    
    // Parse CPU (e.g., "1", "0.5", "2")
    let cpu_cores: f64 = req.cpu.parse().unwrap_or(1.0);
    
    // Parse memory (e.g., "1024m", "2g")
    let memory_mb = parse_memory_to_mb(&req.memory);
    let memory_gb = memory_mb / 1024.0;
    
    // Parse disk (e.g., "10g", "100g")
    let disk_gb = parse_disk_to_gb(&req.disk);
    
    // Calculate RU components
    let cpu_ru = cpu_cores * ru_config.cpu_weight;
    let memory_ru = memory_gb * ru_config.memory_weight;
    let storage_ru = disk_gb * ru_config.storage_weight * 0.01; // Storage is less impactful
    let base_ru = ru_config.base_ru;
    
    let total_ru_per_hour = base_ru + cpu_ru + memory_ru + storage_ru;
    let ru_per_day = total_ru_per_hour * 24.0;
    let ru_per_month = ru_per_day * 30.0;
    
    // Pricing
    let price_per_hour = total_ru_per_hour * 0.001;
    let price_per_day = ru_per_day * 0.001;
    let price_per_month = ru_per_month * 0.001;
    
    let response = RUEstimateResponse {
        ru_per_hour: total_ru_per_hour,
        ru_per_day,
        ru_per_month,
        price_per_hour,
        price_per_day,
        price_per_month,
        breakdown: RUBreakdown {
            base: base_ru,
            cpu: cpu_ru,
            memory: memory_ru,
            storage: storage_ru,
        },
    };
    
    Ok(Json(ApiResponse::success(response)))
}

fn parse_memory_to_mb(memory: &str) -> f64 {
    let memory = memory.to_lowercase();
    if let Some(num_str) = memory.strip_suffix("g") {
        num_str.parse::<f64>().unwrap_or(1.0) * 1024.0
    } else if let Some(num_str) = memory.strip_suffix("m") {
        num_str.parse::<f64>().unwrap_or(1024.0)
    } else {
        memory.parse::<f64>().unwrap_or(1024.0)
    }
}

fn parse_disk_to_gb(disk: &str) -> f64 {
    let disk = disk.to_lowercase();
    if let Some(num_str) = disk.strip_suffix("g") {
        num_str.parse::<f64>().unwrap_or(10.0)
    } else if let Some(num_str) = disk.strip_suffix("t") {
        num_str.parse::<f64>().unwrap_or(1.0) * 1024.0
    } else {
        disk.parse::<f64>().unwrap_or(10.0)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RUConfigResponse {
    pub cpu_weight: f64,
    pub memory_weight: f64,
    pub io_weight: f64,
    pub network_weight: f64,
    pub storage_weight: f64,
    pub base_ru: f64,
    pub ru_price_per_hour: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RUEstimateRequest {
    pub cpu: String,
    pub memory: String,
    pub disk: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RUEstimateResponse {
    pub ru_per_hour: f64,
    pub ru_per_day: f64,
    pub ru_per_month: f64,
    pub price_per_hour: f64,
    pub price_per_day: f64,
    pub price_per_month: f64,
    pub breakdown: RUBreakdown,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RUBreakdown {
    pub base: f64,
    pub cpu: f64,
    pub memory: f64,
    pub storage: f64,
}
