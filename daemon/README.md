## Features

- **Docker Integration**: Full Docker API integration for container management
- **WebSocket Support**: Real-time container logs, stats, and command execution
- **Volume Management**: Disk usage tracking and limits
- **Statistics Logging**: System and container statistics collection
- **HTTP API**: RESTful API for container and system management
- **Authentication**: Basic authentication for API endpoints
- **Cross-Platform**: Supports Linux, macOS, and Windows

## Architecture

The daemon is structured with the following components:

- **Config**: Configuration management
- **Docker**: Docker client wrapper and container operations
- **WebSocket**: Real-time WebSocket communication
- **Routes**: HTTP API endpoints
- **Stats**: System statistics collection and logging
- **Volume**: Volume management and disk usage tracking
- **Auth**: Authentication middleware

## Installation

### Prerequisites

- Go 1.21 or later
- Docker installed and running
- Git (for version information)

### Build from Source

```bash
# Clone the repository
git clone <repository-url>
cd daemon

# Install dependencies
make deps

# Build the binary
make build

# Run the daemon
make run
```

### Using Docker

```bash
# Build Docker image
make docker-build

# Run with Docker
make docker-run
```

## Configuration

The daemon uses a `config.json` file for configuration:

```json
{
  "version": "0.3.0",
  "port": 8070,
  "key": "daemon-key",
  "remote": "https://api.nadhi.dev/panel",
  "mysql": {
    "host": "localhost",
    "user": "nadhi",
    "password": "password"
  },
  "volumes_path": "./volumes",
  "storage_path": "./storage",
  "available_ports": [25000, 25001, 25002, 25003, 25004]
}
```

### Configuration Options

- `version`: Daemon version
- `port`: HTTP server port
- `key`: Authentication key for API access
- `remote`: Remote API endpoint
- `mysql`: MySQL database configuration (for future use)
- `volumes_path`: Path to store container volumes
- `storage_path`: Path to store daemon data and statistics

## API Endpoints

### Status and Information

- `GET /` - Daemon status and Docker information
- `GET /stats` - System statistics and container counts

### Container Management

- `GET /instances` - List all containers
- `POST /instances` - Create a new container (not implemented)
- `DELETE /instances?id={id}` - Delete a container (not implemented)
- `POST /instances/{id}/power` - Container power actions (start, stop, restart, kill)
- `GET /instances/{id}/stats` - Get container statistics

### WebSocket Endpoints

- `ws://host:port/exec/{containerID}` - Container execution and logs
- `ws://host:port/exec/{containerID}/{volumeID}` - Container execution with volume ID
- `ws://host:port/stats/{containerID}` - Real-time container statistics
- `ws://host:port/stats/{containerID}/{volumeID}` - Real-time stats with volume ID

## WebSocket Protocol

### Authentication

Send an authentication message first:

```json
{
  "event": "auth",
  "args": ["your-auth-key"]
}
```

### Commands

After authentication, you can send various commands:

```json
{
  "event": "cmd",
  "command": "ls -la"
}
```

### Power Actions

```json
{
  "event": "power:start"
}
```

```json
{
  "event": "power:stop"
}
```

```json
{
  "event": "power:restart"
}
```

## Development

### Project Structure

```
skyportd-go/
├── cmd/skyportd/          # Main application entry point
├── internal/              # Internal packages
│   ├── auth/             # Authentication middleware
│   ├── config/           # Configuration management
│   ├── docker/           # Docker client wrapper
│   ├── routes/           # HTTP route handlers
│   ├── stats/            # Statistics collection
│   ├── volume/           # Volume management
│   └── websocket/        # WebSocket handling
├── build/                # Build artifacts
├── volumes/              # Container volumes (created at runtime)
├── storage/              # Daemon storage (created at runtime)
├── config.json           # Configuration file
├── Dockerfile            # Docker build file
├── Makefile              # Build automation
└── README.md             # This file
```

### Building

```bash
# Build for current platform
make build

# Build for all platforms
make build-all

# Run tests
make test

# Run with coverage
make test-coverage

# Format code
make fmt

# Lint code (requires golangci-lint)
make lint
```

### Running

```bash
# Run directly
make run

# Run in development mode
make dev

# Install to system
make install
```

## Docker Support

The daemon includes full Docker support with:

- Container lifecycle management (start, stop, restart, kill)
- Real-time log streaming
- Statistics collection
- Volume mounting and management
- Resource limit enforcement

## Volume Management

The daemon provides volume management features:

- Disk usage calculation
- Storage limits enforcement
- Automatic container stopping when limits exceeded
- Volume size reporting in MiB

## Statistics

System and container statistics are collected including:

- CPU usage percentage
- Memory usage and limits
- Disk usage and limits
- Network I/O statistics
- Process counts
- System uptime

## Authentication

All API endpoints (except WebSocket) use HTTP Basic Authentication. WebSocket connections handle authentication through the initial auth message.

## Logging

The daemon uses structured logging with logrus, providing:

- Timestamp information
- Log levels (debug, info, warn, error)
- Contextual fields
- JSON formatting option

## Error Handling

Comprehensive error handling throughout:

- Docker connectivity checks
- Graceful degradation when services unavailable
- Proper HTTP status codes
- WebSocket error messages
- Resource cleanup on shutdown

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Run `make fmt` and `make lint`
6. Submit a pull request
