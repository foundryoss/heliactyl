#!/bin/bash

BASE_URL="http://localhost:8070"

echo "=== lightd Container Test ==="

# First check if daemon is running
echo "0. Checking if lightd daemon is running:"
HEALTH_RESPONSE=$(curl -s -w "HTTP_CODE:%{http_code}" "$BASE_URL/health" 2>/dev/null)
HTTP_CODE=$(echo "$HEALTH_RESPONSE" | grep -o "HTTP_CODE:[0-9]*" | cut -d: -f2)
HEALTH_BODY=$(echo "$HEALTH_RESPONSE" | sed 's/HTTP_CODE:[0-9]*$//')

echo "HTTP Code: $HTTP_CODE"
echo "Health Response: $HEALTH_BODY"

if [ "$HTTP_CODE" != "200" ]; then
    echo "❌ Daemon not responding on $BASE_URL"
    echo "Make sure lightd is running with: cargo run"
    exit 1
else
    echo "✅ Daemon is running"
fi

# Test 1: Create nginx container with auto port allocation
echo -e "\n1. Creating nginx container with auto port allocation:"
CREATE_RESPONSE=$(curl -s -w "HTTP_CODE:%{http_code}" -X POST "$BASE_URL/containers" \
  -H "Content-Type: application/json" \
  -d '{
    "image": "nginx:alpine",
    "name": "test-nginx-auto",
    "ports": {
      "80": "auto"
    }
  }' 2>/dev/null)

CREATE_HTTP_CODE=$(echo "$CREATE_RESPONSE" | grep -o "HTTP_CODE:[0-9]*" | cut -d: -f2)
CREATE_BODY=$(echo "$CREATE_RESPONSE" | sed 's/HTTP_CODE:[0-9]*$//')

echo "HTTP Code: $CREATE_HTTP_CODE"
echo "Create response: $CREATE_BODY"

if [ "$CREATE_HTTP_CODE" != "200" ]; then
    echo "❌ Failed to create container (HTTP $CREATE_HTTP_CODE)"
    exit 1
fi

# Extract container ID and allocated port using jq if available, otherwise grep
if command -v jq >/dev/null 2>&1; then
    CONTAINER_ID=$(echo "$CREATE_BODY" | jq -r '.data.container_id // empty')
    ALLOCATED_PORT=$(echo "$CREATE_BODY" | jq -r '.data.allocated_ports."80" // empty')
else
    # Fallback parsing without jq
    CONTAINER_ID=$(echo "$CREATE_BODY" | grep -o '"container_id":"[^"]*"' | cut -d'"' -f4)
    ALLOCATED_PORT=$(echo "$CREATE_BODY" | grep -o '"80":"[^"]*"' | cut -d'"' -f4)
fi

echo "Container ID: $CONTAINER_ID"
echo "Allocated Port: $ALLOCATED_PORT"

if [ -z "$CONTAINER_ID" ] || [ "$CONTAINER_ID" = "null" ]; then
    echo "❌ Failed to extract container ID from response"
    exit 1
fi

# Test 2: Start the container
echo -e "\n2. Starting container:"
START_RESPONSE=$(curl -s -w "HTTP_CODE:%{http_code}" -X POST "$BASE_URL/containers/$CONTAINER_ID/start" 2>/dev/null)
START_HTTP_CODE=$(echo "$START_RESPONSE" | grep -o "HTTP_CODE:[0-9]*" | cut -d: -f2)
START_BODY=$(echo "$START_RESPONSE" | sed 's/HTTP_CODE:[0-9]*$//')

echo "HTTP Code: $START_HTTP_CODE"
echo "Start response: $START_BODY"

# Check if start was successful
if command -v jq >/dev/null 2>&1; then
    START_SUCCESS=$(echo "$START_BODY" | jq -r '.success // false')
else
    if echo "$START_BODY" | grep -q '"success":true'; then
        START_SUCCESS="true"
    else
        START_SUCCESS="false"
    fi
fi

if [ "$START_SUCCESS" = "true" ]; then
    echo "✅ Container started successfully"
    
    # Test 3: Check if nginx is responding
    if [ -n "$ALLOCATED_PORT" ] && [ "$ALLOCATED_PORT" != "null" ] && [ "$ALLOCATED_PORT" != "" ]; then
        echo -e "\n3. Testing nginx response on port $ALLOCATED_PORT:"
        sleep 3  # Give nginx time to start
        
        HTTP_RESPONSE=$(curl -s -o /dev/null -w "%{http_code}" "http://localhost:$ALLOCATED_PORT" 2>/dev/null || echo "000")
        if [ "$HTTP_RESPONSE" = "200" ]; then
            echo "✅ Nginx is responding correctly (HTTP $HTTP_RESPONSE)"
        else
            echo "⚠️  Nginx not responding yet (HTTP $HTTP_RESPONSE) - this might be normal"
        fi
    else
        echo "⚠️  No allocated port found to test"
    fi
else
    echo "❌ Container failed to start"
    if command -v jq >/dev/null 2>&1; then
        ERROR_MSG=$(echo "$START_BODY" | jq -r '.message // "Unknown error"')
    else
        ERROR_MSG=$(echo "$START_BODY" | grep -o '"message":"[^"]*"' | cut -d'"' -f4)
    fi
    echo "Error: $ERROR_MSG"
fi

# Test 4: List containers to see status
echo -e "\n4. Listing containers:"
LIST_RESPONSE=$(curl -s "$BASE_URL/containers" 2>/dev/null)
if command -v jq >/dev/null 2>&1; then
    echo "$LIST_RESPONSE" | jq '.data[] | select(.name | contains("test-nginx")) | {id: .id[0:12], name: .name, state: .state, ports: .ports}'
else
    echo "List response: $LIST_RESPONSE"
fi

# Cleanup
echo -e "\n5. Cleanup:"
echo "Stopping container..."
#STOP_RESPONSE=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/stop" 2>/dev/null)
#if command -v jq >/dev/null 2>&1; then
 #   STOP_SUCCESS=$(echo "$STOP_RESPONSE" | jq -r '.success // false')
 #   echo "Stop result: $STOP_SUCCESS"
#else
#    echo "Stop response: $STOP_RESPONSE"
#fi

sleep 1

echo "Removing container..."
#REMOVE_RESPONSE=$(curl -s -X DELETE "$BASE_URL/containers/$CONTAINER_ID" 2>/dev/null)
if command -v jq >/dev/null 2>&1; then
    REMOVE_SUCCESS=$(echo "$REMOVE_RESPONSE" | jq -r '.success // false')
    echo "Remove result: $REMOVE_SUCCESS"
else
    echo "Remove response: $REMOVE_RESPONSE"
fi

echo -e "\n=== Test completed! ==="