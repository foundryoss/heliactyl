use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt, stream::SplitSink};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info, warn};

use crate::{
    models::{WebSocketMessage, ContainerStats},
    types::AppState,
    docker::ContainerManager,
    websocket::WebSocketToken,
};

type WebSocketSender = SplitSink<WebSocket, Message>;

pub struct WebSocketConnection {
    pub token: WebSocketToken,
    pub socket: WebSocket,
    pub state: Arc<AppState>,
    pub stats_rx: broadcast::Receiver<ContainerStats>,
    pub logs_rx: broadcast::Receiver<String>,
    pub status_rx: broadcast::Receiver<String>,
}

impl WebSocketConnection {
    pub fn new(
        token: WebSocketToken,
        socket: WebSocket,
        state: Arc<AppState>,
        stats_rx: broadcast::Receiver<ContainerStats>,
        logs_rx: broadcast::Receiver<String>,
        status_rx: broadcast::Receiver<String>,
    ) -> Self {
        Self {
            token,
            socket,
            state,
            stats_rx,
            logs_rx,
            status_rx,
        }
    }

    pub async fn handle(self) {
        info!("WebSocket connection established for container: {}", self.token.container_id);
        
        let (sender, mut receiver) = self.socket.split();
        let sender = Arc::new(Mutex::new(sender));
        
        // Extract values from self before moving
        let token = self.token;
        let state = self.state;
        let mut stats_rx = self.stats_rx;
        let mut logs_rx = self.logs_rx;
        let mut status_rx = self.status_rx;
        
        // Send initial status
        {
            let mut sender_guard = sender.lock().await;
            if let Err(e) = Self::send_initial_status_static(&token, &state, &mut *sender_guard).await {
                error!("Failed to send initial status: {}", e);
                return;
            }
        }

        // Spawn tasks for different event streams
        let sender_clone1 = sender.clone();
        let sender_clone2 = sender.clone();
        let sender_clone3 = sender.clone();
        
        // Stats broadcasting task
        let stats_task = tokio::spawn(async move {
            while let Ok(stats) = stats_rx.recv().await {
                let message = WebSocketMessage {
                    event: "stats".to_string(),
                    args: vec![serde_json::to_string(&stats).unwrap_or_default()],
                };
                
                if let Ok(json) = serde_json::to_string(&message) {
                    if let Err(e) = sender_clone1.lock().await.send(Message::Text(json)).await {
                        error!("Failed to send stats: {}", e);
                        break;
                    }
                }
            }
        });

        // Logs broadcasting task
        let logs_task = tokio::spawn(async move {
            while let Ok(log_line) = logs_rx.recv().await {
                let message = WebSocketMessage {
                    event: "console output".to_string(),
                    args: vec![log_line],
                };
                
                if let Ok(json) = serde_json::to_string(&message) {
                    if let Err(e) = sender_clone2.lock().await.send(Message::Text(json)).await {
                        error!("Failed to send log: {}", e);
                        break;
                    }
                }
            }
        });

        // Status broadcasting task
        let status_task = tokio::spawn(async move {
            while let Ok(status) = status_rx.recv().await {
                let message = WebSocketMessage {
                    event: "status".to_string(),
                    args: vec![status],
                };
                
                if let Ok(json) = serde_json::to_string(&message) {
                    if let Err(e) = sender_clone3.lock().await.send(Message::Text(json)).await {
                        error!("Failed to send status: {}", e);
                        break;
                    }
                }
            }
        });

        // Handle incoming messages
        let container_id = token.container_id.clone();
        let state_clone = state.clone();
        
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    let mut sender_guard = sender.lock().await;
                    if let Err(e) = Self::handle_incoming_message_static(&text, &container_id, &state_clone, &mut *sender_guard).await {
                        error!("Error handling message: {}", e);
                    }
                }
                Ok(Message::Close(_)) => {
                    info!("WebSocket connection closed for container: {}", container_id);
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        // Clean up tasks
        stats_task.abort();
        logs_task.abort();
        status_task.abort();
        
        info!("WebSocket connection ended for container: {}", container_id);
    }

    async fn send_initial_status(&self, sender: &mut WebSocketSender) -> anyhow::Result<()> {
        Self::send_initial_status_static(&self.token, &self.state, sender).await
    }

    async fn send_initial_status_static(token: &WebSocketToken, state: &Arc<AppState>, sender: &mut WebSocketSender) -> anyhow::Result<()> {
        // Get current container status
        let state_manager = state.state_manager.lock().await;
        let status = if let Some((_, container_state)) = state_manager.find_by_container_id(&token.container_id) {
            container_state.state.clone()
        } else {
            "unknown".to_string()
        };
        drop(state_manager);

        let message = WebSocketMessage {
            event: "status".to_string(),
            args: vec![status],
        };

        let json = serde_json::to_string(&message)?;
        sender.send(Message::Text(json)).await?;
        
        Ok(())
    }

