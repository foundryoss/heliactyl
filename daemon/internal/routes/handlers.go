package routes

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	containerPkg "NightLightd/internal/container"
	"NightLightd/internal/network"

	"github.com/gorilla/mux"
	"github.com/sirupsen/logrus"
)

// handleInstances handles instance listing
func (h *Handler) handleInstances(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()

	containers, err := h.dockerClient.ListContainers(ctx)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, "Failed to list containers")
		return
	}
	h.writeSuccess(w, containers)
}

// handleCreateInstance handles container creation
func (h *Handler) handleCreateInstance(w http.ResponseWriter, r *http.Request) {
	var req containerPkg.DeploymentRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		h.writeError(w, http.StatusBadRequest, "Invalid JSON")
		return
	}

	if req.ID == "" || req.Image == "" {
		h.writeError(w, http.StatusBadRequest, "ID and Image are required")
		return
	}

	// Validate port bindings
	if req.PortBindings != nil {
		for containerPort, hostBindings := range req.PortBindings {
			for _, binding := range hostBindings {
				port, err := strconv.Atoi(binding.HostPort)
				if err != nil || port < 1 || port > 65535 {
					h.writeError(w, http.StatusBadRequest, fmt.Sprintf("Invalid port specification: %s", binding.HostPort))
					return
				}

				// Ensure requested host port is available on the host
				if ok, _ := network.IsTCPPortAvailable(port); !ok {
					h.writeError(w, http.StatusBadRequest, fmt.Sprintf("Host port %d is not available", port))
					return
				}
			}
			_ = containerPort // Use the variable to avoid unused variable error
		}
	}

	// Create container creation job
	jobID, err := h.jobManager.CreateContainerJob(&req)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("Failed to create job: %s", err.Error()))
		return
	}

	// Return job ID for tracking
	h.writeSuccess(w, map[string]interface{}{
		"message":  "Container creation job started",
		"jobId":    jobID,
		"volumeId": req.ID,
	})
}

// handleInstanceDetails handles getting instance details
func (h *Handler) handleInstanceDetails(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["id"]

	ctx := r.Context()
	container := h.dockerClient.GetContainer(containerID)

	containerInfo, err := container.Inspect(ctx)
	if err != nil {
		h.writeError(w, http.StatusNotFound, "Container not found")
		return
	}

	h.writeSuccess(w, containerInfo)
}

// handleDeleteInstance handles container deletion
func (h *Handler) handleDeleteInstance(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["id"]

	ctx := r.Context()
	if err := h.containerManager.Delete(ctx, containerID); err != nil {
		h.writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	h.writeSuccess(w, map[string]string{
		"message":     "Container deleted successfully",
		"containerId": containerID,
	})
}

// handleRedeployInstance handles container redeployment
func (h *Handler) handleRedeployInstance(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["id"]
	volumeID := vars["volumeId"]

	var req containerPkg.DeploymentRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		h.writeError(w, http.StatusBadRequest, "Invalid JSON")
		return
	}

	ctx := r.Context()

	// Delete existing container
	if err := h.containerManager.Delete(ctx, containerID); err != nil {
		h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("Failed to delete existing container: %s", err.Error()))
		return
	}

	// Set the volume ID from URL
	req.ID = volumeID

	// Return immediately
	h.writeSuccess(w, map[string]interface{}{
		"message":  "Redeployment started",
		"volumeId": volumeID,
	})

	// Start redeployment in background
	go func() {
		if err := h.containerManager.Deploy(ctx, &req); err != nil {
			h.log.WithError(err).Error("Container redeployment failed")
		}
	}()
}

// handleReinstallInstance handles container reinstallation
func (h *Handler) handleReinstallInstance(w http.ResponseWriter, r *http.Request) {
	// Similar to redeploy but with script processing
	h.handleRedeployInstance(w, r)
}

// handleEditInstance handles container editing
func (h *Handler) handleEditInstance(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["id"]

	var req struct {
		Image    string `json:"Image"`
		Memory   int64  `json:"Memory"`
		CPU      int64  `json:"Cpu"`
		VolumeID string `json:"VolumeId"`
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		h.writeError(w, http.StatusBadRequest, "Invalid JSON")
		return
	}

	ctx := r.Context()
	container := h.dockerClient.GetContainer(containerID)

	// Get current container info
	containerInfo, err := container.Inspect(ctx)
	if err != nil {
		h.writeError(w, http.StatusNotFound, "Container not found")
		return
	}

	// Stop and remove current container
	if containerInfo.State.Running {
		if err := container.Stop(ctx); err != nil {
			h.writeError(w, http.StatusInternalServerError, "Failed to stop container")
			return
		}
	}

	if err := container.Remove(ctx, true); err != nil {
		h.writeError(w, http.StatusInternalServerError, "Failed to remove container")
		return
	}

	// Create new deployment request with updated values
	deployReq := &containerPkg.DeploymentRequest{
		ID:     req.VolumeID,
		Image:  req.Image,
		Memory: req.Memory,
		CPU:    req.CPU,
		Env:    containerInfo.Config.Env,
		Cmd:    containerInfo.Config.Cmd,
	}

	// Convert exposed ports and port bindings
	if containerInfo.Config.ExposedPorts != nil {
		deployReq.Ports = containerInfo.Config.ExposedPorts
	}
	if containerInfo.HostConfig.PortBindings != nil {
		deployReq.PortBindings = containerInfo.HostConfig.PortBindings
	}

	// Start deployment in background
	go func() {
		if err := h.containerManager.Deploy(ctx, deployReq); err != nil {
			h.log.WithError(err).Error("Container edit failed")
		}
	}()

	h.writeSuccess(w, map[string]interface{}{
		"message":        "Container edit started",
		"oldContainerId": containerID,
	})
}

// handleContainerState handles getting container state
func (h *Handler) handleContainerState(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	volumeID := vars["volumeId"]

	state := h.containerManager.GetState(volumeID)
	h.writeSuccess(w, state)
}

