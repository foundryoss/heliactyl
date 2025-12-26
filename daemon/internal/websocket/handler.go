package websocket

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/docker/docker/api/types"
	"github.com/gorilla/websocket"
	"github.com/sirupsen/logrus"

	"NightLightd/internal/config"
	"NightLightd/internal/container"
	"NightLightd/internal/docker"
	"NightLightd/internal/volume"
)

// Handler manages WebSocket connections
type Handler struct {
	config           *config.Config
	dockerClient     *docker.Client
	volumeManager    *volume.Manager
	containerManager *container.Manager
	log              *logrus.Logger
	upgrader         websocket.Upgrader
	connections      map[string]*Connection
	mu               sync.RWMutex
}

// Connection represents a WebSocket connection
type Connection struct {
	conn          *websocket.Conn
	containerID   string
	volumeID      string
	authenticated bool
	mu            sync.Mutex
}

// Message represents a WebSocket message
type Message struct {
	Event   string   `json:"event"`
	Args    []string `json:"args,omitempty"`
	Command string   `json:"command,omitempty"`
}

// NewHandler creates a new WebSocket handler
func NewHandler(cfg *config.Config, dockerClient *docker.Client, volumeManager *volume.Manager, containerManager *container.Manager, log *logrus.Logger) *Handler {
	return &Handler{
		config:           cfg,
		dockerClient:     dockerClient,
		volumeManager:    volumeManager,
		containerManager: containerManager,
		log:              log,
		upgrader: websocket.Upgrader{
			CheckOrigin: func(r *http.Request) bool {
				return true // Allow all origins for now
			},
		},
		connections: make(map[string]*Connection),
	}
}

// HandleWebSocket handles WebSocket connections
func (h *Handler) HandleWebSocket(w http.ResponseWriter, r *http.Request) {
	conn, err := h.upgrader.Upgrade(w, r, nil)
	if err != nil {
		h.log.WithError(err).Error("Failed to upgrade WebSocket connection")
		return
	}
	defer conn.Close()

	// Parse URL to get container ID and volume ID
	urlParts := strings.Split(r.URL.Path, "/")
	if len(urlParts) < 3 {
		h.log.Error("Invalid WebSocket URL format")
		conn.WriteMessage(websocket.TextMessage, []byte("Invalid URL format"))
		return
	}

	containerID := urlParts[2]
	volumeID := containerID // Use container ID as volume ID by default
	if len(urlParts) > 3 {
		volumeID = urlParts[3]
	}

	connection := &Connection{
		conn:        conn,
		containerID: containerID,
		volumeID:    volumeID,
	}

	// Store connection
	connKey := fmt.Sprintf("%s-%s", containerID, volumeID)
	h.mu.Lock()
	h.connections[connKey] = connection
	h.mu.Unlock()

	// Clean up on disconnect
	defer func() {
		h.mu.Lock()
		delete(h.connections, connKey)
		h.mu.Unlock()
	}()

	// Handle messages
	for {
		var msg Message
		err := conn.ReadJSON(&msg)
		if err != nil {
			if websocket.IsUnexpectedCloseError(err, websocket.CloseGoingAway, websocket.CloseAbnormalClosure) {
				h.log.WithError(err).Error("WebSocket error")
			}
			break
		}

		h.log.WithFields(logrus.Fields{
			"event":       msg.Event,
			"containerID": containerID,
		}).Debug("Received WebSocket message")

		if err := h.handleMessage(connection, &msg, r); err != nil {
			h.log.WithError(err).Error("Failed to handle WebSocket message")
			conn.WriteMessage(websocket.TextMessage, []byte(fmt.Sprintf("Error: %s", err.Error())))
		}
	}
}

