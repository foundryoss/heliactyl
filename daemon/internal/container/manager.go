package container

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	_ "strconv"
	"strings"
	"sync"
	"time"

	"github.com/docker/docker/api/types"
	"github.com/docker/go-connections/nat"
	"github.com/sirupsen/logrus"

	"NightLightd/internal/config"
	"NightLightd/internal/docker"
	"NightLightd/internal/volume"
)

// State represents container deployment state
type State string

const (
	StateInstalling State = "INSTALLING"
	StateReady      State = "READY"
	StateFailed     State = "FAILED"
	StateUnknown    State = "UNKNOWN"
)

// ContainerState represents the state of a container deployment
type ContainerState struct {
	State           State      `json:"state"`
	ContainerID     string     `json:"containerId,omitempty"`
	DiskLimit       int64      `json:"diskLimit,omitempty"`
	MemoryLimit     int64      `json:"memoryLimit,omitempty"`
	CPULimit        int64      `json:"cpuLimit,omitempty"`
	AttachedVolumes []string   `json:"attachedVolumes,omitempty"`
	Locked          bool       `json:"locked,omitempty"`
	LockReason      string     `json:"lockReason,omitempty"`
	LockedAt        *time.Time `json:"lockedAt,omitempty"`
	// Freeze state
	Frozen        bool       `json:"frozen,omitempty"`
	FreezeMessage string     `json:"freezeMessage,omitempty"`
	FrozenAt      *time.Time `json:"frozenAt,omitempty"`

	// Port mappings: containerPort -> hostPort
	Ports map[string]string `json:"ports,omitempty"`

	// Environment variables (as a map)
	Env map[string]string `json:"env,omitempty"`

	// Resource Units (RU) accounting
	RUCumulative float64 `json:"ru_cumulative,omitempty"`
	RULimit      float64 `json:"ru_limit,omitempty"`

	// LastSample stores the last sampled metrics (not a time series)
	LastSample struct {
		Timestamp   *time.Time `json:"timestamp,omitempty"`
		MemoryUsed  uint64     `json:"memory_used,omitempty"`
		CPUPercent  float64    `json:"cpu_percent,omitempty"`
		VolumeSizeM float64    `json:"volume_size_mib,omitempty"`
	} `json:"last_sample,omitempty"`
}

// CreateRequest represents a container creation request (alias for DeploymentRequest)
type CreateRequest = DeploymentRequest

// DeploymentRequest represents a container deployment request
type DeploymentRequest struct {
	ID           string                 `json:"Id"`
	Image        string                 `json:"Image"`
	Cmd          []string               `json:"Cmd,omitempty"`
	Env          []string               `json:"Env,omitempty"`
	Hostname     string                 `json:"Hostname,omitempty"`
	IP           string                 `json:"IP,omitempty"`
	User         string                 `json:"User,omitempty"`
	Ports        nat.PortSet            `json:"Ports,omitempty"`
	PortBindings nat.PortMap            `json:"PortBindings,omitempty"`
	Memory       int64                  `json:"Memory,omitempty"` // in MB
	CPU          int64                  `json:"Cpu,omitempty"`
	Disk         int64                  `json:"Disk,omitempty"` // in MB
	Scripts      *DeploymentScripts     `json:"Scripts,omitempty"`
	Variables    map[string]interface{} `json:"variables,omitempty"`
	// RU limit in RUs (GB-hours). Must be > 0.
	RULimit float64 `json:"ru_limit,omitempty"`
}

// DeploymentScripts represents installation scripts
type DeploymentScripts struct {
	Install []InstallScript `json:"Install,omitempty"`
}

// InstallScript represents a script to download and execute
type InstallScript struct {
	Path string `json:"Path"`
	URI  string `json:"Uri"`
}

// Manager handles container lifecycle management
type Manager struct {
	config        *config.Config
	dockerClient  *docker.Client
	volumeManager *volume.Manager
	log           *logrus.Logger
	states        map[string]*ContainerState
	statesMu      sync.RWMutex
	statesFile    string
}

// GetDockerClient returns the Docker client (for internal use by API handlers)
func (m *Manager) GetDockerClient() *docker.Client {
	return m.dockerClient
}

// NewManager creates a new container manager
func NewManager(cfg *config.Config, dockerClient *docker.Client, volumeManager *volume.Manager, log *logrus.Logger) *Manager {
	statesFile := filepath.Join(cfg.StoragePath, "states.json")

	manager := &Manager{
		config:        cfg,
		dockerClient:  dockerClient,
		volumeManager: volumeManager,
		log:           log,
		states:        make(map[string]*ContainerState),
		statesFile:    statesFile,
	}

	// Load existing states
	if err := manager.loadStates(); err != nil {
		log.WithError(err).Warn("Failed to load container states")
	}

	return manager
}

