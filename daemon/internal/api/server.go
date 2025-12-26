package api

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strconv"
	"strings"
	"time"

	"NightLightd/internal/auth"
	"NightLightd/internal/billing"
	"NightLightd/internal/config"
	"NightLightd/internal/container"
	"NightLightd/internal/jobs"
	"NightLightd/internal/monitoring"
	"NightLightd/internal/snapshot"

	"github.com/docker/docker/api/types"
	"github.com/gorilla/mux"
	"github.com/sirupsen/logrus"
)

// Server represents the HTTP API server
type Server struct {
	config         *config.DaemonConfig
	logger         *logrus.Logger
	authManager    auth.AuthManager
	authMiddleware *auth.Middleware
	router         *mux.Router
	server         *http.Server

	// Service dependencies (will be injected)
	containerManager *container.Manager
	billingTracker   *billing.Tracker
	monitor          *monitoring.Monitor

	// Optional managers (can be set later)
	jobManager      *jobs.Manager
	snapshotManager *snapshot.Manager
}

// NewServer creates a new API server instance
func NewServer(cfg *config.DaemonConfig, logger *logrus.Logger, authManager auth.AuthManager, billingTracker *billing.Tracker, monitor *monitoring.Monitor, containerManager *container.Manager) *Server {
	authMiddleware := auth.NewMiddleware(authManager)

	s := &Server{
		config:           cfg,
		logger:           logger,
		authManager:      authManager,
		authMiddleware:   authMiddleware,
		billingTracker:   billingTracker,
		monitor:          monitor,
		containerManager: containerManager,
		jobManager:       nil,
		snapshotManager:  nil,
	}

	s.setupRoutes()

	return s
}

// SetJobManager allows wiring in the jobs manager (used for snapshot/restore)
func (s *Server) SetJobManager(jm *jobs.Manager) {
	s.jobManager = jm
}

// SetSnapshotManager allows wiring in the snapshot manager
func (s *Server) SetSnapshotManager(sm *snapshot.Manager) {
	s.snapshotManager = sm
}

// Start starts the HTTP server
func (s *Server) Start(ctx context.Context) error {
	s.server = &http.Server{
		Addr:    fmt.Sprintf(":%d", s.config.APIPort),
		Handler: s.router,
	}

	s.logger.WithField("port", s.config.APIPort).Info("Starting API server")

	go func() {
		if err := s.server.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			s.logger.WithError(err).Error("API server failed")
		}
	}()

	return nil
}

// Stop stops the HTTP server gracefully
func (s *Server) Stop(ctx context.Context) error {
	if s.server == nil {
		return nil
	}

	s.logger.Info("Stopping API server")

	shutdownCtx, cancel := context.WithTimeout(ctx, 30*time.Second)
	defer cancel()

	return s.server.Shutdown(shutdownCtx)
}

// setupRoutes configures all API routes
func (s *Server) setupRoutes() {
	s.router = mux.NewRouter()

	// Add CORS middleware for all routes
	s.router.Use(s.corsMiddleware)

	// Add logging middleware
	s.router.Use(s.loggingMiddleware)

	// API prefix
	api := s.router.PathPrefix("/api").Subrouter()

	// Authentication routes (no auth required)
	s.setupAuthRoutes(api)

	// System status routes (no auth required for basic status)
	s.setupStatusRoutes(api)

	// Protected routes (require authentication)
	protected := api.PathPrefix("").Subrouter()
	protected.Use(s.authMiddleware.RequireAuth)

	// Container management routes
	s.setupContainerRoutes(protected)

	// Daemon management routes
	s.setupDaemonRoutes(protected)

	// Filesystem routes will be added in later tasks
	// Cloudflare routes will be added in later tasks
}

// setupAuthRoutes configures authentication routes
func (s *Server) setupAuthRoutes(router *mux.Router) {
	authRoutes := router.PathPrefix("/auth").Subrouter()

	// POST /api/auth?token= - Validate token and return token info
	authRoutes.HandleFunc("", s.handleAuth).Methods("POST")

	// Token management routes (require authentication)
	tokenRoutes := authRoutes.PathPrefix("/tokens").Subrouter()
	tokenRoutes.Use(s.authMiddleware.RequirePermission(auth.PermissionAdminWrite))

	tokenRoutes.HandleFunc("", s.handleCreateToken).Methods("POST")
	tokenRoutes.HandleFunc("", s.handleListTokens).Methods("GET")
	tokenRoutes.HandleFunc("/{tokenId}", s.handleRevokeToken).Methods("DELETE")

	// Security audit routes (require admin read permission)
	auditRoutes := authRoutes.PathPrefix("/audit").Subrouter()
	auditRoutes.Use(s.authMiddleware.RequirePermission(auth.PermissionAdminRead))

	auditRoutes.HandleFunc("/events", s.handleSecurityEvents).Methods("GET")
}

// setupStatusRoutes configures system status routes
func (s *Server) setupStatusRoutes(router *mux.Router) {
	// GET /api/status - Basic system status (no auth required)
	router.HandleFunc("/status", s.handleStatus).Methods("GET")
}

// setupContainerRoutes configures container management routes
func (s *Server) setupContainerRoutes(router *mux.Router) {
	// Container collection routes
	containers := router.PathPrefix("/containers").Subrouter()
	containers.HandleFunc("", s.handleListContainers).Methods("GET")
	containers.HandleFunc("", s.handleCreateContainer).Methods("POST")

	// Individual container routes
	container := router.PathPrefix("/container/{containerId}").Subrouter()

	// Basic container operations
	container.HandleFunc("", s.handleGetContainer).Methods("GET")
	container.HandleFunc("", s.handleDeleteContainer).Methods("DELETE")

	// Container power operations
	container.HandleFunc("/power", s.handleContainerPower).Methods("POST")

	// Container rebuild
	container.HandleFunc("/rebuild", s.handleContainerRebuild).Methods("POST")

	// Container statistics
	container.HandleFunc("/stats", s.handleContainerStats).Methods("GET")

	// Container locking
	container.HandleFunc("/lock", s.handleContainerLock).Methods("POST")

	// Container execution
	container.HandleFunc("/exec", s.handleContainerExec).Methods("POST")

	// Container snapshots
	container.HandleFunc("/snapshot", s.handleContainerSnapshots).Methods("GET", "POST")
	container.HandleFunc("/restore", s.handleContainerRestore).Methods("POST")

	// Container image management
	container.HandleFunc("/image", s.handleContainerImage).Methods("GET", "POST")

	// Container limits
	container.HandleFunc("/limits", s.handleContainerLimits).Methods("POST")

	// Container freezing
	container.HandleFunc("/freeze", s.handleContainerFreeze).Methods("POST")

	// Container environment variables
	container.HandleFunc("/env", s.handleContainerEnv).Methods("GET", "POST")

	// Container config (bulk patch) - atomically update image, limits, env, install_content, ru_limit, etc.
	container.HandleFunc("/config", s.handleContainerConfig).Methods("POST")

	// Container network ports
	container.HandleFunc("/network/ports", s.handleContainerPorts).Methods("GET", "POST")

	// Container logs
	container.HandleFunc("/logs", s.handleContainerLogs).Methods("GET")

	// Container WebSocket logs
	container.HandleFunc("/websocket", s.handleContainerWebSocket).Methods("GET")
}