// handleMessage handles individual WebSocket messages
func (h *Handler) handleMessage(conn *Connection, msg *Message, r *http.Request) error {
	switch msg.Event {
	case "auth":
		return h.handleAuth(conn, msg, r.URL.Path)
	case "cmd":
		if !conn.authenticated {
			return fmt.Errorf("unauthorized access")
		}
		return h.handleCommand(conn, msg.Command)
	case "power:start":
		if !conn.authenticated {
			return fmt.Errorf("unauthorized access")
		}
		return h.handlePowerAction(conn, "start")
	case "power:stop":
		if !conn.authenticated {
			return fmt.Errorf("unauthorized access")
		}
		return h.handlePowerAction(conn, "stop")
	case "power:restart":
		if !conn.authenticated {
			return fmt.Errorf("unauthorized access")
		}
		return h.handlePowerAction(conn, "restart")
	default:
		return fmt.Errorf("unsupported event: %s", msg.Event)
	}
}

// handleAuth handles authentication
func (h *Handler) handleAuth(conn *Connection, msg *Message, requestPath string) error {
	if len(msg.Args) == 0 {
		return fmt.Errorf("password required")
	}

	password := msg.Args[0]
	if password != h.config.Key {
		h.log.Warn("WebSocket authentication failure")
		conn.conn.WriteMessage(websocket.TextMessage, []byte("Authentication failed"))
		return fmt.Errorf("authentication failed")
	}

	// CRITICAL: Check if container is running AND managed by NightLightd before allowing connection
	ctx := context.Background()
	container := h.dockerClient.GetContainer(conn.containerID)

	containerInfo, err := container.Inspect(ctx)
	if err != nil {
		h.log.WithError(err).Warn("Container not found during WebSocket auth")
		conn.conn.WriteMessage(websocket.TextMessage, []byte("Container not found"))
		return fmt.Errorf("container not found: %s", conn.containerID)
	}

	// Check if this container is managed by NightLightd (has corresponding volume)
	volumeID := strings.TrimPrefix(containerInfo.Name, "/")
	if !h.isContainerManagedByNightLightd(volumeID) {
		h.log.WithFields(logrus.Fields{
			"containerID": conn.containerID,
			"volumeId":    volumeID,
		}).Warn("WebSocket connection denied - container not managed by NightLightd")

		conn.conn.WriteMessage(websocket.TextMessage, []byte(
			"\r\n\u001b[31m[NightLightd] \x1b[0mConnection denied: Container not managed by NightLightd\r\n"))
		return fmt.Errorf("container not managed by NightLightd: %s", volumeID)
	}

	// Only allow WebSocket connections to running containers (like Pterodactyl)
	if !containerInfo.State.Running {
		h.log.WithFields(logrus.Fields{
			"containerID": conn.containerID,
			"state":       containerInfo.State.Status,
		}).Warn("WebSocket connection denied - container not running")

		conn.conn.WriteMessage(websocket.TextMessage, []byte(fmt.Sprintf(
			"\r\n\u001b[31m[NightLightd] \x1b[0mConnection denied: Container is %s (not running)\r\n",
			containerInfo.State.Status)))
		return fmt.Errorf("container not running: %s", containerInfo.State.Status)
	}

	conn.authenticated = true
	h.log.WithFields(logrus.Fields{
		"containerID": conn.containerID,
		"state":       containerInfo.State.Status,
	}).Info("WebSocket authentication successful - container is running")

	// Send welcome message
	welcomeMsg := "\r\n\u001b[33m[NightLightd] \x1b[0mconnected!\r\n"
	conn.conn.WriteMessage(websocket.TextMessage, []byte(welcomeMsg))

	// Start appropriate session based on URL
	if strings.HasPrefix(requestPath, "/exec/") {
		return h.setupExecSession(conn)
	} else if strings.HasPrefix(requestPath, "/stats/") {
		return h.setupStatsSession(conn)
	}

	return nil
}