// Deploy creates and starts a new container
func (m *Manager) Deploy(ctx context.Context, req *DeploymentRequest) error {
	m.log.WithFields(logrus.Fields{
		"volumeId": req.ID,
		"image":    req.Image,
	}).Info("Starting container deployment")

	// RU limit must be present and positive
	if req.RULimit <= 0 {
		return fmt.Errorf("ru_limit must be provided and greater than zero")
	}

	// Set initial state
	if err := m.updateState(req.ID, StateInstalling, "", req.Disk, req.Memory, req.CPU, []string{req.ID}); err != nil {
		return fmt.Errorf("failed to update state: %w", err)
	}

	// Create volume directory - ONLY place where volumes are created
	volumePath := filepath.Join(m.config.VolumesPath, req.ID)
	// Ensure the path is absolute
	if !filepath.IsAbs(volumePath) {
		cwd, err := os.Getwd()
		if err != nil {
			m.updateState(req.ID, StateFailed, "", req.Disk, req.Memory, req.CPU, []string{req.ID})
			return fmt.Errorf("failed to get current directory: %w", err)
		}
		volumePath = filepath.Join(cwd, volumePath)
	}

	// Create volume directory (following original JS pattern: line 158-159 in Deployment.js)
	if err := os.MkdirAll(volumePath, 0755); err != nil {
		m.updateState(req.ID, StateFailed, "", req.Disk, req.Memory, req.CPU, []string{req.ID})
		return fmt.Errorf("failed to create volume directory: %w", err)
	}

	m.log.WithFields(logrus.Fields{
		"volumeId":   req.ID,
		"volumePath": volumePath,
	}).Info("Volume directory created and will be attached to container")

	// Pull image
	if err := m.dockerClient.PullImage(ctx, req.Image); err != nil {
		m.updateState(req.ID, StateFailed, "", req.Disk, req.Memory, req.CPU, []string{req.ID})
		return fmt.Errorf("failed to pull image: %w", err)
	}

	// Prepare environment variables
	env := req.Env
	if req.Variables != nil {
		for key, value := range req.Variables {
			env = append(env, fmt.Sprintf("%s=%v", key, value))
		}
	}

	// Add primary port to environment
	if len(req.PortBindings) > 0 {
		for _, bindings := range req.PortBindings {
			if len(bindings) > 0 {
				env = append(env, fmt.Sprintf("PRIMARY_PORT=%s", bindings[0].HostPort))
				break
			}
		}
	}

	// Create container configuration
	containerConfig := &docker.ContainerConfig{
		Name:         req.ID, // Container name MUST match volume ID
		Image:        req.Image,
		Cmd:          req.Cmd,
		Env:          env,
		ExposedPorts: req.Ports,
		PortBindings: req.PortBindings,
		Memory:       req.Memory * 1024 * 1024, // Convert MB to bytes
		CPUCount:     req.CPU,
		VolumePath:   volumePath,
		WorkingDir:   "/app",
		NetworkMode:  "bridge",
	}

	// Create container (with short timeout to avoid long hangs)
	ctxCreate, cancelCreate := context.WithTimeout(ctx, 30*time.Second)
	defer cancelCreate()

	container, err := m.dockerClient.CreateContainer(ctxCreate, containerConfig)
	if err != nil {
		m.updateState(req.ID, StateFailed, "", req.Disk, req.Memory, req.CPU, []string{req.ID})
		m.log.WithError(err).Error("CreateContainer failed")
		return fmt.Errorf("failed to create container: %w", err)
	}
	m.log.WithFields(logrus.Fields{"containerId": container.ID, "name": req.ID}).Info("Container created (docker responded)")

	// Ensure install.sh exists in the volume (can be empty). If user provided install content in Variables.install_content, write it.
	installPath := filepath.Join(volumePath, "install.sh")
	if contentRaw, ok := req.Variables["install_content"]; ok {
		if s, ok := contentRaw.(string); ok && s != "" {
			if err := os.WriteFile(installPath, []byte(s), 0755); err != nil {
				m.log.WithError(err).Warn("Failed to write install.sh from variables")
			} else {
				m.log.WithField("installPath", installPath).Info("Wrote install.sh from variables")
			}
		} else {
			// ensure file exists
			if _, err := os.Stat(installPath); os.IsNotExist(err) {
				os.WriteFile(installPath, []byte(""), 0755)
			}
		}
	} else {
		if _, err := os.Stat(installPath); os.IsNotExist(err) {
			os.WriteFile(installPath, []byte(""), 0755)
		}
	}

	// Process installation scripts if provided (legacy script entries)
	if req.Scripts != nil && len(req.Scripts.Install) > 0 {
		if err := m.processInstallScripts(req.Scripts.Install, volumePath, req.Variables); err != nil {
			m.log.WithError(err).Warn("Failed to process install scripts")
		}
	}

	// Start container (with short timeout)
	m.log.WithField("containerId", container.ID[:12]).Info("Starting container")
	ctxStart, cancelStart := context.WithTimeout(ctx, 30*time.Second)
	defer cancelStart()
	if err := container.Start(ctxStart); err != nil {
		m.updateState(req.ID, StateFailed, container.ID, req.Disk, req.Memory, req.CPU, []string{req.ID})
		m.log.WithError(err).Error("Container start failed")
		return fmt.Errorf("failed to start container: %w", err)
	}
	m.log.WithField("containerId", container.ID[:12]).Info("Container started")

	// Run install.sh inside the container (non-blocking but with timeout)
	go func() {
		ctxExec, cancel := context.WithTimeout(context.Background(), 60*time.Second)
		defer cancel()

		cmd := types.ExecConfig{
			AttachStdout: true,
			AttachStderr: true,
			AttachStdin:  false,
			Cmd:          []string{"/bin/sh", "-c", "chmod +x /app/data/install.sh && /app/data/install.sh"},
		}
		// Set user for exec if provided
		if req.User != "" {
			cmd.User = req.User
		}

		execResp, err := m.dockerClient.ContainerExecCreate(ctxExec, container.ID, cmd)
		if err != nil {
			m.log.WithError(err).Warn("Failed to create exec for install.sh")
			return
		}

		hijacked, err := m.dockerClient.ContainerExecAttach(ctxExec, execResp.ID, types.ExecStartCheck{Tty: false})
		if err != nil {
			m.log.WithError(err).Warn("Failed to attach to install.sh exec")
			return
		}
		defer hijacked.Close()

		// Read output (and log)
		buf := make([]byte, 1024)
		for {
			n, err := hijacked.Reader.Read(buf)
			if n > 0 {
				m.log.WithField("install_output", string(buf[:n])).Info("install.sh output")
			}
			if err != nil {
				if err != io.EOF {
					m.log.WithError(err).Warn("Error reading install.sh output")
				}
				break
			}
		}
	}()

	// Update state to ready
	if err := m.updateState(req.ID, StateReady, container.ID, req.Disk, req.Memory, req.CPU, []string{req.ID}); err != nil {
		m.log.WithError(err).Warn("Failed to update state to ready")
	} else {
		// Inspect container to capture ports and env for state persistence
		if info, err := container.Inspect(ctx); err == nil {
			portsMap := make(map[string]string)
			for p, bindings := range info.NetworkSettings.Ports {
				cp := string(p)
				if len(bindings) > 0 {
					portsMap[cp] = bindings[0].HostPort
				} else {
					portsMap[cp] = ""
				}
			}

			envMap := make(map[string]string)
			for _, e := range info.Config.Env {
				parts := strings.SplitN(e, "=", 2)
				if len(parts) == 2 {
					envMap[parts[0]] = parts[1]
				} else {
					envMap[parts[0]] = ""
				}
			}

			m.statesMu.Lock()
			if st, exists := m.states[req.ID]; exists {
				st.Ports = portsMap
				st.Env = envMap
				// Ensure RU limit is set on the state
				if req.RULimit > 0 {
					st.RULimit = req.RULimit
				}
			}
			m.statesMu.Unlock()
			m.saveStatesAsync()
		}
	}

	m.log.WithFields(logrus.Fields{
		"volumeId":    req.ID,
		"containerId": container.ID[:12],
	}).Info("Container deployment completed successfully")

	return nil
}

