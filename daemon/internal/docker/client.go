package docker

import (
	"bufio"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"time"

	"github.com/docker/docker/api/types"
	"github.com/docker/docker/api/types/container"
	"github.com/docker/docker/api/types/mount"
	"github.com/docker/docker/client"
	"github.com/docker/go-connections/nat"
	"github.com/sirupsen/logrus"
)

// Client wraps the Docker client
type Client struct {
	cli *client.Client
	log *logrus.Logger
}

// NewClient creates a new Docker client
func NewClient(log *logrus.Logger) (*Client, error) {
	cli, err := client.NewClientWithOpts(client.FromEnv, client.WithAPIVersionNegotiation())
	if err != nil {
		return nil, fmt.Errorf("failed to create docker client: %w", err)
	}

	return &Client{
		cli: cli,
		log: log,
	}, nil
}

// Close closes the Docker client
func (c *Client) Close() error {
	return c.cli.Close()
}

// ContainerExecCreate creates an exec instance
func (c *Client) ContainerExecCreate(ctx context.Context, containerID string, config types.ExecConfig) (types.IDResponse, error) {
	return c.cli.ContainerExecCreate(ctx, containerID, config)
}

// ContainerExecAttach attaches to an exec instance
func (c *Client) ContainerExecAttach(ctx context.Context, execID string, config types.ExecStartCheck) (types.HijackedResponse, error) {
	return c.cli.ContainerExecAttach(ctx, execID, config)
}

// Ping checks if Docker is available
func (c *Client) Ping(ctx context.Context) error {
	_, err := c.cli.Ping(ctx)
	return err
}

// Info returns Docker system information
func (c *Client) Info(ctx context.Context) (types.Info, error) {
	return c.cli.Info(ctx)
}

// ListContainers lists all containers
func (c *Client) ListContainers(ctx context.Context) ([]types.Container, error) {
	return c.cli.ContainerList(ctx, types.ContainerListOptions{All: true})
}

// GetContainer returns a container by ID
func (c *Client) GetContainer(containerID string) *Container {
	return &Container{
		ID:     containerID,
		client: c.cli,
		log:    c.log,
	}
}

// Container represents a Docker container
type Container struct {
	ID     string
	client *client.Client
	log    *logrus.Logger
}

// Inspect inspects the container
func (c *Container) Inspect(ctx context.Context) (types.ContainerJSON, error) {
	return c.client.ContainerInspect(ctx, c.ID)
}

// Start starts the container
func (c *Container) Start(ctx context.Context) error {
	return c.client.ContainerStart(ctx, c.ID, types.ContainerStartOptions{})
}

// Stop stops the container
func (c *Container) Stop(ctx context.Context) error {
	timeout := 10
	return c.client.ContainerStop(ctx, c.ID, container.StopOptions{Timeout: &timeout})
}

// Kill kills the container
func (c *Container) Kill(ctx context.Context) error {
	return c.client.ContainerKill(ctx, c.ID, "SIGKILL")
}

// Restart restarts the container
func (c *Container) Restart(ctx context.Context) error {
	timeout := 10
	return c.client.ContainerRestart(ctx, c.ID, container.StopOptions{Timeout: &timeout})
}

// Logs returns container logs
func (c *Container) Logs(ctx context.Context, follow bool) (io.ReadCloser, error) {
	options := types.ContainerLogsOptions{
		ShowStdout: true,
		ShowStderr: true,
		Follow:     follow,
		Timestamps: true,
	}
	return c.client.ContainerLogs(ctx, c.ID, options)
}

// Stats returns container stats
func (c *Container) Stats(ctx context.Context, stream bool) (types.ContainerStats, error) {
	return c.client.ContainerStats(ctx, c.ID, stream)
}

// Attach attaches to the container
func (c *Container) Attach(ctx context.Context) (types.HijackedResponse, error) {
	return c.client.ContainerAttach(ctx, c.ID, types.ContainerAttachOptions{
		Stream: true,
		Stdin:  true,
		Stdout: true,
		Stderr: true,
	})
}

// Remove removes the container
func (c *Container) Remove(ctx context.Context, force bool) error {
	options := types.ContainerRemoveOptions{
		Force: force,
	}
	return c.client.ContainerRemove(ctx, c.ID, options)
}

// ContainerConfig represents container creation configuration
type ContainerConfig struct {
	Name         string      `json:"name"`
	Image        string      `json:"image"`
	Cmd          []string    `json:"cmd,omitempty"`
	Env          []string    `json:"env,omitempty"`
	Hostname     string      `json:"hostname,omitempty"`
	User         string      `json:"user,omitempty"`
	ExposedPorts nat.PortSet `json:"exposedPorts,omitempty"`
	PortBindings nat.PortMap `json:"portBindings,omitempty"`
	Memory       int64       `json:"memory,omitempty"` // in bytes
	CPUCount     int64       `json:"cpuCount,omitempty"`
	VolumePath   string      `json:"volumePath,omitempty"`
	WorkingDir   string      `json:"workingDir,omitempty"`
	NetworkMode  string      `json:"networkMode,omitempty"`
}

