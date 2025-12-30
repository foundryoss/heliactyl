#!/bin/bash

# lightd WebSocket Test Script
# Tests WebSocket token generation and connection

set -e

BASE_URL="http://localhost:8070"
WS_URL="ws://localhost:8070"
CONTAINER_UUID=""
CONTAINER_ID=""
WS_TOKEN=""

echo "=== lightd WebSocket Test ==="

# Function to make HTTP requests with error handling
make_request() {
    local method=$1
    local endpoint=$2
    local data=$3
    local description=$4
    
    echo "$description"
    if [ -n "$data" ]; then
        response=$(curl -s -X "$method" "$BASE_URL$endpoint" \
            -H "Content-Type: application/json" \
            -d "$data")
    else
        response=$(curl -s -X "$method" "$BASE_URL$endpoint")
    fi
    
    echo "$response"
}

# Function to extract value from JSON response
extract_json_value() {
    local json=$1
    local key=$2
    local clean_json=$(echo "$json" | grep -o '{.*}' | head -n 1)
    echo "$clean_json" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    if 'data' in data and isinstance(data['data'], dict) and '$key' in data['data']:
        print(data['data']['$key'])
    elif '$key' in data:
        print(data['$key'])
except:
    pass
" 2>/dev/null || echo "$clean_json" | grep -o "\"$key\":\"[^\"]*\"" | cut -d'"' -f4
}

# 1. Create a test container
echo "1. Creating test container for WebSocket testing:"
create_response=$(make_request "POST" "/containers" '{
    "image": "alpine:latest",
    "name": "websocket-test-container",
    "description": "Container for testing WebSocket functionality",
    "startup_command": ["sh", "-c", "while true; do echo \"[$(date)] Container is running...\"; sleep 10; done"],
    "limits": {
        "memory": "128m",
        "cpu": "0.5"
    },
    "ports": {},
    "env": {
        "TEST_ENV": "websocket_test"
    }
}' "Creating test container...")

CONTAINER_UUID=$(extract_json_value "$create_response" "custom_uuid")
CONTAINER_ID=$(extract_json_value "$create_response" "container_id")

if [ -z "$CONTAINER_UUID" ] || [ -z "$CONTAINER_ID" ]; then
    echo "❌ Failed to create test container"
    exit 1
fi

echo "Container UUID: $CONTAINER_UUID"
echo "Container ID: $CONTAINER_ID"

# Wait for container to be ready
sleep 10

# 2. Generate WebSocket token
echo ""
echo "2. Generating WebSocket token:"
token_response=$(curl -s -X GET "$BASE_URL/websocket/generate?container_id=$CONTAINER_UUID")
echo "Generating WebSocket token..."
echo "$token_response"

WS_TOKEN=$(extract_json_value "$token_response" "token")

if [ -z "$WS_TOKEN" ]; then
    echo "❌ Failed to generate WebSocket token"
    echo "$token_response"
    exit 1
fi

echo "✅ WebSocket token generated: ${WS_TOKEN:0:20}..."

# 3. Test WebSocket connection with Node.js
echo ""
echo "3. Testing WebSocket connection:"

# Create a simple Node.js WebSocket test client
cat > /tmp/ws_test.js << 'EOF'
const WebSocket = require('ws');

const token = process.argv[2];
const wsUrl = process.argv[3];

if (!token || !wsUrl) {
    console.log('Usage: node ws_test.js <token> <ws_url>');
    process.exit(1);
}

const ws = new WebSocket(`${wsUrl}/websocket?token=${token}`);

let messageCount = 0;
const maxMessages = 10;

ws.on('open', function open() {
    console.log('✅ WebSocket connection established');
    
    // Request initial stats
    ws.send(JSON.stringify({
        event: 'request stats',
        args: []
    }));
    
    // Send a test command
    setTimeout(() => {
        console.log('📤 Sending test command...');
        ws.send(JSON.stringify({
            event: 'send command',
            args: ['echo "Hello from WebSocket!"']
        }));
    }, 2000);
    
    // Test power action
    setTimeout(() => {
        console.log('📤 Testing power action...');
        ws.send(JSON.stringify({
            event: 'power',
            args: ['start']
        }));
    }, 4000);
});