// RemoveContainer removes a container and optionally its associated volume
func (m *Manager) RemoveContainer(ctx context.Context, containerID string, removeVolume bool) error {
	container := m.dockerClient.GetContainer(containerID)

	// Get container info to find volume ID
	containerInfo, err := container.Inspect(ctx)
	if err != nil {
		return fmt.Errorf("failed to inspect container: %w", err)
	}

	volumeID := strings.TrimPrefix(containerInfo.Name, "/")

	m.log.WithFields(logrus.Fields{
		"containerId":  containerID[:12],
		"volumeId":     volumeID,
		"removeVolume": removeVolume,
	}).Info("Deleting container")

	// Stop container if running
	if containerInfo.State.Running {
		if err := container.Stop(ctx); err != nil {
			m.log.WithError(err).Warn("Failed to stop container")
		}
	}

	// Remove container
	if err := container.Remove(ctx, true); err != nil {
		return fmt.Errorf("failed to remove container: %w", err)
	}

	// Remove volume if requested
	if removeVolume {
		volumePath := filepath.Join(m.config.VolumesPath, volumeID)
		// Ensure the path is absolute
		if !filepath.IsAbs(volumePath) {
			cwd, err := os.Getwd()
			if err == nil {
				volumePath = filepath.Join(cwd, volumePath)
			}
		}

		if err := os.RemoveAll(volumePath); err != nil {
			m.log.WithError(err).Warn("Failed to remove volume directory")
		}
	}

	// Remove state
	m.statesMu.Lock()
	delete(m.states, volumeID)
	m.statesMu.Unlock()

	if err := m.saveStates(); err != nil {
		m.log.WithError(err).Warn("Failed to save states after deletion")
	}

	m.log.WithField("containerId", containerID[:12]).Info("Container deleted successfully")
	return nil
}

// Delete removes a container and its associated volume (legacy convenience wrapper)
func (m *Manager) Delete(ctx context.Context, containerID string) error {
	return m.RemoveContainer(ctx, containerID, true)
}

// SyncWithDocker synchronizes container states with actual Docker containers
// ONLY for containers that were created by NightLightd (have corresponding volume directories)
func (m *Manager) SyncWithDocker(ctx context.Context) error {
	m.log.Info("Synchronizing container states with Docker (NightLightd containers only)")

	// Get all Docker containers
	containers, err := m.dockerClient.ListContainers(ctx)
	if err != nil {
		return fmt.Errorf("failed to list Docker containers: %w", err)
	}

	m.statesMu.Lock()
	defer m.statesMu.Unlock()

	syncCount := 0
	for _, container := range containers {
		// Extract volume ID from container name (remove leading slash)
		volumeID := strings.TrimPrefix(container.Names[0], "/")

		// CRITICAL: Only sync containers that have a corresponding volume directory
		// This ensures we only manage containers created by NightLightd
		volumePath := filepath.Join(m.config.VolumesPath, volumeID)
		if !filepath.IsAbs(volumePath) {
			if cwd, err := os.Getwd(); err == nil {
				volumePath = filepath.Join(cwd, volumePath)
			}
		}

		// Check if volume directory exists - if not, this container wasn't created by us
		if _, err := os.Stat(volumePath); os.IsNotExist(err) {
			// Skip containers that don't have corresponding volume directories
			m.log.WithFields(logrus.Fields{
				"containerID": container.ID,
				"volumeId":    volumeID,
			}).Debug("Skipping external container during sync - no volume directory found")
			continue
		}

		// Check if we have state for this container
		if state, exists := m.states[volumeID]; exists {
			// Update container ID if it changed
			if state.ContainerID != container.ID {
				m.log.WithFields(logrus.Fields{
					"volumeId":       volumeID,
					"oldContainerID": state.ContainerID,
					"newContainerID": container.ID,
				}).Info("Updated container ID from Docker sync")
				state.ContainerID = container.ID
				syncCount++
			}
		} else {
			// Create new state for NightLightd container that we don't have state for
			m.log.WithFields(logrus.Fields{
				"volumeId":    volumeID,
				"containerID": container.ID,
			}).Info("Found NightLightd container during Docker sync")

			m.states[volumeID] = &ContainerState{
				State:           StateReady, // Assume ready if running in Docker
				ContainerID:     container.ID,
				AttachedVolumes: []string{volumeID},
			}
			syncCount++
		}
	}

	if syncCount > 0 {
		m.log.WithField("syncCount", syncCount).Info("Docker sync completed, saving updated states")
		// Unlock before saving to avoid deadlock
		m.statesMu.Unlock()
		if err := m.saveStates(); err != nil {
			m.log.WithError(err).Warn("Failed to save synced states")
		}
		m.statesMu.Lock() // Re-lock for defer
	} else {
		m.log.Info("Docker sync completed - no NightLightd containers found")
	}

	return nil
}