// CreateContainer creates a new container with the given configuration
func (c *Client) CreateContainer(ctx context.Context, config *ContainerConfig) (*Container, error) {
	// Set defaults
	if config.WorkingDir == "" {
		config.WorkingDir = "/app"
	}
	if config.NetworkMode == "" {
		config.NetworkMode = "bridge"
	}

	// Create container configuration
	containerConfig := &container.Config{
		Image:        config.Image,
		Cmd:          config.Cmd,
		Env:          config.Env,
		ExposedPorts: config.ExposedPorts,
		WorkingDir:   config.WorkingDir,
		AttachStdout: true,
		AttachStderr: true,
		AttachStdin:  true,
		Tty:          true,
		OpenStdin:    true,
		Hostname:     config.Hostname,
		User:         config.User,
	}

	// Create host configuration
	hostConfig := &container.HostConfig{
		PortBindings: config.PortBindings,
		NetworkMode:  container.NetworkMode(config.NetworkMode),
		RestartPolicy: container.RestartPolicy{
			Name: "unless-stopped",
		},
	}

	// Add volume mount if specified
	if config.VolumePath != "" {
		hostConfig.Mounts = []mount.Mount{
			{
				Type:   mount.TypeBind,
				Source: config.VolumePath,
				Target: "/app/data",
			},
		}
	}

	// Set resource limits
	if config.Memory > 0 {
		hostConfig.Memory = config.Memory
	}
	if config.CPUCount > 0 {
		hostConfig.NanoCPUs = config.CPUCount * 1e9
	}

	// Create the container
	resp, err := c.cli.ContainerCreate(ctx, containerConfig, hostConfig, nil, nil, config.Name)
	if err != nil {
		return nil, fmt.Errorf("failed to create container: %w", err)
	}

	c.log.WithFields(logrus.Fields{
		"containerID": resp.ID[:12],
		"name":        config.Name,
		"image":       config.Image,
	}).Info("Container created successfully")

	return &Container{
		ID:     resp.ID,
		client: c.cli,
		log:    c.log,
	}, nil
}

// PullImage pulls an image from registry
func (c *Client) PullImage(ctx context.Context, imageName string) error {
	c.log.WithField("image", imageName).Info("Pulling image")

	// Add timeout to the context if not already set
	if _, hasDeadline := ctx.Deadline(); !hasDeadline {
		var cancel context.CancelFunc
		ctx, cancel = context.WithTimeout(ctx, 3*time.Minute)
		defer cancel()
	}

	reader, err := c.cli.ImagePull(ctx, imageName, types.ImagePullOptions{})
	if err != nil {
		return fmt.Errorf("failed to pull image %s: %w", imageName, err)
	}
	defer reader.Close()

	// Read the pull progress with timeout
	done := make(chan error, 1)
	go func() {
		scanner := bufio.NewScanner(reader)
		for scanner.Scan() {
			var progress map[string]interface{}
			if err := json.Unmarshal(scanner.Bytes(), &progress); err == nil {
				if status, ok := progress["status"].(string); ok {
					c.log.WithField("image", imageName).Debug(status)
				}
			}
		}
		done <- scanner.Err()
	}()

	select {
	case err := <-done:
		if err != nil {
			return fmt.Errorf("error reading pull progress: %w", err)
		}
	case <-ctx.Done():
		return fmt.Errorf("image pull timeout: %w", ctx.Err())
	}

	c.log.WithField("image", imageName).Info("Image pulled successfully")
	return nil
}

type ContainerStats struct {
	CPUPercent      float64   `json:"cpu_percent"`
	MemoryUsage     uint64    `json:"memory_usage"`
	MemoryLimit     uint64    `json:"memory_limit"`
	MemoryPercent   float64   `json:"memory_percent"`
	NetworkRx       uint64    `json:"network_rx"`
	NetworkTx       uint64    `json:"network_tx"`
	BlockRead       uint64    `json:"block_read"`
	BlockWrite      uint64    `json:"block_write"`
	PIDs            uint64    `json:"pids"`
	Timestamp       time.Time `json:"timestamp"`
	VolumeSize      string    `json:"volume_size,omitempty"`
	DiskLimit       int64     `json:"disk_limit,omitempty"`
	StorageExceeded bool      `json:"storage_exceeded,omitempty"`
}

// ParseStats parses Docker stats response into ContainerStats
func ParseStats(statsJSON []byte) (*ContainerStats, error) {
	var v types.StatsJSON
	if err := json.Unmarshal(statsJSON, &v); err != nil {
		return nil, err
	}

	// Calculate CPU percentage
	cpuDelta := float64(v.CPUStats.CPUUsage.TotalUsage - v.PreCPUStats.CPUUsage.TotalUsage)
	systemDelta := float64(v.CPUStats.SystemUsage - v.PreCPUStats.SystemUsage)
	cpuPercent := 0.0
	if systemDelta > 0.0 && cpuDelta > 0.0 {
		cpuPercent = (cpuDelta / systemDelta) * float64(len(v.CPUStats.CPUUsage.PercpuUsage)) * 100.0
	}

	// Calculate memory percentage
	memoryPercent := 0.0
	if v.MemoryStats.Limit > 0 {
		memoryPercent = float64(v.MemoryStats.Usage) / float64(v.MemoryStats.Limit) * 100.0
	}

	// Get network stats
	var networkRx, networkTx uint64
	for _, network := range v.Networks {
		networkRx += network.RxBytes
		networkTx += network.TxBytes
	}

	// Get block I/O stats
	var blockRead, blockWrite uint64
	for _, blkio := range v.BlkioStats.IoServiceBytesRecursive {
		if blkio.Op == "read" {
			blockRead += blkio.Value
		} else if blkio.Op == "write" {
			blockWrite += blkio.Value
		}
	}

	return &ContainerStats{
		CPUPercent:    cpuPercent,
		MemoryUsage:   v.MemoryStats.Usage,
		MemoryLimit:   v.MemoryStats.Limit,
		MemoryPercent: memoryPercent,
		NetworkRx:     networkRx,
		NetworkTx:     networkTx,
		BlockRead:     blockRead,
		BlockWrite:    blockWrite,
		PIDs:          v.PidsStats.Current,
		Timestamp:     time.Now(),
	}, nil
}
