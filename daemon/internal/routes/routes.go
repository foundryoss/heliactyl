package routes

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"

	"github.com/gorilla/mux"
	"github.com/sirupsen/logrus"

	"NightLightd/internal/config"
	"NightLightd/internal/container"
	"NightLightd/internal/docker"
	"NightLightd/internal/filesystem"
	"NightLightd/internal/jobs"
	"NightLightd/internal/snapshot"
	"NightLightd/internal/stats"
	"NightLightd/internal/volume"
)

// Handler handles HTTP routes
type Handler struct {
	config            *config.Config
	dockerClient      *docker.Client
	statsLogger       *stats.Logger
	volumeManager     *volume.Manager
	containerManager  *container.Manager
	filesystemManager *filesystem.Manager
	jobManager        *jobs.Manager
	snapshotManager   *snapshot.Manager
	log               *logrus.Logger
	startTime         time.Time
}

// NewHandler creates a new route handler
func NewHandler(cfg *config.Config, dockerClient *docker.Client, statsLogger *stats.Logger, volumeManager *volume.Manager, containerManager *container.Manager, filesystemManager *filesystem.Manager, jobManager *jobs.Manager, snapshotManager *snapshot.Manager, log *logrus.Logger) *Handler {
	return &Handler{
		config:            cfg,
		dockerClient:      dockerClient,
		statsLogger:       statsLogger,
		volumeManager:     volumeManager,
		containerManager:  containerManager,
		filesystemManager: filesystemManager,
		jobManager:        jobManager,
		snapshotManager:   snapshotManager,
		log:               log,
		startTime:         time.Now(),
	}
}

// SetupRoutes sets up HTTP routes
func (h *Handler) SetupRoutes(router *mux.Router) {
	// Main status endpoint
	router.HandleFunc("/", h.handleStatus).Methods("GET")

	// Stats endpoint
	router.HandleFunc("/stats", h.handleStats).Methods("GET")

	// Instance management endpoints
	instances := router.PathPrefix("/instances").Subrouter()
	instances.HandleFunc("", h.handleInstances).Methods("GET")
	instances.HandleFunc("/create", h.handleCreateInstance).Methods("POST")
	instances.HandleFunc("/{id}", h.handleInstanceDetails).Methods("GET")
	instances.HandleFunc("/{id}", h.handleDeleteInstance).Methods("DELETE")
	instances.HandleFunc("/{id}/power", h.handleInstancePower).Methods("POST")
	instances.HandleFunc("/{id}/stats", h.handleInstanceStats).Methods("GET")
	instances.HandleFunc("/{id}/ports", h.handleInstancePorts).Methods("GET")
	instances.HandleFunc("/redeploy/{id}/{volumeId}", h.handleRedeployInstance).Methods("POST")
	instances.HandleFunc("/reinstall/{id}/{volumeId}", h.handleReinstallInstance).Methods("POST")
	instances.HandleFunc("/edit/{id}", h.handleEditInstance).Methods("PUT")

	// Job management endpoints
	jobs := router.PathPrefix("/jobs").Subrouter()
	jobs.HandleFunc("", h.handleListJobs).Methods("GET")
	jobs.HandleFunc("/{id}", h.handleGetJob).Methods("GET")
	jobs.HandleFunc("/{id}/logs", h.handleJobLogs).Methods("GET")

	// Container state endpoint
	router.HandleFunc("/state/{volumeId}", h.handleContainerState).Methods("GET")

	// File system endpoints - new pattern: /fs/{containerID}?volume={volumeID}
	fs := router.PathPrefix("/fs/{containerID}").Subrouter()
	fs.HandleFunc("/files", h.handleListFiles).Methods("GET")
	fs.HandleFunc("/files/create/{filename}", h.handleCreateFile).Methods("POST")
	fs.HandleFunc("/files/edit/{filename}", h.handleEditFile).Methods("POST")
	fs.HandleFunc("/files/delete/{filename}", h.handleDeleteFile).Methods("DELETE")
	fs.HandleFunc("/files/rename/{filename}", h.handleRenameFile).Methods("POST")
	fs.HandleFunc("/files/view/{filename}", h.handleViewFile).Methods("GET")
	fs.HandleFunc("/files/download/{filename}", h.handleDownloadFile).Methods("GET")
	fs.HandleFunc("/search", h.handleSearchFiles).Methods("GET")
	fs.HandleFunc("/upload", h.handleUploadFile).Methods("POST")

	// Container volumes endpoint
	instances.HandleFunc("/{id}/volumes", h.handleInstanceVolumes).Methods("GET")

	// Container management endpoints
	container := router.PathPrefix("/api/container/{containerID}").Subrouter()
	container.HandleFunc("", h.handleContainerDelete).Methods("DELETE")
	container.HandleFunc("/state", h.handleContainerStateByID).Methods("GET")
	container.HandleFunc("/lock", h.handleContainerLock).Methods("POST")
	container.HandleFunc("/snapshot", h.handleContainerSnapshot).Methods("GET", "POST")
	container.HandleFunc("/restore", h.handleContainerRestore).Methods("POST")
	container.HandleFunc("/freeze", h.handleContainerFreeze).Methods("POST")
	container.HandleFunc("/power", h.handleContainerPower).Methods("POST")
	container.HandleFunc("/env", h.handleContainerEnv).Methods("GET", "POST")

	// New comprehensive filesystem API
	api := router.PathPrefix("/api/fs/{containerID}").Subrouter()
	api.HandleFunc("", h.handleFSRoot).Methods("GET", "POST")
	api.HandleFunc("/file/rename", h.handleFSRename).Methods("POST")
	api.HandleFunc("/file/modify", h.handleFSModify).Methods("POST", "DELETE")
	api.HandleFunc("/file/chmod", h.handleFSChmod).Methods("GET", "POST")
	api.HandleFunc("/file/contents", h.handleFSContents).Methods("GET")
	api.HandleFunc("/file/download", h.handleFSDownload).Methods("GET")
	api.HandleFunc("/file/stream", h.handleFSStream).Methods("GET")
	api.HandleFunc("/file/search", h.handleFSSearch).Methods("GET")
	api.HandleFunc("/file/upload", h.handleFSUpload).Methods("POST")
	api.HandleFunc("/upload", h.handleFSUploadPath).Methods("POST")
}

