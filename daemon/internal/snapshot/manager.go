package snapshot

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/sirupsen/logrus"

	"NightLightd/internal/config"
	"NightLightd/internal/container"
	"NightLightd/internal/docker"
)

// SnapshotInfo represents a container snapshot
type SnapshotInfo struct {
	ID          string    `json:"id"`
	ContainerID string    `json:"containerId"`
	VolumeID    string    `json:"volumeId"`
	Name        string    `json:"name"`
	Description string    `json:"description"`
	CreatedAt   time.Time `json:"createdAt"`
	Size        int64     `json:"size"`
	Path        string    `json:"path"`
	ImageName   string    `json:"imageName"`
	Status      string    `json:"status"`
}

// Manager handles container snapshots
type Manager struct {
	config           *config.Config
	dockerClient     *docker.Client
	containerManager *container.Manager
	log              *logrus.Logger
	snapshots        map[string]*SnapshotInfo
	snapshotsMu      sync.RWMutex
	snapshotsFile    string
	snapshotsPath    string
}

// NewManager creates a new snapshot manager
func NewManager(cfg *config.Config, dockerClient *docker.Client, containerManager *container.Manager, log *logrus.Logger) *Manager {
	snapshotsPath := filepath.Join(cfg.StoragePath, "snapshots")
	snapshotsFile := filepath.Join(cfg.StoragePath, "snapshots.json")

	// Create snapshots directory
	if err := os.MkdirAll(snapshotsPath, 0755); err != nil {
		log.WithError(err).Warn("Failed to create snapshots directory")
	}

	manager := &Manager{
		config:           cfg,
		dockerClient:     dockerClient,
		containerManager: containerManager,
		log:              log,
		snapshots:        make(map[string]*SnapshotInfo),
		snapshotsFile:    snapshotsFile,
		snapshotsPath:    snapshotsPath,
	}

	// Load existing snapshots
	if err := manager.loadSnapshots(); err != nil {
		log.WithError(err).Warn("Failed to load snapshots")
	}

	return manager
}