// setupDaemonRoutes configures daemon management routes
// MOST OF THESE AREN'T EVEN IMPLEMENTED YET.
func (s *Server) setupDaemonRoutes(router *mux.Router) {
	daemon := router.PathPrefix("/daemon").Subrouter()

	// Daemon power operations
	daemon.HandleFunc("/power", s.handleDaemonPower).Methods("GET")

	// Daemon resource usage
	daemon.HandleFunc("/usage", s.handleDaemonUsage).Methods("GET")

	// Daemon tunnels
	daemon.HandleFunc("/tunnels", s.handleDaemonTunnels).Methods("GET")

	// Daemon settings
	daemon.HandleFunc("/settings", s.handleDaemonSettings).Methods("GET", "POST")
}

// Authentication handlers

func (s *Server) handleAuth(w http.ResponseWriter, r *http.Request) {
	token := r.URL.Query().Get("token")
	if token == "" {
		s.writeError(w, http.StatusBadRequest, "MISSING_TOKEN", "Token parameter is required", nil)
		return
	}

	tokenInfo, err := s.authManager.ValidateToken(r.Context(), token)
	if err != nil {
		s.writeError(w, http.StatusUnauthorized, "INVALID_TOKEN", err.Error(), nil)
		return
	}

	s.writeSuccess(w, map[string]interface{}{
		"token_info": tokenInfo,
		"message":    "Token is valid",
	})
}

func (s *Server) handleCreateToken(w http.ResponseWriter, r *http.Request) {
	var req auth.CreateTokenRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		s.writeError(w, http.StatusBadRequest, "INVALID_JSON", "Invalid JSON in request body", nil)
		return
	}

	token, err := s.authManager.CreateToken(r.Context(), &req)
	if err != nil {
		s.writeError(w, http.StatusBadRequest, "TOKEN_CREATION_FAILED", err.Error(), nil)
		return
	}

	s.writeSuccess(w, map[string]interface{}{
		"token": token,
	})
}

func (s *Server) handleListTokens(w http.ResponseWriter, r *http.Request) {
	tokens, err := s.authManager.ListTokens(r.Context())
	if err != nil {
		s.writeError(w, http.StatusInternalServerError, "LIST_TOKENS_FAILED", err.Error(), nil)
		return
	}

	s.writeSuccess(w, map[string]interface{}{
		"tokens": tokens,
	})
}

func (s *Server) handleRevokeToken(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	tokenID := vars["tokenId"]

	if err := s.authManager.RevokeToken(r.Context(), tokenID); err != nil {
		s.writeError(w, http.StatusBadRequest, "TOKEN_REVOCATION_FAILED", err.Error(), nil)
		return
	}

	s.writeSuccess(w, map[string]interface{}{
		"message": "Token revoked successfully",
	})
}

func (s *Server) handleSecurityEvents(w http.ResponseWriter, r *http.Request) {
	limitStr := r.URL.Query().Get("limit")
	limit := 100 // default

	if limitStr != "" {
		if parsed, err := strconv.Atoi(limitStr); err == nil && parsed > 0 && parsed <= 1000 {
			limit = parsed
		}
	}

	// Type cast to access GetSecurityEvents method
	if manager, ok := s.authManager.(*auth.Manager); ok {
		events, err := manager.GetSecurityEvents(r.Context(), limit)
		if err != nil {
			s.writeError(w, http.StatusInternalServerError, "EVENTS_FETCH_FAILED", err.Error(), nil)
			return
		}

		s.writeSuccess(w, map[string]interface{}{
			"events": events,
			"count":  len(events),
		})
	} else {
		s.writeError(w, http.StatusInternalServerError, "EVENTS_NOT_SUPPORTED", "Security events not supported by current auth manager", nil)
	}
}

// Status handlers

func (s *Server) handleStatus(w http.ResponseWriter, r *http.Request) {
	// Get system usage
	systemUsage, err := s.monitor.GetSystemUsage(r.Context())
	if err != nil {
		s.logger.WithError(err).Error("Failed to get system usage")
		systemUsage = nil // Continue with partial data
	}

	status := map[string]interface{}{
		"status":    "running",
		"timestamp": time.Now(),
		"version":   "1.0.0", // This could be injected from build info
	}

	if systemUsage != nil {
		status["system_usage"] = systemUsage
	}

	s.writeSuccess(w, status)
}

// Container handlers (placeholder implementations)

func (s *Server) handleListContainers(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()

	containers, err := s.containerManager.ListContainers(ctx)
	if err != nil {
		s.writeError(w, http.StatusInternalServerError, "LIST_CONTAINERS_FAILED", err.Error(), nil)
		return
	}

	s.writeSuccess(w, map[string]interface{}{
		"containers": containers,
		"count":      len(containers),
	})
}

func (s *Server) handleCreateContainer(w http.ResponseWriter, r *http.Request) {
	var req container.CreateRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		s.writeError(w, http.StatusBadRequest, "INVALID_JSON", "Invalid JSON in request body", nil)
		return
	}

	// Validate required fields
	if req.ID == "" {
		s.writeError(w, http.StatusBadRequest, "MISSING_ID", "Container ID is required", nil)
		return
	}
	if req.Image == "" {
		s.writeError(w, http.StatusBadRequest, "MISSING_IMAGE", "Container image is required", nil)
		return
	}

	ctx := r.Context()
	if err := s.containerManager.Deploy(ctx, &req); err != nil {
		s.writeError(w, http.StatusInternalServerError, "DEPLOYMENT_FAILED", err.Error(), nil)
		return
	}

	s.writeSuccess(w, map[string]interface{}{
		"message":     "Container created successfully",
		"containerId": req.ID,
		"volumeId":    req.ID,
	})
}