// GetAttachedVolumes returns the volumes attached to a container
// In NightLightd, container name = volume ID, so we extract the name from the container
func (m *Manager) GetAttachedVolumes(containerID string) ([]string, error) {
	// Get container info to extract the name
	ctx := context.Background()
	container := m.dockerClient.GetContainer(containerID)

	containerInfo, err := container.Inspect(ctx)
	if err != nil {
		return nil, fmt.Errorf("container not found: %s", containerID)
	}

	// Extract container name (remove leading slash)
	containerName := strings.TrimPrefix(containerInfo.Name, "/")

	// In NightLightd, container name = volume ID
	// Check if we have state for this volume
	m.statesMu.RLock()
	defer m.statesMu.RUnlock()

	if _, exists := m.states[containerName]; exists {
		return []string{containerName}, nil
	}

	return nil, fmt.Errorf("no volume found for container %s (name: %s)", containerID, containerName)
}

// GetEnv returns a map of environment variables for the given container
func (m *Manager) GetEnv(ctx context.Context, containerID string) (map[string]string, error) {
	container := m.dockerClient.GetContainer(containerID)

	info, err := container.Inspect(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to inspect container: %w", err)
	}

	envMap := make(map[string]string)
	for _, e := range info.Config.Env {
		parts := strings.SplitN(e, "=", 2)
		if len(parts) == 2 {
			envMap[parts[0]] = parts[1]
		} else {
			envMap[parts[0]] = ""
		}
	}

	return envMap, nil
}

// UpdateEnv updates environment variables for a container by recreating it with the new env
// updates: map of variable name -> pointer to string; nil value means delete the variable
func (m *Manager) UpdateEnv(ctx context.Context, containerID string, updates map[string]*string) error {
	container := m.dockerClient.GetContainer(containerID)

	info, err := container.Inspect(ctx)
	if err != nil {
		return fmt.Errorf("failed to inspect container: %w", err)
	}

	volumeID := strings.TrimPrefix(info.Name, "/")

	// Check lock and frozen state
	m.statesMu.RLock()
	state, exists := m.states[volumeID]
	m.statesMu.RUnlock()
	if !exists {
		return fmt.Errorf("container state not found: %s", volumeID)
	}
	if state.Locked {
		return fmt.Errorf("container is locked: %s", state.LockReason)
	}
	if state.Frozen {
		return fmt.Errorf("container is frozen: %s", state.FreezeMessage)
	}

	// Build current env map
	envMap := make(map[string]string)
	for _, e := range info.Config.Env {
		parts := strings.SplitN(e, "=", 2)
		if len(parts) == 2 {
			envMap[parts[0]] = parts[1]
		} else {
			envMap[parts[0]] = ""
		}
	}

	// Apply updates
	for k, v := range updates {
		if v == nil {
			delete(envMap, k)
		} else {
			envMap[k] = *v
		}
	}

	// Convert back to list
	newEnv := make([]string, 0, len(envMap))
	for k, v := range envMap {
		newEnv = append(newEnv, fmt.Sprintf("%s=%s", k, v))
	}

	// Build new container config based on inspected container
	hostConfig := info.HostConfig
	dockerConfig := &docker.ContainerConfig{
		Name:         volumeID,
		Image:        info.Config.Image,
		Cmd:          info.Config.Cmd,
		Env:          newEnv,
		Hostname:     info.Config.Hostname,
		User:         info.Config.User,
		ExposedPorts: info.Config.ExposedPorts,
		PortBindings: hostConfig.PortBindings,
		Memory:       hostConfig.Memory,
		WorkingDir:   info.Config.WorkingDir,
		NetworkMode:  string(hostConfig.NetworkMode),
	}

	// CPU conversion
	if hostConfig.NanoCPUs > 0 {
		dockerConfig.CPUCount = hostConfig.NanoCPUs / 1e9
	}

	// Determine volume path
	volumePath := filepath.Join(m.config.VolumesPath, volumeID)
	if !filepath.IsAbs(volumePath) {
		cwd, err := os.Getwd()
		if err == nil {
			volumePath = filepath.Join(cwd, volumePath)
		}
	}
	dockerConfig.VolumePath = volumePath

	// Stop container if running
	if info.State.Running {
		if err := container.Stop(ctx); err != nil {
			m.log.WithError(err).Warn("Failed to stop container before updating env")
		}
	}

	// Remove old container
	if err := container.Remove(ctx, true); err != nil {
		m.log.WithError(err).Warn("Failed to remove old container during env update")
	}

	// Create new container
	newContainer, err := m.dockerClient.CreateContainer(ctx, dockerConfig)
	if err != nil {
		return fmt.Errorf("failed to create new container: %w", err)
	}

	// Start new container
	if err := newContainer.Start(ctx); err != nil {
		return fmt.Errorf("failed to start new container: %w", err)
	}

	// Update state with new container ID
	m.statesMu.Lock()
	state.ContainerID = newContainer.ID
	m.statesMu.Unlock()

	// Inspect new container to update ports and env in state
	if infoNew, err := newContainer.Inspect(ctx); err == nil {
		portsMap := make(map[string]string)
		for p, bindings := range infoNew.NetworkSettings.Ports {
			cp := string(p)
			if len(bindings) > 0 {
				portsMap[cp] = bindings[0].HostPort
			} else {
				portsMap[cp] = ""
			}
		}

		envMap := make(map[string]string)
		for _, e := range infoNew.Config.Env {
			parts := strings.SplitN(e, "=", 2)
			if len(parts) == 2 {
				envMap[parts[0]] = parts[1]
			} else {
				envMap[parts[0]] = ""
			}
		}

		// Update state safely and persist async
		m.statesMu.Lock()
		state.Ports = portsMap
		state.Env = envMap
		m.statesMu.Unlock()
		m.saveStatesAsync()
	}

	m.log.WithFields(logrus.Fields{
		"volumeId":    volumeID,
		"containerId": newContainer.ID[:12],
	}).Info("Updated container environment and recreated container")

	return nil
}

// GetContainerByVolumeID returns container ID for a given volume ID
func (m *Manager) GetContainerByVolumeID(volumeID string) (string, error) {
	m.statesMu.RLock()
	defer m.statesMu.RUnlock()

	if state, exists := m.states[volumeID]; exists {
		return state.ContainerID, nil
	}

	return "", fmt.Errorf("volume not found: %s", volumeID)
}

