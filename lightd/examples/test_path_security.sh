#!/bin/bash

# lightd Path Security Test Script
# Tests that directory traversal attacks are properly blocked

set -e

BASE_URL="http://localhost:8070"
CONTAINER_UUID=""
CONTAINER_ID=""

echo "=== lightd Path Security Test ==="

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

# Function to check if request was blocked (should return error)
check_blocked() {
    local response=$1
    local description=$2
    
    if echo "$response" | grep -q '"success":false' || echo "$response" | grep -q "forbidden\|not allowed\|invalid"; then
        echo "✅ $description - BLOCKED (as expected)"
        return 0
    else
        echo "❌ $description - NOT BLOCKED (security issue!)"
        return 1
    fi
}

# 1. Create a test container
echo "1. Creating test container for security testing:"
create_response=$(make_request "POST" "/containers" '{
    "image": "alpine:latest",
    "name": "security-test-container",
    "description": "Container for testing path security",
    "startup_command": ["sh", "-c", "sleep 3600"],
    "limits": {
        "memory": "64m",
        "cpu": "0.2"
    },
    "ports": {},
    "env": {}
}' "Creating test container...")

CONTAINER_UUID=$(extract_json_value "$create_response" "custom_uuid")
CONTAINER_ID=$(extract_json_value "$create_response" "container_id")

if [ -z "$CONTAINER_UUID" ] || [ -z "$CONTAINER_ID" ]; then
    echo "❌ Failed to create test container"
    exit 1
fi

echo "Container UUID: $CONTAINER_UUID"
echo "Container ID: $CONTAINER_ID"

# 2. Start the container
echo ""
echo "2. Starting container..."
start_response=$(make_request "POST" "/containers/$CONTAINER_UUID/start" '{}' "Starting container...")
sleep 5

# 3. Test directory traversal attacks
echo ""
echo "3. Testing directory traversal attacks:"

# Test various directory traversal patterns
traversal_patterns=(
    "../"
    "../../"
    "../../../etc/passwd"
    "..\\..\\windows\\system32"
    "/.."
    "/../../"
    "/../../../etc"
    "/workspace/../../../etc/passwd"
    "~/../../etc/passwd"
    "\$HOME/../etc/passwd"
    "/workspace/../../etc/passwd"
    "..%2F..%2F..%2Fetc%2Fpasswd"
    "....//....//....//etc//passwd"
    "/workspace/../../../root/.ssh/id_rsa"
    "../proc/version"
    "../../bin/sh"
)

