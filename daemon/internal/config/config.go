package config

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"time"
)

// Config represents the basic daemon configuration (legacy)
type Config struct {
	Version        string      `json:"version"`
	Port           int         `json:"port"`
	Key            string      `json:"key"`
	Remote         string      `json:"remote"`
	MySQL          MySQLConfig `json:"mysql"`
	VolumesPath    string      `json:"volumes_path"`
	StoragePath    string      `json:"storage_path"`
	AvailablePorts []int       `json:"available_ports,omitempty"`
}

// MySQLConfig represents MySQL configuration
type MySQLConfig struct {
	Host     string `json:"host"`
	User     string `json:"user"`
	Password string `json:"password"`
}

// DaemonConfig represents the daemon configuration
type DaemonConfig struct {
	// Server configuration
	APIPort  int    `json:"api_port"`
	APIToken string `json:"api_token"`
	LogLevel string `json:"log_level"`

	// Storage configuration
	DaemonStoragePath    string `json:"daemon_storage_path"`
	ContainerStoragePath string `json:"container_storage_path"`
	SnapshotStoragePath  string `json:"snapshot_storage_path"`

	// Network configuration
	// NOTE: Instead of a numeric port range, configure specific host ports available for allocation using `available_ports`.
	PortRangeStart int   `json:"port_range_start"`
	PortRangeEnd   int   `json:"port_range_end"`
	AvailablePorts []int `json:"available_ports,omitempty"`

	// Docker configuration
	DockerHost string `json:"docker_host"`

	// Cloudflare configuration
	CloudflareToken string `json:"cloudflare_token"`
	CloudflareZone  string `json:"cloudflare_zone"`

	// Worker configuration
	BillingWorkerInterval     time.Duration `json:"billing_worker_interval"`
	HealthWorkerInterval      time.Duration `json:"health_worker_interval"`
	TunnelWorkerInterval      time.Duration `json:"tunnel_worker_interval"`
	PersistenceWorkerInterval time.Duration `json:"persistence_worker_interval"`

	// Resource limits
	MaxContainers         int     `json:"max_containers"`
	MaxCPUPerContainer    float64 `json:"max_cpu_per_container"`
	MaxMemoryPerContainer int64   `json:"max_memory_per_container"`

	// Billing configuration
	BillingRates BillingRates `json:"billing_rates"`
}

// BillingRates represents billing rate configuration
type BillingRates struct {
	CPUPerHour       float64 `json:"cpu_per_hour"`
	MemoryGBPerHour  float64 `json:"memory_gb_per_hour"`
	DiskGBPerHour    float64 `json:"disk_gb_per_hour"`
	NetworkGBPerHour float64 `json:"network_gb_per_hour"`
}

// DefaultDaemonConfig returns a configuration with default values
func DefaultDaemonConfig() *DaemonConfig {
	return &DaemonConfig{
		APIPort:  8080,
		LogLevel: "info",

		DaemonStoragePath:    "./pkg.lat/daemon",
		ContainerStoragePath: "./pkg.lat/volumes/container",
		SnapshotStoragePath:  "./pkg.lat/volumes/snapshots",

		PortRangeStart: 25000,
		PortRangeEnd:   30000,

		DockerHost: "unix:///var/run/docker.sock",

		BillingWorkerInterval:     10 * time.Second,
		HealthWorkerInterval:      30 * time.Second,
		TunnelWorkerInterval:      60 * time.Second,
		PersistenceWorkerInterval: 5 * time.Minute,

		MaxContainers:         100,
		MaxCPUPerContainer:    4.0,
		MaxMemoryPerContainer: 8589934592,

		BillingRates: BillingRates{
			CPUPerHour:       0.05,
			MemoryGBPerHour:  0.01,
			DiskGBPerHour:    0.001,
			NetworkGBPerHour: 0.02,
		},
	}
}

// LoadDaemonConfig loads configuration from file with defaults and validation
func LoadDaemonConfig(configPath string) (*DaemonConfig, error) {
	config := DefaultDaemonConfig()

	if _, err := os.Stat(configPath); os.IsNotExist(err) {
		if err := SaveDaemonConfig(config, configPath); err != nil {
			return nil, fmt.Errorf("failed to create default config: %w", err)
		}
		return config, nil
	}

	data, err := os.ReadFile(configPath)
	if err != nil {
		return nil, fmt.Errorf("failed to read config file: %w", err)
	}

	if err := json.Unmarshal(data, config); err != nil {
		return nil, fmt.Errorf("failed to parse config file: %w", err)
	}

	if err := ValidateDaemonConfig(config); err != nil {
		return nil, fmt.Errorf("invalid configuration: %w", err)
	}

	return config, nil
}