// handleContainerFreeze handles freezing/unfreezing via legacy API (/api/container/{containerID}/freeze)
func (h *Handler) handleContainerFreeze(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	action := r.URL.Query().Get("do")
	message := r.URL.Query().Get("message")
	startParam := r.URL.Query().Get("start")

	if action == "UNFREEZE" {
		start := false
		if startParam == "true" || startParam == "1" {
			start = true
		}

		if err := h.containerManager.UnfreezeContainer(containerID, start); err != nil {
			h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("UNFREEZE_FAILED: %s", err.Error()))
			return
		}
		h.writeSuccess(w, map[string]interface{}{"message": "Container unfrozen", "containerId": containerID})
		return
	}

	// Default: FREEZE
	if message == "" {
		message = "Manually frozen"
	}

	if err := h.containerManager.FreezeContainer(containerID, message); err != nil {
		h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("FREEZE_FAILED: %s", err.Error()))
		return
	}

	h.writeSuccess(w, map[string]interface{}{"message": "Container frozen", "containerId": containerID, "reason": message})
}

// handleContainerDelete handles deletion via legacy API (/api/container/{containerID})
func (h *Handler) handleContainerDelete(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	mode := r.URL.Query().Get("mode")

	if mode == "" {
		mode = "safe"
	}

	// Check lock/freeze state first
	if locked, reason, err := h.containerManager.IsContainerLocked(containerID); err == nil && locked {
		h.writeError(w, http.StatusLocked, fmt.Sprintf("CONTAINER_LOCKED: %s", reason))
		return
	} else if err != nil {
		h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("LOCK_CHECK_FAILED: %s", err.Error()))
		return
	}

	if frozen, msg, err := h.containerManager.IsContainerFrozen(containerID); err == nil && frozen {
		h.writeError(w, http.StatusLocked, fmt.Sprintf("CONTAINER_FROZEN: %s", msg))
		return
	} else if err != nil {
		h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("FREEZE_CHECK_FAILED: %s", err.Error()))
		return
	}

	removeVolume := false
	if mode == "immediate" {
		removeVolume = true
	} else if mode != "safe" {
		h.writeError(w, http.StatusBadRequest, "INVALID_MODE: mode must be 'safe' or 'immediate'")
		return
	}

	ctx := r.Context()
	if err := h.containerManager.RemoveContainer(ctx, containerID, removeVolume); err != nil {
		h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("DELETE_FAILED: %s", err.Error()))
		return
	}

	h.writeSuccess(w, map[string]interface{}{"message": "Container deleted", "containerId": containerID, "mode": mode})
}

// handleContainerStateByID returns the saved state for a container given its container ID
func (h *Handler) handleContainerStateByID(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]

	volumeID, err := h.containerManager.GetVolumeIDByContainerID(containerID)
	if err != nil {
		h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", containerID))
		return
	}

	state := h.containerManager.GetState(volumeID)
	h.writeSuccess(w, state)
}

// handleInstancePorts handles getting container ports
func (h *Handler) handleInstancePorts(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["id"]

	ctx := r.Context()
	container := h.dockerClient.GetContainer(containerID)

	containerInfo, err := container.Inspect(ctx)
	if err != nil {
		h.writeError(w, http.StatusNotFound, "Container not found")
		return
	}

	h.writeSuccess(w, map[string]interface{}{
		"ports":        containerInfo.Config.ExposedPorts,
		"portBindings": containerInfo.HostConfig.PortBindings,
	})
}

// handleContainerPower handles power actions under legacy API (/api/container/{containerID}/power)
func (h *Handler) handleContainerPower(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	action := r.URL.Query().Get("action")

	valid := map[string]bool{"start": true, "stop": true, "restart": true, "kill": true}
	if !valid[action] {
		h.writeError(w, http.StatusBadRequest, "INVALID_ACTION")
		return
	}

	// Check lock/freeze
	if locked, reason, err := h.containerManager.IsContainerLocked(containerID); err == nil && locked {
		h.writeError(w, http.StatusLocked, fmt.Sprintf("Container is locked: %s", reason))
		return
	} else if err != nil {
		h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("LOCK_CHECK_FAILED: %s", err.Error()))
		return
	}

	if frozen, msg, err := h.containerManager.IsContainerFrozen(containerID); err == nil && frozen {
		h.writeError(w, http.StatusLocked, fmt.Sprintf("Container is frozen: %s", msg))
		return
	} else if err != nil {
		h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("FREEZE_CHECK_FAILED: %s", err.Error()))
		return
	}

	ctx := r.Context()
	if err := h.containerManager.PowerAction(ctx, containerID, action); err != nil {
		h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("POWER_FAILED: %s", err.Error()))
		return
	}

	h.writeSuccess(w, map[string]string{"action": action, "container": containerID})
}

// handleContainerEnv handles get/post env under legacy API (/api/container/{containerID}/env)
func (h *Handler) handleContainerEnv(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	ctx := r.Context()

	if r.Method == "GET" {
		env, err := h.containerManager.GetEnv(ctx, containerID)
		if err != nil {
			h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("ENV_FETCH_FAILED: %s", err.Error()))
			return
		}
		h.writeSuccess(w, map[string]interface{}{"env": env})
		return
	}

	// POST - update env
	var payload struct {
		Name  string             `json:"name,omitempty"`
		Value *string            `json:"value,omitempty"`
		Env   map[string]*string `json:"env,omitempty"`
	}
	if err := json.NewDecoder(r.Body).Decode(&payload); err != nil {
		h.writeError(w, http.StatusBadRequest, "INVALID_JSON")
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
		h.writeError(w, http.StatusBadRequest, "INVALID_REQUEST")
		return
	}

	// Use a timeout so UpdateEnv can't hang forever
	ctxUpdate, cancel := context.WithTimeout(ctx, 60*time.Second)
	defer cancel()

	if err := h.containerManager.UpdateEnv(ctxUpdate, containerID, updates); err != nil {
		h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("ENV_UPDATE_FAILED: %s", err.Error()))
		return
	}

	// Return updated env
	env, err := h.containerManager.GetEnv(ctx, containerID)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, fmt.Sprintf("ENV_FETCH_FAILED: %s", err.Error()))
		return
	}

	h.writeSuccess(w, map[string]interface{}{"env": env, "message": "Environment updated"})
}

// File system handlers