blocked_count=0
total_tests=${#traversal_patterns[@]}

for pattern in "${traversal_patterns[@]}"; do
    echo ""
    echo "Testing pattern: '$pattern'"
    
    # Test file listing
    list_response=$(make_request "GET" "/containers/$CONTAINER_UUID/files?path=$(echo "$pattern" | sed 's/ /%20/g')" "" "Testing directory listing with: $pattern")
    if check_blocked "$list_response" "Directory listing"; then
        ((blocked_count++))
    fi
    
    # Test file reading
    read_response=$(make_request "GET" "/containers/$CONTAINER_UUID/files/content$(echo "$pattern" | sed 's/^/\//' | sed 's/ /%20/g')" "" "Testing file reading with: $pattern")
    if check_blocked "$read_response" "File reading"; then
        ((blocked_count++))
    fi
    
    # Test file writing
    write_response=$(make_request "POST" "/containers/$CONTAINER_UUID/files/write" "{
        \"path\": \"$pattern\",
        \"content\": \"malicious content\"
    }" "Testing file writing with: $pattern")
    if check_blocked "$write_response" "File writing"; then
        ((blocked_count++))
    fi
done

# 4. Test other dangerous patterns
echo ""
echo "4. Testing other dangerous patterns:"

dangerous_patterns=(
    "/etc/passwd"
    "/root/.ssh/id_rsa"
    "/proc/version"
    "/sys/class/net"
    "~root/.bashrc"
    "\$HOME/.profile"
    "/workspace/\$(whoami)"
    "/workspace/\`id\`"
    "/workspace/file|cat /etc/passwd"
    "/workspace/file;cat /etc/passwd"
    "/workspace/file&cat /etc/passwd"
    "/workspace/file(cat /etc/passwd)"
    "/workspace/file{cat,/etc/passwd}"
    "/workspace/file[cat /etc/passwd]"
)

for pattern in "${dangerous_patterns[@]}"; do
    echo ""
    echo "Testing dangerous pattern: '$pattern'"
    
    write_response=$(make_request "POST" "/containers/$CONTAINER_UUID/files/write" "{
        \"path\": \"$pattern\",
        \"content\": \"test content\"
    }" "Testing dangerous pattern: $pattern")
    if check_blocked "$write_response" "Dangerous pattern"; then
        ((blocked_count++))
    fi
done

# 5. Test null bytes and control characters
echo ""
echo "5. Testing null bytes and control characters:"

control_patterns=(
    "/workspace/file\x00.txt"
    "/workspace/file\n.txt"
    "/workspace/file\r.txt"
    "/workspace/file\t.txt"
    "/workspace/file\x01.txt"
)

for pattern in "${control_patterns[@]}"; do
    echo ""
    echo "Testing control character pattern: '$pattern'"
    
    write_response=$(make_request "POST" "/containers/$CONTAINER_UUID/files/write" "{
        \"path\": \"$pattern\",
        \"content\": \"test content\"
    }" "Testing control characters: $pattern")
    if check_blocked "$write_response" "Control characters"; then
        ((blocked_count++))
    fi
done

# 6. Test legitimate paths (should work)
echo ""
echo "6. Testing legitimate paths (should work):"

legitimate_patterns=(
    "/test.txt"
    "/folder/file.txt"
    "/data/logs/app.log"
    "/config/settings.json"
    "/uploads/image.png"
)

working_count=0
for pattern in "${legitimate_patterns[@]}"; do
    echo ""
    echo "Testing legitimate pattern: '$pattern'"
    
    write_response=$(make_request "POST" "/containers/$CONTAINER_UUID/files/write" "{
        \"path\": \"$pattern\",
        \"content\": \"legitimate content\"
    }" "Testing legitimate path: $pattern")
    
    if echo "$write_response" | grep -q '"success":true'; then
        echo "✅ Legitimate path allowed: $pattern"
        ((working_count++))
    else
        echo "❌ Legitimate path blocked: $pattern"
    fi
done

# 7. Test archive operations with dangerous paths
echo ""
echo "7. Testing archive operations with dangerous paths:"

archive_response=$(make_request "POST" "/containers/$CONTAINER_UUID/files/archive" '{
    "source_path": "../../etc",
    "archive_path": "/malicious.tar"
}' "Testing archive with traversal path...")
if check_blocked "$archive_response" "Archive with traversal"; then
    ((blocked_count++))
fi

# 8. Cleanup
echo ""
echo "8. Cleanup:"
stop_response=$(make_request "POST" "/containers/$CONTAINER_UUID/stop" '{}' "Stopping container...")
remove_response=$(make_request "DELETE" "/containers/$CONTAINER_UUID" "" "Removing container...")

# 9. Results
echo ""
echo "=== Path Security Test Results ==="
echo ""
echo "Dangerous patterns tested: $((${#traversal_patterns[@]} * 3 + ${#dangerous_patterns[@]} + ${#control_patterns[@]} + 1))"
echo "Patterns blocked: $blocked_count"
echo "Legitimate paths tested: ${#legitimate_patterns[@]}"
echo "Legitimate paths working: $working_count"
echo ""

if [ $blocked_count -gt $((total_tests * 2)) ] && [ $working_count -eq ${#legitimate_patterns[@]} ]; then
    echo "🔒 SECURITY TEST PASSED!"
    echo "✅ Directory traversal attacks are properly blocked"
    echo "✅ Legitimate paths are allowed"
    echo "✅ Path sanitization is working correctly"
else
    echo "🚨 SECURITY TEST FAILED!"
    echo "❌ Some dangerous patterns were not blocked"
    echo "❌ Path sanitization needs improvement"
    exit 1
fi

echo ""
echo "Path security is properly implemented!"