func (s *Server) handleGetContainer(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerId"]

	ctx := r.Context()
	containerInfo, err := s.containerManager.GetContainer(ctx, containerID)
	if err != nil {
		s.writeError(w, http.StatusNotFound, "CONTAINER_NOT_FOUND", err.Error(), nil)
		return
	}

	s.writeSuccess(w, containerInfo)
}

func (s *Server) handleDeleteContainer(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerId"]
	mode := r.URL.Query().Get("mode")

	if mode == "" {
		mode = "safe"
	}

	// Check lock/frozen state first
	if locked, reason, err := s.containerManager.IsContainerLocked(containerID); err == nil && locked {
		s.writeError(w, http.StatusLocked, "CONTAINER_LOCKED", fmt.Sprintf("Container is locked: %s", reason), nil)
		return
	} else if err != nil {
		// if we can't determine lock state, surface error
		s.writeError(w, http.StatusInternalServerError, "LOCK_CHECK_FAILED", err.Error(), nil)
		return
	}

	if frozen, msg, err := s.containerManager.IsContainerFrozen(containerID); err == nil && frozen {
		s.writeError(w, http.StatusLocked, "CONTAINER_FROZEN", fmt.Sprintf("Container is frozen: %s", msg), nil)
		return
	} else if err != nil {
		s.writeError(w, http.StatusInternalServerError, "FREEZE_CHECK_FAILED", err.Error(), nil)
		return
	}

	removeVolume := false
	if mode == "immediate" {
		removeVolume = true
	} else if mode != "safe" {
		s.writeError(w, http.StatusBadRequest, "INVALID_MODE", "mode must be 'safe' or 'immediate'", nil)
		return
	}

	ctx := r.Context()
	if err := s.containerManager.RemoveContainer(ctx, containerID, removeVolume); err != nil {
		s.writeError(w, http.StatusInternalServerError, "DELETE_FAILED", err.Error(), nil)
		return
	}

	s.writeSuccess(w, map[string]interface{}{"message": "Container deleted", "containerId": containerID, "mode": mode})
}

func (s *Server) handleContainerPower(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerId"]
	action := r.URL.Query().Get("action")

	validActions := map[string]bool{
		"start":   true,
		"stop":    true,
		"restart": true,
		"kill":    true,
	}

	if !validActions[action] {
		s.writeError(w, http.StatusBadRequest, "INVALID_ACTION", "Action must be one of: start, stop, restart, kill", nil)
		return
	}

	// Check lock/freeze state
	if locked, reason, err := s.containerManager.IsContainerLocked(containerID); err == nil && locked {
		s.writeError(w, http.StatusLocked, "CONTAINER_LOCKED", fmt.Sprintf("Container is locked: %s", reason), nil)
		return
	} else if err != nil {
		s.writeError(w, http.StatusInternalServerError, "LOCK_CHECK_FAILED", err.Error(), nil)
		return
	}
	if frozen, msg, err := s.containerManager.IsContainerFrozen(containerID); err == nil && frozen {
		s.writeError(w, http.StatusLocked, "CONTAINER_FROZEN", fmt.Sprintf("Container is frozen: %s", msg), nil)
		return
	} else if err != nil {
		s.writeError(w, http.StatusInternalServerError, "FREEZE_CHECK_FAILED", err.Error(), nil)
		return
	}

	// Perform action
	ctx := r.Context()
	if err := s.containerManager.PowerAction(ctx, containerID, action); err != nil {
		// Map known errors to HTTP status codes
		if strings.Contains(err.Error(), "locked") || strings.Contains(err.Error(), "frozen") {
			s.writeError(w, http.StatusLocked, "ACTION_BLOCKED", err.Error(), nil)
			return
		}
		s.writeError(w, http.StatusInternalServerError, "ACTION_FAILED", err.Error(), nil)
		return
	}

	s.writeSuccess(w, map[string]interface{}{"message": fmt.Sprintf("Container %s", action), "containerId": containerID})
}

func (s *Server) handleContainerRebuild(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerId"]

	ctx := r.Context()

	// Get container info to find volume ID
	containerInfo, err := s.containerManager.GetContainer(ctx, containerID)
	if err != nil {
		s.writeError(w, http.StatusNotFound, "CONTAINER_NOT_FOUND", err.Error(), nil)
		return
	}

	// Check if container is locked or frozen
	if containerInfo.Locked {
		s.writeError(w, http.StatusLocked, "CONTAINER_LOCKED", fmt.Sprintf("Container is locked: %s", containerInfo.LockReason), nil)
		return
	}
	if containerInfo.Frozen {
		s.writeError(w, http.StatusLocked, "CONTAINER_FROZEN", fmt.Sprintf("Container is frozen: %s", containerInfo.FreezeMessage), nil)
		return
	}

	// Stop the container first
	if err := s.containerManager.PowerAction(ctx, containerID, "stop"); err != nil {
		s.logger.WithError(err).Warn("Failed to stop container during rebuild")
	}

	// Remove the container (but keep the volume)
	if err := s.containerManager.RemoveContainer(ctx, containerID, false); err != nil {
		s.writeError(w, http.StatusInternalServerError, "REBUILD_FAILED", fmt.Sprintf("Failed to remove container: %s", err.Error()), nil)
		return
	}

	// Create a new deployment request based on the existing container
	deployReq := &container.DeploymentRequest{
		ID:     containerInfo.VolumeID,
		Image:  containerInfo.Image,
		Memory: containerInfo.MemoryLimit,
		CPU:    containerInfo.CPULimit,
		Disk:   containerInfo.DiskLimit,
	}

	// Deploy the container again
	if err := s.containerManager.Deploy(ctx, deployReq); err != nil {
		s.writeError(w, http.StatusInternalServerError, "REBUILD_FAILED", fmt.Sprintf("Failed to redeploy container: %s", err.Error()), nil)
		return
	}

	s.writeSuccess(w, map[string]interface{}{
		"message":     "Container rebuilt successfully",
		"containerId": containerID,
		"volumeId":    containerInfo.VolumeID,
	})
}