// handleListFiles handles listing files in a volume
func (h *Handler) handleListFiles(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	subPath := r.URL.Query().Get("path")
	volumeID := r.URL.Query().Get("volume")

	// If no volume specified, use the first attached volume
	if volumeID == "" {
		volumes, err := h.containerManager.GetAttachedVolumes(containerID)
		if err != nil {
			h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
			return
		}
		if len(volumes) == 0 {
			h.writeError(w, http.StatusBadRequest, "No volumes attached to container")
			return
		}
		volumeID = volumes[0] // Use first volume
	}

	// Validate volume access - CRITICAL SECURITY CHECK
	if err := h.validateVolumeAccess(containerID, volumeID); err != nil {
		h.writeError(w, http.StatusForbidden, fmt.Sprintf("Access denied: %s", err.Error()))
		return
	}

	files, err := h.filesystemManager.ListFiles(volumeID, subPath)
	if err != nil {
		if strings.Contains(err.Error(), "no such file or directory") {
			h.writeError(w, http.StatusNotFound, "Volume or directory not found")
		} else if strings.Contains(err.Error(), "outside of the volume") {
			h.writeError(w, http.StatusBadRequest, err.Error())
		} else {
			h.writeError(w, http.StatusInternalServerError, err.Error())
		}
		return
	}

	h.writeSuccess(w, map[string]interface{}{
		"files": files,
	})
}

