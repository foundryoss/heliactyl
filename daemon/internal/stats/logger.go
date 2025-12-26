package stats

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"sync"
	"time"

	"github.com/shirou/gopsutil/v3/cpu"
	"github.com/shirou/gopsutil/v3/disk"
	"github.com/shirou/gopsutil/v3/mem"
	"github.com/sirupsen/logrus"
)

// SystemStats represents system statistics
type SystemStats struct {
	CPUPercent    float64   `json:"cpu_percent"`
	MemoryUsed    uint64    `json:"memory_used"`
	MemoryTotal   uint64    `json:"memory_total"`
	MemoryPercent float64   `json:"memory_percent"`
	DiskUsed      uint64    `json:"disk_used"`
	DiskTotal     uint64    `json:"disk_total"`
	DiskPercent   float64   `json:"disk_percent"`
	Timestamp     time.Time `json:"timestamp"`
	Uptime        float64   `json:"uptime"`
	GoRoutines    int       `json:"goroutines"`
}

// Logger handles system statistics logging
type Logger struct {
	log         *logrus.Logger
	storagePath string
	mu          sync.RWMutex
	startTime   time.Time
}

// NewLogger creates a new stats logger
func NewLogger(log *logrus.Logger, storagePath string) *Logger {
	return &Logger{
		log:         log,
		storagePath: storagePath,
		startTime:   time.Now(),
	}
}

// InitLogger initializes the stats logger
func (l *Logger) InitLogger() error {
	// Ensure storage directory exists
	if err := os.MkdirAll(l.storagePath, 0755); err != nil {
		return fmt.Errorf("failed to create storage directory: %w", err)
	}

	l.log.Info("Stats logger initialized")
	return nil
}

// GetSystemStats retrieves current system statistics
func (l *Logger) GetSystemStats() (*SystemStats, error) {
	l.mu.RLock()
	defer l.mu.RUnlock()

	// Get CPU usage
	cpuPercent, err := cpu.Percent(time.Second, false)
	if err != nil {
		return nil, fmt.Errorf("failed to get CPU stats: %w", err)
	}

	// Get memory stats
	memStats, err := mem.VirtualMemory()
	if err != nil {
		return nil, fmt.Errorf("failed to get memory stats: %w", err)
	}

	// Get disk stats
	diskStats, err := disk.Usage("/")
	if err != nil {
		return nil, fmt.Errorf("failed to get disk stats: %w", err)
	}

	// Calculate uptime
	uptime := time.Since(l.startTime).Seconds()

	stats := &SystemStats{
		CPUPercent:    cpuPercent[0],
		MemoryUsed:    memStats.Used,
		MemoryTotal:   memStats.Total,
		MemoryPercent: memStats.UsedPercent,
		DiskUsed:      diskStats.Used,
		DiskTotal:     diskStats.Total,
		DiskPercent:   diskStats.UsedPercent,
		Timestamp:     time.Now(),
		Uptime:        uptime,
		GoRoutines:    runtime.NumGoroutine(),
	}

	return stats, nil
}

// SaveStats saves statistics to storage
func (l *Logger) SaveStats(stats *SystemStats) error {
	l.mu.Lock()
	defer l.mu.Unlock()

	statsFile := filepath.Join(l.storagePath, "system_stats.json")

	// Read existing stats
	var allStats []SystemStats
	if data, err := os.ReadFile(statsFile); err == nil {
		json.Unmarshal(data, &allStats)
	}

	// Add new stats
	allStats = append(allStats, *stats)

	// Keep only last 1000 entries
	if len(allStats) > 1000 {
		allStats = allStats[len(allStats)-1000:]
	}

	// Write back to file
	data, err := json.MarshalIndent(allStats, "", "  ")
	if err != nil {
		return fmt.Errorf("failed to marshal stats: %w", err)
	}

	if err := os.WriteFile(statsFile, data, 0644); err != nil {
		return fmt.Errorf("failed to write stats file: %w", err)
	}

	return nil
}

// GetTotalStats returns aggregated statistics
func (l *Logger) GetTotalStats() (*SystemStats, error) {
	return l.GetSystemStats()
}