// SaveDaemonConfig saves configuration to file
func SaveDaemonConfig(config *DaemonConfig, configPath string) error {
	dir := filepath.Dir(configPath)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return fmt.Errorf("failed to create config directory: %w", err)
	}

	data, err := json.MarshalIndent(config, "", "  ")
	if err != nil {
		return fmt.Errorf("failed to marshal config: %w", err)
	}

	if err := os.WriteFile(configPath, data, 0644); err != nil {
		return fmt.Errorf("failed to write config file: %w", err)
	}

	return nil
}

// ValidateDaemonConfig validates the configuration parameters
func ValidateDaemonConfig(config *DaemonConfig) error {
	if config.APIPort <= 0 || config.APIPort > 65535 {
		return fmt.Errorf("api_port must be between 1 and 65535, got %d", config.APIPort)
	}

	if config.APIToken == "" {
		return fmt.Errorf("api_token is required")
	}

	validLogLevels := map[string]bool{
		"trace": true, "debug": true, "info": true, "warn": true, "error": true, "fatal": true, "panic": true,
	}
	if !validLogLevels[config.LogLevel] {
		return fmt.Errorf("log_level must be one of: trace, debug, info, warn, error, fatal, panic")
	}

	if config.PortRangeStart <= 0 || config.PortRangeStart > 65535 {
		return fmt.Errorf("port_range_start must be between 1 and 65535, got %d", config.PortRangeStart)
	}

	if config.PortRangeEnd <= 0 || config.PortRangeEnd > 65535 {
		return fmt.Errorf("port_range_end must be between 1 and 65535, got %d", config.PortRangeEnd)
	}

	if config.PortRangeStart >= config.PortRangeEnd {
		return fmt.Errorf("port_range_start (%d) must be less than port_range_end (%d)", config.PortRangeStart, config.PortRangeEnd)
	}

	if config.MaxContainers <= 0 {
		return fmt.Errorf("max_containers must be positive, got %d", config.MaxContainers)
	}

	if config.MaxCPUPerContainer <= 0 {
		return fmt.Errorf("max_cpu_per_container must be positive, got %f", config.MaxCPUPerContainer)
	}

	if config.MaxMemoryPerContainer <= 0 {
		return fmt.Errorf("max_memory_per_container must be positive, got %d", config.MaxMemoryPerContainer)
	}

	if config.DaemonStoragePath == "" {
		return fmt.Errorf("daemon_storage_path cannot be empty")
	}

	if config.ContainerStoragePath == "" {
		return fmt.Errorf("container_storage_path cannot be empty")
	}

	if config.SnapshotStoragePath == "" {
		return fmt.Errorf("snapshot_storage_path cannot be empty")
	}

	if config.BillingWorkerInterval <= 0 {
		return fmt.Errorf("billing_worker_interval must be positive")
	}

	if config.HealthWorkerInterval <= 0 {
		return fmt.Errorf("health_worker_interval must be positive")
	}

	if config.TunnelWorkerInterval <= 0 {
		return fmt.Errorf("tunnel_worker_interval must be positive")
	}

	if config.PersistenceWorkerInterval <= 0 {
		return fmt.Errorf("persistence_worker_interval must be positive")
	}

	// Require explicit list of available host ports to allocate from. This ensures the daemon
	// does not rely on implicit/ranged defaults and uses the ports specified in config.json.
	if len(config.AvailablePorts) == 0 {
		return fmt.Errorf("available_ports must be a non-empty list of host ports to allocate from")
	}

	return nil
}

// Load loads configuration from a file (legacy)
func Load(filename string) (*Config, error) {
	data, err := os.ReadFile(filename)
	if err != nil {
		return nil, fmt.Errorf("failed to read config file: %w", err)
	}

	var config Config
	if err := json.Unmarshal(data, &config); err != nil {
		return nil, fmt.Errorf("failed to parse config file: %w", err)
	}

	if config.Port == 0 {
		config.Port = 8080
	}
	if config.VolumesPath == "" {
		config.VolumesPath = "./volumes"
	}
	if config.StoragePath == "" {
		config.StoragePath = "./storage"
	}

	return &config, nil
}