// handleCreateFile handles creating a new file
func (h *Handler) handleCreateFile(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	filename := vars["filename"]
	subPath := r.URL.Query().Get("path")
	volumeID := r.URL.Query().Get("volume")

	// If no volume specified, use the first attached volume
	if volumeID == "" {
		volumes, err := h.containerManager.GetAttachedVolumes(containerID)
		if err != nil {
			h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
			return
		}
		if len(volumes) == 0 {
			h.writeError(w, http.StatusBadRequest, "No volumes attached to container")
			return
		}
		volumeID = volumes[0] // Use first volume
	}

	// Validate volume access - CRITICAL SECURITY CHECK
	if err := h.validateVolumeAccess(containerID, volumeID); err != nil {
		h.writeError(w, http.StatusForbidden, fmt.Sprintf("Access denied: %s", err.Error()))
		return
	}

	var req struct {
		Content string `json:"content"`
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		h.writeError(w, http.StatusBadRequest, "Invalid JSON")
		return
	}

	filePath := filename
	if subPath != "" {
		filePath = filepath.Join(subPath, filename)
	}

	if err := h.filesystemManager.CreateFile(volumeID, filePath, []byte(req.Content)); err != nil {
		h.writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	h.writeSuccess(w, map[string]string{
		"message": "File created successfully",
	})
}

// handleEditFile handles editing an existing file
func (h *Handler) handleEditFile(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	filename := vars["filename"]
	subPath := r.URL.Query().Get("path")
	volumeID := r.URL.Query().Get("volume")

	// If no volume specified, use the first attached volume
	if volumeID == "" {
		volumes, err := h.containerManager.GetAttachedVolumes(containerID)
		if err != nil {
			h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
			return
		}
		if len(volumes) == 0 {
			h.writeError(w, http.StatusBadRequest, "No volumes attached to container")
			return
		}
		volumeID = volumes[0] // Use first volume
	}

	// Validate volume access - CRITICAL SECURITY CHECK
	if err := h.validateVolumeAccess(containerID, volumeID); err != nil {
		h.writeError(w, http.StatusForbidden, fmt.Sprintf("Access denied: %s", err.Error()))
		return
	}

	var req struct {
		Content string `json:"content"`
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		h.writeError(w, http.StatusBadRequest, "Invalid JSON")
		return
	}

	filePath := filename
	if subPath != "" {
		filePath = filepath.Join(subPath, filename)
	}

	if err := h.filesystemManager.WriteFile(volumeID, filePath, []byte(req.Content)); err != nil {
		if strings.Contains(err.Error(), "not supported for") {
			h.writeError(w, http.StatusBadRequest, err.Error())
		} else {
			h.writeError(w, http.StatusInternalServerError, err.Error())
		}
		return
	}

	h.writeSuccess(w, map[string]string{
		"message": "File updated successfully",
	})
}

// handleDeleteFile handles deleting a file
func (h *Handler) handleDeleteFile(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	filename := vars["filename"]
	subPath := r.URL.Query().Get("path")
	volumeID := r.URL.Query().Get("volume")

	// If no volume specified, use the first attached volume
	if volumeID == "" {
		volumes, err := h.containerManager.GetAttachedVolumes(containerID)
		if err != nil {
			h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
			return
		}
		if len(volumes) == 0 {
			h.writeError(w, http.StatusBadRequest, "No volumes attached to container")
			return
		}
		volumeID = volumes[0] // Use first volume
	}

	// Validate volume access - CRITICAL SECURITY CHECK
	if err := h.validateVolumeAccess(containerID, volumeID); err != nil {
		h.writeError(w, http.StatusForbidden, fmt.Sprintf("Access denied: %s", err.Error()))
		return
	}

	filePath := filename
	if subPath != "" {
		filePath = filepath.Join(subPath, filename)
	}

	if err := h.filesystemManager.DeleteFile(volumeID, filePath); err != nil {
		h.writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	h.writeSuccess(w, map[string]string{
		"message": "File deleted successfully",
	})
}

// handleRenameFile handles renaming a file
func (h *Handler) handleRenameFile(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	filename := vars["filename"]
	volumeID := r.URL.Query().Get("volume")

	// If no volume specified, use the first attached volume
	if volumeID == "" {
		volumes, err := h.containerManager.GetAttachedVolumes(containerID)
		if err != nil {
			h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
			return
		}
		if len(volumes) == 0 {
			h.writeError(w, http.StatusBadRequest, "No volumes attached to container")
			return
		}
		volumeID = volumes[0] // Use first volume
	}

	// Validate volume access - CRITICAL SECURITY CHECK
	if err := h.validateVolumeAccess(containerID, volumeID); err != nil {
		h.writeError(w, http.StatusForbidden, fmt.Sprintf("Access denied: %s", err.Error()))
		return
	}
	subPath := r.URL.Query().Get("path")

	var req struct {
		NewName string `json:"newName"`
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		h.writeError(w, http.StatusBadRequest, "Invalid JSON")
		return
	}

	if req.NewName == "" {
		h.writeError(w, http.StatusBadRequest, "New name is required")
		return
	}

	filePath := filename
	if subPath != "" {
		filePath = filepath.Join(subPath, filename)
	}

	if err := h.filesystemManager.RenameFile(volumeID, filePath, req.NewName); err != nil {
		h.writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	h.writeSuccess(w, map[string]string{
		"message": "File renamed successfully",
		"from":    filename,
		"to":      req.NewName,
	})
}

// handleViewFile handles viewing file contents
func (h *Handler) handleViewFile(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	filename := vars["filename"]
	subPath := r.URL.Query().Get("path")
	volumeID := r.URL.Query().Get("volume")

	// If no volume specified, use the first attached volume
	if volumeID == "" {
		volumes, err := h.containerManager.GetAttachedVolumes(containerID)
		if err != nil {
			h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
			return
		}
		if len(volumes) == 0 {
			h.writeError(w, http.StatusBadRequest, "No volumes attached to container")
			return
		}
		volumeID = volumes[0] // Use first volume
	}

	// Validate volume access - CRITICAL SECURITY CHECK
	if err := h.validateVolumeAccess(containerID, volumeID); err != nil {
		h.writeError(w, http.StatusForbidden, fmt.Sprintf("Access denied: %s", err.Error()))
		return
	}

	filePath := filename
	if subPath != "" {
		filePath = filepath.Join(subPath, filename)
	}

	content, err := h.filesystemManager.ReadFile(volumeID, filePath)
	if err != nil {
		if strings.Contains(err.Error(), "not supported for") {
			h.writeError(w, http.StatusBadRequest, err.Error())
		} else {
			h.writeError(w, http.StatusInternalServerError, err.Error())
		}
		return
	}

	h.writeSuccess(w, map[string]interface{}{
		"file":    filePath,
		"content": string(content),
		"size":    len(content),
	})
}

// handleDownloadFile handles file downloads
func (h *Handler) handleDownloadFile(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	filename := vars["filename"]
	subPath := r.URL.Query().Get("path")
	volumeID := r.URL.Query().Get("volume")

	// If no volume specified, use the first attached volume
	if volumeID == "" {
		volumes, err := h.containerManager.GetAttachedVolumes(containerID)
		if err != nil {
			h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
			return
		}
		if len(volumes) == 0 {
			h.writeError(w, http.StatusBadRequest, "No volumes attached to container")
			return
		}
		volumeID = volumes[0] // Use first volume
	}

	// Validate volume access - CRITICAL SECURITY CHECK
	if err := h.validateVolumeAccess(containerID, volumeID); err != nil {
		h.writeError(w, http.StatusForbidden, fmt.Sprintf("Access denied: %s", err.Error()))
		return
	}

	filePath := filename
	if subPath != "" {
		filePath = filepath.Join(subPath, filename)
	}

	reader, err := h.filesystemManager.StreamFile(volumeID, filePath)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, err.Error())
		return
	}
	defer reader.Close()

	w.Header().Set("Content-Disposition", fmt.Sprintf("attachment; filename=\"%s\"", filename))
	w.Header().Set("Content-Type", "application/octet-stream")

	if _, err := io.Copy(w, reader); err != nil {
		h.log.WithError(err).Error("Failed to stream file")
	}
}

// handleSearchFiles handles file searching
func (h *Handler) handleSearchFiles(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	query := r.URL.Query().Get("q")
	volumeID := r.URL.Query().Get("volume")

	// If no volume specified, use the first attached volume
	if volumeID == "" {
		volumes, err := h.containerManager.GetAttachedVolumes(containerID)
		if err != nil {
			h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
			return
		}
		if len(volumes) == 0 {
			h.writeError(w, http.StatusBadRequest, "No volumes attached to container")
			return
		}
		volumeID = volumes[0] // Use first volume
	}

	// Validate volume access - CRITICAL SECURITY CHECK
	if err := h.validateVolumeAccess(containerID, volumeID); err != nil {
		h.writeError(w, http.StatusForbidden, fmt.Sprintf("Access denied: %s", err.Error()))
		return
	}

	if query == "" {
		h.writeError(w, http.StatusBadRequest, "Search query is required")
		return
	}

	files, err := h.filesystemManager.SearchFiles(volumeID, query)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	h.writeSuccess(w, map[string]interface{}{
		"files": files,
	})
}

// handleUploadFile handles file uploads
func (h *Handler) handleUploadFile(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	subPath := r.URL.Query().Get("path")
	volumeID := r.URL.Query().Get("volume")

	// If no volume specified, use the first attached volume
	if volumeID == "" {
		volumes, err := h.containerManager.GetAttachedVolumes(containerID)
		if err != nil {
			h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
			return
		}
		if len(volumes) == 0 {
			h.writeError(w, http.StatusBadRequest, "No volumes attached to container")
			return
		}
		volumeID = volumes[0] // Use first volume
	}

	// Validate volume access - CRITICAL SECURITY CHECK
	if err := h.validateVolumeAccess(containerID, volumeID); err != nil {
		h.writeError(w, http.StatusForbidden, fmt.Sprintf("Access denied: %s", err.Error()))
		return
	}

	// Parse multipart form
	if err := r.ParseMultipartForm(32 << 20); err != nil { // 32MB max
		h.writeError(w, http.StatusBadRequest, "Failed to parse multipart form")
		return
	}

	file, header, err := r.FormFile("file")
	if err != nil {
		h.writeError(w, http.StatusBadRequest, "File field required")
		return
	}
	defer file.Close()

	// Get target path from query parameter or form value
	targetPath := subPath
	if targetPath == "" {
		targetPath = r.FormValue("path")
	}
	if targetPath == "" {
		targetPath = header.Filename
	}

	// Read file content
	content, err := io.ReadAll(file)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, "Failed to read uploaded file")
		return
	}

	// Write file
	if err := h.filesystemManager.WriteFile(volumeID, targetPath, content); err != nil {
		h.writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	h.writeSuccess(w, map[string]interface{}{
		"message":  "File uploaded successfully",
		"filename": header.Filename,
		"path":     targetPath,
		"size":     len(content),
		"volumeId": volumeID,
	})
}

