#!/bin/bash

BASE_URL="http://localhost:8070"

echo "Testing lightd API..."

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
    "name": "test-nginx",
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

# Stop container
echo "5. Stopping container:"
curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/stop" | jq .
echo -e "\n"

# Remove container
#echo "6. Removing container:"
#curl -s -X DELETE "$BASE_URL/containers/$CONTAINER_ID" | jq .
#echo -e "\n"

# Create volume
echo "7. Creating volume:"
curl -s -X POST "$BASE_URL/volumes" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "test-volume",
    "driver": "local"
  }' | jq .
echo -e "\n"

# List volumes
echo "8. Listing volumes:"
curl -s "$BASE_URL/volumes" | jq .
echo -e "\n"

# Remove volume
echo "9. Removing volume:"
curl -s -X DELETE "$BASE_URL/volumes/test-volume" | jq .
echo -e "\n"

echo "API test completed!"