// handleCommand handles command execution
func (h *Handler) handleCommand(conn *Connection, command string) error {
	ctx := context.Background()

	// Create exec instance
	execConfig := types.ExecConfig{
		AttachStdout: true,
		AttachStderr: true,
		AttachStdin:  true,
		Cmd:          []string{"/bin/sh", "-c", command},
		Tty:          true,
	}

	execResp, err := h.dockerClient.ContainerExecCreate(ctx, conn.containerID, execConfig)
	if err != nil {
		return fmt.Errorf("failed to create exec: %w", err)
	}

	// Start exec
	execStartConfig := types.ExecStartCheck{
		Tty: true,
	}

	hijackedResp, err := h.dockerClient.ContainerExecAttach(ctx, execResp.ID, execStartConfig)
	if err != nil {
		return fmt.Errorf("failed to attach to exec: %w", err)
	}
	defer hijackedResp.Close()

	// Send command output back to WebSocket
	go func() {
		buf := make([]byte, 1024)
		for {
			n, err := hijackedResp.Reader.Read(buf)
			if err != nil {
				if err != io.EOF {
					h.log.WithError(err).Debug("Error reading exec output")
				}
				break
			}

			if n > 0 {
				output := string(buf[:n])
				// Format output with docker prefix to indicate command execution
				formattedOutput := h.formatLogMessage(output)

				conn.mu.Lock()
				if conn.conn != nil {
					conn.conn.WriteMessage(websocket.TextMessage, []byte(formattedOutput))
				}
				conn.mu.Unlock()
			}
		}
	}()

	return nil
}

// handlePowerAction handles power actions (start, stop, restart)
func (h *Handler) handlePowerAction(conn *Connection, action string) error {
	ctx := context.Background()
	container := h.dockerClient.GetContainer(conn.containerID)

	// Check storage limits before start/restart
	if action == "start" || action == "restart" {
		if err := h.checkStorageLimit(conn.containerID, conn.volumeID); err != nil {
			msg := fmt.Sprintf("\r\n\u001b[31m[NightLightd] \x1b[0mCannot %s: %s\r\n", action, err.Error())
			conn.conn.WriteMessage(websocket.TextMessage, []byte(msg))
			return nil
		}
	}

	// Send working message
	workingMsg := fmt.Sprintf("\r\n\u001b[33m[NightLightd] \x1b[0mWorking on %s...\r\n", action)
	conn.conn.WriteMessage(websocket.TextMessage, []byte(workingMsg))

	// Perform action
	var err error
	switch action {
	case "start":
		err = container.Start(ctx)
	case "stop":
		err = container.Kill(ctx)
	case "restart":
		err = container.Restart(ctx)
	default:
		return fmt.Errorf("invalid action: %s", action)
	}

	// Send result message
	if err != nil {
		errorMsg := fmt.Sprintf("\r\n\u001b[31m[NightLightd] \x1b[0mAction failed: %s\r\n", err.Error())
		conn.conn.WriteMessage(websocket.TextMessage, []byte(errorMsg))
		return err
	}

	// Update container state after successful power action
	go func() {
		// Wait a moment for Docker to update the container state
		time.Sleep(1 * time.Second)

		// Sync container states with Docker to ensure our state is current
		if err := h.containerManager.SyncWithDocker(ctx); err != nil {
			h.log.WithError(err).Warn("Failed to sync container states after power action")
		}
	}()

	successMsg := fmt.Sprintf("\r\n\u001b[32m[NightLightd] \x1b[0m%s action completed.\r\n",
		strings.ToUpper(action[:1])+action[1:])
	conn.conn.WriteMessage(websocket.TextMessage, []byte(successMsg))

	// Start streaming logs after successful action
	go h.streamDockerLogs(conn)

	return nil
}

// setupExecSession sets up an exec session
func (h *Handler) setupExecSession(conn *Connection) error {
	go h.streamDockerLogs(conn)
	return nil
}

// setupStatsSession sets up a stats streaming session
func (h *Handler) setupStatsSession(conn *Connection) error {
	go h.streamContainerStats(conn)
	return nil
}

// streamDockerLogs streams Docker logs to WebSocket
func (h *Handler) streamDockerLogs(conn *Connection) {
	ctx := context.Background()
	container := h.dockerClient.GetContainer(conn.containerID)

	logStream, err := container.Logs(ctx, true)
	if err != nil {
		h.log.WithError(err).Error("Failed to get container logs")
		return
	}
	defer logStream.Close()

	// Stream logs
	buf := make([]byte, 1024)
	for {
		n, err := logStream.Read(buf)
		if err != nil {
			if err != io.EOF {
				h.log.WithError(err).Error("Error reading container logs")
			}
			break
		}

		if n > 0 {
			logMessage := h.formatLogMessage(string(buf[:n]))
			conn.mu.Lock()
			if conn.conn != nil {
				conn.conn.WriteMessage(websocket.TextMessage, []byte(logMessage))
			}
			conn.mu.Unlock()
		}
	}
}