// validateVolumeAccess validates that a volume belongs to a container and exists
func (h *Handler) validateVolumeAccess(containerID, volumeID string) error {
	// Get the volume ID that belongs to this container
	attachedVolumes, err := h.containerManager.GetAttachedVolumes(containerID)
	if err != nil {
		return fmt.Errorf("container not found: %s", containerID)
	}

	// Check if the requested volume is attached to this container
	volumeFound := false
	for _, attachedVolume := range attachedVolumes {
		if attachedVolume == volumeID {
			volumeFound = true
			break
		}
	}

	if !volumeFound {
		// Log the mismatch for debugging
		h.log.WithFields(logrus.Fields{
			"containerID":     containerID,
			"requestedVolume": volumeID,
			"attachedVolumes": attachedVolumes,
		}).Warn("Volume access validation failed - volume not attached to container")

		return fmt.Errorf("volume %s is not attached to container %s", volumeID, containerID)
	}

	// Additional security check: ensure volume ID doesn't contain path traversal
	if strings.Contains(volumeID, "..") || strings.Contains(volumeID, "/") || strings.Contains(volumeID, "\\") {
		return fmt.Errorf("invalid volume ID: contains illegal characters")
	}

	return nil
}
func (h *Handler) handleInstanceVolumes(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["id"]

	volumes, err := h.containerManager.GetAttachedVolumes(containerID)
	if err != nil {
		h.writeError(w, http.StatusNotFound, fmt.Sprintf("Failed to get volumes for container: %s", err.Error()))
		return
	}

	h.writeSuccess(w, map[string]interface{}{
		"containerID": containerID,
		"volumes":     volumes,
	})
}

// Job management handlers

// handleListJobs handles listing all jobs
func (h *Handler) handleListJobs(w http.ResponseWriter, r *http.Request) {
	jobs := h.jobManager.ListJobs()
	h.writeSuccess(w, map[string]interface{}{
		"jobs": jobs,
	})
}

// handleGetJob handles getting a specific job
func (h *Handler) handleGetJob(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	jobID := vars["id"]

	job, err := h.jobManager.GetJob(jobID)
	if err != nil {
		h.writeError(w, http.StatusNotFound, fmt.Sprintf("Job not found: %s", err.Error()))
		return
	}

	h.writeSuccess(w, job)
}

// handleJobLogs handles streaming job logs via HTTP Server-Sent Events (SSE)
func (h *Handler) handleJobLogs(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	jobID := vars["id"]

	// Check if job exists
	job, err := h.jobManager.GetJob(jobID)
	if err != nil {
		h.writeError(w, http.StatusNotFound, fmt.Sprintf("Job not found: %s", err.Error()))
		return
	}

	// Set headers for Server-Sent Events
	w.Header().Set("Content-Type", "text/event-stream")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")
	w.Header().Set("Access-Control-Allow-Origin", "*")
	w.Header().Set("Access-Control-Allow-Headers", "Cache-Control")

	// Send existing logs first
	for _, logEntry := range job.Logs {
		logData := fmt.Sprintf("data: {\"timestamp\":\"%s\",\"level\":\"%s\",\"message\":\"%s\"}\n\n",
			logEntry.Timestamp.Format(time.RFC3339),
			logEntry.Level,
			logEntry.Message)

		if _, err := w.Write([]byte(logData)); err != nil {
			return
		}

		if f, ok := w.(http.Flusher); ok {
			f.Flush()
		}
	}

	// Subscribe to new logs
	logChan, err := h.jobManager.SubscribeToLogs(jobID)
	if err != nil {
		h.log.WithError(err).Error("Failed to subscribe to job logs")
		return
	}

	// Stream new logs as they come
	for {
		select {
		case logEntry, ok := <-logChan:
			if !ok {
				// Channel closed, job completed
				return
			}

			logData := fmt.Sprintf("data: {\"timestamp\":\"%s\",\"level\":\"%s\",\"message\":\"%s\"}\n\n",
				logEntry.Timestamp.Format(time.RFC3339),
				logEntry.Level,
				logEntry.Message)

			if _, err := w.Write([]byte(logData)); err != nil {
				return
			}

			if f, ok := w.(http.Flusher); ok {
				f.Flush()
			}

		case <-r.Context().Done():
			// Client disconnected
			return
		}
	}
}

// New comprehensive filesystem API handlers

// handleFSRoot handles /api/fs/{containerID} - get all files or refresh
func (h *Handler) handleFSRoot(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	volumeID := r.URL.Query().Get("volume")
	path := r.URL.Query().Get("path")

	if path == "" {
		path = "/"
	}

	// Get volume ID if not specified
	if volumeID == "" {
		volumes, err := h.containerManager.GetAttachedVolumes(containerID)
		if err != nil {
			h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
			return
		}
		if len(volumes) == 0 {
			h.writeError(w, http.StatusBadRequest, "No volumes attached to container")
			return
		}
		volumeID = volumes[0]
	}

	// Validate volume access
	if err := h.validateVolumeAccess(containerID, volumeID); err != nil {
		h.writeError(w, http.StatusForbidden, fmt.Sprintf("Access denied: %s", err.Error()))
		return
	}

	switch r.Method {
	case "GET":
		// Get all files
		files, err := h.filesystemManager.ListFiles(volumeID, path)
		if err != nil {
			h.writeError(w, http.StatusInternalServerError, err.Error())
			return
		}
		h.writeSuccess(w, map[string]interface{}{
			"path":  path,
			"files": files,
		})

	case "POST":
		// Refresh/reload - just return current files
		files, err := h.filesystemManager.ListFiles(volumeID, path)
		if err != nil {
			h.writeError(w, http.StatusInternalServerError, err.Error())
			return
		}
		h.writeSuccess(w, map[string]interface{}{
			"path":      path,
			"files":     files,
			"refreshed": true,
		})
	}
}

