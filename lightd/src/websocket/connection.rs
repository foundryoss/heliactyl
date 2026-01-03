//! WebSocket connection handler
//!
//! Channel-based WebSocket that stays alive and streams logs when container is running.
//! Uses Docker Events API for efficient real-time container state monitoring.
//! Also streams container stats (CPU, memory, network) when running.
//! DO NOT MODIFY THIS FILE WITHOUT UPDATING THE TEST SCRIPT IN `lightd/examples/test_websocket.sh`

use axum::extract::ws::{Message, WebSocket};
use bollard::container::{LogOutput, LogsOptions, StatsOptions};
use bollard::system::EventsOptions;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;
use tokio::time::Instant;
use tracing::{debug, info, warn};

use crate::types::AppState;
use super::{WebSocketToken, WsMessage};

/// Calculate "since" timestamp for docker logs
#[inline]
fn docker_since(mins: u64) -> i64 {
    if mins == 0 {
        return SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
    }

    SystemTime::now()
        .checked_sub(Duration::from_secs(mins * 60))
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Container state events
#[derive(Debug, Clone)]
enum ContainerEvent {
    Started,
    Stopped,
    Died,
}

/// Simplified container stats for WebSocket transmission
#[derive(Debug, Clone, Serialize)]
pub struct ContainerStats {
    pub memory_bytes: u64,
    pub memory_limit_bytes: u64,
    pub cpu_absolute: f64,
    pub network: NetworkStats,
    pub uptime: u64,
    pub state: String,
    pub disk_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetworkStats {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

/// WebSocket connection - channel-based, stays alive
pub struct WebSocketConnection {
    pub token: WebSocketToken,
    pub socket: WebSocket,
    pub state: Arc<AppState>,
}

impl WebSocketConnection {
    pub fn new(token: WebSocketToken, socket: WebSocket, state: Arc<AppState>) -> Self {
        Self { token, socket, state }
    }

    /// Check initial container state
    async fn check_initial_state(docker: &bollard::Docker, container_id: &str) -> bool {
        docker
            .inspect_container(container_id, None)
            .await
            .ok()
            .and_then(|info| info.state)
            .and_then(|s| s.running)
            .unwrap_or(false)
    }

    /// Get container start time for uptime calculation
    async fn get_container_start_time(docker: &bollard::Docker, container_id: &str) -> Option<i64> {
        docker
            .inspect_container(container_id, None)
            .await
            .ok()
            .and_then(|info| info.state)
            .and_then(|s| s.started_at)
            .and_then(|started| {
                // Parse ISO 8601 timestamp
                chrono::DateTime::parse_from_rfc3339(&started)
                    .ok()
                    .map(|dt| dt.timestamp())
            })
    }

    /// Calculate CPU percentage from stats
    fn calculate_cpu_percent(
        cpu_delta: u64,
        system_delta: u64,
        num_cpus: u64,
    ) -> f64 {
        if system_delta == 0 || num_cpus == 0 {
            return 0.0;
        }
        ((cpu_delta as f64 / system_delta as f64) * num_cpus as f64 * 100.0 * 100.0).round() / 100.0
    }

    /// Spawn Docker events listener for this specific container
    async fn spawn_event_listener(
        docker: Arc<bollard::Docker>,
        container_id: String,
    ) -> mpsc::UnboundedReceiver<ContainerEvent> {
        let (tx, rx) = mpsc::unbounded_channel();

        tokio::spawn(async move {
            let mut filters = HashMap::new();
            filters.insert("container".to_string(), vec![container_id.clone()]);
            filters.insert("type".to_string(), vec!["container".to_string()]);
            filters.insert("event".to_string(), vec![
                "start".to_string(),
                "die".to_string(),
                "stop".to_string(),
            ]);

            let options = EventsOptions {
                since: Some(docker_since(0).to_string()),
                filters,
                ..Default::default()
            };

            let mut event_stream = docker.events(Some(options));

            while let Some(event_result) = event_stream.next().await {
                match event_result {
                    Ok(event) => {
                        let container_event = match event.action.as_deref() {
                            Some("start") => Some(ContainerEvent::Started),
                            Some("die") | Some("stop") => Some(ContainerEvent::Stopped),
                            _ => None,
                        };

                        if let Some(ce) = container_event {
                            if tx.send(ce).is_err() {
                                debug!("Event receiver dropped, stopping listener");
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Docker events stream error: {}", e);
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }

            debug!("Docker events listener stopped for: {}", container_id);
        });

        rx
    }

    /// Main handler - keeps websocket alive as a channel
    pub async fn handle(self) {
        let container_id = self.token.container_id.clone();
        let container_uuid = self.token.container_uuid.clone();
        let docker = Arc::new(self.state.docker.client.clone());

        info!("WebSocket channel opened for container: {} ({})", container_uuid, container_id);

        let (mut tx, mut rx) = self.socket.split();

        // Get current container status (lock-free)
        let current_status = self.state.state_manager.get_container(&container_uuid)
            .map(|cs| cs.state.clone())
            .unwrap_or_else(|| "offline".to_string());

        // Send init message
        let init_msg = WsMessage::init(&container_id, &container_uuid, &current_status);
        if tx.send(Message::Text(init_msg.to_json())).await.is_err() {
            warn!("Failed to send init message, closing connection");
            return;
        }

        info!("Sent init message, container status: {}", current_status);

        // Check initial state and spawn event listener
        let is_running = Self::check_initial_state(&docker, &container_id).await;
        let mut event_rx = Self::spawn_event_listener(docker.clone(), container_id.clone()).await;

        const PING_INTERVAL: Duration = Duration::from_secs(30);
        const STATS_INTERVAL: Duration = Duration::from_secs(1);
        let mut last_ping = Instant::now();
        let mut last_stats = Instant::now();
        let mut log_stream: Option<_> = None;
        let mut stats_stream: Option<_> = None;
        let mut current_running = is_running;
        let mut container_start_time: Option<i64> = None;

        // Start log and stats streams if container is already running
        if is_running {
            let log_options = LogsOptions::<String> {
                follow: true,
                stdout: true,
                stderr: true,
                since: docker_since(0),
                timestamps: false,
                ..Default::default()
            };
            log_stream = Some(docker.logs(&container_id, Some(log_options)));
            
            // Start stats stream (streaming mode)
            let stats_options = StatsOptions {
                stream: true,
                one_shot: false,
            };
            stats_stream = Some(docker.stats(&container_id, Some(stats_options)));
            container_start_time = Self::get_container_start_time(&docker, &container_id).await;
            
            let _ = tx.send(Message::Text(WsMessage::status("running").to_json())).await;
        }

        loop {
            tokio::select! {
                biased;

                // Docker events - most efficient way to detect state changes
                event = event_rx.recv() => {
                    match event {
                        Some(ContainerEvent::Started) if !current_running => {
                            info!("Container {} started (via event)", container_id);
                            current_running = true;

                            let _ = tx.send(Message::Text(WsMessage::status("running").to_json())).await;
                            let _ = tx.send(Message::Text(
                                WsMessage::daemon_message("Container started, streaming logs...").to_json()
                            )).await;

                            // Start log stream
                            let log_options = LogsOptions::<String> {
                                follow: true,
                                stdout: true,
                                stderr: true,
                                since: docker_since(0),
                                timestamps: false,
                                ..Default::default()
                            };
                            log_stream = Some(docker.logs(&container_id, Some(log_options)));
                            
                            // Start stats stream
                            let stats_options = StatsOptions {
                                stream: true,
                                one_shot: false,
                            };
                            stats_stream = Some(docker.stats(&container_id, Some(stats_options)));
                            container_start_time = Self::get_container_start_time(&docker, &container_id).await;
                        }
                        Some(ContainerEvent::Stopped) | Some(ContainerEvent::Died) if current_running => {
                            info!("Container {} stopped (via event)", container_id);
                            current_running = false;

                            let _ = tx.send(Message::Text(WsMessage::status("stopped").to_json())).await;
                            let _ = tx.send(Message::Text(
                                WsMessage::daemon_message("Container stopped").to_json()
                            )).await;

                            log_stream = None;
                            stats_stream = None;
                            container_start_time = None;
                        }
                        None => {
                            warn!("Event listener died, reconnecting...");
                            event_rx = Self::spawn_event_listener(docker.clone(), container_id.clone()).await;
                        }
                        _ => {}
                    }
                }

                // Stream logs if active
                log_result = async {
                    match log_stream.as_mut() {
                        Some(stream) => stream.next().await,
                        None => {
                            // No active stream, yield briefly
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            None
                        }
                    }
                } => {
                    if let Some(result) = log_result {
                        match result {
                            Ok(log_output) => {
                                let message_bytes = match log_output {
                                    LogOutput::StdOut { message } |
                                    LogOutput::StdErr { message } |
                                    LogOutput::Console { message } |
                                    LogOutput::StdIn { message } => message,
                                };

                                let message = String::from_utf8_lossy(&message_bytes);
                                
                                for line in message.lines() {
                                    let line = line.trim();
                                    if !line.is_empty() {
                                        let msg = WsMessage::console_output(line).to_json();
                                        if tx.send(Message::Text(msg)).await.is_err() {
                                            debug!("Client disconnected while sending log");
                                            return;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                warn!("Docker logs error: {}", e);
                                log_stream = None;
                            }
                        }
                    }
                }

                // Stream stats if active (throttled to avoid flooding)
                stats_result = async {
                    match stats_stream.as_mut() {
                        Some(stream) if last_stats.elapsed() >= STATS_INTERVAL => {
                            stream.next().await
                        }
                        Some(_) => {
                            // Throttle stats - wait until interval passes
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            None
                        }
                        None => {
                            // No active stream, yield briefly
                            tokio::time::sleep(Duration::from_millis(500)).await;
                            None
                        }
                    }
                } => {
                    if let Some(result) = stats_result {
                        match result {
                            Ok(stats) => {
                                last_stats = Instant::now();
                                
                                // Extract memory stats (memory_stats is a direct struct, usage/limit are Option<u64>)
                                let memory_bytes = stats.memory_stats.usage.unwrap_or(0);
                                let memory_limit_bytes = stats.memory_stats.limit.unwrap_or(0);
                                
                                // Calculate CPU percentage (cpu_stats and precpu_stats are direct structs)
                                let cpu_absolute = {
                                    let cpu = &stats.cpu_stats;
                                    let precpu = &stats.precpu_stats;
                                    
                                    let cpu_delta = cpu.cpu_usage.total_usage
                                        .saturating_sub(precpu.cpu_usage.total_usage);
                                    
                                    let system_delta = cpu.system_cpu_usage.unwrap_or(0)
                                        .saturating_sub(precpu.system_cpu_usage.unwrap_or(0));
                                    
                                    let num_cpus = cpu.online_cpus
                                        .or_else(|| cpu.cpu_usage.percpu_usage.as_ref().map(|p| p.len() as u64))
                                        .unwrap_or(1);
                                    
                                    Self::calculate_cpu_percent(cpu_delta, system_delta, num_cpus)
                                };
                                
                                // Calculate network stats (aggregate all interfaces)
                                let (rx_bytes, tx_bytes) = stats.networks
                                    .as_ref()
                                    .map(|networks| {
                                        networks.values().fold((0u64, 0u64), |(rx, tx), net| {
                                            (rx + net.rx_bytes, tx + net.tx_bytes)
                                        })
                                    })
                                    .unwrap_or((0, 0));
                                
                                // Calculate uptime
                                let uptime = container_start_time
                                    .map(|start| {
                                        let now = SystemTime::now()
                                            .duration_since(SystemTime::UNIX_EPOCH)
                                            .map(|d| d.as_secs() as i64)
                                            .unwrap_or(0);
                                        (now - start).max(0) as u64
                                    })
                                    .unwrap_or(0);
                                
                                // Get disk usage from blkio stats (blkio_stats is a direct struct)
                                let disk_bytes = stats.blkio_stats.io_service_bytes_recursive
                                    .as_ref()
                                    .map(|entries| {
                                        entries.iter()
                                            .filter(|e| e.op == "write" || e.op == "Write")
                                            .map(|e| e.value)
                                            .sum()
                                    })
                                    .unwrap_or(0);
                                
                                let container_stats = ContainerStats {
                                    memory_bytes,
                                    memory_limit_bytes,
                                    cpu_absolute,
                                    network: NetworkStats { rx_bytes, tx_bytes },
                                    uptime,
                                    state: "running".to_string(),
                                    disk_bytes,
                                };
                                
                                let msg = WsMessage::stats(
                                    container_stats.memory_bytes,
                                    container_stats.memory_limit_bytes,
                                    container_stats.cpu_absolute,
                                    container_stats.network.rx_bytes,
                                    container_stats.network.tx_bytes,
                                    container_stats.uptime,
                                    &container_stats.state,
                                    container_stats.disk_bytes,
                                ).to_json();
                                if tx.send(Message::Text(msg)).await.is_err() {
                                    debug!("Client disconnected while sending stats");
                                    return;
                                }
                            }
                            Err(e) => {
                                warn!("Docker stats error: {}", e);
                                stats_stream = None;
                            }
                        }
                    }
                }

                // Handle client messages
                client_msg = rx.next() => {
                    match client_msg {
                        Some(Ok(Message::Close(_))) => {
                            info!("Client closed connection for: {}", container_uuid);
                            break;
                        }
                        Some(Ok(Message::Ping(data))) => {
                            let _ = tx.send(Message::Pong(data)).await;
                        }
                        Some(Ok(Message::Pong(_))) => {
                            // Connection alive
                        }
                        Some(Ok(Message::Text(text))) => {
                            debug!("Received client message: {}", text);
                            // TODO: Add command handling
                        }
                        Some(Err(e)) => {
                            debug!("Client error: {}", e);
                            break;
                        }
                        None => {
                            debug!("Client disconnected");
                            break;
                        }
                        _ => {}
                    }
                }

                // Periodic ping
                _ = tokio::time::sleep(Duration::from_secs(15)) => {
                    if last_ping.elapsed() >= PING_INTERVAL {
                        if tx.send(Message::Ping(vec![1, 2, 3])).await.is_err() {
                            debug!("Ping failed, client disconnected");
                            break;
                        }
                        last_ping = Instant::now();
                    }
                }
            }
        }

        info!("WebSocket channel closed for container: {}", container_uuid);
    }
}