// streamContainerStats streams container statistics
func (h *Handler) streamContainerStats(conn *Connection) {
	ctx := context.Background()
	container := h.dockerClient.GetContainer(conn.containerID)

	ticker := time.NewTicker(2 * time.Second)
	defer ticker.Stop()

	for {
		select {
		case <-ticker.C:
			stats, err := h.getContainerStats(container, conn.volumeID)
			if err != nil {
				h.log.WithError(err).Error("Failed to get container stats")
				continue
			}

			statsJSON, err := json.Marshal(stats)
			if err != nil {
				h.log.WithError(err).Error("Failed to marshal stats")
				continue
			}

			conn.mu.Lock()
			if conn.conn != nil {
				conn.conn.WriteMessage(websocket.TextMessage, statsJSON)
			}
			conn.mu.Unlock()

		case <-ctx.Done():
			return
		}
	}
}

// getContainerStats gets container statistics with volume information
func (h *Handler) getContainerStats(container *docker.Container, volumeID string) (*docker.ContainerStats, error) {
	ctx := context.Background()

	statsReader, err := container.Stats(ctx, false)
	if err != nil {
		return nil, err
	}
	defer statsReader.Body.Close()

	statsData, err := io.ReadAll(statsReader.Body)
	if err != nil {
		return nil, err
	}

	stats, err := docker.ParseStats(statsData)
	if err != nil {
		return nil, err
	}

	// Add volume size information
	volumeSize, err := h.volumeManager.GetVolumeSize(volumeID)
	if err != nil {
		h.log.WithError(err).Warn("Failed to get volume size")
		stats.VolumeSize = "0"
	} else {
		stats.VolumeSize = fmt.Sprintf("%.2f", volumeSize)
	}

	// Check storage limits
	diskLimit, err := h.volumeManager.GetDiskLimit(volumeID)
	if err == nil && diskLimit > 0 {
		stats.DiskLimit = diskLimit
		volumeSizeMiB, _ := strconv.ParseFloat(stats.VolumeSize, 64)
		stats.StorageExceeded = volumeSizeMiB >= float64(diskLimit)
	}

	return stats, nil
}

// checkStorageLimit checks if storage limit is exceeded
func (h *Handler) checkStorageLimit(containerID, volumeID string) error {
	diskLimit, err := h.volumeManager.GetDiskLimit(volumeID)
	if err != nil || diskLimit <= 0 {
		return nil // No limit set
	}

	volumeSize, err := h.volumeManager.GetVolumeSize(volumeID)
	if err != nil {
		return nil // Can't check, allow operation
	}

	if volumeSize >= float64(diskLimit) {
		return fmt.Errorf("storage limit exceeded (%.2f MiB / %d MiB). Delete files or increase limit", volumeSize, diskLimit)
	}

	return nil
}

// formatLogMessage formats log messages for display
func (h *Handler) formatLogMessage(content string) string {
	lines := strings.Split(content, "\n")
	var formatted []string

	for _, line := range lines {
		if len(line) > 0 {
			formatted = append(formatted, fmt.Sprintf("\r\n\u001b[34m[docker] \x1b[0m%s\r\n", line))
		}
	}

	return strings.Join(formatted, "")
}

// isContainerManagedByNightLightd checks if a container is managed by NightLightd
// by verifying if it has a corresponding volume directory
func (h *Handler) isContainerManagedByNightLightd(volumeID string) bool {
	// Check if volume directory exists
	volumePath := filepath.Join(h.config.VolumesPath, volumeID)
	if !filepath.IsAbs(volumePath) {
		if cwd, err := os.Getwd(); err == nil {
			volumePath = filepath.Join(cwd, volumePath)
		}
	}

	// Check if volume directory exists
	if _, err := os.Stat(volumePath); os.IsNotExist(err) {
		return false
	}

	// Also check if container manager has state for this volume
	state := h.containerManager.GetState(volumeID)
	return state.State != container.StateUnknown
}
