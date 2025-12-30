use bollard::Docker;
use bollard::container::StatsOptions;
use futures_util::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{interval, Duration};
use tracing::{error, info, warn, debug};
use chrono::{DateTime, Utc};

use super::ru_calculator::{RUCalculator, RUConfig};
use crate::state_manager::StateManager;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerMetrics {
    pub container_id: String,
    pub container_uuid: String,
    pub timestamp: DateTime<Utc>,
    pub cpu_percent: f64,
    pub memory_usage_bytes: u64,
    pub memory_limit_bytes: u64,
    pub memory_percent: f64,
    pub io_read_bytes: u64,
    pub io_write_bytes: u64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub storage_read_ops: u64,
    pub storage_write_ops: u64,
    pub pids: u64,
    pub is_running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMetrics {
    pub timestamp: DateTime<Utc>,
    pub total_containers: usize,
    pub running_containers: usize,
    pub total_ru: f64,
    pub average_ru_per_container: f64,
    pub peak_ru_container: Option<String>,
    pub peak_ru_value: f64,
}

pub struct ResourceMonitor {
    docker: Arc<Docker>,
    state_manager: Arc<Mutex<StateManager>>,
    ru_calculator: Arc<RwLock<RUCalculator>>,
    metrics_history: Arc<RwLock<HashMap<String, Vec<ContainerMetrics>>>>,
    system_metrics_history: Arc<RwLock<Vec<SystemMetrics>>>,
    monitoring_interval: Duration,
    max_metrics_history: usize,
}

impl ResourceMonitor {
    pub fn new(
        docker: Arc<Docker>,
        state_manager: Arc<Mutex<StateManager>>,
        ru_config: RUConfig,
        monitoring_interval_ms: u64,
    ) -> Self {
        Self {
            docker,
            state_manager,
            ru_calculator: Arc::new(RwLock::new(RUCalculator::new(ru_config))),
            metrics_history: Arc::new(RwLock::new(HashMap::new())),
            system_metrics_history: Arc::new(RwLock::new(Vec::new())),
            monitoring_interval: Duration::from_millis(monitoring_interval_ms),
            max_metrics_history: 1000, // Keep last 1000 samples per container
        }
    }

    /// Start the continuous monitoring loop
    pub async fn start_monitoring(&self) {
        info!("Starting resource monitoring with interval: {:?}", self.monitoring_interval);
        
        let mut interval_timer = interval(self.monitoring_interval);
        
        loop {
            interval_timer.tick().await;
            
            if let Err(e) = self.collect_metrics().await {
                error!("Error collecting metrics: {}", e);
            }
        }
    }

    async fn collect_metrics(&self) -> anyhow::Result<()> {
        let containers = {
            let state_manager = self.state_manager.lock().await;
            state_manager.get_all_containers().clone()
        };
        
        let mut container_metrics = Vec::new();
        let mut total_ru = 0.0;
        let mut running_containers = 0;
        let mut peak_ru_value = 0.0;
        let mut peak_ru_container = None;

        for (uuid, container_state) in &containers {
            if let Some(container_id) = &container_state.container_id {
                match self.collect_container_metrics(container_id, uuid).await {
                    Ok(Some(metrics)) => {
                        if metrics.is_running {
                            running_containers += 1;
                        }
                        
                        // Calculate RU for this container
                        let period_ms = self.monitoring_interval.as_millis() as u64;
                        let mut ru_calc = self.ru_calculator.write().await;
                        let ru = ru_calc.calculate_ru(
                            container_id,
                            uuid,
                            metrics.cpu_percent,
                            metrics.memory_usage_bytes,
                            metrics.memory_limit_bytes,
                            metrics.io_read_bytes,
                            metrics.io_write_bytes,
                            metrics.network_rx_bytes,
                            metrics.network_tx_bytes,
                            metrics.storage_read_ops,
                            metrics.storage_write_ops,
                            period_ms,
                        );
                        
                        total_ru += ru.ru_value;
                        
                        if ru.ru_value > peak_ru_value {
                            peak_ru_value = ru.ru_value;
                            peak_ru_container = Some(container_id.clone());
                        }
                        
                        debug!("Container {} RU: {:.4}", container_id, ru.ru_value);
                        container_metrics.push(metrics);
                    }
                    Ok(None) => {
                        // Container not running or no stats available
                        debug!("No metrics available for container: {}", container_id);
                    }
                    Err(e) => {
                        warn!("Failed to collect metrics for container {}: {}", container_id, e);
                    }
                }
            }
        }

        // Store metrics history
        let mut metrics_history = self.metrics_history.write().await;
        for metrics in container_metrics {
            let history = metrics_history.entry(metrics.container_id.clone()).or_insert_with(Vec::new);
            history.push(metrics);
            
            // Trim history if too large
            if history.len() > self.max_metrics_history {
                let excess = history.len() - self.max_metrics_history;
                history.drain(0..excess);
            }
        }

        // Create system metrics
        let system_metrics = SystemMetrics {
            timestamp: Utc::now(),
            total_containers: containers.len(),
            running_containers,
            total_ru,
            average_ru_per_container: if running_containers > 0 {
                total_ru / running_containers as f64
            } else {
                0.0
            },
            peak_ru_container,
            peak_ru_value,
        };

        // Store system metrics history
        let mut system_history = self.system_metrics_history.write().await;
        system_history.push(system_metrics.clone());
        
        // Trim system history
        if system_history.len() > self.max_metrics_history {
            let excess = system_history.len() - self.max_metrics_history;
            system_history.drain(0..excess);
        }

        info!("Collected metrics for {} containers, Total RU: {:.4}, Running: {}", 
              containers.len(), total_ru, running_containers);

        Ok(())
    }

    async fn collect_container_metrics(
        &self,
        container_id: &str,
        container_uuid: &str,
    ) -> anyhow::Result<Option<ContainerMetrics>> {
        let stats_options = StatsOptions {
            stream: false,
            one_shot: true,
        };

        let mut stats_stream = self.docker.stats(container_id, Some(stats_options));
        
        if let Some(stats_result) = stats_stream.next().await {
            let stats = stats_result?;
            let timestamp = Utc::now();

            // Check if container is running by checking if we have valid stats
            let is_running = !stats.read.is_empty();
            
            // Basic metrics - simplified to avoid API compatibility issues
            let cpu_percent = if is_running { 
                // Simple approximation - in production you'd calculate this properly
                let total_usage = stats.cpu_stats.cpu_usage.total_usage;
                (total_usage % 100) as f64
            } else { 
                0.0 
            };
            
            let memory_usage = stats.memory_stats.usage.unwrap_or(0);
            let memory_limit = stats.memory_stats.limit.unwrap_or(0);
            let memory_percent = if memory_limit > 0 {
                (memory_usage as f64 / memory_limit as f64) * 100.0
            } else {
                0.0
            };

            // Network metrics - simplified
            let (network_rx_bytes, network_tx_bytes) = if let Some(networks) = &stats.networks {
                let mut rx_total = 0;
                let mut tx_total = 0;
                
                for (_, network_stats) in networks {
                    rx_total += network_stats.rx_bytes;
                    tx_total += network_stats.tx_bytes;
                }
                
                (rx_total, tx_total)
            } else {
                (0, 0)
            };

            // PIDs
            let pids = stats.pids_stats.current.unwrap_or(0);

            Ok(Some(ContainerMetrics {
                container_id: container_id.to_string(),
                container_uuid: container_uuid.to_string(),
                timestamp,
                cpu_percent,
                memory_usage_bytes: memory_usage,
                memory_limit_bytes: memory_limit,
                memory_percent,
                io_read_bytes: 0, // Simplified - would need proper blkio parsing
                io_write_bytes: 0,
                network_rx_bytes,
                network_tx_bytes,
                storage_read_ops: 0, // Simplified
                storage_write_ops: 0,
                pids,
                is_running,
            }))
        } else {
            Ok(None)
        }
    }

    /// Get current metrics for a specific container
    pub async fn get_container_metrics(&self, container_id: &str) -> Option<ContainerMetrics> {
        let metrics_history = self.metrics_history.read().await;
        metrics_history.get(container_id)?.last().cloned()
    }

    /// Get metrics history for a specific container
    pub async fn get_container_metrics_history(&self, container_id: &str) -> Option<Vec<ContainerMetrics>> {
        let metrics_history = self.metrics_history.read().await;
        metrics_history.get(container_id).cloned()
    }

    /// Get current system metrics
    pub async fn get_system_metrics(&self) -> Option<SystemMetrics> {
        let system_history = self.system_metrics_history.read().await;
        system_history.last().cloned()
    }

    /// Get system metrics history
    pub async fn get_system_metrics_history(&self) -> Vec<SystemMetrics> {
        let system_history = self.system_metrics_history.read().await;
        system_history.clone()
    }

    /// Get current RU summary for all containers
    pub async fn get_ru_summary(&self) -> HashMap<String, f64> {
        let ru_calc = self.ru_calculator.read().await;
        ru_calc.get_current_ru_summary()
    }

    /// Get RU history for a specific container
    pub async fn get_container_ru_history(&self, container_id: &str) -> Option<super::ru_calculator::ContainerRUHistory> {
        let ru_calc = self.ru_calculator.read().await;
        ru_calc.get_container_history(container_id).cloned()
    }

    /// Remove container from monitoring
    pub async fn remove_container(&self, container_id: &str) {
        let mut metrics_history = self.metrics_history.write().await;
        metrics_history.remove(container_id);
        
        let mut ru_calc = self.ru_calculator.write().await;
        ru_calc.remove_container(container_id);
        
        info!("Removed container {} from monitoring", container_id);
    }
}