    async fn handle_incoming_message(
        &self,
        text: &str,
        container_id: &str,
        state: &AppState,
        sender: &mut WebSocketSender,
    ) -> anyhow::Result<()> {
        Self::handle_incoming_message_static(text, container_id, state, sender).await
    }

    async fn handle_incoming_message_static(
        text: &str,
        container_id: &str,
        state: &AppState,
        sender: &mut WebSocketSender,
    ) -> anyhow::Result<()> {
        let message: WebSocketMessage = serde_json::from_str(text)?;
        
        match message.event.as_str() {
            "send command" => {
                if let Some(command) = message.args.first() {
                    Self::handle_command_static(container_id, command, state, sender).await?;
                }
            }
            "power" => {
                if let Some(action) = message.args.first() {
                    Self::handle_power_action_static(container_id, action, state, sender).await?;
                }
            }
            "request stats" => {
                Self::send_current_stats_static(container_id, state, sender).await?;
            }
            _ => {
                warn!("Unknown WebSocket event: {}", message.event);
            }
        }
        
        Ok(())
    }

    async fn handle_command(
        &self,
        container_id: &str,
        command: &str,
        state: &AppState,
        sender: &mut WebSocketSender,
    ) -> anyhow::Result<()> {
        Self::handle_command_static(container_id, command, state, sender).await
    }

    async fn handle_command_static(
        container_id: &str,
        command: &str,
        state: &AppState,
        sender: &mut WebSocketSender,
    ) -> anyhow::Result<()> {
        info!("Executing command in container {}: {}", container_id, command);
        
        // Check if container is suspended
        let state_manager = state.state_manager.lock().await;
        if let Some((uuid, _)) = state_manager.find_by_container_id(container_id) {
            if state_manager.is_container_suspended(uuid) {
                let error_msg = WebSocketMessage {
                    event: "error".to_string(),
                    args: vec!["Cannot execute commands in suspended container".to_string()],
                };
                let json = serde_json::to_string(&error_msg)?;
                sender.send(Message::Text(json)).await?;
                return Ok(());
            }
        }
        drop(state_manager);

        let manager = ContainerManager::new(state.docker.client());
        
        // Use timeout version to prevent blocking - 10 second timeout for command output
        match manager.exec_command_with_timeout(container_id, vec!["sh", "-c", command], 10).await {
            Ok(output) => {
                let response = WebSocketMessage {
                    event: "command response".to_string(),
                    args: vec![output],
                };
                let json = serde_json::to_string(&response)?;
                sender.send(Message::Text(json)).await?;
            }
            Err(e) => {
                let error_msg = WebSocketMessage {
                    event: "error".to_string(),
                    args: vec![format!("Command execution failed: {}", e)],
                };
                let json = serde_json::to_string(&error_msg)?;
                sender.send(Message::Text(json)).await?;
            }
        }
        
        Ok(())
    }

    async fn handle_power_action(
        &self,
        container_id: &str,
        action: &str,
        state: &AppState,
        sender: &mut WebSocketSender,
    ) -> anyhow::Result<()> {
        Self::handle_power_action_static(container_id, action, state, sender).await
    }

    async fn handle_power_action_static(
        container_id: &str,
        action: &str,
        state: &AppState,
        sender: &mut WebSocketSender,
    ) -> anyhow::Result<()> {
        info!("Power action for container {}: {}", container_id, action);
        
        let manager = ContainerManager::new(state.docker.client());
        let result = match action {
            "start" => manager.start(container_id).await,
            "stop" => manager.stop(container_id).await,
            "restart" => {
                manager.stop(container_id).await?;
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                manager.start(container_id).await
            }
            "kill" => manager.kill(container_id).await,
            _ => {
                let error_msg = WebSocketMessage {
                    event: "error".to_string(),
                    args: vec![format!("Unknown power action: {}", action)],
                };
                let json = serde_json::to_string(&error_msg)?;
                sender.send(Message::Text(json)).await?;
                return Ok(());
            }
        };

        let response = match result {
            Ok(_) => WebSocketMessage {
                event: "power response".to_string(),
                args: vec![format!("Power action '{}' completed successfully", action)],
            },
            Err(e) => WebSocketMessage {
                event: "error".to_string(),
                args: vec![format!("Power action '{}' failed: {}", action, e)],
            },
        };

        let json = serde_json::to_string(&response)?;
        sender.send(Message::Text(json)).await?;
        
        Ok(())
    }