// CreateSnapshot creates a snapshot of a container
func (m *Manager) CreateSnapshot(containerID, name, description string) (*SnapshotInfo, error) {
	// Check if container is locked
	locked, lockReason, err := m.containerManager.IsContainerLocked(containerID)
	if err != nil {
		return nil, fmt.Errorf("failed to check container lock status: %w", err)
	}
	if locked {
		return nil, fmt.Errorf("container is locked: %s", lockReason)
	}

	// Lock container during snapshot
	if err := m.containerManager.LockContainer(containerID, "Creating snapshot"); err != nil {
		return nil, fmt.Errorf("failed to lock container: %w", err)
	}
	defer m.containerManager.UnlockContainer(containerID)

	// Get container info
	ctx := context.Background()
	container := m.dockerClient.GetContainer(containerID)
	containerInfo, err := container.Inspect(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to inspect container: %w", err)
	}

	volumeID := strings.TrimPrefix(containerInfo.Name, "/")
	// Check for an existing in-progress snapshot for this volume (resume support)
	m.snapshotsMu.RLock()
	var inprogressID string
	for id, s := range m.snapshots {
		if s.VolumeID == volumeID && s.Status == "inprogress" {
			inprogressID = id
			break
		}
	}
	m.snapshotsMu.RUnlock()

	var snapshotID string
	if inprogressID != "" {
		m.log.WithField("resumeSnapshot", inprogressID).Info("Resuming in-progress snapshot")
		snapshotID = inprogressID
	} else {
		snapshotID = fmt.Sprintf("snap_%s_%d", volumeID, time.Now().Unix())
	}

	// Create snapshot directory if it doesn't exist
	snapshotDir := filepath.Join(m.snapshotsPath, snapshotID)
	if err := os.MkdirAll(snapshotDir, 0755); err != nil {
		return nil, fmt.Errorf("failed to create snapshot directory: %w", err)
	}

	m.log.WithFields(logrus.Fields{
		"containerID": containerID,
		"volumeID":    volumeID,
		"snapshotID":  snapshotID,
	}).Info("Creating container snapshot")

	// Ensure container isn't running (always freeze/stop before snapshot)
	wasRunning := containerInfo.State.Running
	if wasRunning {
		if err := container.Stop(ctx); err != nil {
			return nil, fmt.Errorf("failed to stop container: %w", err)
		}
		m.log.Info("Container stopped for snapshot")
	}

	// Paths
	volumePath := filepath.Join(m.config.VolumesPath, volumeID)
	inprogressPath := filepath.Join(snapshotDir, "volume.inprogress")
	finalVolumePath := filepath.Join(snapshotDir, "volume")

	// Mark snapshot as inprogress in per-snapshot metadata
	snapshot := &SnapshotInfo{
		ID:          snapshotID,
		ContainerID: containerID,
		VolumeID:    volumeID,
		Name:        name,
		Description: description,
		CreatedAt:   time.Now(),
		Size:        0,
		Path:        snapshotDir,
		ImageName:   containerInfo.Config.Image,
		Status:      "inprogress",
	}
	if err := m.writeSnapshotMetadata(snapshot); err != nil {
		m.log.WithError(err).Warn("Failed to write per-snapshot metadata (inprogress)")
	}

	m.log.WithField("snapshotDir", snapshotDir).Info("Starting rsync-based snapshot (resumable)")

	// Try rsync with xattrs; if rsync not available, fallback to copyDirectory
	if err := m.rsyncCopy(volumePath, inprogressPath); err != nil {
		m.log.WithError(err).Warn("rsync failed, falling back to filesystem copy")
		// Fallback: use existing directory copy (not resumable)
		if err2 := m.copyDirectory(volumePath, inprogressPath); err2 != nil {
			return nil, fmt.Errorf("failed to create volume backup: %w (rsync err: %v)", err2, err)
		}
	}

	// Move inprogress to final volume path atomically
	if err := os.Rename(inprogressPath, finalVolumePath); err != nil {
		m.log.WithError(err).Warn("Failed to atomically rename inprogress snapshot; trying copy")
		// Attempt to copy contents into final path
		if err2 := m.copyDirectory(inprogressPath, finalVolumePath); err2 != nil {
			return nil, fmt.Errorf("failed to finalize snapshot: %w", err2)
		}
		// Clean up inprogress path
		os.RemoveAll(inprogressPath)
	}

	// Save container configuration
	configPath := filepath.Join(snapshotDir, "container.json")
	configData, err := json.MarshalIndent(containerInfo, "", "  ")
	if err != nil {
		return nil, fmt.Errorf("failed to marshal container config: %w", err)
	}

	if err := os.WriteFile(configPath, configData, 0644); err != nil {
		return nil, fmt.Errorf("failed to save container config: %w", err)
	}

	// Calculate snapshot size
	size, err := m.calculateDirectorySize(snapshotDir)
	if err != nil {
		m.log.WithError(err).Warn("Failed to calculate snapshot size")
		size = 0
	}

	// Update snapshot info to completed
	snapshot.Size = size
	snapshot.Status = "completed"
	if err := m.writeSnapshotMetadata(snapshot); err != nil {
		m.log.WithError(err).Warn("Failed to write per-snapshot metadata (completed)")
	}

	// Save snapshot info to global metadata
	m.snapshotsMu.Lock()
	m.snapshots[snapshotID] = snapshot
	m.snapshotsMu.Unlock()

	if err := m.saveSnapshots(); err != nil {
		m.log.WithError(err).Warn("Failed to save snapshots metadata")
	}

	// Restart container if it was running
	if wasRunning {
		if err := container.Start(ctx); err != nil {
			m.log.WithError(err).Error("Failed to restart container after snapshot")
		} else {
			m.log.Info("Container restarted after snapshot")
		}
	}

	m.log.WithFields(logrus.Fields{
		"snapshotID": snapshotID,
		"size":       size,
	}).Info("Snapshot created successfully")

	return snapshot, nil
}