// handleFSRename handles /api/fs/{containerID}/file/rename?file= - rename a file
func (h *Handler) handleFSRename(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	volumeID := r.URL.Query().Get("volume")
	filePath := r.URL.Query().Get("file")

	if filePath == "" {
		h.writeError(w, http.StatusBadRequest, "file parameter is required")
		return
	}

	// Security check: prevent path traversal
	if strings.Contains(filePath, "..") {
		h.writeError(w, http.StatusBadRequest, "Path traversal not allowed")
		return
	}

	// Get volume ID if not specified
	if volumeID == "" {
		volumes, err := h.containerManager.GetAttachedVolumes(containerID)
		if err != nil {
			h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
			return
		}
		if len(volumes) == 0 {
			h.writeError(w, http.StatusBadRequest, "No volumes attached to container")
			return
		}
		volumeID = volumes[0]
	}

	// Validate volume access
	if err := h.validateVolumeAccess(containerID, volumeID); err != nil {
		h.writeError(w, http.StatusForbidden, fmt.Sprintf("Access denied: %s", err.Error()))
		return
	}

	var req struct {
		NewName string `json:"newName"`
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		h.writeError(w, http.StatusBadRequest, "Invalid JSON")
		return
	}

	if req.NewName == "" {
		h.writeError(w, http.StatusBadRequest, "newName is required")
		return
	}

	// Security check: prevent path traversal in new name
	if strings.Contains(req.NewName, "..") || strings.Contains(req.NewName, "/") {
		h.writeError(w, http.StatusBadRequest, "Invalid new name: path traversal not allowed")
		return
	}

	if err := h.filesystemManager.RenameFile(volumeID, filePath, req.NewName); err != nil {
		h.writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	h.writeSuccess(w, map[string]string{
		"message": "File renamed successfully",
		"from":    filePath,
		"to":      req.NewName,
	})
}

// handleFSModify handles /api/fs/{containerID}/file/modify?file= - modify or delete a file
func (h *Handler) handleFSModify(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	volumeID := r.URL.Query().Get("volume")
	filePath := r.URL.Query().Get("file")

	if filePath == "" {
		h.writeError(w, http.StatusBadRequest, "file parameter is required")
		return
	}

	// Security check: prevent path traversal
	if strings.Contains(filePath, "..") {
		h.writeError(w, http.StatusBadRequest, "Path traversal not allowed")
		return
	}

	// Get volume ID if not specified
	if volumeID == "" {
		volumes, err := h.containerManager.GetAttachedVolumes(containerID)
		if err != nil {
			h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
			return
		}
		if len(volumes) == 0 {
			h.writeError(w, http.StatusBadRequest, "No volumes attached to container")
			return
		}
		volumeID = volumes[0]
	}

	// Validate volume access
	if err := h.validateVolumeAccess(containerID, volumeID); err != nil {
		h.writeError(w, http.StatusForbidden, fmt.Sprintf("Access denied: %s", err.Error()))
		return
	}

	switch r.Method {
	case "POST":
		// Modify file content
		var req struct {
			Content string `json:"content"`
		}

		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			h.writeError(w, http.StatusBadRequest, "Invalid JSON")
			return
		}

		if err := h.filesystemManager.WriteFile(volumeID, filePath, []byte(req.Content)); err != nil {
			h.writeError(w, http.StatusInternalServerError, err.Error())
			return
		}

		h.writeSuccess(w, map[string]string{
			"message": "File modified successfully",
			"file":    filePath,
		})

	case "DELETE":
		// Delete file
		if err := h.filesystemManager.DeleteFile(volumeID, filePath); err != nil {
			h.writeError(w, http.StatusInternalServerError, err.Error())
			return
		}

		h.writeSuccess(w, map[string]string{
			"message": "File deleted successfully",
			"file":    filePath,
		})
	}
}

// handleFSChmod handles /api/fs/{containerID}/file/chmod?file= - change or get file permissions
func (h *Handler) handleFSChmod(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	volumeID := r.URL.Query().Get("volume")
	filePath := r.URL.Query().Get("file")

	if filePath == "" {
		h.writeError(w, http.StatusBadRequest, "file parameter is required")
		return
	}

	// Security check: prevent path traversal
	if strings.Contains(filePath, "..") {
		h.writeError(w, http.StatusBadRequest, "Path traversal not allowed")
		return
	}

	// Get volume ID if not specified
	if volumeID == "" {
		volumes, err := h.containerManager.GetAttachedVolumes(containerID)
		if err != nil {
			h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
			return
		}
		if len(volumes) == 0 {
			h.writeError(w, http.StatusBadRequest, "No volumes attached to container")
			return
		}
		volumeID = volumes[0]
	}

	// Validate volume access
	if err := h.validateVolumeAccess(containerID, volumeID); err != nil {
		h.writeError(w, http.StatusForbidden, fmt.Sprintf("Access denied: %s", err.Error()))
		return
	}

	switch r.Method {
	case "GET":
		// Get current permissions
		permissions, err := h.filesystemManager.GetFilePermissions(volumeID, filePath)
		if err != nil {
			h.writeError(w, http.StatusInternalServerError, err.Error())
			return
		}

		h.writeSuccess(w, map[string]interface{}{
			"file":        filePath,
			"permissions": permissions,
		})

	case "POST":
		// Change permissions
		var req struct {
			Mode string `json:"mode"`
		}

		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			h.writeError(w, http.StatusBadRequest, "Invalid JSON")
			return
		}

		if req.Mode == "" {
			h.writeError(w, http.StatusBadRequest, "mode is required")
			return
		}

		if err := h.filesystemManager.SetFilePermissions(volumeID, filePath, req.Mode); err != nil {
			h.writeError(w, http.StatusInternalServerError, err.Error())
			return
		}

		h.writeSuccess(w, map[string]string{
			"message": "File permissions changed successfully",
			"file":    filePath,
			"mode":    req.Mode,
		})
	}
}

// handleFSContents handles /api/fs/{containerID}/file/contents?file= - get file contents as JSON
func (h *Handler) handleFSContents(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	volumeID := r.URL.Query().Get("volume")
	filePath := r.URL.Query().Get("file")

	if filePath == "" {
		h.writeError(w, http.StatusBadRequest, "file parameter is required")
		return
	}

	// Security check: prevent path traversal
	if strings.Contains(filePath, "..") {
		h.writeError(w, http.StatusBadRequest, "Path traversal not allowed")
		return
	}

	// Get volume ID if not specified
	if volumeID == "" {
		volumes, err := h.containerManager.GetAttachedVolumes(containerID)
		if err != nil {
			h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
			return
		}
		if len(volumes) == 0 {
			h.writeError(w, http.StatusBadRequest, "No volumes attached to container")
			return
		}
		volumeID = volumes[0]
	}

	// Validate volume access
	if err := h.validateVolumeAccess(containerID, volumeID); err != nil {
		h.writeError(w, http.StatusForbidden, fmt.Sprintf("Access denied: %s", err.Error()))
		return
	}

	content, err := h.filesystemManager.ReadFile(volumeID, filePath)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	h.writeSuccess(w, map[string]interface{}{
		"file":     filePath,
		"content":  string(content),
		"size":     len(content),
		"encoding": "utf-8",
	})
}

