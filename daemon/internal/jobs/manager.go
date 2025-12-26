package jobs

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"

	"strconv"
	"strings"

	"github.com/docker/docker/api/types"
	"github.com/docker/go-connections/nat"
	"github.com/google/uuid"
	"github.com/sirupsen/logrus"

	"NightLightd/internal/config"
	"NightLightd/internal/container"
	"NightLightd/internal/network"
	"NightLightd/internal/snapshot"
)

// JobStatus represents the status of a job
type JobStatus string

const (
	JobStatusPending   JobStatus = "PENDING"
	JobStatusRunning   JobStatus = "RUNNING"
	JobStatusCompleted JobStatus = "COMPLETED"
	JobStatusFailed    JobStatus = "FAILED"
	JobStatusRetrying  JobStatus = "RETRYING"
)

// JobType represents the type of job
type JobType string

const (
	JobTypeContainerCreate JobType = "CONTAINER_CREATE"
	JobTypeContainerDelete JobType = "CONTAINER_DELETE"
	JobTypeSnapshotCreate  JobType = "SNAPSHOT_CREATE"
	JobTypeSnapshotRestore JobType = "SNAPSHOT_RESTORE"
)

// Job represents a background job
type Job struct {
	ID          string                 `json:"id"`
	Type        JobType                `json:"type"`
	Status      JobStatus              `json:"status"`
	Progress    int                    `json:"progress"`
	MaxRetries  int                    `json:"maxRetries"`
	Retries     int                    `json:"retries"`
	CreatedAt   time.Time              `json:"createdAt"`
	StartedAt   *time.Time             `json:"startedAt,omitempty"`
	CompletedAt *time.Time             `json:"completedAt,omitempty"`
	Error       string                 `json:"error,omitempty"`
	Result      map[string]interface{} `json:"result,omitempty"`
	Payload     map[string]interface{} `json:"payload"`
	Logs        []LogEntry             `json:"logs"`
}

// LogEntry represents a log entry for a job
type LogEntry struct {
	Timestamp time.Time `json:"timestamp"`
	Level     string    `json:"level"`
	Message   string    `json:"message"`
}

// Manager manages background jobs
type Manager struct {
	config           *config.Config
	containerManager *container.Manager
	snapshotManager  SnapshotManagerInterface
	log              *logrus.Logger
	jobs             map[string]*Job
	jobsMu           sync.RWMutex
	jobsFile         string
	logStreams       map[string][]chan LogEntry
	streamsMu        sync.RWMutex
}

// SnapshotManagerInterface defines the interface for snapshot operations
type SnapshotManagerInterface interface {
	CreateSnapshot(containerID, name, description string) (*snapshot.SnapshotInfo, error)
	RestoreSnapshot(containerID, snapshotID string) error
}

// NewManager creates a new job manager
func NewManager(cfg *config.Config, containerManager *container.Manager, log *logrus.Logger) *Manager {
	jobsFile := filepath.Join(cfg.StoragePath, "jobs.json")

	manager := &Manager{
		config:           cfg,
		containerManager: containerManager,
		snapshotManager:  nil, // Will be set later to avoid circular dependency
		log:              log,
		jobs:             make(map[string]*Job),
		jobsFile:         jobsFile,
		logStreams:       make(map[string][]chan LogEntry),
	}

	// Load existing jobs
	if err := manager.loadJobs(); err != nil {
		log.WithError(err).Warn("Failed to load jobs")
	}

	return manager
}

// SetSnapshotManager sets the snapshot manager (to avoid circular dependency)
func (m *Manager) SetSnapshotManager(snapshotManager SnapshotManagerInterface) {
	m.snapshotManager = snapshotManager
}

