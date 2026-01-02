use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RUConfig {
    /// CPU weight in RU calculation (default: 1.0)
    /// 100% CPU usage = 1.0 * cpu_weight RU per second
    pub cpu_weight: f64,
    /// Memory weight in RU calculation (default: 0.5)
    /// 100% memory usage = 0.5 * memory_weight RU per second
    pub memory_weight: f64,
    /// I/O weight in RU calculation (default: 2.0)
    /// 1 MB/s I/O = 1.0 * io_weight RU per second
    pub io_weight: f64,
    /// Network weight in RU calculation (default: 1.5)
    /// 1 MB/s network = 1.0 * network_weight RU per second
    pub network_weight: f64,
    /// Storage weight in RU calculation (default: 0.8)
    /// 100 IOPS = 1.0 * storage_weight RU per second
    pub storage_weight: f64,
    /// Base RU per container per second (idle cost)
    pub base_ru: f64,
}

impl Default for RUConfig {
    fn default() -> Self {
        Self {
            cpu_weight: 1.0,
            memory_weight: 0.5,
            io_weight: 2.0,
            network_weight: 1.5,
            storage_weight: 0.8,
            base_ru: 0.1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceUnit {
    pub container_id: String,
    pub container_uuid: String,
    pub timestamp: DateTime<Utc>,
    pub ru_value: f64,
    pub breakdown: RUBreakdown,
    pub period_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RUBreakdown {
    pub cpu_ru: f64,
    pub memory_ru: f64,
    pub io_ru: f64,
    pub network_ru: f64,
    pub storage_ru: f64,
    pub base_ru: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerRUHistory {
    pub container_id: String,
    pub container_uuid: String,
    pub total_ru: f64,
    pub average_ru_per_second: f64,
    pub peak_ru: f64,
    pub samples: Vec<ResourceUnit>,
    pub created_at: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
}

pub struct RUCalculator {
    config: RUConfig,
    history: HashMap<String, ContainerRUHistory>,
    max_history_samples: usize,
}

impl RUCalculator {
    pub fn new(config: RUConfig) -> Self {
        Self {
            config,
            history: HashMap::new(),
            max_history_samples: 1000, // Keep last 1000 samples per container
        }
    }

    /// Calculate RU for a container based on its metrics
    /// All metrics should be DELTAS for the given period, not cumulative values
    pub fn calculate_ru(
        &mut self,
        container_id: &str,
        container_uuid: &str,
        cpu_percent: f64,
        memory_usage_bytes: u64,
        memory_limit_bytes: u64,
        io_read_bytes: u64,
        io_write_bytes: u64,
        network_rx_bytes: u64,
        network_tx_bytes: u64,
        storage_read_ops: u64,
        storage_write_ops: u64,
        period_ms: u64,
    ) -> ResourceUnit {
        let timestamp = Utc::now();
        
        // Calculate individual RU components
        // Scale by period to get RU per second, then multiply by actual period
        let period_seconds = period_ms as f64 / 1000.0;
        
        let cpu_ru = self.calculate_cpu_ru(cpu_percent) * period_seconds;
        let memory_ru = self.calculate_memory_ru(memory_usage_bytes, memory_limit_bytes) * period_seconds;
        let io_ru = self.calculate_io_ru(io_read_bytes, io_write_bytes, period_ms);
        let network_ru = self.calculate_network_ru(network_rx_bytes, network_tx_bytes, period_ms);
        let storage_ru = self.calculate_storage_ru(storage_read_ops, storage_write_ops, period_ms);
        let base_ru = self.config.base_ru * period_seconds;

        let total_ru = cpu_ru + memory_ru + io_ru + network_ru + storage_ru + base_ru;

        debug!(
            "RU breakdown for {}: CPU={:.6} ({}%), Mem={:.6} ({}%), I/O={:.6} ({}/{} bytes), Net={:.6} ({}/{} bytes), Storage={:.6}, Base={:.6}, Total={:.6}",
            container_uuid,
            cpu_ru, cpu_percent,
            memory_ru, if memory_limit_bytes > 0 { (memory_usage_bytes as f64 / memory_limit_bytes as f64) * 100.0 } else { 0.0 },
            io_ru, io_read_bytes, io_write_bytes,
            network_ru, network_rx_bytes, network_tx_bytes,
            storage_ru,
            base_ru,
            total_ru
        );

        let breakdown = RUBreakdown {
            cpu_ru,
            memory_ru,
            io_ru,
            network_ru,
            storage_ru,
            base_ru,
        };

        let ru = ResourceUnit {
            container_id: container_id.to_string(),
            container_uuid: container_uuid.to_string(),
            timestamp,
            ru_value: total_ru,
            breakdown,
            period_ms,
        };

        // Update history
        self.update_history(&ru);

        ru
    }

    fn calculate_cpu_ru(&self, cpu_percent: f64) -> f64 {
        // CPU RU per second: percentage of CPU usage * weight
        // 100% CPU = 1.0 * weight RU per second
        (cpu_percent / 100.0) * self.config.cpu_weight
    }

    fn calculate_memory_ru(&self, usage_bytes: u64, limit_bytes: u64) -> f64 {
        if limit_bytes == 0 {
            return 0.0;
        }
        
        // Memory RU per second: percentage of memory limit used * weight
        let memory_percent = (usage_bytes as f64 / limit_bytes as f64) * 100.0;
        (memory_percent / 100.0) * self.config.memory_weight
    }

    fn calculate_io_ru(&self, read_bytes: u64, write_bytes: u64, period_ms: u64) -> f64 {
        // I/O RU: MB transferred during period * weight
        // These are already delta values for this period
        if period_ms == 0 {
            return 0.0;
        }
        let total_mb = (read_bytes + write_bytes) as f64 / (1024.0 * 1024.0);
        total_mb * self.config.io_weight
    }

    fn calculate_network_ru(&self, rx_bytes: u64, tx_bytes: u64, period_ms: u64) -> f64 {
        // Network RU: MB transferred during period * weight
        // These are already delta values for this period
        if period_ms == 0 {
            return 0.0;
        }
        let total_mb = (rx_bytes + tx_bytes) as f64 / (1024.0 * 1024.0);
        total_mb * self.config.network_weight
    }

    fn calculate_storage_ru(&self, read_ops: u64, write_ops: u64, period_ms: u64) -> f64 {
        // Storage RU: operations during period / 100 * weight
        // These are already delta values for this period
        if period_ms == 0 {
            return 0.0;
        }
        let total_ops = (read_ops + write_ops) as f64;
        (total_ops / 100.0) * self.config.storage_weight
    }

    fn update_history(&mut self, ru: &ResourceUnit) {
        let history = self.history.entry(ru.container_id.clone()).or_insert_with(|| {
            ContainerRUHistory {
                container_id: ru.container_id.clone(),
                container_uuid: ru.container_uuid.clone(),
                total_ru: 0.0,
                average_ru_per_second: 0.0,
                peak_ru: 0.0,
                samples: Vec::new(),
                created_at: ru.timestamp,
                last_updated: ru.timestamp,
            }
        });

        // Add new sample
        history.samples.push(ru.clone());
        history.last_updated = ru.timestamp;
        history.total_ru += ru.ru_value;

        // Update peak RU
        if ru.ru_value > history.peak_ru {
            history.peak_ru = ru.ru_value;
        }

        // Calculate average RU per second
        if !history.samples.is_empty() {
            let duration_seconds = (history.last_updated - history.created_at).num_seconds() as f64;
            if duration_seconds > 0.0 {
                history.average_ru_per_second = history.total_ru / duration_seconds;
            }
        }

        // Trim history if too large
        if history.samples.len() > self.max_history_samples {
            let excess = history.samples.len() - self.max_history_samples;
            history.samples.drain(0..excess);
        }
    }

    pub fn get_container_history(&self, container_id: &str) -> Option<&ContainerRUHistory> {
        self.history.get(container_id)
    }

    pub fn get_all_histories(&self) -> &HashMap<String, ContainerRUHistory> {
        &self.history
    }

    pub fn remove_container(&mut self, container_id: &str) {
        self.history.remove(container_id);
    }

    pub fn get_current_ru_summary(&self) -> HashMap<String, f64> {
        self.history
            .iter()
            .filter_map(|(_id, history)| {
                history.samples.last().map(|sample| (sample.container_uuid.clone(), sample.ru_value))
            })
            .collect()
    }

    pub fn get_total_system_ru(&self) -> f64 {
        self.get_current_ru_summary().values().sum()
    }
}