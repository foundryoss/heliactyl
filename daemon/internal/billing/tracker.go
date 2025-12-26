package billing

import (
	_"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"time"

	"github.com/sirupsen/logrus"
)

// BillingRecord represents a billing record for a container
type BillingRecord struct {
	ContainerID string    `json:"container_id"`
	VolumeID    string    `json:"volume_id"`
	StartTime   time.Time `json:"start_time"`
	EndTime     time.Time `json:"end_time"`
	CPUUsage    float64   `json:"cpu_usage"`
	MemoryUsage uint64    `json:"memory_usage"`
	DiskUsage   uint64    `json:"disk_usage"`
	NetworkRx   uint64    `json:"network_rx"`
	NetworkTx   uint64    `json:"network_tx"`
	Duration    float64   `json:"duration_seconds"`
}

// Tracker handles billing tracking for containers
type Tracker struct {
	log         *logrus.Logger
	storagePath string
	records     []BillingRecord
	mu          sync.RWMutex
}

// NewTracker creates a new billing tracker
func NewTracker(log *logrus.Logger, storagePath string) *Tracker {
	return &Tracker{
		log:         log,
		storagePath: storagePath,
		records:     make([]BillingRecord, 0),
	}
}

// StartTracking starts tracking billing for a container
func (t *Tracker) StartTracking(containerID, volumeID string) error {
	t.mu.Lock()
	defer t.mu.Unlock()

	record := BillingRecord{
		ContainerID: containerID,
		VolumeID:    volumeID,
		StartTime:   time.Now(),
	}

	t.records = append(t.records, record)
	
	t.log.WithFields(logrus.Fields{
		"containerID": containerID,
		"volumeID":    volumeID,
	}).Info("Started billing tracking")

	return nil
}

// StopTracking stops tracking billing for a container
func (t *Tracker) StopTracking(containerID string) error {
	t.mu.Lock()
	defer t.mu.Unlock()

	for i, record := range t.records {
		if record.ContainerID == containerID && record.EndTime.IsZero() {
			t.records[i].EndTime = time.Now()
			t.records[i].Duration = t.records[i].EndTime.Sub(t.records[i].StartTime).Seconds()
			
			t.log.WithFields(logrus.Fields{
				"containerID": containerID,
				"duration":    t.records[i].Duration,
			}).Info("Stopped billing tracking")
			
			return t.saveBillingRecords()
		}
	}

	return fmt.Errorf("no active billing record found for container %s", containerID)
}

// UpdateUsage updates usage statistics for a container
func (t *Tracker) UpdateUsage(containerID string, cpuUsage float64, memoryUsage, diskUsage, networkRx, networkTx uint64) error {
	t.mu.Lock()
	defer t.mu.Unlock()

	for i, record := range t.records {
		if record.ContainerID == containerID && record.EndTime.IsZero() {
			t.records[i].CPUUsage = cpuUsage
			t.records[i].MemoryUsage = memoryUsage
			t.records[i].DiskUsage = diskUsage
			t.records[i].NetworkRx = networkRx
			t.records[i].NetworkTx = networkTx
			return nil
		}
	}

	return fmt.Errorf("no active billing record found for container %s", containerID)
}

// GetRecords returns all billing records
func (t *Tracker) GetRecords() []BillingRecord {
	t.mu.RLock()
	defer t.mu.RUnlock()

	// Return a copy to avoid race conditions
	records := make([]BillingRecord, len(t.records))
	copy(records, t.records)
	return records
}

// GetRecordsForContainer returns billing records for a specific container
func (t *Tracker) GetRecordsForContainer(containerID string) []BillingRecord {
	t.mu.RLock()
	defer t.mu.RUnlock()

	var records []BillingRecord
	for _, record := range t.records {
		if record.ContainerID == containerID {
			records = append(records, record)
		}
	}
	return records
}

// saveBillingRecords saves billing records to disk
func (t *Tracker) saveBillingRecords() error {
	billingFile := filepath.Join(t.storagePath, "billing.json")
	
	data, err := json.MarshalIndent(t.records, "", "  ")
	if err != nil {
		return fmt.Errorf("failed to marshal billing records: %w", err)
	}

	if err := os.WriteFile(billingFile, data, 0644); err != nil {
		return fmt.Errorf("failed to write billing records: %w", err)
	}

	return nil
}

// LoadBillingRecords loads billing records from disk
func (t *Tracker) LoadBillingRecords() error {
	billingFile := filepath.Join(t.storagePath, "billing.json")
	
	if _, err := os.Stat(billingFile); os.IsNotExist(err) {
		return nil // File doesn't exist, start with empty records
	}

	data, err := os.ReadFile(billingFile)
	if err != nil {
		return fmt.Errorf("failed to read billing records: %w", err)
	}

	t.mu.Lock()
	defer t.mu.Unlock()

	if err := json.Unmarshal(data, &t.records); err != nil {
		return fmt.Errorf("failed to unmarshal billing records: %w", err)
	}

	t.log.WithField("recordCount", len(t.records)).Info("Loaded billing records")
	return nil
}