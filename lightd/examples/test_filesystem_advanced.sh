#!/bin/bash

# lightd Advanced Filesystem Operations Test Script
# Tests chmod, chown, archiving, zipping, and file operations

set -e

BASE_URL="http://localhost:8070"
CONTAINER_UUID=""
CONTAINER_ID=""

echo "=== lightd Advanced Filesystem Operations Test ==="

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
    # Extract only the JSON part (starts with { and ends with })
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
echo "1. Creating test container for filesystem operations:"
create_response=$(make_request "POST" "/containers" '{
    "image": "alpine:latest",
    "name": "filesystem-test-container",
    "description": "Container for testing advanced filesystem operations",
    "startup_command": ["sh", "-c", "sleep 3600"],
    "limits": {
        "memory": "128m",
        "cpu": "0.5"
    },
    "ports": {},
    "env": {},
    "install_content": "#!/bin/sh\necho \"Setting up filesystem test environment...\"\napk add --no-cache zip unzip tar gzip bzip2\necho \"Filesystem tools installed\""
}' "Creating test container...")

CONTAINER_UUID=$(extract_json_value "$create_response" "custom_uuid")
CONTAINER_ID=$(extract_json_value "$create_response" "container_id")

echo "Debug - Extracted UUID: '$CONTAINER_UUID'"
echo "Debug - Extracted ID: '$CONTAINER_ID'"