// GetVolumeIDByContainerID returns a volume ID for a given container ID if known
func (m *Manager) GetVolumeIDByContainerID(containerID string) (string, error) {
	// Fast path: search in in-memory states
	m.statesMu.RLock()
	for vid, state := range m.states {
		if state.ContainerID == containerID {
			m.statesMu.RUnlock()
			return vid, nil
		}
	}
	m.statesMu.RUnlock()

	// Fallback: inspect container to derive name
	ctx := context.Background()
	container := m.dockerClient.GetContainer(containerID)
	info, err := container.Inspect(ctx)
	if err != nil {
		return "", fmt.Errorf("container not found: %s", containerID)
	}

	volumeID := strings.TrimPrefix(info.Name, "/")
	return volumeID, nil
}

// GetState returns the state of a container by volume ID
func (m *Manager) GetState(volumeID string) *ContainerState {
	m.statesMu.RLock()
	defer m.statesMu.RUnlock()

	if state, exists := m.states[volumeID]; exists {
		return state
	}

	return &ContainerState{State: StateUnknown}
}

// StoragePath returns the storage path from the configuration
func (m *Manager) StoragePath() string {
	return m.config.StoragePath
}

// ListVolumeIDs returns a slice of known volume IDs
func (m *Manager) ListVolumeIDs() []string {
	m.statesMu.RLock()
	defer m.statesMu.RUnlock()

	ids := make([]string, 0, len(m.states))
	for id := range m.states {
		ids = append(ids, id)
	}
	return ids
}

// GetVolumeSize returns the size of a volume in MiB
func (m *Manager) GetVolumeSize(volumeID string) (float64, error) {
	return m.volumeManager.GetVolumeSize(volumeID)
}

// GetAllStates returns a copy of all container states (used by external systems)
func (m *Manager) GetAllStates() map[string]*ContainerState {
	m.statesMu.RLock()
	defer m.statesMu.RUnlock()

	copyMap := make(map[string]*ContainerState, len(m.states))
	for k, v := range m.states {
		copyMap[k] = v
	}
	return copyMap
}

// AddRU adds `delta` resource units to the container's cumulative RU and updates the last sample.
// It also checks the container's RU limit and freezes the container if the limit is reached.
func (m *Manager) AddRU(containerID string, delta float64, sampleTimestamp time.Time, memoryUsed uint64, cpuPercent float64) error {
	// Resolve volume ID (fast path)
	m.statesMu.RLock()
	for vid, state := range m.states {
		if state.ContainerID == containerID {
			m.statesMu.RUnlock()
			m.statesMu.Lock()
			defer m.statesMu.Unlock()

			// Update RU and last sample
			state.RUCumulative += delta
			state.LastSample.Timestamp = &sampleTimestamp
			state.LastSample.MemoryUsed = memoryUsed
			state.LastSample.CPUPercent = cpuPercent

			// Update volume size if possible
			if vs, err := m.volumeManager.GetVolumeSize(vid); err == nil {
				state.LastSample.VolumeSizeM = vs
			}

			// Persist
			m.saveStatesAsync()

			// Freeze if limit reached
			if state.RULimit > 0 && state.RUCumulative >= state.RULimit {
				// Try to freeze (best effort)
				m.log.WithFields(logrus.Fields{"containerID": containerID, "volumeID": vid, "ru": state.RUCumulative, "limit": state.RULimit}).Info("RU limit reached, freezing container")
				// call FreezeContainer (which will persist state)
				if err := m.FreezeContainer(containerID, "RU_FREEZE: LIMIT REACHED"); err != nil {
					m.log.WithError(err).Warn("Failed to freeze container after RU limit reached")
				}
			}

			return nil
		}
	}
	m.statesMu.RUnlock()

	// Fallback: inspect container to find volume ID
	ctx := context.Background()
	container := m.dockerClient.GetContainer(containerID)
	info, err := container.Inspect(ctx)
	if err != nil {
		return fmt.Errorf("container not found: %s", containerID)
	}

	volumeID := strings.TrimPrefix(info.Name, "/")

	m.statesMu.Lock()
	defer m.statesMu.Unlock()

	state, exists := m.states[volumeID]
	if !exists {
		return fmt.Errorf("container state not found: %s", volumeID)
	}

	state.RUCumulative += delta
	state.LastSample.Timestamp = &sampleTimestamp
	state.LastSample.MemoryUsed = memoryUsed
	state.LastSample.CPUPercent = cpuPercent

	if vs, err := m.volumeManager.GetVolumeSize(volumeID); err == nil {
		state.LastSample.VolumeSizeM = vs
	}

	m.saveStatesAsync()

	if state.RULimit > 0 && state.RUCumulative >= state.RULimit {
		m.log.WithFields(logrus.Fields{"containerID": containerID, "volumeID": volumeID, "ru": state.RUCumulative, "limit": state.RULimit}).Info("RU limit reached, freezing container")
		if err := m.FreezeContainer(containerID, "RU_FREEZE: LIMIT REACHED"); err != nil {
			m.log.WithError(err).Warn("Failed to freeze container after RU limit reached")
		}
	}

	return nil
}

// updateState updates the state of a container
func (m *Manager) updateState(volumeID string, state State, containerID string, diskLimit int64, memoryLimit int64, cpuLimit int64, attachedVolumes []string) error {
	m.statesMu.Lock()
	m.states[volumeID] = &ContainerState{
		State:           state,
		ContainerID:     containerID,
		DiskLimit:       diskLimit,
		MemoryLimit:     memoryLimit,
		CPULimit:        cpuLimit,
		AttachedVolumes: attachedVolumes,
	}
	m.statesMu.Unlock()

	return m.saveStates()
}