// RestoreSnapshot restores a container from a snapshot
func (m *Manager) RestoreSnapshot(containerID, snapshotID string) error {
	// Check if container is locked
	locked, lockReason, err := m.containerManager.IsContainerLocked(containerID)
	if err != nil {
		return fmt.Errorf("failed to check container lock status: %w", err)
	}
	if locked {
		return fmt.Errorf("container is locked: %s", lockReason)
	}

	// Lock container during restore
	if err := m.containerManager.LockContainer(containerID, "Restoring from snapshot"); err != nil {
		return fmt.Errorf("failed to lock container: %w", err)
	}
	defer m.containerManager.UnlockContainer(containerID)

	// Get snapshot info
	m.snapshotsMu.RLock()
	snapshot, exists := m.snapshots[snapshotID]
	m.snapshotsMu.RUnlock()

	if !exists {
		return fmt.Errorf("snapshot not found: %s", snapshotID)
	}

	// Get container info
	ctx := context.Background()
	container := m.dockerClient.GetContainer(containerID)
	containerInfo, err := container.Inspect(ctx)
	if err != nil {
		return fmt.Errorf("failed to inspect container: %w", err)
	}

	volumeID := strings.TrimPrefix(containerInfo.Name, "/")

	m.log.WithFields(logrus.Fields{
		"containerID": containerID,
		"volumeID":    volumeID,
		"snapshotID":  snapshotID,
	}).Info("Restoring container from snapshot")

	// Stop container if running
	wasRunning := containerInfo.State.Running
	if wasRunning {
		if err := container.Stop(ctx); err != nil {
			return fmt.Errorf("failed to stop container: %w", err)
		}
		m.log.Info("Container stopped for restore")
	}

	// Backup current volume (in case restore fails)
	volumePath := filepath.Join(m.config.VolumesPath, volumeID)
	backupPath := volumePath + "_backup_" + fmt.Sprintf("%d", time.Now().Unix())

	if err := m.copyDirectory(volumePath, backupPath); err != nil {
		m.log.WithError(err).Warn("Failed to create backup of current volume")
	} else {
		defer os.RemoveAll(backupPath) // Clean up backup on success
	}

	// Prepare atomic restore into temporary path
	tmpRestore := volumePath + "_restoretmp_" + fmt.Sprintf("%d", time.Now().Unix())
	snapshotVolumePath := filepath.Join(snapshot.Path, "volume")

	m.log.WithFields(logrus.Fields{
		"tmpRestore": tmpRestore,
		"snapshot":   snapshotVolumePath,
	}).Info("Starting rsync-based restore (atomic)")

	// Try rsync from snapshot to tmpRestore
	if err := m.rsyncCopy(snapshotVolumePath, tmpRestore); err != nil {
		m.log.WithError(err).Warn("rsync restore failed, falling back to filesystem copy")
		if err2 := m.copyDirectory(snapshotVolumePath, tmpRestore); err2 != nil {
			// Attempt to restore from backup
			if backupErr := m.copyDirectory(backupPath, volumePath); backupErr != nil {
				m.log.WithError(backupErr).Error("Failed to restore from backup after failed restore")
			}
			return fmt.Errorf("failed to restore volume into tmp path: %w (rsync err: %v)", err2, err)
		}
	}

	// Move current volume out, replace with tmpRestore atomically
	if _, err := os.Stat(volumePath); err == nil {
		if err := os.RemoveAll(volumePath); err != nil {
			m.log.WithError(err).Error("Failed to remove existing volume before atomic rename")
			// Attempt to rollback
			if backupErr := m.copyDirectory(backupPath, volumePath); backupErr != nil {
				m.log.WithError(backupErr).Error("Failed to restore from backup after failed rename")
			}
			return fmt.Errorf("failed to remove existing volume: %w", err)
		}
	}

	// Rename tmpRestore to volumePath
	if err := os.Rename(tmpRestore, volumePath); err != nil {
		m.log.WithError(err).Warn("Rename failed during restore; trying copy fallback")
		if err2 := m.copyDirectory(tmpRestore, volumePath); err2 != nil {
			// Attempt to rollback
			if backupErr := m.copyDirectory(backupPath, volumePath); backupErr != nil {
				m.log.WithError(backupErr).Error("Failed to restore from backup after failed restore copy")
			}
			return fmt.Errorf("failed to finalize restore: %w", err2)
		}
		// Clean up tmpRestore
		os.RemoveAll(tmpRestore)
	}

	// Restart container if it was running
	if wasRunning {
		if err := container.Start(ctx); err != nil {
			m.log.WithError(err).Error("Failed to restart container after restore")
		} else {
			m.log.Info("Container restarted after restore")
		}
	}

	m.log.WithField("snapshotID", snapshotID).Info("Snapshot restored successfully")
	return nil
}