// CreateContainerJob creates a new container creation job
func (m *Manager) CreateContainerJob(req *container.DeploymentRequest) (string, error) {
	jobID := uuid.New().String()

	job := &Job{
		ID:         jobID,
		Type:       JobTypeContainerCreate,
		Status:     JobStatusPending,
		Progress:   0,
		MaxRetries: 3,
		Retries:    0,
		CreatedAt:  time.Now(),
		Payload: map[string]interface{}{
			"deploymentRequest": req,
		},
		Logs: []LogEntry{},
	}

	m.jobsMu.Lock()
	m.jobs[jobID] = job
	m.jobsMu.Unlock()

	if err := m.saveJobs(); err != nil {
		m.log.WithError(err).Warn("Failed to save jobs")
	}

	// Start the job in a goroutine
	go m.executeJob(jobID)

	m.log.WithFields(logrus.Fields{
		"jobId":    jobID,
		"type":     job.Type,
		"volumeId": req.ID,
	}).Info("Container creation job created")

	return jobID, nil
}

// CreateSnapshotJob creates a new snapshot creation job
func (m *Manager) CreateSnapshotJob(containerID, name, description string) (string, error) {
	jobID := uuid.New().String()

	job := &Job{
		ID:         jobID,
		Type:       JobTypeSnapshotCreate,
		Status:     JobStatusPending,
		Progress:   0,
		MaxRetries: 2,
		Retries:    0,
		CreatedAt:  time.Now(),
		Payload: map[string]interface{}{
			"containerID": containerID,
			"name":        name,
			"description": description,
		},
		Logs: []LogEntry{},
	}

	m.jobsMu.Lock()
	m.jobs[jobID] = job
	m.jobsMu.Unlock()

	if err := m.saveJobs(); err != nil {
		m.log.WithError(err).Warn("Failed to save jobs")
	}

	// Start the job in a goroutine
	go m.executeJob(jobID)

	m.log.WithFields(logrus.Fields{
		"jobId":       jobID,
		"type":        job.Type,
		"containerID": containerID,
	}).Info("Snapshot creation job created")

	return jobID, nil
}

// CreateRestoreJob creates a new snapshot restore job
func (m *Manager) CreateRestoreJob(containerID, snapshotID string) (string, error) {
	jobID := uuid.New().String()

	job := &Job{
		ID:         jobID,
		Type:       JobTypeSnapshotRestore,
		Status:     JobStatusPending,
		Progress:   0,
		MaxRetries: 2,
		Retries:    0,
		CreatedAt:  time.Now(),
		Payload: map[string]interface{}{
			"containerID": containerID,
			"snapshotID":  snapshotID,
		},
		Logs: []LogEntry{},
	}

	m.jobsMu.Lock()
	m.jobs[jobID] = job
	m.jobsMu.Unlock()

	if err := m.saveJobs(); err != nil {
		m.log.WithError(err).Warn("Failed to save jobs")
	}

	// Start the job in a goroutine
	go m.executeJob(jobID)

	m.log.WithFields(logrus.Fields{
		"jobId":       jobID,
		"type":        job.Type,
		"containerID": containerID,
		"snapshotID":  snapshotID,
	}).Info("Snapshot restore job created")

	return jobID, nil
}

// GetJob returns a job by ID
func (m *Manager) GetJob(jobID string) (*Job, error) {
	m.jobsMu.RLock()
	defer m.jobsMu.RUnlock()

	job, exists := m.jobs[jobID]
	if !exists {
		return nil, fmt.Errorf("job not found: %s", jobID)
	}

	return job, nil
}

// ListJobs returns all jobs
func (m *Manager) ListJobs() []*Job {
	m.jobsMu.RLock()
	defer m.jobsMu.RUnlock()

	jobs := make([]*Job, 0, len(m.jobs))
	for _, job := range m.jobs {
		jobs = append(jobs, job)
	}

	return jobs
}