// Response represents a standard API response
type Response struct {
	Success bool        `json:"success"`
	Data    interface{} `json:"data,omitempty"`
	Error   string      `json:"error,omitempty"`
}

// writeJSON writes a JSON response
func (h *Handler) writeJSON(w http.ResponseWriter, status int, data interface{}) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	json.NewEncoder(w).Encode(data)
}

// writeError writes an error response
func (h *Handler) writeError(w http.ResponseWriter, status int, message string) {
	h.log.WithFields(logrus.Fields{
		"status":  status,
		"message": message,
	}).Error("API Error")
	h.writeJSON(w, status, Response{Success: false, Error: message})
}

// writeSuccess writes a success response
func (h *Handler) writeSuccess(w http.ResponseWriter, data interface{}) {
	h.writeJSON(w, http.StatusOK, Response{Success: true, Data: data})
}

// handleStatus handles the main status endpoint
func (h *Handler) handleStatus(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()

	// Get Docker info
	dockerInfo, err := h.dockerClient.Info(ctx)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, "Docker is not running - NightLightd will not function properly.")
		return
	}

	// Check Docker ping
	isDockerRunning := "not running"
	if err := h.dockerClient.Ping(ctx); err == nil {
		isDockerRunning = "running"
	}

	response := map[string]interface{}{
		"versionFamily":  1,
		"versionRelease": "NightLightd " + h.config.Version,
		"online":         true,
		"remote":         h.config.Remote,
		"mysql": map[string]string{
			"host":     h.config.MySQL.Host,
			"user":     h.config.MySQL.User,
			"password": h.config.MySQL.Password,
		},
		"docker": map[string]interface{}{
			"status":     isDockerRunning,
			"systemInfo": dockerInfo,
		},
	}

	h.writeSuccess(w, response)
}

