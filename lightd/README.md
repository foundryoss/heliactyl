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

# TODO
Yes—if you have more than ~1 active viewer per container, “single shared streamer per container” is *dramatically* more effective than the current design.

- **Current behavior:** every WebSocket connection spawns its own `docker.logs(follow=true)` and `docker.stats(stream=true)` stream. With $N$ viewers you do ~$2N$ long-lived Docker streams per container. Docker Engine becomes the bottleneck fast (CPU, memory, socket backpressure).
- **Shared-streamer behavior:** you do **1 logs stream + 1 stats stream per container**, then fan out in-process to all viewers. That changes the Docker load from **O(N)** to **O(1)** per container, which is usually the real win.

Will it make you closer to Wings?
- Architecturally, yes: Wings-like daemons generally avoid “one Docker stats/log stream per viewer” and instead **multiplex** to many clients. Your existing event hub is already the right primitive; you’re just not using it to eliminate duplicate Docker subscriptions yet.
- You’ll still need a couple details to match Wings-like UX: on connect, fetch a **one-time tail** (e.g., last 100 lines) for that viewer, then join the shared live stream; optionally keep a small ring buffer per container to serve “recent output” instantly.

Tradeoff: slightly more daemon-side bookkeeping (per-container tasks + subscriber refcount + optional buffer), but the payoff under real usage is big and it’s the most direct way to reduce overhead without switching languages.