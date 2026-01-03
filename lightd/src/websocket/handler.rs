//! WebSocket handler using broadcast channels
//!
//! Supports multiple connections per container.
//! Never blocks - all updates come via event hub.
//! Uses lock-free state manager for status queries.

use axum::extract::ws::{Message, WebSocket};
use bollard::container::{LogOutput, LogsOptions, StatsOptions};
use bollard::exec::{CreateExecOptions, StartExecResults};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::broadcast;
use tracing::{debug, info, warn, error};

use crate::types::AppState;
use crate::services::{ContainerEvent, ContainerEventHub, EventContainerStats};
use super::{WebSocketToken, WsMessage};

#[derive(Debug, Deserialize)]
struct ClientMessage {
    event: String,
    args: Vec<String>,
}

#[inline]
fn docker_since(secs_ago: u64) -> i64 {
    SystemTime::now()
        .checked_sub(Duration::from_secs(secs_ago))
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub struct WebSocketHandler {
    token: WebSocketToken,
    socket: WebSocket,
    state: Arc<AppState>,
}

impl WebSocketHandler {
    pub fn new(token: WebSocketToken, socket: WebSocket, state: Arc<AppState>) -> Self {
        Self { token, socket, state }
    }

    pub async fn handle(self) {
        let container_id = self.token.container_id.clone();
        let container_uuid = self.token.container_uuid.clone();
        let docker = Arc::new(self.state.docker.client.clone());
        let event_hub = self.state.event_hub.clone();

        info!("WebSocket opened for container: {} ({})", container_uuid, container_id);

        let (mut ws_tx, mut ws_rx) = self.socket.split();

        // Subscribe to container events
        let mut event_rx = event_hub.subscribe(&container_id).await;

        // Get current status directly from Docker (no lock needed)
        let current_status = Self::get_container_status(&docker, &container_id).await;
        let is_running = current_status == "running";

        // Send init message
        let init_msg = WsMessage::init(&container_id, &container_uuid, &current_status);
        if ws_tx.send(Message::Text(init_msg.to_json())).await.is_err() {
            warn!("Failed to send init message");
            event_hub.unsubscribe(&container_id).await;
            return;
        }

        // Start streaming if running
        let log_task = if is_running {
            Some(Self::spawn_log_streamer(docker.clone(), container_id.clone(), event_hub.clone()))
        } else {
            None
        };

        let stats_task = if is_running {
            Some(Self::spawn_stats_streamer(docker.clone(), container_id.clone(), event_hub.clone()))
        } else {
            None
        };

        let mut container_running = is_running;
        let mut current_log_task: Option<tokio::task::JoinHandle<()>> = log_task;
        let mut current_stats_task: Option<tokio::task::JoinHandle<()>> = stats_task;

        loop {
            tokio::select! {
                event = event_rx.recv() => {
                    match event {
                        Ok(ContainerEvent::StateChanged { state }) => {
                            let was_running = container_running;
                            container_running = state == "running";

                            let msg = WsMessage::status(&state).to_json();
                            if ws_tx.send(Message::Text(msg)).await.is_err() {
                                break;
                            }

                            if container_running && !was_running {
                                current_log_task = Some(Self::spawn_log_streamer(
                                    docker.clone(), container_id.clone(), event_hub.clone(),
                                ));
                                current_stats_task = Some(Self::spawn_stats_streamer(
                                    docker.clone(), container_id.clone(), event_hub.clone(),
                                ));
                            } else if !container_running && was_running {
                                if let Some(task) = current_log_task.take() { task.abort(); }
                                if let Some(task) = current_stats_task.take() { task.abort(); }
                            }
                        }
                        Ok(ContainerEvent::ConsoleOutput { line }) => {
                            let msg = WsMessage::console_output(&line).to_json();
                            if ws_tx.send(Message::Text(msg)).await.is_err() { break; }
                        }
                        Ok(ContainerEvent::Stats(stats)) => {
                            let msg = WsMessage::stats(
                                stats.memory_bytes, stats.memory_limit_bytes, stats.cpu_percent,
                                stats.network_rx_bytes, stats.network_tx_bytes, stats.uptime,
                                if container_running { "running" } else { "stopped" },
                                stats.disk_bytes,
                            ).to_json();
                            if ws_tx.send(Message::Text(msg)).await.is_err() { break; }
                        }
                        Ok(ContainerEvent::DaemonMessage { message }) => {
                            let msg = WsMessage::daemon_message(&message).to_json();
                            if ws_tx.send(Message::Text(msg)).await.is_err() { break; }
                        }
                        Ok(ContainerEvent::PowerActionStarted { action }) => {
                            let msg = WsMessage::daemon_message(&format!("Power action started: {}", action)).to_json();
                            if ws_tx.send(Message::Text(msg)).await.is_err() { break; }
                        }
                        Ok(ContainerEvent::PowerActionCompleted { action, success, message }) => {
                            let status = if success { "completed" } else { "failed" };
                            let msg = WsMessage::daemon_message(&format!("Power action {}: {} - {}", action, status, message)).to_json();
                            if ws_tx.send(Message::Text(msg)).await.is_err() { break; }
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            debug!("WebSocket lagged {} messages for {}", n, container_id);
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            warn!("Event channel closed for {}", container_id);
                            break;
                        }
                    }
                }

                msg = ws_rx.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
                                if client_msg.event == "send_command" && !client_msg.args.is_empty() {
                                    let cmd = &client_msg.args[0];
                                    debug!("Executing command in {}: {}", container_id, cmd);
                                    
                                    let docker_clone = docker.clone();
                                    let cid = container_id.clone();
                                    let hub = event_hub.clone();
                                    let command = cmd.clone();
                                    
                                    tokio::spawn(async move {
                                        Self::execute_command(&docker_clone, &cid, &command, &hub).await;
                                    });
                                }
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            if ws_tx.send(Message::Pong(data)).await.is_err() { break; }
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            debug!("Client disconnected");
                            break;
                        }
                        Some(Err(e)) => {
                            warn!("WebSocket error: {}", e);
                            break;
                        }
                        _ => {}
                    }
                }

                _ = tokio::time::sleep(Duration::from_secs(30)) => {
                    if ws_tx.send(Message::Ping(vec![])).await.is_err() { break; }
                }
            }
        }

        if let Some(task) = current_log_task { task.abort(); }
        if let Some(task) = current_stats_task { task.abort(); }
        event_hub.unsubscribe(&container_id).await;
        info!("WebSocket closed for container: {}", container_id);
    }

    /// Get container status directly from Docker (no state manager lock)
    async fn get_container_status(docker: &bollard::Docker, container_id: &str) -> String {
        match tokio::time::timeout(
            Duration::from_secs(2),
            docker.inspect_container(container_id, None)
        ).await {
            Ok(Ok(info)) => {
                if let Some(state) = info.state {
                    if state.running == Some(true) { "running".to_string() }
                    else if state.paused == Some(true) { "paused".to_string() }
                    else { "stopped".to_string() }
                } else {
                    "unknown".to_string()
                }
            }
            Ok(Err(_)) => "offline".to_string(),
            Err(_) => "timeout".to_string(),
        }
    }

    fn spawn_log_streamer(
        docker: Arc<bollard::Docker>,
        container_id: String,
        event_hub: Arc<ContainerEventHub>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let opts = LogsOptions::<String> {
                follow: true,
                stdout: true,
                stderr: true,
                since: docker_since(0),
                timestamps: false,
                ..Default::default()
            };

            let mut stream = docker.logs(&container_id, Some(opts));

            while let Some(result) = stream.next().await {
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
                                event_hub.broadcast_console(&container_id, line).await;
                            }
                        }
                    }
                    Err(e) => {
                        debug!("Log stream error for {}: {}", container_id, e);
                        break;
                    }
                }
            }
            debug!("Log streamer ended for {}", container_id);
        })
    }

    fn spawn_stats_streamer(
        docker: Arc<bollard::Docker>,
        container_id: String,
        event_hub: Arc<ContainerEventHub>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let opts = StatsOptions { stream: true, one_shot: false };
            let mut stream = docker.stats(&container_id, Some(opts));
            let start_time = std::time::Instant::now();

            while let Some(result) = stream.next().await {
                match result {
                    Ok(stats) => {
                        let cpu_stats = &stats.cpu_stats;
                        let precpu_stats = &stats.precpu_stats;
                        
                        let cpu_delta = cpu_stats.cpu_usage.total_usage
                            .saturating_sub(precpu_stats.cpu_usage.total_usage);
                        let system_delta = cpu_stats.system_cpu_usage.unwrap_or(0)
                            .saturating_sub(precpu_stats.system_cpu_usage.unwrap_or(0));
                        let num_cpus = cpu_stats.online_cpus.unwrap_or(1) as f64;

                        let cpu_percent = if system_delta > 0 && num_cpus > 0.0 {
                            ((cpu_delta as f64 / system_delta as f64) * num_cpus * 100.0 * 100.0).round() / 100.0
                        } else {
                            0.0
                        };

                        let memory_usage = stats.memory_stats.usage.unwrap_or(0);
                        let memory_limit = stats.memory_stats.limit.unwrap_or(0);

                        let (rx_bytes, tx_bytes) = stats.networks.as_ref()
                            .map(|networks| {
                                networks.values().fold((0u64, 0u64), |(rx, tx), net| {
                                    (rx + net.rx_bytes, tx + net.tx_bytes)
                                })
                            })
                            .unwrap_or((0, 0));

                        let uptime = start_time.elapsed().as_secs();

                        event_hub.broadcast_stats(&container_id, EventContainerStats {
                            memory_bytes: memory_usage,
                            memory_limit_bytes: memory_limit,
                            cpu_percent,
                            network_rx_bytes: rx_bytes,
                            network_tx_bytes: tx_bytes,
                            uptime,
                            disk_bytes: 0,
                        }).await;
                    }
                    Err(e) => {
                        debug!("Stats stream error for {}: {}", container_id, e);
                        break;
                    }
                }
            }
            debug!("Stats streamer ended for {}", container_id);
        })
    }

    async fn execute_command(
        docker: &bollard::Docker,
        container_id: &str,
        command: &str,
        event_hub: &ContainerEventHub,
    ) {
        let exec_config = CreateExecOptions {
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            cmd: Some(vec!["sh", "-c", command]),
            ..Default::default()
        };

        match docker.create_exec(container_id, exec_config).await {
            Ok(exec) => {
                match docker.start_exec(&exec.id, None).await {
                    Ok(StartExecResults::Attached { mut output, .. }) => {
                        while let Some(result) = output.next().await {
                            match result {
                                Ok(log_output) => {
                                    let message_bytes = match log_output {
                                        LogOutput::StdOut { message } |
                                        LogOutput::StdErr { message } |
                                        LogOutput::Console { message } |
                                        LogOutput::StdIn { message } => message,
                                    };

                                    let text = String::from_utf8_lossy(&message_bytes);
                                    for line in text.lines() {
                                        let line = line.trim_end();
                                        if !line.is_empty() {
                                            event_hub.broadcast_console(container_id, line).await;
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("Exec output error for {}: {}", container_id, e);
                                    break;
                                }
                            }
                        }
                    }
                    Ok(StartExecResults::Detached) => {
                        debug!("Exec started in detached mode for {}", container_id);
                    }
                    Err(e) => {
                        error!("Failed to start exec for {}: {}", container_id, e);
                        event_hub.broadcast_console(container_id, &format!("Error: {}", e)).await;
                    }
                }
            }
            Err(e) => {
                error!("Failed to create exec for {}: {}", container_id, e);
                event_hub.broadcast_console(container_id, &format!("Error: {}", e)).await;
            }
        }
    }
}