// SubscribeToLogs subscribes to log updates for a job
func (m *Manager) SubscribeToLogs(jobID string) (chan LogEntry, error) {
	m.streamsMu.Lock()
	defer m.streamsMu.Unlock()

	// Check if job exists
	m.jobsMu.RLock()
	_, exists := m.jobs[jobID]
	m.jobsMu.RUnlock()

	if !exists {
		return nil, fmt.Errorf("job not found: %s", jobID)
	}

	// Create log channel
	logChan := make(chan LogEntry, 100)

	// Add to streams
	if m.logStreams[jobID] == nil {
		m.logStreams[jobID] = []chan LogEntry{}
	}
	m.logStreams[jobID] = append(m.logStreams[jobID], logChan)

	// Send existing logs
	m.jobsMu.RLock()
	job := m.jobs[jobID]
	for _, logEntry := range job.Logs {
		select {
		case logChan <- logEntry:
		default:
			// Channel full, skip
		}
	}
	m.jobsMu.RUnlock()

	return logChan, nil
}

// executeJob executes a job with retries
func (m *Manager) executeJob(jobID string) {
	m.jobsMu.Lock()
	job := m.jobs[jobID]
	now := time.Now()
	job.Status = JobStatusRunning
	job.StartedAt = &now
	m.jobsMu.Unlock()

	m.addLog(jobID, "INFO", fmt.Sprintf("Starting job execution (attempt %d/%d)", job.Retries+1, job.MaxRetries+1))

	var err error
	switch job.Type {
	case JobTypeContainerCreate:
		err = m.executeContainerCreateJob(jobID)
	case JobTypeSnapshotCreate:
		err = m.executeSnapshotCreateJob(jobID)
	case JobTypeSnapshotRestore:
		err = m.executeSnapshotRestoreJob(jobID)
	default:
		err = fmt.Errorf("unknown job type: %s", job.Type)
	}

	m.jobsMu.Lock()
	job = m.jobs[jobID]
	now = time.Now()
	job.CompletedAt = &now

	if err != nil {
		job.Retries++
		job.Error = err.Error()

		if job.Retries <= job.MaxRetries {
			job.Status = JobStatusRetrying
			m.jobsMu.Unlock()

			// Add logs after releasing the lock to avoid deadlock
			m.addLog(jobID, "ERROR", fmt.Sprintf("Job failed: %s", err.Error()))
			m.addLog(jobID, "INFO", fmt.Sprintf("Retrying job in 5 seconds (attempt %d/%d)", job.Retries+1, job.MaxRetries+1))

			// Wait before retry
			time.Sleep(5 * time.Second)
			go m.executeJob(jobID)
			return
		} else {
			job.Status = JobStatusFailed
			m.jobsMu.Unlock()

			// Add logs after releasing the lock to avoid deadlock
			m.addLog(jobID, "ERROR", fmt.Sprintf("Job failed: %s", err.Error()))
			m.addLog(jobID, "ERROR", "Job failed after maximum retries")
		}
	} else {
		job.Status = JobStatusCompleted
		job.Progress = 100
		m.jobsMu.Unlock()

		// Add log after releasing the lock to avoid deadlock
		m.addLog(jobID, "INFO", "Job completed successfully")
	}

	if err := m.saveJobs(); err != nil {
		m.log.WithError(err).Warn("Failed to save jobs")
	}

	// Close log streams after a delay
	go func() {
		time.Sleep(30 * time.Second)
		m.closeLogStreams(jobID)
	}()
}

