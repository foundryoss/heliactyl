use bollard::Docker;
use bollard::container::StatsOptions;
use futures_util::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tokio::time::{interval, Duration, MissedTickBehavior};
use tracing::{error, info, warn, debug};
use chrono::{DateTime, Utc};

use super::ru_calculator::{RUCalculator, RUConfig};
use crate::state_manager::StateManager;
use crate::config::RemoteConfig;
use std::collections::HashSet;

/// Previous stats for delta calculation
#[derive(Debug, Clone, Default)]
struct PreviousStats {
    pub cpu_total_usage: u64,
    pub system_cpu_usage: u64,
    pub io_read_bytes: u64,
    pub io_write_bytes: u64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub storage_read_ops: u64,
    pub storage_write_ops: u64,
    pub timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerMetrics {
    pub container_id: String,
    pub container_uuid: String,
    pub timestamp: DateTime<Utc>,
    #[serde(default)]
    pub sample_period_ms: u64,
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
    remote_config: Option<RemoteConfig>,
    http_client: reqwest::Client,
    /// Accumulated RU not yet successfully posted to the panel (per container UUID)
    pending_ru: Arc<RwLock<HashMap<String, f64>>>,
    /// Containers that the panel says do not exist; avoid spamming posts.
    billing_disabled: Arc<RwLock<HashSet<String>>>,
    /// Track previous stats for delta calculations (CPU, I/O, network)
    previous_stats: Arc<RwLock<HashMap<String, PreviousStats>>>,
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
            max_metrics_history: 1000,
            remote_config: None,
            http_client: reqwest::Client::new(),
            pending_ru: Arc::new(RwLock::new(HashMap::new())),
            billing_disabled: Arc::new(RwLock::new(HashSet::new())),
            previous_stats: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set remote config for posting RU to panel
    pub fn with_remote(mut self, remote_config: Option<RemoteConfig>) -> Self {
        self.remote_config = remote_config;
        self
    }

    /// Start the continuous monitoring loop
    pub async fn start_monitoring(&self) {
        info!("Starting resource monitoring with interval: {:?}", self.monitoring_interval);
        
        let mut interval_timer = interval(self.monitoring_interval);
        // If a collection cycle takes longer than the configured interval, do NOT "catch up"
        // by running multiple back-to-back cycles. Catch-up causes tiny sample periods and makes
        // RU charges look incorrectly low under load.
        interval_timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
        
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

        let mut interval_ru_by_uuid: HashMap<String, f64> = HashMap::new();

        for (uuid, container_state) in &containers {
            if let Some(container_id) = &container_state.container_id {
                match self.collect_container_metrics(container_id, uuid).await {
                    Ok(Some(metrics)) => {
                        if metrics.is_running {
                            running_containers += 1;
                        }

                        // Calculate RU only for running containers.
                        // IMPORTANT: `ru_value` here is a per-interval charge, not a cumulative counter.
                        let interval_ru = if metrics.is_running {
                            let period_ms = metrics.sample_period_ms.max(1);
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

                            debug!(
                                "Container {} interval RU: {:.6} (period={}ms)",
                                container_id,
                                ru.ru_value,
                                period_ms
                            );
                            ru.ru_value
                        } else {
                            0.0
                        };

                        interval_ru_by_uuid.insert(uuid.clone(), interval_ru);
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

        // Post RU to remote panel if configured
        if let Some(ref remote) = self.remote_config {
            if remote.enabled {
                self.post_ru_to_panel(&containers, &interval_ru_by_uuid).await;
            }
        }

        info!("Collected metrics for {} containers, Total RU: {:.4}, Running: {}", 
              containers.len(), total_ru, running_containers);

        Ok(())
    }

    /// Post RU charges to the remote panel.
    ///
    /// `interval_ru_by_uuid` values are RU for *this monitoring interval* (already scaled by the
    /// effective sample period). We accumulate into `pending_ru` and only clear on successful post,
    /// preventing under-billing during panel downtime.
    async fn post_ru_to_panel(
        &self,
        containers: &HashMap<String, crate::state_manager::ContainerState>,
        interval_ru_by_uuid: &HashMap<String, f64>,
    ) {
        let remote = match &self.remote_config {
            Some(r) if r.enabled => r,
            _ => return,
        };

        for (uuid, _container_state) in containers {
            if self.billing_disabled.read().await.contains(uuid) {
                continue;
            }

            // Accumulate per-interval RU into pending, then decide if we should post.
            // IMPORTANT: do not hold locks across `.await`.
            let (amount_to_post, should_post) = {
                let mut pending = self.pending_ru.write().await;

                let interval_ru = interval_ru_by_uuid.get(uuid).copied().unwrap_or(0.0);
                if interval_ru > 0.0 {
                    *pending.entry(uuid.clone()).or_insert(0.0) += interval_ru;
                }

                let amount = pending.get(uuid).copied().unwrap_or(0.0);
                (amount, amount > 0.001)
            };

            if !should_post {
                continue;
            }

            let url = format!("{}/api/lightd/deduct", remote.url);

            let payload = serde_json::json!({
                "container_id": uuid,
                "amount": amount_to_post,
                "description": format!("Container usage - {:.4} RU", amount_to_post)
            });

            match self
                .http_client
                .post(&url)
                .header("Authorization", format!("Bearer {}", remote.secret))
                .header("Content-Type", "application/json")
                .json(&payload)
                .send()
                .await
            {
                Ok(response) => {
                    if response.status().is_success() {
                        // Check if user is suspended - if so, stop billing this container
                        let body_text = response.text().await.unwrap_or_default();
                        if let Ok(response_json) = serde_json::from_str::<serde_json::Value>(&body_text) {
                            if response_json.get("suspended").and_then(|v| v.as_bool()).unwrap_or(false) {
                                warn!(
                                    "User is suspended for container {}. Disabling billing.",
                                    uuid
                                );
                                let mut disabled = self.billing_disabled.write().await;
                                disabled.insert(uuid.clone());
                            }
                        }

                        debug!(
                            "Posted RU {:.6} for container {} to panel",
                            amount_to_post,
                            uuid
                        );

                        let mut pending = self.pending_ru.write().await;
                        pending.insert(uuid.clone(), 0.0);
                    } else {
                        let status = response.status();
                        let body = response.text().await.unwrap_or_default();
                        warn!("Panel rejected RU post for {}: {} - {}", uuid, status, body);

                        // If the panel says the server doesn't exist for this container,
                        // disable further billing to avoid endless spam.
                        if status.as_u16() == 404 {
                            let mut disabled = self.billing_disabled.write().await;
                            disabled.insert(uuid.clone());
                            let mut pending = self.pending_ru.write().await;
                            pending.insert(uuid.clone(), 0.0);

                            warn!(
                                "Disabling RU posts for {} due to panel 404; container likely not registered on panel",
                                uuid
                            );
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to post RU to panel for {}: {}", uuid, e);
                }
            }
        }

        // Clean up containers that no longer exist
        let current_containers: std::collections::HashSet<_> = containers.keys().cloned().collect();
        {
            let mut pending = self.pending_ru.write().await;
            pending.retain(|k, _| current_containers.contains(k));
        }

        {
            let mut disabled = self.billing_disabled.write().await;
            disabled.retain(|k| current_containers.contains(k));
        }
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

        // Add timeout to prevent hanging on unresponsive containers
        let stats_future = async {
            let mut stats_stream = self.docker.stats(container_id, Some(stats_options));
            stats_stream.next().await
        };
        
        let stats_result = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            stats_future
        ).await {
            Ok(Some(result)) => result,
            Ok(None) => return Ok(None),
            Err(_) => {
                warn!("Stats collection timed out for container {} - may be unresponsive", container_id);
                return Ok(None);
            }
        };
        
        let stats = match stats_result {
            Ok(s) => s,
            Err(e) => {
                // Container might be stopped or not found
                debug!("Could not get stats for container {}: {}", container_id, e);
                return Ok(None);
            }
        };
        
        let timestamp = Utc::now();

        // If Docker returned stats successfully, treat container as running.
        // (Stopped containers typically error on stats.)
        let is_running = true;

        // Get previous stats for delta calculation
        let mut prev_stats_map = self.previous_stats.write().await;
        let prev_stats = prev_stats_map.entry(container_id.to_string()).or_default();

        let sample_period_ms = prev_stats
            .timestamp
            .and_then(|prev_ts| {
                let ms = (timestamp - prev_ts).num_milliseconds();
                if ms > 0 { Some(ms as u64) } else { None }
            })
            .unwrap_or_else(|| self.monitoring_interval.as_millis().max(1) as u64);

        // ===== CPU Calculation (proper delta-based) =====
        let cpu_total_usage = stats.cpu_stats.cpu_usage.total_usage;
        let system_cpu_usage = stats.cpu_stats.system_cpu_usage.unwrap_or(0);
        let num_cpus = stats.cpu_stats.online_cpus.unwrap_or(1) as f64;
        
        let cpu_percent = if prev_stats.timestamp.is_some() && prev_stats.system_cpu_usage > 0 {
            let cpu_delta = cpu_total_usage.saturating_sub(prev_stats.cpu_total_usage) as f64;
            let system_delta = system_cpu_usage.saturating_sub(prev_stats.system_cpu_usage) as f64;
            
            if system_delta > 0.0 && cpu_delta >= 0.0 {
                // Calculate CPU percentage: (container CPU delta / system CPU delta) * num_cpus * 100
                (cpu_delta / system_delta) * num_cpus * 100.0
            } else {
                0.0
            }
        } else {
            0.0 // First sample, no delta available
        };

        // Clamp CPU percent to reasonable bounds (0-100 * num_cpus)
        let cpu_percent = cpu_percent.clamp(0.0, num_cpus * 100.0);

        // ===== Memory Metrics =====
        let memory_usage = stats.memory_stats.usage.unwrap_or(0);
        let memory_limit = stats.memory_stats.limit.unwrap_or(0);
        let memory_percent = if memory_limit > 0 {
            (memory_usage as f64 / memory_limit as f64) * 100.0
        } else {
            0.0
        };

        // ===== I/O Metrics (block I/O) =====
        let (io_read_bytes_total, io_write_bytes_total, storage_read_ops_total, storage_write_ops_total) = 
            if let Some(io_stats) = &stats.blkio_stats.io_service_bytes_recursive {
                let mut read = 0u64;
                let mut write = 0u64;
                for stat in io_stats {
                    match stat.op.as_str() {
                        "read" | "Read" => read += stat.value,
                        "write" | "Write" => write += stat.value,
                        _ => {}
                    }
                }
                
                // Get I/O ops from io_serviced_recursive
                let (read_ops, write_ops) = if let Some(io_ops) = &stats.blkio_stats.io_serviced_recursive {
                    let mut r_ops = 0u64;
                    let mut w_ops = 0u64;
                    for stat in io_ops {
                        match stat.op.as_str() {
                            "read" | "Read" => r_ops += stat.value,
                            "write" | "Write" => w_ops += stat.value,
                            _ => {}
                        }
                    }
                    (r_ops, w_ops)
                } else {
                    (0, 0)
                };
                
                (read, write, read_ops, write_ops)
            } else {
                (0, 0, 0, 0)
            };

        // Calculate I/O deltas
        let io_read_delta = io_read_bytes_total.saturating_sub(prev_stats.io_read_bytes);
        let io_write_delta = io_write_bytes_total.saturating_sub(prev_stats.io_write_bytes);
        let storage_read_ops_delta = storage_read_ops_total.saturating_sub(prev_stats.storage_read_ops);
        let storage_write_ops_delta = storage_write_ops_total.saturating_sub(prev_stats.storage_write_ops);

        // ===== Network Metrics =====
        let (network_rx_total, network_tx_total) = if let Some(networks) = &stats.networks {
            let mut rx_total = 0u64;
            let mut tx_total = 0u64;
            
            for (_, network_stats) in networks {
                rx_total += network_stats.rx_bytes;
                tx_total += network_stats.tx_bytes;
            }
            
            (rx_total, tx_total)
        } else {
            (0, 0)
        };

        // Calculate network deltas
        let network_rx_delta = network_rx_total.saturating_sub(prev_stats.network_rx_bytes);
        let network_tx_delta = network_tx_total.saturating_sub(prev_stats.network_tx_bytes);

        // ===== PIDs =====
        let pids = stats.pids_stats.current.unwrap_or(0);

        // Update previous stats for next iteration
        *prev_stats = PreviousStats {
            cpu_total_usage,
            system_cpu_usage,
            io_read_bytes: io_read_bytes_total,
            io_write_bytes: io_write_bytes_total,
            network_rx_bytes: network_rx_total,
            network_tx_bytes: network_tx_total,
            storage_read_ops: storage_read_ops_total,
            storage_write_ops: storage_write_ops_total,
            timestamp: Some(timestamp),
        };

        debug!(
            "Container {} metrics - CPU: {:.2}%, Memory: {:.2}%, I/O R/W: {}/{} bytes, Net R/W: {}/{} bytes",
            container_id, cpu_percent, memory_percent, io_read_delta, io_write_delta, network_rx_delta, network_tx_delta
        );

        Ok(Some(ContainerMetrics {
            container_id: container_id.to_string(),
            container_uuid: container_uuid.to_string(),
            timestamp,
            sample_period_ms,
            cpu_percent,
            memory_usage_bytes: memory_usage,
            memory_limit_bytes: memory_limit,
            memory_percent,
            io_read_bytes: io_read_delta,
            io_write_bytes: io_write_delta,
            network_rx_bytes: network_rx_delta,
            network_tx_bytes: network_tx_delta,
            storage_read_ops: storage_read_ops_delta,
            storage_write_ops: storage_write_ops_delta,
            pids,
            is_running,
        }))
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