func (s *Server) handleContainerStats(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerId"]

	ctx := r.Context()
	stats, err := s.monitor.GetContainerStats(ctx, containerID)
	if err != nil {
		s.writeError(w, http.StatusInternalServerError, "STATS_FAILED", err.Error(), nil)
		return
	}

	// Attach RU info (cumulative + limit + last sample) when available
	var ruInfo map[string]interface{}
	if vid, err := s.containerManager.GetVolumeIDByContainerID(containerID); err == nil {
		state := s.containerManager.GetState(vid)
		ruInfo = map[string]interface{}{
			"ru_cumulative": state.RUCumulative,
			"ru_limit":      state.RULimit,
			"last_sample": map[string]interface{}{
				"timestamp":       state.LastSample.Timestamp,
				"memory_used":     state.LastSample.MemoryUsed,
				"cpu_percent":     state.LastSample.CPUPercent,
				"volume_size_mib": state.LastSample.VolumeSizeM,
			},
		}
	}

	response := map[string]interface{}{"stats": stats, "ru": ruInfo}
	s.writeSuccess(w, response)
}

func (s *Server) handleContainerLock(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerId"]
	action := r.URL.Query().Get("do")
	reason := r.URL.Query().Get("reason")

	if action != "LOCK" && action != "UNLOCK" {
		s.writeError(w, http.StatusBadRequest, "INVALID_ACTION", "Action must be LOCK or UNLOCK", nil)
		return
	}

	if action == "LOCK" {
		if reason == "" {
			reason = "Manually locked"
		}
		if err := s.containerManager.LockContainer(containerID, reason); err != nil {
			s.writeError(w, http.StatusInternalServerError, "LOCK_FAILED", err.Error(), nil)
			return
		}
		s.writeSuccess(w, map[string]interface{}{"message": "Container locked", "containerId": containerID, "reason": reason})
	} else {
		if err := s.containerManager.UnlockContainer(containerID); err != nil {
			s.writeError(w, http.StatusInternalServerError, "UNLOCK_FAILED", err.Error(), nil)
			return
		}
		s.writeSuccess(w, map[string]interface{}{"message": "Container unlocked", "containerId": containerID})
	}
}

func (s *Server) handleContainerExec(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerId"]

	var req struct {
		Command string `json:"command"`
		User    string `json:"user,omitempty"`
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		s.writeError(w, http.StatusBadRequest, "INVALID_JSON", "Invalid JSON in request body", nil)
		return
	}

	if req.Command == "" {
		s.writeError(w, http.StatusBadRequest, "MISSING_COMMAND", "Command is required", nil)
		return
	}

	ctx := r.Context()

	// Create exec configuration
	execConfig := types.ExecConfig{
		AttachStdout: true,
		AttachStderr: true,
		AttachStdin:  false,
		Cmd:          []string{"/bin/sh", "-c", req.Command},
	}

	if req.User != "" {
		execConfig.User = req.User
	}

	// Create exec instance
	execResp, err := s.containerManager.GetDockerClient().ContainerExecCreate(ctx, containerID, execConfig)
	if err != nil {
		s.writeError(w, http.StatusInternalServerError, "EXEC_CREATE_FAILED", err.Error(), nil)
		return
	}

	// Attach to exec instance
	hijacked, err := s.containerManager.GetDockerClient().ContainerExecAttach(ctx, execResp.ID, types.ExecStartCheck{Tty: false})
	if err != nil {
		s.writeError(w, http.StatusInternalServerError, "EXEC_ATTACH_FAILED", err.Error(), nil)
		return
	}
	defer hijacked.Close()

	// Read output
	output := make([]string, 0)
	scanner := bufio.NewScanner(hijacked.Reader)
	for scanner.Scan() {
		output = append(output, scanner.Text())
	}

	if err := scanner.Err(); err != nil {
		s.writeError(w, http.StatusInternalServerError, "EXEC_READ_FAILED", err.Error(), nil)
		return
	}

	s.writeSuccess(w, map[string]interface{}{
		"output":      output,
		"containerId": containerID,
		"command":     req.Command,
	})
}

func (s *Server) handleContainerSnapshots(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerId"]

	ctx := r.Context()

	if r.Method == "GET" {
		// List snapshots via snapshot manager
		if s.snapshotManager == nil {
			s.writeError(w, http.StatusNotImplemented, "NOT_IMPLEMENTED", "Snapshot manager not available", nil)
			return
		}

		snapshots, err := s.snapshotManager.ListSnapshots(containerID)
		if err != nil {
			s.writeError(w, http.StatusInternalServerError, "SNAPSHOT_LIST_FAILED", err.Error(), nil)
			return
		}

		s.writeSuccess(w, map[string]interface{}{
			"snapshots":   snapshots,
			"containerId": containerID,
		})
		return
	}

	// POST - create a snapshot job
	var req struct {
		Name        string `json:"name,omitempty"`
		Description string `json:"description,omitempty"`
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		s.writeError(w, http.StatusBadRequest, "INVALID_JSON", "Invalid JSON in request body", nil)
		return
	}

	if s.jobManager == nil {
		s.writeError(w, http.StatusNotImplemented, "NOT_IMPLEMENTED", "Job manager not available", nil)
		return
	}

	if req.Name == "" {
		req.Name = fmt.Sprintf("Snapshot_%d", time.Now().Unix())
	}

	jobID, err := s.jobManager.CreateSnapshotJob(containerID, req.Name, req.Description)
	if err != nil {
		s.writeError(w, http.StatusInternalServerError, "SNAPSHOT_CREATE_FAILED", err.Error(), nil)
		return
	}

	s.writeSuccess(w, map[string]interface{}{
		"message":     "Snapshot creation job started",
		"jobId":       jobID,
		"containerId": containerID,
		"name":        req.Name,
	})
}

func (s *Server) handleContainerRestore(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerId"]

	var req struct {
		SnapshotID string `json:"snapshot_id"`
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		s.writeError(w, http.StatusBadRequest, "INVALID_JSON", "Invalid JSON in request body", nil)
		return
	}

	if req.SnapshotID == "" {
		s.writeError(w, http.StatusBadRequest, "MISSING_SNAPSHOT_ID", "Snapshot ID is required", nil)
		return
	}

	// Create restore job via job manager
	if s.jobManager == nil {
		s.writeError(w, http.StatusNotImplemented, "NOT_IMPLEMENTED", "Job manager not available", nil)
		return
	}

	jobID, err := s.jobManager.CreateRestoreJob(containerID, req.SnapshotID)
	if err != nil {
		s.writeError(w, http.StatusInternalServerError, "SNAPSHOT_RESTORE_FAILED", err.Error(), nil)
		return
	}

	s.writeSuccess(w, map[string]interface{}{
		"message":     "Snapshot restore job started",
		"jobId":       jobID,
		"containerId": containerID,
		"snapshotId":  req.SnapshotID,
	})
}

