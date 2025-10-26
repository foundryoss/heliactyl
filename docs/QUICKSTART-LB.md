# Quick Start: Load Balancer Testing

## Option 1: Automated Test (Recommended)

Run the automated test script that sets up everything:

```bash
cd server-rs
./test-loadbalancer.sh
```

This will:
- ✅ Create test configs
- ✅ Build the project
- ✅ Start 2 backend servers (ports 3001, 3002)
- ✅ Start load balancer (port 8787)
- ✅ Run test requests
- ✅ Show live stats

Press `Ctrl+C` to stop all servers.

## Option 2: Manual Testing

### Step 1: Build
```bash
cd server-rs
cargo build --release
```

### Step 2: Start Backend Servers

**Terminal 1 - Backend 1:**
```bash
cd server-rs
cargo run --release -- --config config-backend1.heli
```

**Terminal 2 - Backend 2:**
```bash
cd server-rs
cargo run --release -- --config config-backend2.heli
```

### Step 3: Start Load Balancer

**Terminal 3 - Load Balancer:**
```bash
cd server-rs
cargo run --release -- --config config-lb.heli
```

### Step 4: Test It

**Terminal 4 - Run Tests:**
```bash
cd server-rs
./test-lb-simple.sh
```

Or manually:
```bash
# Send requests
curl http://localhost:8787/status

# Check stats
curl http://localhost:8787/api/loadbalancer/stats | jq
```

## What to Expect

You should see:
- Requests distributed across backend servers
- Connection counts updating in stats
- Health checks running every 10 seconds
- Automatic failover if a backend goes down

## Troubleshooting

**"No healthy backend servers available"**
- Make sure backend servers are running
- Check they respond to `/status` endpoint
- Wait 10 seconds for health check to complete

**Port already in use**
- Change ports in config files
- Kill existing processes: `lsof -ti:3001 | xargs kill`

**Connection refused**
- Ensure Redis is running: `redis-server`
- Ensure MongoDB is accessible