// loadStates loads container states from file
func (m *Manager) loadStates() error {
	if _, err := os.Stat(m.statesFile); os.IsNotExist(err) {
		return nil // File doesn't exist, start with empty states
	}

	data, err := os.ReadFile(m.statesFile)
	if err != nil {
		return fmt.Errorf("failed to read states file: %w", err)
	}

	m.statesMu.Lock()
	defer m.statesMu.Unlock()

	if err := json.Unmarshal(data, &m.states); err != nil {
		return fmt.Errorf("failed to unmarshal states: %w", err)
	}

	// Migration: ensure all states have the new fields
	migrationNeeded := false
	for volumeID, state := range m.states {
		if state.AttachedVolumes == nil {
			// Migrate old state: assume volume ID matches the container's volume
			state.AttachedVolumes = []string{volumeID}
			migrationNeeded = true
			m.log.WithField("volumeId", volumeID).Info("Migrated container state to include attached volumes")
		}
	}

	// Save migrated states back to file
	if migrationNeeded {
		m.log.Info("Container states migrated, saving updated states")
		// Unlock before calling saveStates to avoid deadlock
		m.statesMu.Unlock()
		if err := m.saveStates(); err != nil {
			m.log.WithError(err).Warn("Failed to save migrated states")
		}
		m.statesMu.Lock() // Re-lock for defer
	}

	return nil
}

// saveStates saves container states to file
func (m *Manager) saveStates() error {
	m.statesMu.RLock()
	data, err := json.MarshalIndent(m.states, "", "  ")
	m.statesMu.RUnlock()

	if err != nil {
		return fmt.Errorf("failed to marshal states: %w", err)
	}

	if err := os.WriteFile(m.statesFile, data, 0644); err != nil {
		return fmt.Errorf("failed to write states file: %w", err)
	}

	return nil
}

// processInstallScripts downloads and processes installation scripts
func (m *Manager) processInstallScripts(scripts []InstallScript, volumePath string, variables map[string]interface{}) error {
	// This is a simplified implementation
	// In a full implementation, you would download scripts from URLs and process them
	m.log.WithField("scriptsCount", len(scripts)).Info("Processing install scripts")

	// For now, just log that we would process scripts
	for _, script := range scripts {
		m.log.WithFields(logrus.Fields{
			"path": script.Path,
			"uri":  script.URI,
		}).Debug("Would process install script")
	}

	return nil
}

// CheckStorageLimit checks if a container's storage exceeds its limit
func (m *Manager) CheckStorageLimit(ctx context.Context, containerID string) (bool, error) {
	container := m.dockerClient.GetContainer(containerID)

	// Get container info to find volume ID
	containerInfo, err := container.Inspect(ctx)
	if err != nil {
		return false, fmt.Errorf("failed to inspect container: %w", err)
	}

	volumeID := strings.TrimPrefix(containerInfo.Name, "/")
	state := m.GetState(volumeID)

	if state.DiskLimit <= 0 {
		return false, nil // No limit set
	}

	// Get current volume size
	volumeSize, err := m.volumeManager.GetVolumeSize(volumeID)
	if err != nil {
		return false, fmt.Errorf("failed to get volume size: %w", err)
	}

	// Check if size exceeds limit (volumeSize is in MiB, diskLimit is in MB)
	return volumeSize >= float64(state.DiskLimit), nil
}

// saveStatesAsync saves states in a background goroutine to avoid blocking callers
func (m *Manager) saveStatesAsync() {
	go func() {
		if err := m.saveStates(); err != nil {
			m.log.WithError(err).Warn("Failed to save states (async)")
		}
	}()
}

// LockContainer locks a container with a custom reason
func (m *Manager) LockContainer(containerID, reason string) error {
	// Fast path: treat containerID as volumeID if state exists
	m.statesMu.RLock()
	if state, exists := m.states[containerID]; exists {
		m.statesMu.RUnlock()
		m.statesMu.Lock()
		now := time.Now()
		state.Locked = true
		state.LockReason = reason
		state.LockedAt = &now
		m.statesMu.Unlock()

		// Save asynchronously
		m.saveStatesAsync()

		m.log.WithFields(logrus.Fields{
			"containerID": containerID,
			"volumeID":    containerID,
			"reason":      reason,
		}).Info("Container locked (fast path)")

		return nil
	}
	m.statesMu.RUnlock()

	// Fallback: inspect container to find volume ID
	ctx := context.Background()
	container := m.dockerClient.GetContainer(containerID)

	containerInfo, err := container.Inspect(ctx)
	if err != nil {
		return fmt.Errorf("container not found: %s", containerID)
	}

	volumeID := strings.TrimPrefix(containerInfo.Name, "/")

	m.statesMu.Lock()
	defer m.statesMu.Unlock()

	state, exists := m.states[volumeID]
	if !exists {
		return fmt.Errorf("container state not found: %s", volumeID)
	}

	now := time.Now()
	state.Locked = true
	state.LockReason = reason
	state.LockedAt = &now

	// Save asynchronously
	m.saveStatesAsync()

	m.log.WithFields(logrus.Fields{
		"containerID": containerID,
		"volumeID":    volumeID,
		"reason":      reason,
	}).Info("Container locked")

	return nil
}

// UnlockContainer unlocks a container
func (m *Manager) UnlockContainer(containerID string) error {
	// Fast path: treat containerID as volumeID if state exists
	m.statesMu.RLock()
	if state, exists := m.states[containerID]; exists {
		m.statesMu.RUnlock()
		m.statesMu.Lock()
		state.Locked = false
		state.LockReason = ""
		state.LockedAt = nil
		m.statesMu.Unlock()

		// Save asynchronously
		m.saveStatesAsync()

		m.log.WithFields(logrus.Fields{
			"containerID": containerID,
			"volumeID":    containerID,
		}).Info("Container unlocked (fast path)")

		return nil
	}
	m.statesMu.RUnlock()

	// Fallback: inspect container to find volume ID
	ctx := context.Background()
	container := m.dockerClient.GetContainer(containerID)

	containerInfo, err := container.Inspect(ctx)
	if err != nil {
		return fmt.Errorf("container not found: %s", containerID)
	}

	volumeID := strings.TrimPrefix(containerInfo.Name, "/")

	m.statesMu.Lock()
	defer m.statesMu.Unlock()

	state, exists := m.states[volumeID]
	if !exists {
		return fmt.Errorf("container state not found: %s", volumeID)
	}

	state.Locked = false
	state.LockReason = ""
	state.LockedAt = nil

	// Save asynchronously
	m.saveStatesAsync()

	m.log.WithFields(logrus.Fields{
		"containerID": containerID,
		"volumeID":    volumeID,
	}).Info("Container unlocked")

	return nil
}