// ListSnapshots returns all snapshots for a container
func (m *Manager) ListSnapshots(containerID string) ([]*SnapshotInfo, error) {
	// Get container info to find volume ID
	ctx := context.Background()
	container := m.dockerClient.GetContainer(containerID)
	containerInfo, err := container.Inspect(ctx)
	if err != nil {
		return nil, fmt.Errorf("failed to inspect container: %w", err)
	}

	volumeID := strings.TrimPrefix(containerInfo.Name, "/")

	m.snapshotsMu.RLock()
	defer m.snapshotsMu.RUnlock()

	var snapshots []*SnapshotInfo
	for _, snapshot := range m.snapshots {
		if snapshot.VolumeID == volumeID {
			snapshots = append(snapshots, snapshot)
		}
	}

	return snapshots, nil
}

// DeleteSnapshot deletes a snapshot
func (m *Manager) DeleteSnapshot(snapshotID string) error {
	m.snapshotsMu.Lock()
	snapshot, exists := m.snapshots[snapshotID]
	if !exists {
		m.snapshotsMu.Unlock()
		return fmt.Errorf("snapshot not found: %s", snapshotID)
	}

	delete(m.snapshots, snapshotID)
	m.snapshotsMu.Unlock()

	// Remove snapshot directory
	if err := os.RemoveAll(snapshot.Path); err != nil {
		m.log.WithError(err).Warn("Failed to remove snapshot directory")
	}

	// Save updated snapshots
	if err := m.saveSnapshots(); err != nil {
		m.log.WithError(err).Warn("Failed to save snapshots after deletion")
	}

	m.log.WithField("snapshotID", snapshotID).Info("Snapshot deleted")
	return nil
}

// rsyncCopy tries to copy using rsync preserving xattrs and supporting resume
func (m *Manager) rsyncCopy(src, dst string) error {
	// Ensure destination exists
	if err := os.MkdirAll(dst, 0755); err != nil {
		return err
	}

	// Ensure trailing slash semantics: copy contents of src into dst
	srcWithSlash := filepath.Clean(src) + string(os.PathSeparator)
	dstWithSlash := filepath.Clean(dst) + string(os.PathSeparator)

	// Build rsync args
	args := []string{"-aAX", "--delete", "--partial", "--inplace", "--numeric-ids", srcWithSlash, dstWithSlash}

	cmd := exec.Command("rsync", args...)
	var out bytes.Buffer
	cmd.Stdout = &out
	cmd.Stderr = &out

	m.log.WithFields(logrus.Fields{"cmd": cmd.Args}).Info("Running rsync for snapshot/restore")

	ctx, cancel := context.WithTimeout(context.Background(), 20*time.Minute)
	defer cancel()
	cmd = exec.CommandContext(ctx, "rsync", args...)
	cmd.Stdout = &out
	cmd.Stderr = &out

	if err := cmd.Run(); err != nil {
		m.log.WithField("rsync_output", out.String()).WithError(err).Warn("rsync command failed")
		return err
	}

	return nil
}