// handleFSDownload handles /api/fs/{containerID}/file/download?file= - get download link
func (h *Handler) handleFSDownload(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	volumeID := r.URL.Query().Get("volume")
	filePath := r.URL.Query().Get("file")

	if filePath == "" {
		h.writeError(w, http.StatusBadRequest, "file parameter is required")
		return
	}

	// Security check: prevent path traversal
	if strings.Contains(filePath, "..") {
		h.writeError(w, http.StatusBadRequest, "Path traversal not allowed")
		return
	}

	// Get volume ID if not specified
	if volumeID == "" {
		volumes, err := h.containerManager.GetAttachedVolumes(containerID)
		if err != nil {
			h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
			return
		}
		if len(volumes) == 0 {
			h.writeError(w, http.StatusBadRequest, "No volumes attached to container")
			return
		}
		volumeID = volumes[0]
	}

	// Validate volume access
	if err := h.validateVolumeAccess(containerID, volumeID); err != nil {
		h.writeError(w, http.StatusForbidden, fmt.Sprintf("Access denied: %s", err.Error()))
		return
	}

	reader, err := h.filesystemManager.StreamFile(volumeID, filePath)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, err.Error())
		return
	}
	defer reader.Close()

	// Set download headers
	filename := filepath.Base(filePath)
	w.Header().Set("Content-Disposition", fmt.Sprintf("attachment; filename=\"%s\"", filename))
	w.Header().Set("Content-Type", "application/octet-stream")

	if _, err := io.Copy(w, reader); err != nil {
		h.log.WithError(err).Error("Failed to stream file for download")
	}
}

// handleFSStream handles /api/fs/{containerID}/file/stream?file= - get HTTP stream of file live
func (h *Handler) handleFSStream(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	volumeID := r.URL.Query().Get("volume")
	filePath := r.URL.Query().Get("file")

	if filePath == "" {
		h.writeError(w, http.StatusBadRequest, "file parameter is required")
		return
	}

	// Security check: prevent path traversal
	if strings.Contains(filePath, "..") {
		h.writeError(w, http.StatusBadRequest, "Path traversal not allowed")
		return
	}

	// Get volume ID if not specified
	if volumeID == "" {
		volumes, err := h.containerManager.GetAttachedVolumes(containerID)
		if err != nil {
			h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
			return
		}
		if len(volumes) == 0 {
			h.writeError(w, http.StatusBadRequest, "No volumes attached to container")
			return
		}
		volumeID = volumes[0]
	}

	// Validate volume access
	if err := h.validateVolumeAccess(containerID, volumeID); err != nil {
		h.writeError(w, http.StatusForbidden, fmt.Sprintf("Access denied: %s", err.Error()))
		return
	}

	// Set headers for streaming
	w.Header().Set("Content-Type", "text/plain; charset=utf-8")
	w.Header().Set("Cache-Control", "no-cache")
	w.Header().Set("Connection", "keep-alive")

	// Stream file content (for log files, etc.)
	if err := h.filesystemManager.StreamFileContent(w, volumeID, filePath); err != nil {
		h.log.WithError(err).Error("Failed to stream file content")
		h.writeError(w, http.StatusInternalServerError, err.Error())
		return
	}
}

// handleFSSearch handles /api/fs/{containerID}/file/search?query=&path= - search files
func (h *Handler) handleFSSearch(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	volumeID := r.URL.Query().Get("volume")
	query := r.URL.Query().Get("query")
	searchPath := r.URL.Query().Get("path")

	if query == "" {
		h.writeError(w, http.StatusBadRequest, "query parameter is required")
		return
	}

	if searchPath == "" {
		searchPath = "/"
	}

	// Security check: prevent path traversal
	if strings.Contains(searchPath, "..") {
		h.writeError(w, http.StatusBadRequest, "Path traversal not allowed")
		return
	}

	// Get volume ID if not specified
	if volumeID == "" {
		volumes, err := h.containerManager.GetAttachedVolumes(containerID)
		if err != nil {
			h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
			return
		}
		if len(volumes) == 0 {
			h.writeError(w, http.StatusBadRequest, "No volumes attached to container")
			return
		}
		volumeID = volumes[0]
	}

	// Validate volume access
	if err := h.validateVolumeAccess(containerID, volumeID); err != nil {
		h.writeError(w, http.StatusForbidden, fmt.Sprintf("Access denied: %s", err.Error()))
		return
	}

	results, err := h.filesystemManager.SearchFilesInPath(volumeID, searchPath, query)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	h.writeSuccess(w, map[string]interface{}{
		"query":   query,
		"path":    searchPath,
		"results": results,
	})
}

// handleFSUpload handles /api/fs/{containerID}/file/upload?name=&path= - upload a file
func (h *Handler) handleFSUpload(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	volumeID := r.URL.Query().Get("volume")
	fileName := r.URL.Query().Get("name")
	uploadPath := r.URL.Query().Get("path")

	if fileName == "" {
		h.writeError(w, http.StatusBadRequest, "name parameter is required")
		return
	}

	if uploadPath == "" {
		uploadPath = "/"
	}

	// Security checks
	if strings.Contains(fileName, "..") || strings.Contains(fileName, "/") {
		h.writeError(w, http.StatusBadRequest, "Invalid file name")
		return
	}

	if strings.Contains(uploadPath, "..") {
		h.writeError(w, http.StatusBadRequest, "Path traversal not allowed")
		return
	}

	// Get volume ID if not specified
	if volumeID == "" {
		volumes, err := h.containerManager.GetAttachedVolumes(containerID)
		if err != nil {
			h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
			return
		}
		if len(volumes) == 0 {
			h.writeError(w, http.StatusBadRequest, "No volumes attached to container")
			return
		}
		volumeID = volumes[0]
	}

	// Validate volume access
	if err := h.validateVolumeAccess(containerID, volumeID); err != nil {
		h.writeError(w, http.StatusForbidden, fmt.Sprintf("Access denied: %s", err.Error()))
		return
	}

	// Parse multipart form
	if err := r.ParseMultipartForm(32 << 20); err != nil { // 32MB max
		h.writeError(w, http.StatusBadRequest, "Failed to parse multipart form")
		return
	}

	file, header, err := r.FormFile("file")
	if err != nil {
		h.writeError(w, http.StatusBadRequest, "File field required")
		return
	}
	defer file.Close()

	// Use provided name or header filename
	finalName := fileName
	if finalName == "" {
		finalName = header.Filename
	}

	// Construct full path
	fullPath := filepath.Join(uploadPath, finalName)

	// Read file content
	content, err := io.ReadAll(file)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, "Failed to read uploaded file")
		return
	}

	// Write file
	if err := h.filesystemManager.WriteFile(volumeID, fullPath, content); err != nil {
		h.writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	h.writeSuccess(w, map[string]interface{}{
		"message":  "File uploaded successfully",
		"filename": finalName,
		"path":     fullPath,
		"size":     len(content),
	})
}

