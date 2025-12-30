# lightd - Lightweight Docker Daemon

A simple, fast Docker container management daemon built with Rust, Tokio, and Axum.

## Features

- **Container Management**: Create, start, stop, kill, suspend, and remove containers
- **Volume Management**: Create, list, and remove Docker volumes
- **Attach/Detach**: Connect to and disconnect from running containers
- **RESTful API**: Clean HTTP API for all operations
- **Configuration**: JSON-based configuration for easy customization
- **Async/Await**: Built on Tokio for high performance

## API Endpoints

### Health Check
- `GET /health` - Check daemon status

### Container Operations
- `POST /containers` - Create a new container
- `GET /containers` - List all containers
- `POST /containers/:id/start` - Start a container
- `POST /containers/:id/stop` - Stop a container
- `POST /containers/:id/kill` - Kill a container
- `POST /containers/:id/suspend` - Suspend (pause) a container
- `POST /containers/:id/attach` - Attach to a container
- `POST /containers/:id/detach` - Detach from a container
- `DELETE /containers/:id` - Remove a container

### Volume Operations
- `POST /volumes` - Create a new volume
- `GET /volumes` - List all volumes
- `DELETE /volumes/:name` - Remove a volume

## Configuration

Edit `config.json` to customize:

```json
{
  "server": {
    "host": "0.0.0.0",
    "port": 8080
  },
  "docker": {
    "socket_path": "/var/run/docker.sock"
  },
  "volumes": {
    "base_path": "/var/lib/lightd/volumes",
    "default_size_limit": "10GB"
  },
  "containers": {
    "default_network": "lightd-network",
    "cleanup_timeout": 30,
    "max_containers": 100
  }
}
```

## Usage

### Build and Run
```bash
cargo build --release
./target/release/lightd
```

### Create Container Example
```bash
curl -X POST http://localhost:8080/containers \
  -H "Content-Type: application/json" \
  -d '{
    "image": "nginx:latest",
    "name": "my-nginx",
    "ports": {
      "80": "8080"
    },
    "env": {
      "ENV": "production"
    }
  }'
```

### Start Container
```bash
curl -X POST http://localhost:8080/containers/CONTAINER_ID/start
```

### List Containers
```bash
curl http://localhost:8080/containers
```

## Requirements

- Rust 1.70+
- Docker daemon running
- Access to Docker socket (usually `/var/run/docker.sock`)

## License

MIT License