// IsContainerLocked checks if a container is locked
func (m *Manager) IsContainerLocked(containerID string) (bool, string, error) {
	// Fast path: check if containerID is a volumeID and exists in states
	m.statesMu.RLock()
	if state, exists := m.states[containerID]; exists {
		m.statesMu.RUnlock()
		return state.Locked, state.LockReason, nil
	}
	m.statesMu.RUnlock()

	// Fallback: inspect container to find volume ID
	ctx := context.Background()
	container := m.dockerClient.GetContainer(containerID)

	containerInfo, err := container.Inspect(ctx)
	if err != nil {
		return false, "", fmt.Errorf("container not found: %s", containerID)
	}

	volumeID := strings.TrimPrefix(containerInfo.Name, "/")

	m.statesMu.RLock()
	defer m.statesMu.RUnlock()

	state, exists := m.states[volumeID]
	if !exists {
		return false, "", fmt.Errorf("container state not found: %s", volumeID)
	}

	return state.Locked, state.LockReason, nil
}

// FreezeContainer freezes a container immediately with a message and stops it
func (m *Manager) FreezeContainer(containerID, message string) error {
	// Fallback to find volumeID and update state
	ctx := context.Background()
	container := m.dockerClient.GetContainer(containerID)

	containerInfo, err := container.Inspect(ctx)
	if err != nil {
		return fmt.Errorf("container not found: %s", containerID)
	}

	volumeID := strings.TrimPrefix(containerInfo.Name, "/")

	m.statesMu.Lock()
	state, exists := m.states[volumeID]
	if !exists {
		m.statesMu.Unlock()
		return fmt.Errorf("container state not found: %s", volumeID)
	}

	now := time.Now()
	state.Frozen = true
	state.FreezeMessage = message
	state.FrozenAt = &now
	m.statesMu.Unlock()

	// Persist state asynchronously
	m.saveStatesAsync()

	// Stop the container immediately (force kill)
	if err := container.Kill(ctx); err != nil {
		m.log.WithError(err).Warn("Failed to kill container during freeze")
	}

	m.log.WithFields(logrus.Fields{
		"containerID": containerID,
		"volumeID":    volumeID,
		"message":     message,
	}).Info("Container frozen and stopped")

	return nil
}

// UnfreezeContainer unfreezes a container and optionally starts it
func (m *Manager) UnfreezeContainer(containerID string, start bool) error {
	ctx := context.Background()
	container := m.dockerClient.GetContainer(containerID)

	containerInfo, err := container.Inspect(ctx)
	if err != nil {
		return fmt.Errorf("container not found: %s", containerID)
	}

	volumeID := strings.TrimPrefix(containerInfo.Name, "/")

	m.statesMu.Lock()
	state, exists := m.states[volumeID]
	if !exists {
		m.statesMu.Unlock()
		return fmt.Errorf("container state not found: %s", volumeID)
	}

	state.Frozen = false
	state.FreezeMessage = ""
	state.FrozenAt = nil
	m.statesMu.Unlock()

	// Persist state asynchronously
	m.saveStatesAsync()

	// Optionally start container
	if start {
		if err := container.Start(ctx); err != nil {
			m.log.WithError(err).Warn("Failed to start container during unfreeze")
			return err
		}
	}

	m.log.WithFields(logrus.Fields{
		"containerID": containerID,
		"volumeID":    volumeID,
	}).Info("Container unfrozen")

	return nil
}

// UpdateInstallContent writes a new install.sh into the volume for the given volumeID
func (m *Manager) UpdateInstallContent(ctx context.Context, volumeID string, content string) error {
	volumePath := filepath.Join(m.config.VolumesPath, volumeID)
	if !filepath.IsAbs(volumePath) {
		cwd, _ := os.Getwd()
		volumePath = filepath.Join(cwd, volumePath)
	}

	if err := os.MkdirAll(volumePath, 0755); err != nil {
		return fmt.Errorf("failed to ensure volume path: %w", err)
	}

	installPath := filepath.Join(volumePath, "install.sh")
	if err := os.WriteFile(installPath, []byte(content), 0755); err != nil {
		return fmt.Errorf("failed to write install.sh: %w", err)
	}

	m.log.WithField("installPath", installPath).Info("Wrote install.sh from UpdateInstallContent")
	return nil
}

// SetRULimit updates the RULimit for a given volume and persists state
func (m *Manager) SetRULimit(volumeID string, limit float64) error {
	m.statesMu.Lock()
	st, exists := m.states[volumeID]
	if !exists {
		st = &ContainerState{}
		m.states[volumeID] = st
	}
	st.RULimit = limit
	m.statesMu.Unlock()

	// Persist changes
	return m.saveStates()
}

// IsContainerFrozen checks if a container is frozen and returns message
func (m *Manager) IsContainerFrozen(containerID string) (bool, string, error) {
	// Fast path: check if containerID is a volumeID and exists in states
	m.statesMu.RLock()
	if state, exists := m.states[containerID]; exists {
		m.statesMu.RUnlock()
		return state.Frozen, state.FreezeMessage, nil
	}
	m.statesMu.RUnlock()

	// Fallback: inspect container to find volume ID
	ctx := context.Background()
	container := m.dockerClient.GetContainer(containerID)

	containerInfo, err := container.Inspect(ctx)
	if err != nil {
		return false, "", fmt.Errorf("container not found: %s", containerID)
	}

	volumeID := strings.TrimPrefix(containerInfo.Name, "/")

	m.statesMu.RLock()
	defer m.statesMu.RUnlock()

	state, exists := m.states[volumeID]
	if !exists {
		return false, "", fmt.Errorf("container state not found: %s", volumeID)
	}

	return state.Frozen, state.FreezeMessage, nil
}

