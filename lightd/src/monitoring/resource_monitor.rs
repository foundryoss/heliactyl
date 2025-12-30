use bollard::Docker;
use bollard::container::StatsOptions;
use futures_util::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{interval, Duration, Instant};
use tracing::{error, info, warn, debug};
use chrono::{DateTime, Utc};

use super::ru_calculator::{RUCalculator, ResourceUnit, RUConfig};
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
    previous_stats: Arc<RwLock<HashMap<String, ContainerStatsSnapshot>>>,
}

#[derive(Debug, Clone)]
struct ContainerStatsSnapshot {
    timestamp: Instant,
    cpu_total_usage: u64,
    system_cpu_usage: u64,
    online_cpus: u64,
    io_read_bytes: u64,
    io_write_bytes: u64,
    network_rx_bytes: u64,
    network_tx_bytes: u64,
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
            previous_stats: Arc::new(RwLock::new(HashMap::new())),
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
        let state_manager = self.state_manager.lock().await;
        let containers = state_manager.get_all_containers();
        
        let mut container_metrics = Vec::new();
        let mut total_ru = 0.0;
        let mut running_containers = 0;
        let mut peak_ru_value = 0.0;
        let mut peak_ru_container = None;

        for (uuid, container_state) in containers {
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
        
        drop(state_manager);

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
            let now = Instant::now();
            let timestamp = Utc::now();

            // Check if container is running
            let is_running = stats.read.is_some();
            if !is_running {
                return Ok(Some(ContainerMetrics {
                    container_id: container_id.to_string(),
                    container_uuid: container_uuid.to_string(),
                    timestamp,
                    cpu_percent: 0.0,
                    memory_usage_bytes: 0,
                    memory_limit_bytes: 0,
                    memory_percent: 0.0,
                    io_read_bytes: 0,
                    io_write_bytes: 0,
                    network_rx_bytes: 0,
                    network_tx_bytes: 0,
                    storage_read_ops: 0,
                    storage_write_ops: 0,
                    pids: 0,
                    is_running: false,
                }));
            }

            // Extract metrics from stats
            let cpu_stats = stats.cpu_stats.as_ref().unwrap();
            let precpu_stats = stats.precpu_stats.as_ref().unwrap();
            let memory_stats = stats.memory_stats.as_ref().unwrap();
            
            // Calculate CPU percentage
            let cpu_percent = self.calculate_cpu_percent(cpu_stats, precpu_stats);
            
            // Memory metrics
            let memory_usage = memory_stats.usage.unwrap_or(0);
            let memory_limit = memory_stats.limit.unwrap_or(0);
            let memory_percent = if memory_limit > 0 {
                (memory_usage as f64 / memory_limit as f64) * 100.0
            } else {
                0.0
            };

            // I/O metrics
            let (io_read_bytes, io_write_bytes) = if let Some(blkio_stats) = &stats.blkio_stats {
                let read_bytes = blkio_stats.io_service_bytes_recursive.as_ref()
                    .and_then(|stats| stats.iter().find(|s| s.op == Some("read".to_string())))
                    .map(|s| s.value.unwrap_or(0))
                    .unwrap_or(0);
                
                let write_bytes = blkio_stats.io_service_bytes_recursive.as_ref()
                    .and_then(|stats| stats.iter().find(|s| s.op == Some("write".to_string())))
                    .map(|s| s.value.unwrap_or(0))
                    .unwrap_or(0);
                
                (read_bytes, write_bytes)
            } else {
                (0, 0)
            };

            // Network metrics
            let (network_rx_bytes, network_tx_bytes) = if let Some(networks) = &stats.networks {
                let mut rx_total = 0;
                let mut tx_total = 0;
                
                for (_, network_stats) in networks {
                    rx_total += network_stats.rx_bytes.unwrap_or(0);
                    tx_total += network_stats.tx_bytes.unwrap_or(0);
                }
                
                (rx_total, tx_total)
            } else {
                (0, 0)
            };

            // Storage operations (approximated from blkio)
            let (storage_read_ops, storage_write_ops) = if let Some(blkio_stats) = &stats.blkio_stats {
                let read_ops = blkio_stats.io_serviced_recursive.as_ref()
                    .and_then(|stats| stats.iter().find(|s| s.op == Some("read".to_string())))
                    .map(|s| s.value.unwrap_or(0))
                    .unwrap_or(0);
                
                let write_ops = blkio_stats.io_serviced_recursive.as_ref()
                    .and_then(|stats| stats.iter().find(|s| s.op == Some("write".to_string())))
                    .map(|s| s.value.unwrap_or(0))
                    .unwrap_or(0);
                
                (read_ops, write_ops)
            } else {
                (0, 0)
            };

            // PIDs
            let pids = stats.pids_stats.as_ref()
                .and_then(|pids| pids.current)
                .unwrap_or(0);

            Ok(Some(ContainerMetrics {
                container_id: container_id.to_string(),
                container_uuid: container_uuid.to_string(),
                timestamp,
                cpu_percent,
                memory_usage_bytes: memory_usage,
                memory_limit_bytes: memory_limit,
                memory_percent,
                io_read_bytes,
                io_write_bytes,
                network_rx_bytes,
                network_tx_bytes,
                storage_read_ops,
                storage_write_ops,
                pids,
                is_running: true,
            }))
        } else {
            Ok(None)
        }
    }

    fn calculate_cpu_percent(
        &self,
        cpu_stats: &bollard::models::CpuStats,
        precpu_stats: &bollard::models::CpuStats,
    ) -> f64 {
        let cpu_total = cpu_stats.cpu_usage.as_ref()
            .and_then(|usage| usage.total_usage)
            .unwrap_or(0);
        
        let precpu_total = precpu_stats.cpu_usage.as_ref()
            .and_then(|usage| usage.total_usage)
            .unwrap_or(0);
        
        let system_cpu = cpu_stats.system_cpu_usage.unwrap_or(0);
        let presystem_cpu = precpu_stats.system_cpu_usage.unwrap_or(0);
        
        let online_cpus = cpu_stats.online_cpus.unwrap_or(1) as f64;
        
        let cpu_delta = cpu_total.saturating_sub(precpu_total) as f64;
        let system_delta = system_cpu.saturating_sub(presystem_cpu) as f64;
        
        if system_delta > 0.0 && cpu_delta > 0.0 {
            (cpu_delta / system_delta) * online_cpus * 100.0
        } else {
            0.0
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
        
        let mut prev_stats = self.previous_stats.write().await;
        prev_stats.remove(container_id);
        
        info!("Removed container {} from monitoring", container_id);
    }
}