// copyDirectory recursively copies a directory (fallback when rsync not available)
func (m *Manager) copyDirectory(src, dst string) error {
	return filepath.Walk(src, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}

		// Calculate destination path
		relPath, err := filepath.Rel(src, path)
		if err != nil {
			return err
		}
		dstPath := filepath.Join(dst, relPath)

		if info.IsDir() {
			return os.MkdirAll(dstPath, info.Mode())
		}

		// Copy file
		srcFile, err := os.Open(path)
		if err != nil {
			return err
		}
		defer srcFile.Close()

		// Create destination directory if needed
		if err := os.MkdirAll(filepath.Dir(dstPath), 0755); err != nil {
			return err
		}

		dstFile, err := os.Create(dstPath)
		if err != nil {
			return err
		}
		defer dstFile.Close()

		if _, err := io.Copy(dstFile, srcFile); err != nil {
			return err
		}

		if err := os.Chmod(dstPath, info.Mode()); err != nil {
			return err
		}

		// Try to copy xattrs if available (best-effort)
		_ = copyFileXAttrs(path, dstPath)

		return nil
	})
}

// calculateDirectorySize calculates the total size of a directory
func (m *Manager) calculateDirectorySize(dirPath string) (int64, error) {
	var totalSize int64

	err := filepath.Walk(dirPath, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return nil // Skip errors and continue
		}
		if !info.IsDir() {
			totalSize += info.Size()
		}
		return nil
	})

	return totalSize, err
}

// writeSnapshotMetadata writes a per-snapshot snapshot.json file inside the snapshot dir
func (m *Manager) writeSnapshotMetadata(s *SnapshotInfo) error {
	metaPath := filepath.Join(s.Path, "snapshot.json")
	data, err := json.MarshalIndent(s, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(metaPath, data, 0644)
}

// copyFileXAttrs attempts to copy extended attributes from src to dst (best-effort)
func copyFileXAttrs(src, dst string) error {
	// Best-effort: use setfattr/getfattr if available or ignore
	// For now, this is a no-op placeholder to keep implementation portable
	// Systems with rsync will preserve xattrs; this acts as fallback
	return nil
}

// loadSnapshots loads snapshots from file
func (m *Manager) loadSnapshots() error {
	// First try to load the global snapshots file
	if _, err := os.Stat(m.snapshotsFile); err == nil {
		data, err := os.ReadFile(m.snapshotsFile)
		if err == nil {
			m.snapshotsMu.Lock()
			if err := json.Unmarshal(data, &m.snapshots); err != nil {
				m.snapshotsMu.Unlock()
				return fmt.Errorf("failed to unmarshal snapshots: %w", err)
			}
			m.snapshotsMu.Unlock()
		} else {
			return fmt.Errorf("failed to read snapshots file: %w", err)
		}
	}

	// Additionally, scan snapshots directory for any per-snapshot metadata files (helps resume/inprogress recovery)
	_ = filepath.Walk(m.snapshotsPath, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return nil
		}
		if info.IsDir() {
			metaPath := filepath.Join(path, "snapshot.json")
			if _, err := os.Stat(metaPath); err == nil {
				data, err := os.ReadFile(metaPath)
				if err != nil {
					m.log.WithField("meta", metaPath).WithError(err).Warn("Failed to read per-snapshot metadata")
					return nil
				}
				var s SnapshotInfo
				if err := json.Unmarshal(data, &s); err != nil {
					m.log.WithField("meta", metaPath).WithError(err).Warn("Failed to unmarshal per-snapshot metadata")
					return nil
				}
				m.snapshotsMu.Lock()
				// If not present or older, set it
				if existing, ok := m.snapshots[s.ID]; !ok || existing.Status != "completed" {
					m.snapshots[s.ID] = &s
				}
				m.snapshotsMu.Unlock()
			}
		}
		return nil
	})

	return nil
}

// saveSnapshots saves snapshots to file
func (m *Manager) saveSnapshots() error {
	m.snapshotsMu.RLock()
	data, err := json.MarshalIndent(m.snapshots, "", "  ")
	m.snapshotsMu.RUnlock()

	if err != nil {
		return fmt.Errorf("failed to marshal snapshots: %w", err)
	}

	if err := os.WriteFile(m.snapshotsFile, data, 0644); err != nil {
		return fmt.Errorf("failed to write snapshots file: %w", err)
	}

	return nil
}