func (s *Server) handleContainerImage(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerId"]

	ctx := r.Context()

	if r.Method == "GET" {
		// Get current container image information
		containerInfo, err := s.containerManager.GetContainer(ctx, containerID)
		if err != nil {
			s.writeError(w, http.StatusNotFound, "CONTAINER_NOT_FOUND", err.Error(), nil)
			return
		}

		s.writeSuccess(w, map[string]interface{}{
			"image":       containerInfo.Image,
			"containerId": containerID,
		})
		return
	}

	// POST - update container image (rebuild with new image)
	var req struct {
		Image string `json:"image"`
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		s.writeError(w, http.StatusBadRequest, "INVALID_JSON", "Invalid JSON in request body", nil)
		return
	}

	if req.Image == "" {
		s.writeError(w, http.StatusBadRequest, "MISSING_IMAGE", "Image is required", nil)
		return
	}

	// Get container info
	containerInfo, err := s.containerManager.GetContainer(ctx, containerID)
	if err != nil {
		s.writeError(w, http.StatusNotFound, "CONTAINER_NOT_FOUND", err.Error(), nil)
		return
	}

	// Check if container is locked or frozen
	if containerInfo.Locked {
		s.writeError(w, http.StatusLocked, "CONTAINER_LOCKED", fmt.Sprintf("Container is locked: %s", containerInfo.LockReason), nil)
		return
	}
	if containerInfo.Frozen {
		s.writeError(w, http.StatusLocked, "CONTAINER_FROZEN", fmt.Sprintf("Container is frozen: %s", containerInfo.FreezeMessage), nil)
		return
	}

	// Stop and remove the container
	if err := s.containerManager.PowerAction(ctx, containerID, "stop"); err != nil {
		s.logger.WithError(err).Warn("Failed to stop container during image update")
	}

	if err := s.containerManager.RemoveContainer(ctx, containerID, false); err != nil {
		s.writeError(w, http.StatusInternalServerError, "IMAGE_UPDATE_FAILED", fmt.Sprintf("Failed to remove container: %s", err.Error()), nil)
		return
	}

	// Create deployment request with new image
	deployReq := &container.DeploymentRequest{
		ID:     containerInfo.VolumeID,
		Image:  req.Image,
		Memory: containerInfo.MemoryLimit,
		CPU:    containerInfo.CPULimit,
		Disk:   containerInfo.DiskLimit,
	}

	// Deploy with new image
	if err := s.containerManager.Deploy(ctx, deployReq); err != nil {
		s.writeError(w, http.StatusInternalServerError, "IMAGE_UPDATE_FAILED", fmt.Sprintf("Failed to deploy with new image: %s", err.Error()), nil)
		return
	}

	s.writeSuccess(w, map[string]interface{}{
		"message":     "Container image updated successfully",
		"containerId": containerID,
		"newImage":    req.Image,
	})
}

func (s *Server) handleContainerLimits(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerId"]

	var req struct {
		Memory int64 `json:"memory,omitempty"` // in MB
		CPU    int64 `json:"cpu,omitempty"`
		Disk   int64 `json:"disk,omitempty"` // in MB
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		s.writeError(w, http.StatusBadRequest, "INVALID_JSON", "Invalid JSON in request body", nil)
		return
	}

	ctx := r.Context()

	// Get container info
	containerInfo, err := s.containerManager.GetContainer(ctx, containerID)
	if err != nil {
		s.writeError(w, http.StatusNotFound, "CONTAINER_NOT_FOUND", err.Error(), nil)
		return
	}

	// Check if container is locked or frozen
	if containerInfo.Locked {
		s.writeError(w, http.StatusLocked, "CONTAINER_LOCKED", fmt.Sprintf("Container is locked: %s", containerInfo.LockReason), nil)
		return
	}
	if containerInfo.Frozen {
		s.writeError(w, http.StatusLocked, "CONTAINER_FROZEN", fmt.Sprintf("Container is frozen: %s", containerInfo.FreezeMessage), nil)
		return
	}

	// Use existing limits if not provided
	memory := containerInfo.MemoryLimit
	cpu := containerInfo.CPULimit
	disk := containerInfo.DiskLimit

	if req.Memory > 0 {
		memory = req.Memory
	}
	if req.CPU > 0 {
		cpu = req.CPU
	}
	if req.Disk > 0 {
		disk = req.Disk
	}

	// Stop and remove the container
	if err := s.containerManager.PowerAction(ctx, containerID, "stop"); err != nil {
		s.logger.WithError(err).Warn("Failed to stop container during limits update")
	}

	if err := s.containerManager.RemoveContainer(ctx, containerID, false); err != nil {
		s.writeError(w, http.StatusInternalServerError, "LIMITS_UPDATE_FAILED", fmt.Sprintf("Failed to remove container: %s", err.Error()), nil)
		return
	}

	// Create deployment request with new limits
	deployReq := &container.DeploymentRequest{
		ID:     containerInfo.VolumeID,
		Image:  containerInfo.Image,
		Memory: memory,
		CPU:    cpu,
		Disk:   disk,
	}

	// Deploy with new limits
	if err := s.containerManager.Deploy(ctx, deployReq); err != nil {
		s.writeError(w, http.StatusInternalServerError, "LIMITS_UPDATE_FAILED", fmt.Sprintf("Failed to deploy with new limits: %s", err.Error()), nil)
		return
	}

	s.writeSuccess(w, map[string]interface{}{
		"message":     "Container limits updated successfully",
		"containerId": containerID,
		"limits": map[string]interface{}{
			"memory": memory,
			"cpu":    cpu,
			"disk":   disk,
		},
	})
}