// PowerAction performs a power action (start/stop/restart/kill) on a container
func (m *Manager) PowerAction(ctx context.Context, containerID, action string) error {
	// Check lock/frozen state
	if locked, reason, err := m.IsContainerLocked(containerID); err == nil && locked {
		return fmt.Errorf("container is locked: %s", reason)
	} else if err != nil {
		return fmt.Errorf("failed to check lock state: %w", err)
	}
	if frozen, msg, err := m.IsContainerFrozen(containerID); err == nil && frozen {
		return fmt.Errorf("container is frozen: %s", msg)
	} else if err != nil {
		return fmt.Errorf("failed to check frozen state: %w", err)
	}

	container := m.dockerClient.GetContainer(containerID)

	switch action {
	case "start":
		return container.Start(ctx)
	case "stop":
		return container.Stop(ctx)
	case "restart":
		return container.Restart(ctx)
	case "kill":
		return container.Kill(ctx)
	default:
		return fmt.Errorf("invalid action: %s", action)
	}
}

// ListContainers returns all containers managed by this daemon
func (m *Manager) ListContainers(ctx context.Context) ([]ContainerInfo, error) {
	containers, err := m.dockerClient.ListContainers(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to list Docker containers: %w", err)
	}

	var result []ContainerInfo
	m.statesMu.RLock()
	defer m.statesMu.RUnlock()

	for _, container := range containers {
		// Extract volume ID from container name (remove leading slash)
		volumeID := strings.TrimPrefix(container.Names[0], "/")

		// Only include containers that have corresponding volume directories (managed by us)
		volumePath := filepath.Join(m.config.VolumesPath, volumeID)
		if !filepath.IsAbs(volumePath) {
			if cwd, err := os.Getwd(); err == nil {
				volumePath = filepath.Join(cwd, volumePath)
			}
		}

		// Check if volume directory exists - if not, this container wasn't created by us
		if _, err := os.Stat(volumePath); os.IsNotExist(err) {
			continue
		}

		// Get state information
		state := m.states[volumeID]
		if state == nil {
			state = &ContainerState{State: StateUnknown}
		}

		info := ContainerInfo{
			ID:       container.ID,
			VolumeID: volumeID,
			Name:     volumeID,
			Image:    container.Image,
			Status:   container.Status,
			State:    container.State,
			Created:  time.Unix(container.Created, 0),
			Ports:    container.Ports,
			Labels:   container.Labels,
			// Add our custom state information
			DaemonState:     state.State,
			DiskLimit:       state.DiskLimit,
			MemoryLimit:     state.MemoryLimit,
			CPULimit:        state.CPULimit,
			AttachedVolumes: state.AttachedVolumes,
			Locked:          state.Locked,
			LockReason:      state.LockReason,
			LockedAt:        state.LockedAt,
			Frozen:          state.Frozen,
			FreezeMessage:   state.FreezeMessage,
			FrozenAt:        state.FrozenAt,
		}

		result = append(result, info)
	}

	return result, nil
}

// GetContainer returns information about a specific container
func (m *Manager) GetContainer(ctx context.Context, containerID string) (*ContainerInfo, error) {
	container := m.dockerClient.GetContainer(containerID)

	containerInfo, err := container.Inspect(ctx)
	if err != nil {
		return nil, fmt.Errorf("container not found: %s", containerID)
	}

	volumeID := strings.TrimPrefix(containerInfo.Name, "/")

	// Get state information
	m.statesMu.RLock()
	state := m.states[volumeID]
	m.statesMu.RUnlock()

	if state == nil {
		state = &ContainerState{State: StateUnknown}
	}

	// Parse the created time
	createdTime, err := time.Parse(time.RFC3339Nano, containerInfo.Created)
	if err != nil {
		// Fallback to current time if parsing fails
		createdTime = time.Now()
	}

	info := &ContainerInfo{
		ID:       containerInfo.ID,
		VolumeID: volumeID,
		Name:     volumeID,
		Image:    containerInfo.Config.Image,
		Status:   containerInfo.State.Status,
		State:    containerInfo.State.Status,
		Created:  createdTime,
		// Add our custom state information
		DaemonState:     state.State,
		DiskLimit:       state.DiskLimit,
		MemoryLimit:     state.MemoryLimit,
		CPULimit:        state.CPULimit,
		AttachedVolumes: state.AttachedVolumes,
		Locked:          state.Locked,
		LockReason:      state.LockReason,
		LockedAt:        state.LockedAt,
		Frozen:          state.Frozen,
		FreezeMessage:   state.FreezeMessage,
		FrozenAt:        state.FrozenAt,
	}

	return info, nil
}

// ContainerInfo represents container information with daemon state
type ContainerInfo struct {
	ID       string            `json:"id"`
	VolumeID string            `json:"volumeId"`
	Name     string            `json:"name"`
	Image    string            `json:"image"`
	Status   string            `json:"status"`
	State    string            `json:"state"`
	Created  time.Time         `json:"created"`
	Ports    []types.Port      `json:"ports,omitempty"`
	Labels   map[string]string `json:"labels,omitempty"`

	// Daemon-specific state
	DaemonState     State      `json:"daemonState"`
	DiskLimit       int64      `json:"diskLimit,omitempty"`
	MemoryLimit     int64      `json:"memoryLimit,omitempty"`
	CPULimit        int64      `json:"cpuLimit,omitempty"`
	AttachedVolumes []string   `json:"attachedVolumes,omitempty"`
	Locked          bool       `json:"locked,omitempty"`
	LockReason      string     `json:"lockReason,omitempty"`
	LockedAt        *time.Time `json:"lockedAt,omitempty"`
	Frozen          bool       `json:"frozen,omitempty"`
	FreezeMessage   string     `json:"freezeMessage,omitempty"`
	FrozenAt        *time.Time `json:"frozenAt,omitempty"`
}
