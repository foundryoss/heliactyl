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