func (s *Server) handleContainerFreeze(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerId"]
	action := r.URL.Query().Get("do")
	message := r.URL.Query().Get("message")
	startParam := r.URL.Query().Get("start")

	if action == "UNFREEZE" {
		start := false
		if startParam == "true" || startParam == "1" {
			start = true
		}

		if err := s.containerManager.UnfreezeContainer(containerID, start); err != nil {
			s.writeError(w, http.StatusInternalServerError, "UNFREEZE_FAILED", err.Error(), nil)
			return
		}
		s.writeSuccess(w, map[string]interface{}{"message": "Container unfrozen", "containerId": containerID})
		return
	}

	// Default action: FREEZE
	if message == "" {
		message = "Manually frozen"
	}

	if err := s.containerManager.FreezeContainer(containerID, message); err != nil {
		s.writeError(w, http.StatusInternalServerError, "FREEZE_FAILED", err.Error(), nil)
		return
	}

	s.writeSuccess(w, map[string]interface{}{"message": "Container frozen", "containerId": containerID, "reason": message})
}

func (s *Server) handleContainerEnv(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerId"]

	ctx := r.Context()

	if r.Method == "GET" {
		env, err := s.containerManager.GetEnv(ctx, containerID)
		if err != nil {
			s.writeError(w, http.StatusInternalServerError, "ENV_FETCH_FAILED", err.Error(), nil)
			return
		}

		s.writeSuccess(w, map[string]interface{}{"env": env})
		return
	}

	// POST - update environment
	var payload struct {
		Name  string             `json:"name,omitempty"`
		Value *string            `json:"value,omitempty"`
		Env   map[string]*string `json:"env,omitempty"`
	}

	if err := json.NewDecoder(r.Body).Decode(&payload); err != nil {
		s.writeError(w, http.StatusBadRequest, "INVALID_JSON", "Invalid JSON in request body", nil)
		return
	}

	updates := make(map[string]*string)
	if payload.Env != nil {
		for k, v := range payload.Env {
			updates[k] = v
		}
	} else if payload.Name != "" {
		updates[payload.Name] = payload.Value
	} else {
		s.writeError(w, http.StatusBadRequest, "INVALID_REQUEST", "Must provide either 'name'+'value' or 'env' map", nil)
		return
	}

	if err := s.containerManager.UpdateEnv(ctx, containerID, updates); err != nil {
		s.writeError(w, http.StatusInternalServerError, "ENV_UPDATE_FAILED", err.Error(), nil)
		return
	}

	// Return updated env
	env, err := s.containerManager.GetEnv(ctx, containerID)
	if err != nil {
		s.writeError(w, http.StatusInternalServerError, "ENV_FETCH_FAILED", err.Error(), nil)
		return
	}

	s.writeSuccess(w, map[string]interface{}{"env": env, "message": "Environment updated"})
}

// POST /api/container/{containerId}/config
// Bulk patch an existing container's configuration (image, limits, env, install_content, ru_limit, locked, freeze)
func (s *Server) handleContainerConfig(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerId"]

	var payload struct {
		Image          *string            `json:"image,omitempty"`
		Memory         *int64             `json:"memory,omitempty"`
		CPU            *int64             `json:"cpu,omitempty"`
		Disk           *int64             `json:"disk,omitempty"`
		Env            map[string]*string `json:"env,omitempty"`
		InstallContent *string            `json:"install_content,omitempty"`
		RunInstall     *bool              `json:"run_install,omitempty"`
		RULimit        *float64           `json:"ru_limit,omitempty"`
		Locked         *bool              `json:"locked,omitempty"`
		Freeze         *struct {
			Do      string  `json:"do,omitempty"`
			Message *string `json:"message,omitempty"`
		} `json:"freeze,omitempty"`
	}

	if err := json.NewDecoder(r.Body).Decode(&payload); err != nil {
		s.writeError(w, http.StatusBadRequest, "INVALID_JSON", "Invalid JSON in request body", nil)
		return
	}

	ctx := r.Context()

	containerInfo, err := s.containerManager.GetContainer(ctx, containerID)
	if err != nil {
		s.writeError(w, http.StatusNotFound, "CONTAINER_NOT_FOUND", err.Error(), nil)
		return
	}

	// Respect lock/frozen state
	if containerInfo.Locked {
		s.writeError(w, http.StatusLocked, "CONTAINER_LOCKED", fmt.Sprintf("Container is locked: %s", containerInfo.LockReason), nil)
		return
	}
	if containerInfo.Frozen {
		s.writeError(w, http.StatusLocked, "CONTAINER_FROZEN", fmt.Sprintf("Container is frozen: %s", containerInfo.FreezeMessage), nil)
		return
	}

	// Determine target values
	image := containerInfo.Image
	if payload.Image != nil && *payload.Image != "" {
		image = *payload.Image
	}

	memory := containerInfo.MemoryLimit
	if payload.Memory != nil && *payload.Memory > 0 {
		memory = *payload.Memory
	}

	cpu := containerInfo.CPULimit
	if payload.CPU != nil && *payload.CPU > 0 {
		cpu = *payload.CPU
	}

	disk := containerInfo.DiskLimit
	if payload.Disk != nil && *payload.Disk > 0 {
		disk = *payload.Disk
	}

	// Merge environment
	envMap, err := s.containerManager.GetEnv(ctx, containerID)
	if err != nil {
		s.writeError(w, http.StatusInternalServerError, "ENV_FETCH_FAILED", err.Error(), nil)
		return
	}
	if payload.Env != nil {
		for k, v := range payload.Env {
			if v == nil {
				delete(envMap, k)
			} else {
				envMap[k] = *v
			}
		}
	}
	// Convert env map to slice
	newEnv := make([]string, 0, len(envMap))
	for k, v := range envMap {
		newEnv = append(newEnv, fmt.Sprintf("%s=%s", k, v))
	}

	// If install content provided, write it to volume before redeploy
	if payload.InstallContent != nil {
		if err := s.containerManager.UpdateInstallContent(ctx, containerInfo.VolumeID, *payload.InstallContent); err != nil {
			s.writeError(w, http.StatusInternalServerError, "INSTALL_WRITE_FAILED", fmt.Sprintf("Failed to write install content: %s", err.Error()), nil)
			return
		}
	}

	// If any deployment-affecting fields changed, recreate the container with new settings
	needRedeploy := payload.Image != nil || payload.Memory != nil || payload.CPU != nil || payload.Disk != nil || payload.Env != nil || payload.InstallContent != nil

	if needRedeploy {
		// Stop and remove existing container (best-effort stop)
		if err := s.containerManager.PowerAction(ctx, containerID, "stop"); err != nil {
			s.logger.WithError(err).Warn("Failed to stop container during config update")
		}

		if err := s.containerManager.RemoveContainer(ctx, containerID, false); err != nil {
			s.writeError(w, http.StatusInternalServerError, "CONFIG_UPDATE_FAILED", fmt.Sprintf("Failed to remove container: %s", err.Error()), nil)
			return
		}

		// Build deployment request
		deployReq := &container.DeploymentRequest{
			ID:     containerInfo.VolumeID,
			Image:  image,
			Memory: memory,
			CPU:    cpu,
			Disk:   disk,
			Env:    newEnv,
		}

		if payload.RULimit != nil {
			deployReq.RULimit = *payload.RULimit
		}

		if payload.InstallContent != nil {
			if deployReq.Variables == nil {
				deployReq.Variables = make(map[string]interface{})
			}
			deployReq.Variables["install_content"] = *payload.InstallContent
		}

		if err := s.containerManager.Deploy(ctx, deployReq); err != nil {
			s.writeError(w, http.StatusInternalServerError, "CONFIG_UPDATE_FAILED", fmt.Sprintf("Failed to redeploy container: %s", err.Error()), nil)
			return
		}

		// Optionally run install.sh output automatically; Deploy writes and executes install.sh when present
	}

	// Apply non-deploy state changes (RULimit, lock/freeze)
	if payload.RULimit != nil {
		if err := s.containerManager.SetRULimit(containerInfo.VolumeID, *payload.RULimit); err != nil {
			s.writeError(w, http.StatusInternalServerError, "RULIMIT_UPDATE_FAILED", err.Error(), nil)
			return
		}
	}

	if payload.Locked != nil {
		if *payload.Locked {
			if err := s.containerManager.LockContainer(containerID, "manual lock via config"); err != nil {
				s.writeError(w, http.StatusInternalServerError, "LOCK_FAILED", err.Error(), nil)
				return
			}
		} else {
			if err := s.containerManager.UnlockContainer(containerID); err != nil {
				s.writeError(w, http.StatusInternalServerError, "UNLOCK_FAILED", err.Error(), nil)
				return
			}
		}
	}

	if payload.Freeze != nil {
		if strings.ToUpper(payload.Freeze.Do) == "UNFREEZE" {
			start := false
			if payload.Freeze.Message != nil && (*payload.Freeze.Message == "start" || *payload.Freeze.Message == "true") {
				start = true
			}
			if err := s.containerManager.UnfreezeContainer(containerID, start); err != nil {
				s.writeError(w, http.StatusInternalServerError, "UNFREEZE_FAILED", err.Error(), nil)
				return
			}
		} else {
			msg := "Manual freeze via config"
			if payload.Freeze.Message != nil {
				msg = *payload.Freeze.Message
			}
			if err := s.containerManager.FreezeContainer(containerID, msg); err != nil {
				s.writeError(w, http.StatusInternalServerError, "FREEZE_FAILED", err.Error(), nil)
				return
			}
		}
	}

	s.writeSuccess(w, map[string]interface{}{"message": "Container config updated", "containerId": containerID})
}