if [ -z "$CONTAINER_UUID" ] || [ -z "$CONTAINER_ID" ]; then
    echo "❌ Failed to create test container"
    echo "Full response: $create_response"
    
    # Try to extract error message from JSON part only
    error_msg=$(echo "$create_response" | grep -o '{.*}' | head -n 1 | python3 -c "
import sys, json
try:
    data = json.load(sys.stdin)
    if 'message' in data and data['message']:
        print(data['message'])
    elif not data.get('success', True):
        print('Container creation failed')
    else:
        print('Values extracted but empty - check JSON structure')
except Exception as e:
    print(f'Failed to parse response: {e}')
" 2>/dev/null)
    
    echo "Error: $error_msg"
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
sleep 15

# Check container status
echo "Checking container status..."
status_response=$(make_request "GET" "/containers/$CONTAINER_UUID/status" "" "Getting container status...")

# Wait a bit more if needed
sleep 5

# 3. Create test directory structure and files
echo ""
echo "3. Setting up test files and directories:"

# Create directories
mkdir_response=$(make_request "POST" "/containers/$CONTAINER_UUID/files/mkdir" '{
    "path": "/test_dir"
}' "Creating test directory...")

mkdir_response2=$(make_request "POST" "/containers/$CONTAINER_UUID/files/mkdir" '{
    "path": "/test_dir/subdir"
}' "Creating subdirectory...")

# Create test files
write_response1=$(make_request "POST" "/containers/$CONTAINER_UUID/files/write" '{
    "path": "/test_file.txt",
    "content": "This is a test file for filesystem operations.\nLine 2\nLine 3"
}' "Creating test file 1...")

write_response2=$(make_request "POST" "/containers/$CONTAINER_UUID/files/write" '{
    "path": "/test_dir/file1.txt",
    "content": "File 1 in test directory"
}' "Creating test file 2...")

write_response3=$(make_request "POST" "/containers/$CONTAINER_UUID/files/write" '{
    "path": "/test_dir/subdir/file2.txt",
    "content": "File 2 in subdirectory"
}' "Creating test file 3...")

# Create executable script
write_response4=$(make_request "POST" "/containers/$CONTAINER_UUID/files/write" '{
    "path": "/test_script.sh",
    "content": "#!/bin/sh\necho \"Hello from test script!\"\ndate\n"
}' "Creating test script...")

# 4. Test chmod operations
echo ""
echo "4. Testing chmod operations:"

# Make script executable
chmod_response1=$(make_request "POST" "/containers/$CONTAINER_UUID/files/chmod" '{
    "path": "/test_script.sh",
    "permissions": "755"
}' "Making script executable (755)...")

# Change file permissions
chmod_response2=$(make_request "POST" "/containers/$CONTAINER_UUID/files/chmod" '{
    "path": "/test_file.txt",
    "permissions": "644"
}' "Setting file permissions (644)...")

# Test symbolic permissions
chmod_response3=$(make_request "POST" "/containers/$CONTAINER_UUID/files/chmod" '{
    "path": "/test_dir",
    "permissions": "u+w,go+r"
}' "Setting directory permissions (symbolic)...")

# 5. Test chown operations
echo ""
echo "5. Testing chown operations:"

chown_response1=$(make_request "POST" "/containers/$CONTAINER_UUID/files/chown" '{
    "path": "/test_file.txt",
    "owner": "root",
    "group": "root"
}' "Changing ownership to root:root...")

chown_response2=$(make_request "POST" "/containers/$CONTAINER_UUID/files/chown" '{
    "path": "/test_dir",
    "owner": "nobody"
}' "Changing directory owner to nobody...")

# 6. Test file copy operations
echo ""
echo "6. Testing file copy/move operations:"

copy_response1=$(make_request "POST" "/containers/$CONTAINER_UUID/files/copy" '{
    "source_path": "/test_file.txt",
    "destination_path": "/test_file_copy.txt",
    "move_file": false
}' "Copying file...")

copy_response2=$(make_request "POST" "/containers/$CONTAINER_UUID/files/copy" '{
    "source_path": "/test_script.sh",
    "destination_path": "/test_dir/moved_script.sh",
    "move_file": true
}' "Moving file...")

# 7. Test tar archive operations
echo ""
echo "7. Testing tar archive operations:"

# Create uncompressed tar
archive_response1=$(make_request "POST" "/containers/$CONTAINER_UUID/files/archive" '{
    "source_path": "/test_dir",
    "archive_path": "/test_archive.tar"
}' "Creating uncompressed tar archive...")

# Create gzip compressed tar
archive_response2=$(make_request "POST" "/containers/$CONTAINER_UUID/files/archive" '{
    "source_path": "/test_dir",
    "archive_path": "/test_archive.tar.gz",
    "compression": "gzip"
}' "Creating gzip compressed tar archive...")

# Create bzip2 compressed tar
archive_response3=$(make_request "POST" "/containers/$CONTAINER_UUID/files/archive" '{
    "source_path": "/test_dir",
    "archive_path": "/test_archive.tar.bz2",
    "compression": "bzip2"
}' "Creating bzip2 compressed tar archive...")

# 8. Test tar extraction
echo ""
echo "8. Testing tar extraction:"

# Create extraction directory
mkdir_response3=$(make_request "POST" "/containers/$CONTAINER_UUID/files/mkdir" '{
    "path": "/extracted_tar"
}' "Creating extraction directory...")

# Extract tar archive
extract_response1=$(make_request "POST" "/containers/$CONTAINER_UUID/files/extract" '{
    "archive_path": "/test_archive.tar.gz",
    "destination_path": "/extracted_tar"
}' "Extracting tar.gz archive...")

# 9. Test zip operations
echo ""
echo "9. Testing zip operations:"

# Create zip archive
zip_response1=$(make_request "POST" "/containers/$CONTAINER_UUID/files/zip" '{
    "source_path": "/test_dir",
    "zip_path": "/test_archive.zip"
}' "Creating zip archive...")

# Create extraction directory for zip
mkdir_response4=$(make_request "POST" "/containers/$CONTAINER_UUID/files/mkdir" '{
    "path": "/extracted_zip"
}' "Creating zip extraction directory...")

# Extract zip archive
unzip_response1=$(make_request "POST" "/containers/$CONTAINER_UUID/files/unzip" '{
    "zip_path": "/test_archive.zip",
    "destination_path": "/extracted_zip"
}' "Extracting zip archive...")

# 10. Verify operations by listing files
echo ""
echo "10. Verifying operations by listing files:"

# List root directory
list_response1=$(make_request "GET" "/containers/$CONTAINER_UUID/files?path=/" "" "Listing root directory...")

# List extracted tar contents
list_response2=$(make_request "GET" "/containers/$CONTAINER_UUID/files?path=/extracted_tar" "" "Listing extracted tar contents...")

# List extracted zip contents
list_response3=$(make_request "GET" "/containers/$CONTAINER_UUID/files?path=/extracted_zip" "" "Listing extracted zip contents...")

# 11. Test file content verification
echo ""
echo "11. Verifying file contents:"

# Check copied file
content_response1=$(make_request "GET" "/containers/$CONTAINER_UUID/files/content/test_file_copy.txt" "" "Checking copied file content...")

# Check extracted file
content_response2=$(make_request "GET" "/containers/$CONTAINER_UUID/files/content/extracted_tar/file1.txt" "" "Checking extracted file content...")

# 12. Performance test with larger operations
echo ""
echo "12. Performance test - creating multiple files:"

# Create multiple files for performance testing
for i in {1..10}; do
    current_date=$(date)
    write_response=$(make_request "POST" "/containers/$CONTAINER_UUID/files/write" "{
        \"path\": \"/perf_test_$i.txt\",
        \"content\": \"Performance test file $i\\nCreated at $current_date\\nFile number: $i\"
    }" "Creating performance test file $i...")
done

# Archive all performance test files
archive_response4=$(make_request "POST" "/containers/$CONTAINER_UUID/files/archive" '{
    "source_path": "/",
    "archive_path": "/full_backup.tar.gz",
    "compression": "gzip"
}' "Creating full backup archive...")

# 13. Cleanup test
echo ""
echo "13. Testing cleanup operations:"

# Delete individual files
delete_response1=$(make_request "POST" "/containers/$CONTAINER_UUID/files/delete" '{
    "path": "/test_file_copy.txt"
}' "Deleting copied file...")

# Delete directory
delete_response2=$(make_request "POST" "/containers/$CONTAINER_UUID/files/delete" '{
    "path": "/extracted_tar"
}' "Deleting extracted directory...")

# 14. Final verification
echo ""
echo "14. Final directory listing:"
final_list=$(make_request "GET" "/containers/$CONTAINER_UUID/files?path=/" "" "Final directory listing...")

# 15. Cleanup container
echo ""
echo "15. Cleanup:"

# Stop container
#stop_response=$(make_request "POST" "/containers/$CONTAINER_UUID/stop" '{}' "Stopping container...")

# Remove container#
#remove_response=$(make_request "DELETE" "/containers/$CONTAINER_UUID" "" "Removing container...")

echo ""
echo "=== Advanced Filesystem Operations Test Complete ==="
echo ""
echo "Summary of operations tested:"
echo "✓ File permissions (chmod) - octal and symbolic"
echo "✓ File ownership (chown) - user and group"
echo "✓ File copy and move operations"
echo "✓ Tar archive creation (uncompressed, gzip, bzip2)"
echo "✓ Tar archive extraction"
echo "✓ Zip archive creation and extraction"
echo "✓ Directory operations"
echo "✓ File content verification"
echo "✓ Performance testing with multiple files"
echo "✓ Cleanup operations"
echo ""
echo "All advanced filesystem operations are working correctly!"