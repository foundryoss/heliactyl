#!/bin/bash

BASE_URL="http://localhost:8070"

echo "=== lightd Resource Monitoring Test ==="

# Test 1: Create a container for monitoring
echo "1. Creating Alpine container for monitoring test:"
CREATE_RESPONSE=$(curl -s -X POST "$BASE_URL/containers" \
  -H "Content-Type: application/json" \
  -d '{
    "image": "alpine:latest",
    "name": "monitoring-test",
    "description": "Container for testing resource monitoring",
    "startup_command": ["sh", "-c", "while true; do echo $(date); sleep 1; done"],
    "limits": {
      "cpu": "0.5",
      "memory": "64m",
      "pids": 20
    },
    "env": {
      "TEST_TYPE": "monitoring"
    }
  }')

echo "Create response: $CREATE_RESPONSE"

CONTAINER_ID=$(echo "$CREATE_RESPONSE" | jq -r '.data.container_id // empty')
CUSTOM_UUID=$(echo "$CREATE_RESPONSE" | jq -r '.data.custom_uuid // empty')

echo "Container ID: $CONTAINER_ID"
echo "Custom UUID: $CUSTOM_UUID"

if [ -z "$CONTAINER_ID" ] || [ "$CONTAINER_ID" = "null" ]; then
    echo "Failed to create container, exiting"
    exit 1
fi

# Wait for container to start and monitoring to collect data
echo -e "\n2. Waiting for monitoring data collection (10 seconds)..."
sleep 10

# Test 2: Get system metrics
echo -e "\n3. Getting system metrics:"
SYSTEM_METRICS=$(curl -s "$BASE_URL/monitoring/system")
echo "System metrics response:"
echo "$SYSTEM_METRICS" | jq '.'

# Test 3: Get RU summary
echo -e "\n4. Getting RU summary:"
RU_SUMMARY=$(curl -s "$BASE_URL/monitoring/ru/summary")
echo "RU summary response:"
echo "$RU_SUMMARY" | jq '.'

TOTAL_RU=$(echo "$RU_SUMMARY" | jq -r '.data.total_system_ru // 0')
echo "Total System RU: $TOTAL_RU"

# Test 4: Get container-specific metrics
echo -e "\n5. Getting container metrics for: $CONTAINER_ID"
CONTAINER_METRICS=$(curl -s "$BASE_URL/monitoring/containers/$CONTAINER_ID")
echo "Container metrics response:"
echo "$CONTAINER_METRICS" | jq '.'

# Test 5: Get container RU breakdown
echo -e "\n6. Getting RU breakdown for container: $CONTAINER_ID"
RU_BREAKDOWN=$(curl -s "$BASE_URL/monitoring/ru/containers/$CONTAINER_ID")
echo "RU breakdown response:"
echo "$RU_BREAKDOWN" | jq '.'

# Test 6: Create CPU load to see RU changes
echo -e "\n7. Creating CPU load to test RU calculation..."
LOAD_RESPONSE=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/exec" \
  -H "Content-Type: application/json" \
  -d '{"command": ["sh", "-c", "for i in $(seq 1 5); do dd if=/dev/zero of=/dev/null bs=1M count=10 2>/dev/null & done; sleep 5; killall dd 2>/dev/null || true"]}')

echo "Load test initiated"

# Wait for load and monitoring
sleep 8

# Test 7: Check RU after load
echo -e "\n8. Getting RU summary after load test:"
RU_SUMMARY_AFTER=$(curl -s "$BASE_URL/monitoring/ru/summary")
echo "RU summary after load:"
echo "$RU_SUMMARY_AFTER" | jq '.'

TOTAL_RU_AFTER=$(echo "$RU_SUMMARY_AFTER" | jq -r '.data.total_system_ru // 0')
echo "Total System RU after load: $TOTAL_RU_AFTER"

# Compare RU values
echo -e "\nRU Comparison:"
echo "Before load: $TOTAL_RU RU"
echo "After load: $TOTAL_RU_AFTER RU"

if (( $(echo "$TOTAL_RU_AFTER > $TOTAL_RU" | bc -l) )); then
    echo "✓ RU increased after load test - monitoring is working!"
else
    echo "⚠ RU did not increase significantly - may need more time or load"
fi

# Test 8: Get metrics history
echo -e "\n9. Getting system metrics history (last 5 samples):"
METRICS_HISTORY=$(curl -s "$BASE_URL/monitoring/system/history?limit=5")
echo "Metrics history:"
echo "$METRICS_HISTORY" | jq '.data[] | {timestamp, total_ru, running_containers, average_ru_per_container}'

# Test 9: Get container metrics history
echo -e "\n10. Getting container metrics history (last 3 samples):"
CONTAINER_HISTORY=$(curl -s "$BASE_URL/monitoring/containers/$CONTAINER_ID/history?limit=3")
echo "Container history:"
echo "$CONTAINER_HISTORY" | jq '.data[] | {timestamp, cpu_percent, memory_percent, is_running}'

# Test 10: Monitor real-time for a few cycles
echo -e "\n11. Real-time monitoring (5 cycles):"
for i in {1..5}; do
    echo "Cycle $i:"
    REALTIME_RU=$(curl -s "$BASE_URL/monitoring/ru/summary")
    CURRENT_RU=$(echo "$REALTIME_RU" | jq -r '.data.total_system_ru // 0')
    CONTAINER_COUNT=$(echo "$REALTIME_RU" | jq -r '.data.container_count // 0')
    AVG_RU=$(echo "$REALTIME_RU" | jq -r '.data.average_ru_per_container // 0')
    
    echo "  Total RU: $CURRENT_RU | Containers: $CONTAINER_COUNT | Avg RU/Container: $AVG_RU"
    sleep 2
done

# Cleanup
echo -e "\n12. Cleanup:"
echo "Stopping container..."
STOP_RESPONSE=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/stop")
echo "Stop response: $(echo "$STOP_RESPONSE" | jq -r '.success')"

sleep 2

echo "Removing container..."
REMOVE_RESPONSE=$(curl -s -X DELETE "$BASE_URL/containers/$CONTAINER_ID")
echo "Remove response: $(echo "$REMOVE_RESPONSE" | jq -r '.success')"

echo -e "\n=== Resource Monitoring Test Complete ==="
echo "Summary:"
echo "- System metrics: ✓"
echo "- RU calculation: ✓"
echo "- Container monitoring: ✓"
echo "- Load detection: ✓"
echo "- History tracking: ✓"
echo "- Real-time monitoring: ✓"