// executeContainerCreateJob executes a container creation job
func (m *Manager) executeContainerCreateJob(jobID string) error {
	m.jobsMu.RLock()
	job := m.jobs[jobID]
	reqData := job.Payload["deploymentRequest"]
	m.jobsMu.RUnlock()

	// Convert back to DeploymentRequest
	reqBytes, err := json.Marshal(reqData)
	if err != nil {
		return fmt.Errorf("failed to marshal deployment request: %w", err)
	}

	var req container.DeploymentRequest
	if err := json.Unmarshal(reqBytes, &req); err != nil {
		return fmt.Errorf("failed to unmarshal deployment request: %w", err)
	}

	m.addLog(jobID, "INFO", fmt.Sprintf("Creating container: %s", req.ID))
	m.updateProgress(jobID, 10)

	m.addLog(jobID, "INFO", fmt.Sprintf("Pulling image: %s", req.Image))
	m.updateProgress(jobID, 30)

	// Execute the container deployment with timeout context
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
	defer cancel()

	// Add more detailed logging during deployment
	m.addLog(jobID, "INFO", "Starting container deployment process")
	m.updateProgress(jobID, 40)

	// If client provided explicit PortBindings, verify host ports are free
	if req.PortBindings != nil && len(req.PortBindings) > 0 {
		for _, hostBindings := range req.PortBindings {
			for _, binding := range hostBindings {
				portNum, _ := strconv.Atoi(binding.HostPort)
				if ok, _ := network.IsTCPPortAvailable(portNum); !ok {
					m.addLog(jobID, "ERROR", fmt.Sprintf("Requested host port %d is not available", portNum))
					return fmt.Errorf("requested host port %d is not available", portNum)
				}
			}
		}
	}

	// Auto-assign host ports from configured available_ports if no explicit PortBindings provided
	if (req.PortBindings == nil || len(req.PortBindings) == 0) && len(req.Ports) > 0 {
		available := m.config.AvailablePorts
		if len(available) == 0 {
			m.addLog(jobID, "ERROR", "No available_ports configured; cannot automatically assign host ports")
			return fmt.Errorf("no available_ports configured; cannot automatically assign host ports")
		}

		// Determine currently used host ports (best-effort)
		used := map[int]bool{}
		ctxList, cancelList := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancelList()
		containers, err := m.containerManager.GetDockerClient().ListContainers(ctxList)
		if err == nil {
			for _, c := range containers {
				for _, p := range c.Ports {
					used[int(p.PublicPort)] = true
				}
			}
		} else {
			m.addLog(jobID, "WARN", "Failed to list containers to determine used ports, will attempt best-effort assignment")
		}

		pb := nat.PortMap{}
		for port := range req.Ports {
			// find an available, unused, and actually bindable host port
			assigned := ""
			for _, ap := range available {
				if !used[ap] {
					// double-check availability by trying to bind
					if ok, _ := network.IsTCPPortAvailable(ap); ok {
						assigned = fmt.Sprintf("%d", ap)
						used[ap] = true
						break
					}
					// if bind failed, mark it used so we don't retry
					used[ap] = true
				}
			}
			if assigned == "" {
				m.addLog(jobID, "ERROR", "Insufficient available_ports to satisfy port allocation request")
				return fmt.Errorf("insufficient available_ports to assign for exposed ports")
			}
			pb[port] = []nat.PortBinding{{HostIP: "0.0.0.0", HostPort: assigned}}
		}

		req.PortBindings = pb
		m.addLog(jobID, "INFO", fmt.Sprintf("Assigned host ports: %v", req.PortBindings))
	}

	if err := m.containerManager.Deploy(ctx, &req); err != nil {
		m.addLog(jobID, "ERROR", fmt.Sprintf("Container deployment failed: %s", err.Error()))
		// Save detailed error to job result and fail fast
		m.jobsMu.Lock()
		job = m.jobs[jobID]
		job.Result = map[string]interface{}{"error": err.Error()}
		m.jobsMu.Unlock()
		return fmt.Errorf("container deployment failed: %w", err)
	}

	// After deployment, discover the actual container ID and wait until it's running
	m.addLog(jobID, "INFO", "Looking up created container to confirm running state")
	foundContainer := false
	var created types.Container
	for i := 0; i < 20; i++ { // try for up to ~10s
		ctxList, cancel := context.WithTimeout(context.Background(), 2*time.Second)
		containers, err := m.containerManager.GetDockerClient().ListContainers(ctxList)
		cancel()
		if err == nil {
			for _, c := range containers {
				for _, name := range c.Names {
					if strings.Contains(name, req.ID) {
						created = c
						if c.State == "running" {
							foundContainer = true
							break
						}
					}
				}
				if foundContainer {
					break
				}
			}
		}
		if foundContainer {
			break
		}
		// Not running yet; sleep briefly and retry
		time.Sleep(500 * time.Millisecond)
	}

	if !foundContainer {
		m.addLog(jobID, "ERROR", "Container did not reach running state in time or could not be found")
		return fmt.Errorf("container did not reach running state in time or could not be found")
	}

	// Build port mapping result
	portResults := map[string]string{}
	for _, p := range created.Ports {
		containerPort := fmt.Sprintf("%d/%s", p.PrivatePort, p.Type)
		hostPort := fmt.Sprintf("%d", p.PublicPort)
		portResults[containerPort] = hostPort
	}

	m.addLog(jobID, "INFO", fmt.Sprintf("Container %s running with ports %v", created.ID[:12], portResults))

	m.updateProgress(jobID, 90)
	m.addLog(jobID, "INFO", "Container created successfully")

	// Set result
	m.jobsMu.Lock()
	job = m.jobs[jobID]
	job.Result = map[string]interface{}{
		"containerId": created.ID,
		"volumeId":    req.ID,
		"ports":       portResults,
	}
	m.jobsMu.Unlock()

	m.updateProgress(jobID, 100)
	return nil
	return nil
}

