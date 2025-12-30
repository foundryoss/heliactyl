#!/bin/bash

BASE_URL="http://localhost:8080"

echo "Simple test with hello-world container..."

# Create a simple hello-world container that should work
echo "1. Creating hello-world container:"
CREATE_RESPONSE=$(curl -s -X POST "$BASE_URL/containers" \
  -H "Content-Type: application/json" \
  -d '{
    "image": "hello-world:latest",
    "name": "test-hello"
  }')

echo "Create response: $CREATE_RESPONSE"

# Extract container ID
CONTAINER_ID=$(echo "$CREATE_RESPONSE" | grep -o '"data":"[^"]*"' | cut -d'"' -f4)
echo "Container ID: $CONTAINER_ID"

if [ -z "$CONTAINER_ID" ]; then
    echo "Failed to create container, exiting"
    exit 1
fi

# Start container
echo -e "\n2. Starting container:"
START_RESPONSE=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/start")
echo "Start response: $START_RESPONSE"

# Wait a bit for hello-world to complete
sleep 3

# List containers to see final status
echo -e "\n3. Listing containers:"
LIST_RESPONSE=$(curl -s "$BASE_URL/containers")
echo "List response: $LIST_RESPONSE"

# Clean up
echo -e "\n4. Removing container:"
REMOVE_RESPONSE=$(curl -s -X DELETE "$BASE_URL/containers/$CONTAINER_ID")
echo "Remove response: $REMOVE_RESPONSE"

echo -e "\n=== Now testing with nginx ==="

# Create nginx container with proper command
echo "5. Creating nginx container with command:"
CREATE_RESPONSE2=$(curl -s -X POST "$BASE_URL/containers" \
  -H "Content-Type: application/json" \
  -d '{
    "image": "nginx:alpine",
    "name": "test-nginx-cmd",
    "command": ["nginx", "-g", "daemon off;"],
    "ports": {
      "80": "8080"
    }
  }')

echo "Create response: $CREATE_RESPONSE2"

# Extract container ID
CONTAINER_ID2=$(echo "$CREATE_RESPONSE2" | grep -o '"data":"[^"]*"' | cut -d'"' -f4)
echo "Container ID: $CONTAINER_ID2"

if [ -n "$CONTAINER_ID2" ]; then
    # Start container
    echo -e "\n6. Starting nginx container:"
    START_RESPONSE2=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID2/start")
    echo "Start response: $START_RESPONSE2"

    # Wait a bit
    sleep 2

    # Check status
    echo -e "\n7. Checking nginx status:"
    LIST_RESPONSE2=$(curl -s "$BASE_URL/containers")
    echo "List response: $LIST_RESPONSE2"

    # Clean up
    echo -e "\n8. Stopping and removing nginx:"
    curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID2/stop"
    curl -s -X DELETE "$BASE_URL/containers/$CONTAINER_ID2"
fi

echo -e "\nSimple test completed!"