func (s *Server) handleContainerPorts(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerId"]

	ctx := r.Context()

	if r.Method == "GET" {
		// Get current container port information
		containerInfo, err := s.containerManager.GetContainer(ctx, containerID)
		if err != nil {
			s.writeError(w, http.StatusNotFound, "CONTAINER_NOT_FOUND", err.Error(), nil)
			return
		}

		s.writeSuccess(w, map[string]interface{}{
			"ports":       containerInfo.Ports,
			"containerId": containerID,
		})
		return
	}

	// POST - update container ports (requires container recreation)
	var req struct {
		Ports map[string]string `json:"ports"` // containerPort:hostPort mapping
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		s.writeError(w, http.StatusBadRequest, "INVALID_JSON", "Invalid JSON in request body", nil)
		return
	}

	if len(req.Ports) == 0 {
		s.writeError(w, http.StatusBadRequest, "MISSING_PORTS", "Ports mapping is required", nil)
		return
	}

	// Get container info
	containerInfo, err := s.containerManager.GetContainer(ctx, containerID)
	if err != nil {
		s.writeError(w, http.StatusNotFound, "CONTAINER_NOT_FOUND", err.Error(), nil)
		return
	}

	// Check if container is locked or frozen
	if containerInfo.Locked {
		s.writeError(w, http.StatusLocked, "CONTAINER_LOCKED", fmt.Sprintf("Container is locked: %s", containerInfo.LockReason), nil)
		return
	}
	if containerInfo.Frozen {
		s.writeError(w, http.StatusLocked, "CONTAINER_FROZEN", fmt.Sprintf("Container is frozen: %s", containerInfo.FreezeMessage), nil)
		return
	}

	// Validate port numbers
	for containerPort, hostPort := range req.Ports {
		if hostPortNum, err := strconv.Atoi(hostPort); err != nil || hostPortNum < 1 || hostPortNum > 65535 {
			s.writeError(w, http.StatusBadRequest, "INVALID_PORT", fmt.Sprintf("Invalid host port: %s", hostPort), nil)
			return
		}
		if containerPortNum, err := strconv.Atoi(containerPort); err != nil || containerPortNum < 1 || containerPortNum > 65535 {
			s.writeError(w, http.StatusBadRequest, "INVALID_PORT", fmt.Sprintf("Invalid container port: %s", containerPort), nil)
			return
		}
	}

	s.writeSuccess(w, map[string]interface{}{
		"message":        "Port management requires container recreation - feature coming soon",
		"containerId":    containerID,
		"requestedPorts": req.Ports,
	})
}

func (s *Server) handleContainerLogs(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerId"]
	linesStr := r.URL.Query().Get("lines")

	lines := 100 // default
	if linesStr != "" {
		if parsed, err := strconv.Atoi(linesStr); err == nil && parsed > 0 {
			lines = parsed
		}
	}

	ctx := r.Context()

	// Get container logs from Docker
	container := s.containerManager.GetDockerClient().GetContainer(containerID)
	logsReader, err := container.Logs(ctx, false)
	if err != nil {
		s.writeError(w, http.StatusInternalServerError, "LOGS_FAILED", err.Error(), nil)
		return
	}
	defer logsReader.Close()

	// Read logs and return as text
	logs := make([]string, 0)
	scanner := bufio.NewScanner(logsReader)
	lineCount := 0

	for scanner.Scan() && lineCount < lines {
		logs = append(logs, scanner.Text())
		lineCount++
	}

	if err := scanner.Err(); err != nil {
		s.writeError(w, http.StatusInternalServerError, "LOGS_READ_FAILED", err.Error(), nil)
		return
	}

	s.writeSuccess(w, map[string]interface{}{
		"logs":        logs,
		"lines":       len(logs),
		"containerId": containerID,
	})
}

