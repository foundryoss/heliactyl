package monitoring

import (
	"context"
	"encoding/json"
	"fmt"
	"runtime"
	"time"

	"github.com/docker/docker/api/types"
	"github.com/shirou/gopsutil/v3/cpu"
	"github.com/shirou/gopsutil/v3/disk"
	"github.com/shirou/gopsutil/v3/mem"
	"github.com/sirupsen/logrus"

	"NightLightd/internal/docker"
)

// SystemUsage represents system resource usage
type SystemUsage struct {
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
	Containers    int       `json:"containers"`
}

// ContainerStats represents container statistics
type ContainerStats struct {
	ContainerID   string    `json:"container_id"`
	CPUPercent    float64   `json:"cpu_percent"`
	MemoryUsed    uint64    `json:"memory_used"`
	MemoryLimit   uint64    `json:"memory_limit"`
	MemoryPercent float64   `json:"memory_percent"`
	NetworkRx     uint64    `json:"network_rx"`
	NetworkTx     uint64    `json:"network_tx"`
	BlockRead     uint64    `json:"block_read"`
	BlockWrite    uint64    `json:"block_write"`
	Timestamp     time.Time `json:"timestamp"`
}

// Monitor handles system and container monitoring
type Monitor struct {
	dockerClient *docker.Client
	log          *logrus.Logger
	startTime    time.Time
}

// NewMonitor creates a new monitor instance
func NewMonitor(dockerClient *docker.Client, log *logrus.Logger) *Monitor {
	return &Monitor{
		dockerClient: dockerClient,
		log:          log,
		startTime:    time.Now(),
	}
}

// GetSystemUsage returns current system resource usage
func (m *Monitor) GetSystemUsage(ctx context.Context) (*SystemUsage, error) {
	// Get CPU usage
	cpuPercent, err := cpu.Percent(time.Second, false)
	if err != nil {
		return nil, fmt.Errorf("failed to get CPU usage: %w", err)
	}

	// Get memory usage
	memInfo, err := mem.VirtualMemory()
	if err != nil {
		return nil, fmt.Errorf("failed to get memory usage: %w", err)
	}

	// Get disk usage for root partition
	diskInfo, err := disk.Usage("/")
	if err != nil {
		return nil, fmt.Errorf("failed to get disk usage: %w", err)
	}

	// Get container count
	containers, err := m.dockerClient.ListContainers(ctx)
	containerCount := 0
	if err == nil {
		containerCount = len(containers)
	}

	usage := &SystemUsage{
		CPUPercent:    cpuPercent[0],
		MemoryUsed:    memInfo.Used,
		MemoryTotal:   memInfo.Total,
		MemoryPercent: memInfo.UsedPercent,
		DiskUsed:      diskInfo.Used,
		DiskTotal:     diskInfo.Total,
		DiskPercent:   diskInfo.UsedPercent,
		Timestamp:     time.Now(),
		Uptime:        time.Since(m.startTime).Seconds(),
		GoRoutines:    runtime.NumGoroutine(),
		Containers:    containerCount,
	}

	return usage, nil
}

// GetContainerStats returns statistics for a specific container
func (m *Monitor) GetContainerStats(ctx context.Context, containerID string) (*ContainerStats, error) {
	container := m.dockerClient.GetContainer(containerID)

	// Get container stats from Docker
	statsResponse, err := container.Stats(ctx, false)
	if err != nil {
		return nil, fmt.Errorf("failed to get container stats: %w", err)
	}
	defer statsResponse.Body.Close()

	var dockerStats types.StatsJSON
	if err := json.NewDecoder(statsResponse.Body).Decode(&dockerStats); err != nil {
		return nil, fmt.Errorf("failed to decode container stats: %w", err)
	}

	// Calculate CPU percentage
	cpuPercent := calculateCPUPercent(&dockerStats)

	// Calculate memory percentage
	memoryPercent := 0.0
	if dockerStats.MemoryStats.Limit > 0 {
		memoryPercent = float64(dockerStats.MemoryStats.Usage) / float64(dockerStats.MemoryStats.Limit) * 100.0
	}

	// Calculate network I/O
	var networkRx, networkTx uint64
	for _, network := range dockerStats.Networks {
		networkRx += network.RxBytes
		networkTx += network.TxBytes
	}

	// Calculate block I/O
	var blockRead, blockWrite uint64
	for _, blkio := range dockerStats.BlkioStats.IoServiceBytesRecursive {
		if blkio.Op == "Read" {
			blockRead += blkio.Value
		} else if blkio.Op == "Write" {
			blockWrite += blkio.Value
		}
	}

	stats := &ContainerStats{
		ContainerID:   containerID,
		CPUPercent:    cpuPercent,
		MemoryUsed:    dockerStats.MemoryStats.Usage,
		MemoryLimit:   dockerStats.MemoryStats.Limit,
		MemoryPercent: memoryPercent,
		NetworkRx:     networkRx,
		NetworkTx:     networkTx,
		BlockRead:     blockRead,
		BlockWrite:    blockWrite,
		Timestamp:     time.Now(),
	}

	return stats, nil
}

// calculateCPUPercent calculates CPU usage percentage from Docker stats
func calculateCPUPercent(stats *types.StatsJSON) float64 {
	cpuDelta := float64(stats.CPUStats.CPUUsage.TotalUsage - stats.PreCPUStats.CPUUsage.TotalUsage)
	systemDelta := float64(stats.CPUStats.SystemUsage - stats.PreCPUStats.SystemUsage)

	if systemDelta > 0.0 && cpuDelta > 0.0 {
		return (cpuDelta / systemDelta) * float64(len(stats.CPUStats.CPUUsage.PercpuUsage)) * 100.0
	}
	return 0.0
}