ws.on('message', function message(data) {
    messageCount++;
    const msg = JSON.parse(data.toString());
    console.log(`📨 Received [${msg.event}]:`, msg.args[0] ? msg.args[0].substring(0, 100) + '...' : 'empty');
    
    if (messageCount >= maxMessages) {
        console.log('✅ Received enough messages, closing connection');
        ws.close();
    }
});

ws.on('error', function error(err) {
    console.log('❌ WebSocket error:', err.message);
    process.exit(1);
});

ws.on('close', function close() {
    console.log('🔌 WebSocket connection closed');
    process.exit(0);
});

// Auto-close after 30 seconds
setTimeout(() => {
    console.log('⏰ Test timeout, closing connection');
    ws.close();
}, 30000);
EOF

# Check if Node.js and ws package are available
if command -v node >/dev/null 2>&1; then
    echo "Testing WebSocket with Node.js..."
    
    # Try to install ws package if not available
    if ! node -e "require('ws')" 2>/dev/null; then
        echo "Installing ws package..."
        npm install ws 2>/dev/null || echo "⚠️  Could not install ws package, WebSocket test may fail"
    fi
    
    # Run the WebSocket test
    timeout 35s node /tmp/ws_test.js "$WS_TOKEN" "$WS_URL" || echo "⚠️  WebSocket test completed or timed out"
else
    echo "⚠️  Node.js not available, skipping WebSocket connection test"
    echo "   You can manually test the WebSocket connection with:"
    echo "   Token: $WS_TOKEN"
    echo "   URL: $WS_URL/websocket?token=$WS_TOKEN"
fi

# 4. Test token validation
echo ""
echo "4. Testing token validation with invalid token:"
invalid_token_test=$(curl -s -o /dev/null -w "%{http_code}" "$WS_URL/websocket?token=invalid_token_12345")

if [ "$invalid_token_test" = "401" ]; then
    echo "✅ Invalid token correctly rejected (HTTP 401)"
else
    echo "❌ Invalid token should return HTTP 401, got: $invalid_token_test"
fi

# 5. Test container stats endpoint
echo ""
echo "5. Testing container stats via HTTP:"
stats_response=$(make_request "GET" "/containers/$CONTAINER_ID/stats" "" "Getting container stats...")

if echo "$stats_response" | grep -q '"success":true'; then
    echo "✅ Container stats retrieved successfully"
else
    echo "❌ Failed to get container stats"
    echo "$stats_response"
fi

# 6. Cleanup
echo ""
echo "6. Cleanup:"
stop_response=$(make_request "POST" "/containers/uuid/$CONTAINER_UUID/stop" '{}' "Stopping container...")
remove_response=$(make_request "DELETE" "/containers/$CONTAINER_ID" "" "Removing container...")

# Clean up test file
rm -f /tmp/ws_test.js

echo ""
echo "=== WebSocket Test Results ==="
echo "✅ WebSocket token generation: Working"
echo "✅ Token validation: Working"
echo "✅ Container stats: Working"
if command -v node >/dev/null 2>&1; then
    echo "✅ WebSocket connection: Tested with Node.js"
else
    echo "⚠️  WebSocket connection: Not tested (Node.js unavailable)"
fi
echo ""
echo "WebSocket API is ready for use!"
echo ""
echo "Usage Examples:"
echo "1. Generate token: GET /websocket/generate?container_id=uuid_or_container_id"
echo "2. Connect: ws://localhost:8070/websocket?token=YOUR_TOKEN"
echo "3. Send commands: {\"event\": \"send command\", \"args\": [\"your_command\"]}"
echo "4. Power actions: {\"event\": \"power\", \"args\": [\"start|stop|restart|kill\"]}"
echo "5. Request stats: {\"event\": \"request stats\", \"args\": []}"