func (s *Server) handleContainerWebSocket(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerId"]

	// For now, return information about WebSocket endpoints
	// The actual WebSocket implementation is handled by the websocket package
	s.writeSuccess(w, map[string]interface{}{
		"message": "WebSocket endpoints available",
		"endpoints": map[string]string{
			"exec":  fmt.Sprintf("ws://%s/exec/%s", r.Host, containerID),
			"stats": fmt.Sprintf("ws://%s/stats/%s", r.Host, containerID),
		},
		"containerId": containerID,
	})
}

// Daemon handlers

func (s *Server) handleDaemonPower(w http.ResponseWriter, r *http.Request) {
	action := r.URL.Query().Get("action")

	validActions := map[string]bool{
		"start":   true,
		"stop":    true,
		"restart": true,
		"kill":    true,
	}

	if !validActions[action] {
		s.writeError(w, http.StatusBadRequest, "INVALID_ACTION", "Action must be one of: start, stop, restart, kill", nil)
		return
	}

	switch action {
	case "start":
		s.writeSuccess(w, map[string]interface{}{
			"message": "Daemon is already running",
			"action":  action,
		})
	case "stop":
		// Graceful shutdown - this would typically be handled by the main process
		s.writeSuccess(w, map[string]interface{}{
			"message": "Daemon shutdown initiated",
			"action":  action,
		})
		// In a real implementation, you might signal the main process to shutdown
	case "restart":
		s.writeSuccess(w, map[string]interface{}{
			"message": "Daemon restart not supported via API - use system service manager",
			"action":  action,
		})
	case "kill":
		s.writeSuccess(w, map[string]interface{}{
			"message": "Daemon force stop not supported via API - use system service manager",
			"action":  action,
		})
	}
}

func (s *Server) handleDaemonUsage(w http.ResponseWriter, r *http.Request) {
	systemUsage, err := s.monitor.GetSystemUsage(r.Context())
	if err != nil {
		s.writeError(w, http.StatusInternalServerError, "USAGE_FETCH_FAILED", err.Error(), nil)
		return
	}

	s.writeSuccess(w, map[string]interface{}{
		"usage": systemUsage,
	})
}

func (s *Server) handleDaemonTunnels(w http.ResponseWriter, r *http.Request) {
	// For now, return empty tunnels list
	// In a full implementation, you would integrate with Cloudflare tunnel manager
	s.writeSuccess(w, map[string]interface{}{
		"tunnels": []interface{}{},
		"message": "Tunnel management available - no tunnels configured",
	})
}

func (s *Server) handleDaemonSettings(w http.ResponseWriter, r *http.Request) {
	if r.Method == "GET" {
		// Return current daemon settings (sanitized)
		settings := map[string]interface{}{
			"api_port":                    s.config.APIPort,
			"log_level":                   s.config.LogLevel,
			"daemon_storage_path":         s.config.DaemonStoragePath,
			"container_storage_path":      s.config.ContainerStoragePath,
			"snapshot_storage_path":       s.config.SnapshotStoragePath,
			"available_ports":             s.config.AvailablePorts,
			"max_containers":              s.config.MaxContainers,
			"max_cpu_per_container":       s.config.MaxCPUPerContainer,
			"max_memory_per_container":    s.config.MaxMemoryPerContainer,
			"billing_worker_interval":     s.config.BillingWorkerInterval,
			"health_worker_interval":      s.config.HealthWorkerInterval,
			"tunnel_worker_interval":      s.config.TunnelWorkerInterval,
			"persistence_worker_interval": s.config.PersistenceWorkerInterval,
			"billing_rates":               s.config.BillingRates,
		}

		s.writeSuccess(w, map[string]interface{}{
			"settings": settings,
		})
	} else {
		// POST - update settings
		var req map[string]interface{}
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			s.writeError(w, http.StatusBadRequest, "INVALID_JSON", "Invalid JSON in request body", nil)
			return
		}

		// For now, just return success without actually updating
		// In a full implementation, you would validate and update the configuration
		s.writeSuccess(w, map[string]interface{}{
			"message":          "Settings update received - restart daemon to apply changes",
			"updated_settings": req,
		})
	}
}

// Middleware

func (s *Server) corsMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Access-Control-Allow-Origin", "*")
		w.Header().Set("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS")
		w.Header().Set("Access-Control-Allow-Headers", "Content-Type, Authorization, X-API-Token")

		if r.Method == "OPTIONS" {
			w.WriteHeader(http.StatusOK)
			return
		}

		next.ServeHTTP(w, r)
	})
}

func (s *Server) loggingMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		start := time.Now()

		// Create a response writer wrapper to capture status code
		wrapper := &responseWriter{ResponseWriter: w, statusCode: http.StatusOK}

		next.ServeHTTP(wrapper, r)

		duration := time.Since(start)

		s.logger.WithFields(logrus.Fields{
			"method":      r.Method,
			"path":        r.URL.Path,
			"status":      wrapper.statusCode,
			"duration_ms": duration.Milliseconds(),
			"remote_addr": r.RemoteAddr,
			"user_agent":  r.Header.Get("User-Agent"),
		}).Info("HTTP request")
	})
}

// Response helpers

func (s *Server) writeSuccess(w http.ResponseWriter, data interface{}) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(http.StatusOK)

	response := map[string]interface{}{
		"success": true,
		"data":    data,
	}

	json.NewEncoder(w).Encode(response)
}

func (s *Server) writeError(w http.ResponseWriter, statusCode int, code, message string, details interface{}) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(statusCode)

	errorData := map[string]interface{}{
		"code":    code,
		"message": message,
	}

	if details != nil {
		errorData["details"] = details
	}

	response := map[string]interface{}{
		"success": false,
		"error":   errorData,
	}

	json.NewEncoder(w).Encode(response)
}

// responseWriter wraps http.ResponseWriter to capture status code
type responseWriter struct {
	http.ResponseWriter
	statusCode int
}

func (rw *responseWriter) WriteHeader(code int) {
	rw.statusCode = code
	rw.ResponseWriter.WriteHeader(code)
}
