//! Container Events - Broadcast channel for container state changes
//!
//! This module provides a pub/sub system for container events.
//! Multiple WebSocket connections can subscribe to the same container.
//! Each WebSocket manages its own log/stats streamers.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info};
use serde::{Serialize, Deserialize};

/// Container event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ContainerEvent {
    /// Container state changed (running, stopped, etc.)
    StateChanged { state: String },
    /// Container stats update
    Stats(EventContainerStats),
    /// Console output from container
    ConsoleOutput { line: String },
    /// Daemon message
    DaemonMessage { message: String },
    /// Power action started
    PowerActionStarted { action: String },
    /// Power action completed
    PowerActionCompleted { action: String, success: bool, message: String },
}

/// Container stats for broadcast
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventContainerStats {
    pub memory_bytes: u64,
    pub memory_limit_bytes: u64,
    pub cpu_percent: f64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub uptime: u64,
    pub disk_bytes: u64,
}

/// Event broadcaster for a single container
struct ContainerBroadcaster {
    sender: broadcast::Sender<ContainerEvent>,
    subscriber_count: usize,
}

/// Global event hub for all containers
pub struct ContainerEventHub {
    /// Container ID -> Broadcaster
    broadcasters: RwLock<HashMap<String, ContainerBroadcaster>>,
    /// Buffer size for broadcast channels
    buffer_size: usize,
}

impl ContainerEventHub {
    pub fn new() -> Self {
        Self {
            broadcasters: RwLock::new(HashMap::new()),
            buffer_size: 512, // Buffer up to 512 events
        }
    }

    /// Subscribe to events for a container
    /// Returns a receiver that can be used to receive events
    pub async fn subscribe(&self, container_id: &str) -> broadcast::Receiver<ContainerEvent> {
        let mut broadcasters = self.broadcasters.write().await;
        
        if let Some(broadcaster) = broadcasters.get_mut(container_id) {
            broadcaster.subscriber_count += 1;
            debug!("New subscriber for container {}, total: {}", container_id, broadcaster.subscriber_count);
            broadcaster.sender.subscribe()
        } else {
            // Create new broadcaster for this container
            let (sender, receiver) = broadcast::channel(self.buffer_size);
            broadcasters.insert(container_id.to_string(), ContainerBroadcaster {
                sender,
                subscriber_count: 1,
            });
            info!("Created new event broadcaster for container {}", container_id);
            receiver
        }
    }

    /// Unsubscribe from container events
    pub async fn unsubscribe(&self, container_id: &str) {
        let mut broadcasters = self.broadcasters.write().await;
        
        if let Some(broadcaster) = broadcasters.get_mut(container_id) {
            broadcaster.subscriber_count = broadcaster.subscriber_count.saturating_sub(1);
            debug!("Subscriber left for container {}, remaining: {}", container_id, broadcaster.subscriber_count);
            
            // Keep broadcaster even with 0 subscribers - it will be reused
            // Only remove if we want to clean up memory for containers that are rarely accessed
            if broadcaster.subscriber_count == 0 {
                // Optional: remove after some time, but for now keep it
                // broadcasters.remove(container_id);
                debug!("No subscribers left for container {}, keeping broadcaster", container_id);
            }
        }
    }

    /// Broadcast an event to all subscribers of a container
    pub async fn broadcast(&self, container_id: &str, event: ContainerEvent) {
        let broadcasters = self.broadcasters.read().await;
        
        if let Some(broadcaster) = broadcasters.get(container_id) {
            // send() returns error if no receivers, which is fine
            let _ = broadcaster.sender.send(event);
        } else {
            // No broadcaster exists - create one temporarily for this event
            drop(broadcasters);
            let mut broadcasters = self.broadcasters.write().await;
            let (sender, _) = broadcast::channel(self.buffer_size);
            let _ = sender.send(event);
            broadcasters.insert(container_id.to_string(), ContainerBroadcaster {
                sender,
                subscriber_count: 0,
            });
        }
    }

    /// Broadcast state change to a container
    pub async fn broadcast_state(&self, container_id: &str, state: &str) {
        self.broadcast(container_id, ContainerEvent::StateChanged {
            state: state.to_string(),
        }).await;
    }

    /// Broadcast console output
    pub async fn broadcast_console(&self, container_id: &str, line: &str) {
        self.broadcast(container_id, ContainerEvent::ConsoleOutput {
            line: line.to_string(),
        }).await;
    }

    /// Broadcast daemon message
    pub async fn broadcast_message(&self, container_id: &str, message: &str) {
        self.broadcast(container_id, ContainerEvent::DaemonMessage {
            message: message.to_string(),
        }).await;
    }

    /// Broadcast stats update
    pub async fn broadcast_stats(&self, container_id: &str, stats: EventContainerStats) {
        self.broadcast(container_id, ContainerEvent::Stats(stats)).await;
    }

    /// Broadcast power action started
    pub async fn broadcast_power_started(&self, container_id: &str, action: &str) {
        self.broadcast(container_id, ContainerEvent::PowerActionStarted {
            action: action.to_string(),
        }).await;
    }

    /// Broadcast power action completed
    pub async fn broadcast_power_completed(&self, container_id: &str, action: &str, success: bool, message: &str) {
        self.broadcast(container_id, ContainerEvent::PowerActionCompleted {
            action: action.to_string(),
            success,
            message: message.to_string(),
        }).await;
    }

    /// Get the number of subscribers for a container
    pub async fn subscriber_count(&self, container_id: &str) -> usize {
        let broadcasters = self.broadcasters.read().await;
        broadcasters.get(container_id)
            .map(|b| b.subscriber_count)
            .unwrap_or(0)
    }
}

impl Default for ContainerEventHub {
    fn default() -> Self {
        Self::new()
    }
}
