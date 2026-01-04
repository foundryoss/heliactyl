//! WebSocket module for lightd
//!
//! Channel-based WebSocket system for streaming container logs.
//! Based on docker-logs-streamer-via-web-socket approach:
//! - Use docker.logs() with follow: true and since timestamp
//! - Stream logs directly to websocket client
//! - Keep connection alive with pings
//! - Only stream logs when container is running

use serde::{Deserialize, Serialize};

pub mod connection;
pub mod token_manager;
pub mod handler;

pub use connection::*;
pub use token_manager::*;
pub use handler::WebSocketHandler;

/// WebSocket events (Server -> Client)
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WsEvent {
    /// Initial connection established
    Init,
    /// Console output from container
    ConsoleOutput,
    /// Container status change
    Status,
    /// Container stats (CPU, memory, network, etc.)
    Stats,
    /// Error message
    Error,
    /// Daemon/system message
    DaemonMessage,
}

/// WebSocket message format
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WsMessage {
    pub event: WsEvent,
    pub args: Vec<String>,
}

impl WsMessage {
    /// Create init message with container info
    pub fn init(container_id: &str, container_uuid: &str, status: &str) -> Self {
        Self {
            event: WsEvent::Init,
            args: vec![
                container_id.to_string(),
                container_uuid.to_string(),
                status.to_string(),
            ],
        }
    }

    /// Create console output message
    pub fn console_output(line: &str) -> Self {
        Self {
            event: WsEvent::ConsoleOutput,
            args: vec!["[container@pkg.lat]: ".to_string() + line],
        }
    }

    /// Create status message
    pub fn status(state: &str) -> Self {
        Self {
            event: WsEvent::Status,
            args: vec![state.to_string()],
        }
    }

    /// Create error message
    /// not in use.
    pub fn error(msg: &str) -> Self {
        Self {
            event: WsEvent::Error,
            args: vec![msg.to_string()],
        }
    }

    /// Create daemon message
    pub fn daemon_message(msg: &str) -> Self {
        Self {
            event: WsEvent::DaemonMessage,
            args: vec!["[container@pkg.lat]: ".to_string() + msg],
        }
    }

    /// Create stats message with container resource usage (JSON string)
    /// not in use
    pub fn stats_json(stats_json: &str) -> Self {
        Self {
            event: WsEvent::Stats,
            args: vec![stats_json.to_string()],
        }
    }
    
    /// Create stats message with individual values
    pub fn stats(
        memory_bytes: u64,
        memory_limit_bytes: u64,
        cpu_percent: f64,
        network_rx_bytes: u64,
        network_tx_bytes: u64,
        uptime: u64,
        state: &str,
        disk_bytes: u64,
    ) -> Self {
        let stats_json = serde_json::json!({
            "memory_bytes": memory_bytes,
            "memory_limit_bytes": memory_limit_bytes,
            "cpu_absolute": cpu_percent,
            "network": {
                "rx_bytes": network_rx_bytes,
                "tx_bytes": network_tx_bytes,
            },
            "uptime": uptime,
            "state": state,
            "disk_bytes": disk_bytes,
        });
        Self {
            event: WsEvent::Stats,
            args: vec![stats_json.to_string()],
        }
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}