// handleStats handles the stats endpoint
func (h *Handler) handleStats(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()

	// Get system stats
	totalStats, err := h.statsLogger.GetTotalStats()
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, "Failed to get system stats")
		return
	}

	// Get container count
	containers, err := h.dockerClient.ListContainers(ctx)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, "Failed to list containers")
		return
	}

	onlineContainersCount := 0
	for _, container := range containers {
		if container.State == "running" {
			onlineContainersCount++
		}
	}

	// Calculate uptime
	uptimeSeconds := time.Since(h.startTime).Seconds()
	uptime := h.formatUptime(uptimeSeconds)

	responseStats := map[string]interface{}{
		"totalStats":            totalStats,
		"onlineContainersCount": onlineContainersCount,
		"uptime":                uptime,
	}

	h.writeSuccess(w, responseStats)
}

// handleInstancePower handles power actions for instances
func (h *Handler) handleInstancePower(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["id"]

	// Check if container is locked or frozen
	locked, lockReason, err := h.containerManager.IsContainerLocked(containerID)
	if err != nil {
		h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
		return
	}
	if locked {
		h.writeError(w, http.StatusLocked, fmt.Sprintf("Container is locked: %s", lockReason))
		return
	}
	frozen, freezeMessage, err := h.containerManager.IsContainerFrozen(containerID)
	if err != nil {
		h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
		return
	}
	if frozen {
		h.writeError(w, http.StatusLocked, fmt.Sprintf("Container is frozen: %s", freezeMessage))
		return
	}

	var req struct {
		Action string `json:"action"`
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		h.writeError(w, http.StatusBadRequest, "Invalid JSON")
		return
	}

	ctx := r.Context()
	container := h.dockerClient.GetContainer(containerID)

	switch req.Action {
	case "start":
		err := container.Start(ctx)
		if err != nil {
			h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("Failed to start container: %s", err.Error()))
			return
		}
		h.writeSuccess(w, map[string]string{"action": "started", "container": containerID})

	case "stop":
		err := container.Stop(ctx)
		if err != nil {
			h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("Failed to stop container: %s", err.Error()))
			return
		}
		h.writeSuccess(w, map[string]string{"action": "stopped", "container": containerID})

	case "restart":
		err := container.Restart(ctx)
		if err != nil {
			h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("Failed to restart container: %s", err.Error()))
			return
		}
		h.writeSuccess(w, map[string]string{"action": "restarted", "container": containerID})

	case "kill":
		err := container.Kill(ctx)
		if err != nil {
			h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("Failed to kill container: %s", err.Error()))
			return
		}
		h.writeSuccess(w, map[string]string{"action": "killed", "container": containerID})

	default:
		h.writeError(w, http.StatusBadRequest, "Invalid action. Supported actions: start, stop, restart, kill")
	}
}

// handleInstanceStats handles stats for a specific instance
func (h *Handler) handleInstanceStats(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["id"]

	ctx := r.Context()
	container := h.dockerClient.GetContainer(containerID)

	statsReader, err := container.Stats(ctx, false)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("Failed to get container stats: %s", err.Error()))
		return
	}
	defer statsReader.Body.Close()

	// Read stats data
	statsData, err := io.ReadAll(statsReader.Body)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("Failed to read stats: %s", err.Error()))
		return
	}

	stats, err := docker.ParseStats(statsData)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("Failed to parse stats: %s", err.Error()))
		return
	}

	h.writeSuccess(w, stats)
}

// formatUptime formats uptime in a human-readable format
func (h *Handler) formatUptime(uptimeSeconds float64) string {
	uptime := int(uptimeSeconds)

	days := uptime / 86400
	hours := (uptime % 86400) / 3600
	minutes := (uptime % 3600) / 60

	var parts []string
	if days > 0 {
		parts = append(parts, fmt.Sprintf("%dd", days))
	}
	if hours > 0 {
		parts = append(parts, fmt.Sprintf("%dh", hours))
	}
	if minutes > 0 {
		parts = append(parts, fmt.Sprintf("%dm", minutes))
	}
	if len(parts) == 0 {
		return "0m"
	}

	return strings.Join(parts, " ")
}
