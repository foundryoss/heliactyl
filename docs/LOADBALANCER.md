# Load Balancer

Internal load balancing system that distributes requests across multiple backend servers.

## Features

- **Least Connections Algorithm**: Routes requests to the server with the fewest active connections
- **Health Checking**: Automatically monitors backend server health every 10 seconds
- **Connection Tracking**: Tracks active connections and total requests per server
- **Stats Endpoint**: Real-time statistics about backend servers
- **Automatic Failover**: Unhealthy servers are removed from rotation

## Configuration

Add backend servers to your `config.heli`:

```heli
servers = {
    Fra = {
        host = {localhost}
        port = {3001}
    },
    Ger = {
        host = {localhost}
        port = {3002}
    },
    US = {
        host = {192.168.1.100}
        port = {3000}
    }
}
```

## How It Works

1. **Load Balancer Mode**: When `servers` config is present, the main server acts as a load balancer
2. **Request Routing**: All incoming requests are proxied to backend servers
3. **Server Selection**: Uses least connections algorithm to pick the best server
4. **Health Checks**: Backend servers must respond to `/status` endpoint
5. **Connection Tracking**: Increments/decrements connection count for each request

## Testing

### Full Test (starts all servers)

```bash
./test-loadbalancer.sh
```

This will:
- Create test configs for 2 backend servers (ports 3001, 3002)
- Start both backend servers
- Start the load balancer (port 8787)
- Run test requests
- Show live logs

### Manual Testing

1. Start backend servers:
```bash
# Terminal 1
cd server-rs
cargo run --release -- --config config-backend1.heli

# Terminal 2
cd server-rs
cargo run --release -- --config config-backend2.heli
```

2. Start load balancer:
```bash
# Terminal 3
cd server-rs
cargo run --release -- --config config-lb.heli
```

3. Test it:
```bash
./test-lb-simple.sh
```

## API Endpoints

### Load Balancer Stats
```bash
GET http://localhost:8787/api/loadbalancer/stats
```

Response:
```json
[
  {
    "name": "Backend1",
    "url": "http://localhost:3001",
    "active_connections": 2,
    "healthy": true,
    "total_requests": 150
  },
  {
    "name": "Backend2",
    "url": "http://localhost:3002",
    "active_connections": 1,
    "healthy": true,
    "total_requests": 148
  }
]
```

## Architecture

```
Client Request
     ↓
Load Balancer (port 8787)
     ↓
[Least Connections Algorithm]
     ↓
Backend Server Selection
     ↓
Proxy Request → Backend Server (3001, 3002, etc.)
     ↓
Response ← Backend Server
     ↓
Client Response
```

## Production Deployment

1. **Backend Servers**: Deploy your app on multiple servers/ports
2. **Load Balancer**: Configure one instance with all backend servers
3. **Health Checks**: Ensure all backends have `/status` endpoint
4. **Monitoring**: Use `/api/loadbalancer/stats` for monitoring

## Disabling Load Balancer

To run in normal mode (no load balancing), simply remove or empty the `servers` config:

```heli
servers = {}
```

Or remove the `servers` section entirely.