// handleFSUploadPath handles /api/fs/{containerID}/upload?path= - upload to specific path
func (h *Handler) handleFSUploadPath(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	volumeID := r.URL.Query().Get("volume")
	uploadPath := r.URL.Query().Get("path")

	if uploadPath == "" {
		uploadPath = "/"
	}

	// Security check: prevent path traversal
	if strings.Contains(uploadPath, "..") {
		h.writeError(w, http.StatusBadRequest, "Path traversal not allowed")
		return
	}

	// Get volume ID if not specified
	if volumeID == "" {
		volumes, err := h.containerManager.GetAttachedVolumes(containerID)
		if err != nil {
			h.writeError(w, http.StatusNotFound, fmt.Sprintf("Container not found: %s", err.Error()))
			return
		}
		if len(volumes) == 0 {
			h.writeError(w, http.StatusBadRequest, "No volumes attached to container")
			return
		}
		volumeID = volumes[0]
	}

	// Validate volume access
	if err := h.validateVolumeAccess(containerID, volumeID); err != nil {
		h.writeError(w, http.StatusForbidden, fmt.Sprintf("Access denied: %s", err.Error()))
		return
	}

	// Parse multipart form
	if err := r.ParseMultipartForm(32 << 20); err != nil { // 32MB max
		h.writeError(w, http.StatusBadRequest, "Failed to parse multipart form")
		return
	}

	file, header, err := r.FormFile("file")
	if err != nil {
		h.writeError(w, http.StatusBadRequest, "File field required")
		return
	}
	defer file.Close()

	// Construct full path
	fullPath := filepath.Join(uploadPath, header.Filename)

	// Read file content
	content, err := io.ReadAll(file)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, "Failed to read uploaded file")
		return
	}

	// Write file
	if err := h.filesystemManager.WriteFile(volumeID, fullPath, content); err != nil {
		h.writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	h.writeSuccess(w, map[string]interface{}{
		"message":  "File uploaded successfully",
		"filename": header.Filename,
		"path":     fullPath,
		"size":     len(content),
	})
}

// Container management handlers

// handleContainerLock handles container locking/unlocking
func (h *Handler) handleContainerLock(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]
	action := r.URL.Query().Get("do")

	if action != "LOCK" && action != "UNLOCK" {
		h.writeError(w, http.StatusBadRequest, "Invalid action. Use 'LOCK' or 'UNLOCK'")
		return
	}

	switch action {
	case "LOCK":
		var req struct {
			Reason string `json:"reason"`
		}

		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			h.writeError(w, http.StatusBadRequest, "Invalid JSON")
			return
		}

		if req.Reason == "" {
			req.Reason = "Manually locked"
		}

		if err := h.containerManager.LockContainer(containerID, req.Reason); err != nil {
			h.writeError(w, http.StatusInternalServerError, err.Error())
			return
		}

		h.writeSuccess(w, map[string]interface{}{
			"message":     "Container locked successfully",
			"containerID": containerID,
			"reason":      req.Reason,
		})

	case "UNLOCK":
		if err := h.containerManager.UnlockContainer(containerID); err != nil {
			h.writeError(w, http.StatusInternalServerError, err.Error())
			return
		}

		h.writeSuccess(w, map[string]interface{}{
			"message":     "Container unlocked successfully",
			"containerID": containerID,
		})
	}
}

// handleContainerSnapshot handles snapshot operations
func (h *Handler) handleContainerSnapshot(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]

	switch r.Method {
	case "GET":
		// List snapshots
		snapshots, err := h.snapshotManager.ListSnapshots(containerID)
		if err != nil {
			h.writeError(w, http.StatusInternalServerError, err.Error())
			return
		}

		h.writeSuccess(w, map[string]interface{}{
			"containerID": containerID,
			"snapshots":   snapshots,
		})

	case "POST":
		// Create snapshot as a job
		var req struct {
			Name        string `json:"name"`
			Description string `json:"description"`
		}

		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			h.writeError(w, http.StatusBadRequest, "Invalid JSON")
			return
		}

		if req.Name == "" {
			req.Name = fmt.Sprintf("Snapshot_%d", time.Now().Unix())
		}

		// Create snapshot job instead of direct creation
		jobID, err := h.jobManager.CreateSnapshotJob(containerID, req.Name, req.Description)
		if err != nil {
			h.writeError(w, http.StatusInternalServerError, err.Error())
			return
		}

		h.writeSuccess(w, map[string]interface{}{
			"message":     "Snapshot creation job started",
			"jobId":       jobID,
			"containerID": containerID,
			"name":        req.Name,
		})
	}
}

// handleContainerRestore handles snapshot restoration as a job
func (h *Handler) handleContainerRestore(w http.ResponseWriter, r *http.Request) {
	vars := mux.Vars(r)
	containerID := vars["containerID"]

	var req struct {
		SnapshotID string `json:"snapshotId"`
	}

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		h.writeError(w, http.StatusBadRequest, "Invalid JSON")
		return
	}

	if req.SnapshotID == "" {
		h.writeError(w, http.StatusBadRequest, "snapshotId is required")
		return
	}

	// Create restore job instead of direct restore
	jobID, err := h.jobManager.CreateRestoreJob(containerID, req.SnapshotID)
	if err != nil {
		h.writeError(w, http.StatusInternalServerError, err.Error())
		return
	}

	h.writeSuccess(w, map[string]interface{}{
		"message":     "Container restore job started",
		"jobId":       jobID,
		"containerID": containerID,
		"snapshotID":  req.SnapshotID,
	})
}