    async fn send_current_stats(
        &self,
        container_id: &str,
        state: &AppState,
        sender: &mut WebSocketSender,
    ) -> anyhow::Result<()> {
        Self::send_current_stats_static(container_id, state, sender).await
    }

    async fn send_current_stats_static(
        container_id: &str,
        state: &AppState,
        sender: &mut WebSocketSender,
    ) -> anyhow::Result<()> {
        let manager = ContainerManager::new(state.docker.client());
        
        match manager.get_stats(container_id).await {
            Ok(stats_json) => {
                // Parse Docker stats and convert to our format
                if let Ok(docker_stats) = serde_json::from_str::<Value>(&stats_json) {
                    let stats = Self::convert_docker_stats_to_container_stats_static(&docker_stats, container_id, state).await;
                    
                    let message = WebSocketMessage {
                        event: "stats".to_string(),
                        args: vec![serde_json::to_string(&stats)?],
                    };
                    
                    let json = serde_json::to_string(&message)?;
                    sender.send(Message::Text(json)).await?;
                }
            }
            Err(e) => {
                let error_msg = WebSocketMessage {
                    event: "error".to_string(),
                    args: vec![format!("Failed to get stats: {}", e)],
                };
                let json = serde_json::to_string(&error_msg)?;
                sender.send(Message::Text(json)).await?;
            }
        }
        
        Ok(())
    }

    async fn convert_docker_stats_to_container_stats(
        &self,
        docker_stats: &Value,
        container_id: &str,
        state: &AppState,
    ) -> ContainerStats {
        Self::convert_docker_stats_to_container_stats_static(docker_stats, container_id, state).await
    }

    async fn convert_docker_stats_to_container_stats_static(
        docker_stats: &Value,
        container_id: &str,
        state: &AppState,
    ) -> ContainerStats {
        let memory_usage = docker_stats["memory_stats"]["usage"].as_u64().unwrap_or(0);
        let memory_limit = docker_stats["memory_stats"]["limit"].as_u64().unwrap_or(0);
        
        let cpu_delta = docker_stats["cpu_stats"]["cpu_usage"]["total_usage"].as_u64().unwrap_or(0) as f64
            - docker_stats["precpu_stats"]["cpu_usage"]["total_usage"].as_u64().unwrap_or(0) as f64;
        let system_delta = docker_stats["cpu_stats"]["system_cpu_usage"].as_u64().unwrap_or(0) as f64
            - docker_stats["precpu_stats"]["system_cpu_usage"].as_u64().unwrap_or(0) as f64;
        let cpu_count = docker_stats["cpu_stats"]["online_cpus"].as_u64().unwrap_or(1) as f64;
        
        let cpu_percent = if system_delta > 0.0 && cpu_delta > 0.0 {
            (cpu_delta / system_delta) * cpu_count * 100.0
        } else {
            0.0
        };

        let rx_bytes = docker_stats["networks"]["eth0"]["rx_bytes"].as_u64().unwrap_or(0);
        let tx_bytes = docker_stats["networks"]["eth0"]["tx_bytes"].as_u64().unwrap_or(0);

        // Get REAL state from Docker, not from state_manager
        let container_state = match state.docker.client.inspect_container(container_id, None).await {
            Ok(info) => {
                if let Some(state_info) = info.state {
                    if state_info.paused.unwrap_or(false) {
                        "paused".to_string()
                    } else if state_info.running.unwrap_or(false) {
                        "running".to_string()
                    } else if state_info.restarting.unwrap_or(false) {
                        "restarting".to_string()
                    } else if state_info.dead.unwrap_or(false) {
                        "dead".to_string()
                    } else {
                        state_info.status.map(|s| s.to_string()).unwrap_or_else(|| "stopped".to_string())
                    }
                } else {
                    "unknown".to_string()
                }
            }
            Err(_) => "offline".to_string(),
        };

        // Check if suspended in our state manager
        let is_suspended = {
            let state_manager = state.state_manager.lock().await;
            state_manager.find_by_container_id(container_id)
                .map(|(uuid, _)| state_manager.is_container_suspended(uuid))
                .unwrap_or(false)
        };

        ContainerStats {
            memory_bytes: memory_usage,
            memory_limit_bytes: memory_limit,
            cpu_absolute: cpu_percent,
            network: crate::models::NetworkStats { rx_bytes, tx_bytes },
            uptime: 0, // Would need to calculate from container start time
            state: container_state,
            disk_bytes: 0, // Would need additional Docker API calls
            is_suspended,
        }
    }
}