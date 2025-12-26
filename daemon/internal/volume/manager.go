package volume

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sync"

	"github.com/sirupsen/logrus"
)

// Manager handles volume operations
type Manager struct {
	volumesPath string
	storagePath string
	log         *logrus.Logger
	mu          sync.RWMutex
}

// VolumeState represents volume state information
type VolumeState struct {
	DiskLimit int64 `json:"diskLimit"`
}

// NewManager creates a new volume manager
func NewManager(volumesPath, storagePath string, log *logrus.Logger) *Manager {
	// Ensure paths are absolute
	if !filepath.IsAbs(volumesPath) {
		if cwd, err := os.Getwd(); err == nil {
			volumesPath = filepath.Join(cwd, volumesPath)
		}
	}
	if !filepath.IsAbs(storagePath) {
		if cwd, err := os.Getwd(); err == nil {
			storagePath = filepath.Join(cwd, storagePath)
		}
	}

	return &Manager{
		volumesPath: volumesPath,
		storagePath: storagePath,
		log:         log,
	}
}

// GetVolumeSize calculates the size of a volume in MiB
func (m *Manager) GetVolumeSize(volumeID string) (float64, error) {
	volumePath := filepath.Join(m.volumesPath, volumeID)

	// Check if volume directory exists
	if _, err := os.Stat(volumePath); os.IsNotExist(err) {
		// Volume doesn't exist, return 0 size (don't create it)
		return 0, nil
	}

	totalSize, err := m.calculateDirectorySize(volumePath, 0)
	if err != nil {
		return 0, fmt.Errorf("failed to calculate directory size: %w", err)
	}

	// Convert bytes to MiB
	sizeMiB := float64(totalSize) / (1024 * 1024)
	return sizeMiB, nil
}

// GetDiskLimit gets the disk limit for a volume
func (m *Manager) GetDiskLimit(volumeID string) (int64, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	statesFile := filepath.Join(m.storagePath, "states.json")

	data, err := os.ReadFile(statesFile)
	if err != nil {
		if os.IsNotExist(err) {
			return 0, nil // No states file means no limits
		}
		return 0, fmt.Errorf("failed to read states file: %w", err)
	}

	var states map[string]VolumeState
	if err := json.Unmarshal(data, &states); err != nil {
		return 0, fmt.Errorf("failed to parse states file: %w", err)
	}

	state, exists := states[volumeID]
	if !exists {
		return 0, nil // No limit set for this volume
	}

	return state.DiskLimit, nil
}

// SetDiskLimit sets the disk limit for a volume
func (m *Manager) SetDiskLimit(volumeID string, limit int64) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	statesFile := filepath.Join(m.storagePath, "states.json")

	// Read existing states
	var states map[string]VolumeState
	if data, err := os.ReadFile(statesFile); err == nil {
		json.Unmarshal(data, &states)
	}

	if states == nil {
		states = make(map[string]VolumeState)
	}

	// Update state
	states[volumeID] = VolumeState{
		DiskLimit: limit,
	}

	// Write back to file
	data, err := json.MarshalIndent(states, "", "  ")
	if err != nil {
		return fmt.Errorf("failed to marshal states: %w", err)
	}

	if err := os.WriteFile(statesFile, data, 0644); err != nil {
		return fmt.Errorf("failed to write states file: %w", err)
	}

	return nil
}

// calculateDirectorySize calculates the total size of a directory
func (m *Manager) calculateDirectorySize(dirPath string, currentDepth int) (int64, error) {
	// Prevent infinite recursion
	if currentDepth >= 500 {
		m.log.WithField("path", dirPath).Warn("Maximum depth reached")
		return 0, nil
	}

	var totalSize int64

	entries, err := os.ReadDir(dirPath)
	if err != nil {
		return 0, err
	}

	for _, entry := range entries {
		entryPath := filepath.Join(dirPath, entry.Name())

		if entry.IsDir() {
			dirSize, err := m.calculateDirectorySize(entryPath, currentDepth+1)
			if err != nil {
				m.log.WithError(err).WithField("path", entryPath).Warn("Failed to calculate directory size")
				continue
			}
			totalSize += dirSize
		} else {
			info, err := entry.Info()
			if err != nil {
				m.log.WithError(err).WithField("path", entryPath).Warn("Failed to get file info")
				continue
			}
			totalSize += info.Size()
		}
	}

	return totalSize, nil
}

// EnsureVolumeExists ensures a volume directory exists - ONLY used during container deployment
func (m *Manager) EnsureVolumeExists(volumeID string) error {
	volumePath := filepath.Join(m.volumesPath, volumeID)

	if err := os.MkdirAll(volumePath, 0755); err != nil {
		return fmt.Errorf("failed to create volume directory: %w", err)
	}

	m.log.WithField("volumeId", volumeID).Info("Volume directory created for container deployment")
	return nil
}

// DeleteVolume deletes a volume directory
func (m *Manager) DeleteVolume(volumeID string) error {
	volumePath := filepath.Join(m.volumesPath, volumeID)
	return os.RemoveAll(volumePath)
}

// ListVolumes lists all volumes
func (m *Manager) ListVolumes() ([]string, error) {
	entries, err := os.ReadDir(m.volumesPath)
	if err != nil {
		return nil, err
	}

	var volumes []string
	for _, entry := range entries {
		if entry.IsDir() {
			volumes = append(volumes, entry.Name())
		}
	}

	return volumes, nil
}
