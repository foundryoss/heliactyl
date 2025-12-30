#!/bin/bash

BASE_URL="http://localhost:8070"

echo "Testing lightd API with full functionality..."

# Health check
echo "1. Health check:"
curl -s "$BASE_URL/health" | jq .
echo -e "\n"

# Create a container
echo "2. Creating nginx container:"
CONTAINER_ID=$(curl -s -X POST "$BASE_URL/containers" \
  -H "Content-Type: application/json" \
  -d '{
    "image": "nginx:latest",
    "name": "test-nginx-new",
    "ports": {
      "80": "8080"
    },
    "env": {
      "ENV": "test"
    }
  }' | jq -r '.data')

echo "Container ID: $CONTAINER_ID"
echo -e "\n"

# List containers
echo "3. Listing containers:"
curl -s "$BASE_URL/containers" | jq .
echo -e "\n"

# Start container
echo "4. Starting container:"
curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/start" | jq .
echo -e "\n"

# Wait a bit
sleep 2

# Get container stats
echo "5. Getting container stats:"
curl -s "$BASE_URL/containers/$CONTAINER_ID/stats" | jq .
echo -e "\n"

# Execute command in container
echo "6. Executing command in container:"
curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/exec" \
  -H "Content-Type: application/json" \
  -d '{
    "command": ["ls", "-la", "/etc"]
  }' | jq .
echo -e "\n"

# Get container logs
echo "7. Getting container logs:"
curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/logs" \
  -H "Content-Type: application/json" \
  -d '{
    "follow": false,
    "tail": "50"
  }' | jq .
echo -e "\n"

# Attach to container
echo "8. Attaching to container:"
ATTACH_RESPONSE=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/attach" | jq .)
echo "$ATTACH_RESPONSE"
EXEC_ID=$(echo "$ATTACH_RESPONSE" | jq -r '.data.exec_id')
echo -e "\n"

# Detach from container
echo "9. Detaching from container:"
curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/detach" \
  -H "Content-Type: application/json" \
  -d "{
    \"exec_id\": \"$EXEC_ID\"
  }" | jq .
echo -e "\n"

# Stop container
echo "10. Stopping container:"
curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/stop" | jq .
echo -e "\n"

# Remove container
echo "11. Removing container:"
curl -s -X DELETE "$BASE_URL/containers/$CONTAINER_ID" | jq .
echo -e "\n"

# Create volume
echo "12. Creating volume:"
curl -s -X POST "$BASE_URL/volumes" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "test-volume",
    "driver": "local"
  }' | jq .
echo -e "\n"

# List volumes
echo "13. Listing volumes:"
curl -s "$BASE_URL/volumes" | jq .
echo -e "\n"

# Remove volume
echo "14. Removing volume:"
curl -s -X DELETE "$BASE_URL/volumes/test-volume" | jq .
echo -e "\n"

echo "Full API test completed!"