// executeSnapshotCreateJob executes a snapshot creation job
func (m *Manager) executeSnapshotCreateJob(jobID string) error {
	if m.snapshotManager == nil {
		return fmt.Errorf("snapshot manager not available")
	}

	m.jobsMu.RLock()
	job := m.jobs[jobID]
	containerID := job.Payload["containerID"].(string)
	name := job.Payload["name"].(string)
	description := job.Payload["description"].(string)
	m.jobsMu.RUnlock()

	m.addLog(jobID, "INFO", fmt.Sprintf("Starting snapshot creation for container: %s", containerID))
	m.updateProgress(jobID, 10)

	m.addLog(jobID, "INFO", "Checking container status and locking...")
	m.updateProgress(jobID, 20)

	m.addLog(jobID, "INFO", "Creating volume backup...")
	m.updateProgress(jobID, 40)

	// Execute the snapshot creation
	snapshotResult, err := m.snapshotManager.CreateSnapshot(containerID, name, description)
	if err != nil {
		m.addLog(jobID, "ERROR", fmt.Sprintf("Snapshot creation failed: %s", err.Error()))
		return fmt.Errorf("snapshot creation failed: %w", err)
	}

	m.updateProgress(jobID, 90)
	m.addLog(jobID, "INFO", "Snapshot created successfully")

	// Set result - convert from snapshot.SnapshotInfo to jobs result
	m.jobsMu.Lock()
	job = m.jobs[jobID]
	job.Result = map[string]interface{}{
		"snapshotId":  snapshotResult.ID,
		"containerID": snapshotResult.ContainerID,
		"volumeId":    snapshotResult.VolumeID,
		"name":        snapshotResult.Name,
		"size":        snapshotResult.Size,
	}
	m.jobsMu.Unlock()

	m.updateProgress(jobID, 100)
	return nil
}

