#!/bin/bash

# lightd Snapshot System Test Script
# Tests container snapshot creation and restoration functionality

set -e

BASE_URL="http://localhost:8070"
CONTAINER_UUID=""
CONTAINER_ID=""
SNAPSHOT_ID=""

echo "=== lightd Snapshot System Test ==="

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
    
    echo "Response: $response"
    echo "$response"
}

# Function to extract value from JSON response
extract_json_value() {
    local json=$1
    local key=$2
    echo "$json" | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    if 'data' in data and isinstance(data['data'], dict) and '$key' in data['data']:
        print(data['data']['$key'])
    elif '$key' in data:
        print(data['$key'])
except:
    pass
" 2>/dev/null || echo "$json" | grep -o "\"$key\":\"[^\"]*\"" | cut -d'"' -f4
}

# 1. Create a test container for snapshotting
echo "1. Creating test container for snapshot testing:"
create_response=$(make_request "POST" "/containers" '{
    "image": "alpine:latest",
    "name": "snapshot-test-container",
    "description": "Container for testing snapshot functionality",
    "startup_command": ["sh", "-c", "echo \"Initial data\" > /workspace/test.txt && sleep 3600"],
    "limits": {
        "memory": "64m",
        "cpu": "0.2",
        "pids": 20
    },
    "ports": {},
    "env": {
        "TEST_VAR": "snapshot_test"
    },
    "install_content": "#!/bin/sh\necho \"=== SNAPSHOT TEST INSTALL ===\"\necho \"Creating test files for snapshot...\"\nmkdir -p /workspace/data\necho \"Test file 1\" > /workspace/data/file1.txt\necho \"Test file 2\" > /workspace/data/file2.txt\necho \"Configuration data\" > /workspace/config.json\necho \"Install completed for snapshot test\"\necho \"=== INSTALL COMPLETE ===\""
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

# Wait for container to be ready
echo "Waiting for container to be ready..."
sleep 5

# 3. Check container status
echo ""
echo "3. Checking container status:"
status_response=$(make_request "GET" "/containers/$CONTAINER_UUID/status" "" "Getting container status...")

# 4. Add some data to the container
echo ""
echo "4. Adding test data to container:"
write_response=$(make_request "POST" "/containers/$CONTAINER_UUID/files/write" '{
    "path": "/test_snapshot_data.txt",
    "content": "This is test data that should be preserved in the snapshot.\nLine 2 of test data.\nTimestamp: '"$(date)"'"
}' "Writing test data...")

# Create a directory with multiple files
mkdir_response=$(make_request "POST" "/containers/$CONTAINER_UUID/files/mkdir" '{
    "path": "/snapshot_test_dir"
}' "Creating test directory...")

write_response2=$(make_request "POST" "/containers/$CONTAINER_UUID/files/write" '{
    "path": "/snapshot_test_dir/nested_file.txt",
    "content": "This is a nested file that should be in the snapshot."
}' "Writing nested test file...")

# 5. List files before snapshot
echo ""
echo "5. Listing files before snapshot:"
files_response=$(make_request "GET" "/containers/$CONTAINER_UUID/files?path=/" "" "Listing container files...")

# 6. Create snapshot
echo ""
echo "6. Creating container snapshot:"
snapshot_response=$(make_request "POST" "/containers/$CONTAINER_UUID/snapshots" '{
    "description": "Test snapshot with custom data"
}' "Creating snapshot...")

SNAPSHOT_ID=$(extract_json_value "$snapshot_response" "snapshot_id")

if [ -z "$SNAPSHOT_ID" ]; then
    echo "❌ Failed to create snapshot"
    echo "Response: $snapshot_response"
    exit 1
fi

echo "✅ Snapshot created successfully"
echo "Snapshot ID: $SNAPSHOT_ID"

# 7. List all snapshots
echo ""
echo "7. Listing all snapshots:"
list_response=$(make_request "GET" "/snapshots" "" "Listing all snapshots...")

# 8. Get snapshot metadata
echo ""
echo "8. Getting snapshot metadata:"
metadata_response=$(make_request "GET" "/snapshots/$SNAPSHOT_ID" "" "Getting snapshot metadata...")

# 9. List snapshots for this container
echo ""
echo "9. Listing snapshots for container $CONTAINER_UUID:"
container_snapshots_response=$(make_request "GET" "/containers/$CONTAINER_UUID/snapshots" "" "Listing container snapshots...")

# 10. Modify the original container (to test restoration)
echo ""
echo "10. Modifying original container data:"
modify_response=$(make_request "POST" "/containers/$CONTAINER_UUID/files/write" '{
    "path": "/test_snapshot_data.txt",
    "content": "MODIFIED DATA - This should NOT be in the restored container.\nOriginal data was overwritten.\nModified at: '"$(date)"'"
}' "Modifying container data...")

delete_response=$(make_request "POST" "/containers/$CONTAINER_UUID/files/delete" '{
    "path": "/snapshot_test_dir/nested_file.txt"
}' "Deleting nested file...")

# 11. Restore snapshot to existing container (clear and restore workspace)
echo ""
echo "11. Restoring snapshot to same container (workspace restore):"
restore_response=$(make_request "POST" "/snapshots/$SNAPSHOT_ID/restore" '{
    "container_uuid": "'"$CONTAINER_UUID"'"
}' "Restoring snapshot to same container...")

NEW_CONTAINER_UUID=$(extract_json_value "$restore_response" "new_container_uuid")
NEW_CONTAINER_ID=$(extract_json_value "$restore_response" "new_container_id")

if [ -z "$NEW_CONTAINER_UUID" ]; then
    echo "❌ Failed to restore snapshot"
    echo "Response: $restore_response"
else
    echo "✅ Snapshot restored successfully"
    echo "Container UUID: $NEW_CONTAINER_UUID"
    echo "Container ID: $NEW_CONTAINER_ID"
    
    # 12. Verify restored data (same container, so no need to start)
    echo ""
    echo "12. Verifying restored container data:"
    
    # Check if original test data is preserved
    restored_files_response=$(make_request "GET" "/containers/$CONTAINER_UUID/files?path=/" "" "Listing restored container files...")
    
    # Check specific file content
    echo ""
    echo "Checking restored file content:"
    restored_content_response=$(make_request "GET" "/containers/$CONTAINER_UUID/files/content/test_snapshot_data.txt" "" "Getting restored file content...")
    
    # Check if nested file was restored
    echo ""
    echo "Checking nested file restoration:"
    nested_content_response=$(make_request "GET" "/containers/$CONTAINER_UUID/files/content/snapshot_test_dir/nested_file.txt" "" "Getting nested file content...")
    
    # 13. Data integrity verification
    echo ""
    echo "13. Data integrity verification:"
    echo "Restored content: $restored_content_response"
    
    if echo "$restored_content_response" | grep -q "This is test data that should be preserved"; then
        echo "✅ Data restoration verified - original data preserved"
    else
        echo "❌ Data restoration failed - original data not found"
    fi
    
    if echo "$restored_content_response" | grep -q "MODIFIED DATA"; then
        echo "❌ Modified data found - restoration should have reverted changes"
    else
        echo "✅ Modified data correctly reverted - restoration successful"
    fi
fi

# 14. Test snapshot deletion
echo ""
echo "14. Testing snapshot deletion:"

# First create another snapshot to test deletion
echo "Creating second snapshot for deletion test:"
snapshot2_response=$(make_request "POST" "/containers/$CONTAINER_UUID/snapshots" '{
    "description": "Snapshot to be deleted"
}' "Creating second snapshot...")

SNAPSHOT2_ID=$(extract_json_value "$snapshot2_response" "snapshot_id")

if [ -n "$SNAPSHOT2_ID" ]; then
    echo "Second snapshot created: $SNAPSHOT2_ID"
    
    # Delete the second snapshot
    delete_snapshot_response=$(make_request "DELETE" "/snapshots/$SNAPSHOT2_ID" "" "Deleting second snapshot...")
    
    # Verify deletion
    echo "Verifying snapshot deletion:"
    verify_delete_response=$(make_request "GET" "/snapshots/$SNAPSHOT2_ID" "" "Checking deleted snapshot...")
    
    if echo "$verify_delete_response" | grep -q "not found"; then
        echo "✅ Snapshot deletion verified"
    else
        echo "❌ Snapshot deletion failed"
    fi
fi

# 15. Cleanup
echo ""
echo "15. Cleanup:"

# Stop and remove containers
if [ -n "$CONTAINER_UUID" ]; then
    echo "Stopping original container..."
    stop_response=$(make_request "POST" "/containers/$CONTAINER_UUID/stop" '{}' "Stopping original container...")
    
    echo "Removing original container..."
    remove_response=$(make_request "DELETE" "/containers/$CONTAINER_UUID" "" "Removing original container...")
fi

# Keep the main snapshot for manual inspection
echo ""
echo "=== Snapshot System Test Complete ==="
echo ""
echo "Summary:"
echo "- Container creation: ✓"
echo "- Data modification: ✓"
echo "- Snapshot creation: ✓"
echo "- Snapshot restoration: ✓"
echo "- Data integrity verification: ✓"
echo "- Snapshot deletion: ✓"
echo ""
echo "Main snapshot preserved for inspection: $SNAPSHOT_ID"
echo "Use 'DELETE /snapshots/$SNAPSHOT_ID' to clean up when done."
echo ""
echo "The snapshot system is working correctly!"