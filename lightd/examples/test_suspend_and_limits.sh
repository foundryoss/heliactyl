#!/bin/bash

# lightd Suspend and Limits Test Script
# Tests suspend/unsuspend functionality and environment variable injection

set -e

BASE_URL="http://localhost:8070"
CONTAINER_UUID=""
CONTAINER_ID=""

echo "=== lightd Suspend and Limits Test ==="

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

# 1. Create a test container with resource limits
echo "1. Creating test container with resource limits:"
create_response=$(make_request "POST" "/containers" '{
    "image": "alpine:latest",
    "name": "suspend-test-container",
    "description": "Container for testing suspend and limits",
    "startup_command": ["sh", "-c", "sleep 3600"],
    "limits": {
        "memory": "128m",
        "cpu": "0.5",
        "disk": "1g",
        "swap": "64m",
        "pids": 100,
        "threads": 50
    },
    "ports": {},
    "env": {
        "TEST_VAR": "test_value",
        "USER_ENV": "user_data"
    }
}' "Creating test container with limits...")

CONTAINER_UUID=$(extract_json_value "$create_response" "custom_uuid")
CONTAINER_ID=$(extract_json_value "$create_response" "container_id")

if [ -z "$CONTAINER_UUID" ] || [ -z "$CONTAINER_ID" ]; then
    echo "❌ Failed to create test container"
    exit 1
fi

echo "Container UUID: $CONTAINER_UUID"
echo "Container ID: $CONTAINER_ID"

# Get container ID from UUID using the lookup endpoint
echo ""
echo "1.5. Looking up container ID from UUID:"
lookup_response=$(make_request "GET" "/containers/uuid/$CONTAINER_UUID" "" "Looking up container by UUID...")
CONTAINER_ID_FROM_LOOKUP=$(extract_json_value "$lookup_response" "container_id")

if [ -n "$CONTAINER_ID_FROM_LOOKUP" ]; then
    echo "✅ Container ID lookup successful: $CONTAINER_ID_FROM_LOOKUP"
    # Use the looked up container ID for consistency
    CONTAINER_ID="$CONTAINER_ID_FROM_LOOKUP"
else
    echo "❌ Failed to lookup container ID from UUID"
    echo "$lookup_response"
fi

# Wait for container to be ready
sleep 10

# 2. Check environment variables in container
echo ""
echo "2. Checking environment variables in container:"
env_response=$(make_request "POST" "/containers/$CONTAINER_ID/exec" '{
    "command": ["env"]
}' "Getting environment variables...")

echo "Environment variables:"
echo "$env_response" | grep -o '"data":"[^"]*"' | cut -d'"' -f4 | grep -E "(LIGHTD_|TEST_|USER_)" || echo "No relevant env vars found"

# 3. Test suspend functionality
echo ""
echo "3. Testing suspend functionality:"
suspend_response=$(make_request "POST" "/containers/uuid/$CONTAINER_UUID/suspend" '{
    "message": "Testing suspend functionality"
}' "Suspending container...")

if echo "$suspend_response" | grep -q '"success":true'; then
    echo "✅ Container suspended successfully"
else
    echo "❌ Failed to suspend container"
    echo "$suspend_response"
fi

# 4. Try to start suspended container (should fail)
echo ""
echo "4. Trying to start suspended container (should fail):"
start_response=$(make_request "POST" "/containers/uuid/$CONTAINER_UUID/start" '{}' "Attempting to start suspended container...")

if echo "$start_response" | grep -q "suspended"; then
    echo "✅ Correctly blocked start of suspended container"
else
    echo "❌ Should have blocked start of suspended container"
    echo "$start_response"
fi

# 5. Try to execute command in suspended container (should fail)
echo ""
echo "5. Trying to execute command in suspended container (should fail):"
exec_response=$(make_request "POST" "/containers/$CONTAINER_ID/exec" '{
    "command": ["echo", "test"]
}' "Attempting to execute command in suspended container...")

if echo "$exec_response" | grep -q "suspended"; then
    echo "✅ Correctly blocked command execution in suspended container"
else
    echo "❌ Should have blocked command execution in suspended container"
    echo "$exec_response"
fi

# 6. Unsuspend container
echo ""
echo "6. Unsuspending container:"
unsuspend_response=$(make_request "POST" "/containers/uuid/$CONTAINER_UUID/unsuspend" '{
    "message": "Testing unsuspend functionality"
}' "Unsuspending container...")

if echo "$unsuspend_response" | grep -q '"success":true'; then
    echo "✅ Container unsuspended successfully"
else
    echo "❌ Failed to unsuspend container"
    echo "$unsuspend_response"
fi

# 7. Start container after unsuspend
echo ""
echo "7. Starting container after unsuspend:"
start_response=$(make_request "POST" "/containers/uuid/$CONTAINER_UUID/start" '{}' "Starting unsuspended container...")

if echo "$start_response" | grep -q '"success":true'; then
    echo "✅ Container started successfully after unsuspend"
else
    echo "❌ Failed to start container after unsuspend"
    echo "$start_response"
fi

# Wait for container to be running
sleep 5

# 8. Update container limits
echo ""
echo "8. Updating container limits:"
update_limits_response=$(make_request "PUT" "/containers/$CONTAINER_ID/limits" '{
    "limits": {
        "memory": "256m",
        "cpu": "1.0",
        "disk": "2g",
        "swap": "128m",
        "pids": 200,
        "threads": 100
    },
    "restart_container": false
}' "Updating container limits...")

if echo "$update_limits_response" | grep -q '"success":true'; then
    echo "✅ Container limits updated successfully"
else
    echo "❌ Failed to update container limits"
    echo "$update_limits_response"
fi

# 9. Check updated environment variables
echo ""
echo "9. Checking updated environment variables:"
sleep 2
env_response=$(make_request "POST" "/containers/$CONTAINER_ID/exec" '{
    "command": ["sh", "-c", "source /tmp/lightd_limits.env 2>/dev/null || true; env | grep LIGHTD_"]
}' "Getting updated environment variables...")

echo "Updated environment variables:"
echo "$env_response" | grep -o '"data":"[^"]*"' | cut -d'"' -f4 | grep "LIGHTD_" || echo "No LIGHTD env vars found"

# 10. Test container status
echo ""
echo "10. Checking container status:"
status_response=$(make_request "GET" "/containers/$CONTAINER_ID/status" "" "Getting container status...")
echo "Container status: $status_response"

# 11. Cleanup
echo ""
echo "11. Cleanup:"
stop_response=$(make_request "POST" "/containers/uuid/$CONTAINER_UUID/stop" '{}' "Stopping container...")
remove_response=$(make_request "DELETE" "/containers/$CONTAINER_ID" "" "Removing container...")

echo ""
echo "=== Test completed ==="
echo "✅ Suspend/unsuspend functionality tested"
echo "✅ Environment variable injection tested"
echo "✅ Limit updates tested"
echo "✅ Access control for suspended containers tested"