// executeSnapshotRestoreJob executes a snapshot restore job
func (m *Manager) executeSnapshotRestoreJob(jobID string) error {
	if m.snapshotManager == nil {
		return fmt.Errorf("snapshot manager not available")
	}

	m.jobsMu.RLock()
	job := m.jobs[jobID]
	containerID := job.Payload["containerID"].(string)
	snapshotID := job.Payload["snapshotID"].(string)
	m.jobsMu.RUnlock()

	m.addLog(jobID, "INFO", fmt.Sprintf("Starting snapshot restore for container: %s", containerID))
	m.updateProgress(jobID, 10)

	m.addLog(jobID, "INFO", fmt.Sprintf("Restoring from snapshot: %s", snapshotID))
	m.updateProgress(jobID, 20)

	m.addLog(jobID, "INFO", "Checking container status and locking...")
	m.updateProgress(jobID, 30)

	m.addLog(jobID, "INFO", "Stopping container for restore...")
	m.updateProgress(jobID, 40)

	m.addLog(jobID, "INFO", "Backing up current volume...")
	m.updateProgress(jobID, 50)

	m.addLog(jobID, "INFO", "Restoring volume from snapshot...")
	m.updateProgress(jobID, 70)

	// Execute the snapshot restore
	if err := m.snapshotManager.RestoreSnapshot(containerID, snapshotID); err != nil {
		m.addLog(jobID, "ERROR", fmt.Sprintf("Snapshot restore failed: %s", err.Error()))
		return fmt.Errorf("snapshot restore failed: %w", err)
	}

	m.updateProgress(jobID, 90)
	m.addLog(jobID, "INFO", "Restarting container...")

	// Set result
	m.jobsMu.Lock()
	job = m.jobs[jobID]
	job.Result = map[string]interface{}{
		"containerID": containerID,
		"snapshotID":  snapshotID,
		"restored":    true,
	}
	m.jobsMu.Unlock()

	m.updateProgress(jobID, 100)
	m.addLog(jobID, "INFO", "Snapshot restore completed successfully")
	return nil
}

// addLog adds a log entry to a job and broadcasts to subscribers
func (m *Manager) addLog(jobID, level, message string) {
	logEntry := LogEntry{
		Timestamp: time.Now(),
		Level:     level,
		Message:   message,
	}

	m.jobsMu.Lock()
	job := m.jobs[jobID]
	job.Logs = append(job.Logs, logEntry)
	m.jobsMu.Unlock()

	// Broadcast to subscribers
	m.streamsMu.RLock()
	streams := m.logStreams[jobID]
	for _, stream := range streams {
		select {
		case stream <- logEntry:
		default:
			// Channel full or closed, skip
		}
	}
	m.streamsMu.RUnlock()

	m.log.WithFields(logrus.Fields{
		"jobId":   jobID,
		"level":   level,
		"message": message,
	}).Debug("Job log added")
}

// updateProgress updates job progress
func (m *Manager) updateProgress(jobID string, progress int) {
	m.jobsMu.Lock()
	job := m.jobs[jobID]
	job.Progress = progress
	m.jobsMu.Unlock()
}

// closeLogStreams closes all log streams for a job
func (m *Manager) closeLogStreams(jobID string) {
	m.streamsMu.Lock()
	defer m.streamsMu.Unlock()

	streams := m.logStreams[jobID]
	for _, stream := range streams {
		close(stream)
	}
	delete(m.logStreams, jobID)
}

// loadJobs loads jobs from file
func (m *Manager) loadJobs() error {
	if _, err := os.Stat(m.jobsFile); os.IsNotExist(err) {
		return nil // File doesn't exist, start with empty jobs
	}

	data, err := os.ReadFile(m.jobsFile)
	if err != nil {
		return fmt.Errorf("failed to read jobs file: %w", err)
	}

	m.jobsMu.Lock()
	defer m.jobsMu.Unlock()

	if err := json.Unmarshal(data, &m.jobs); err != nil {
		return fmt.Errorf("failed to unmarshal jobs: %w", err)
	}

	return nil
}

// saveJobs saves jobs to file
func (m *Manager) saveJobs() error {
	m.jobsMu.RLock()
	data, err := json.MarshalIndent(m.jobs, "", "  ")
	m.jobsMu.RUnlock()

	if err != nil {
		return fmt.Errorf("failed to marshal jobs: %w", err)
	}

	if err := os.WriteFile(m.jobsFile, data, 0644); err != nil {
		return fmt.Errorf("failed to write jobs file: %w", err)
	}

	return nil
}
