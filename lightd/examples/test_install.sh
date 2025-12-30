#!/bin/bash

BASE_URL="http://localhost:8070"

echo "=== lightd Installation Test ==="

# Test 1: Create Alpine container with comprehensive installation script
echo "1. Creating Alpine container with comprehensive installation script:"
CREATE_RESPONSE=$(curl -s -X POST "$BASE_URL/containers" \
  -H "Content-Type: application/json" \
  -d '{
    "image": "alpine:latest",
    "name": "alpine-install-test",
    "description": "Alpine container for testing installation functionality",
    "startup_command": ["sleep", "3600"],
    "limits": {
      "cpu": "0.5",
      "memory": "128m",
      "swap": "64m",
      "pids": 50,
      "threads": 25
    },
    "env": {
      "INSTALL_TEST": "true",
      "CONTAINER_TYPE": "alpine-test"
    },
    "install_content": "#!/bin/sh\nset -e\necho \"=== INSTALLATION START ===\"\necho \"Container UUID: $HOSTNAME\"\necho \"Current user: $(whoami)\"\necho \"Current directory: $(pwd)\"\necho \"System info: $(uname -a)\"\n\necho \"Updating package manager...\"\napk update\n\necho \"Installing packages...\"\napk add curl wget htop nano vim git bash\n\necho \"Creating directories in workspace...\"\nmkdir -p data logs config\n\necho \"Creating test files...\"\necho \"Installation completed at $(date)\" > data/install.log\necho \"Container UUID: $HOSTNAME\" >> data/install.log\necho \"Installed packages: curl wget htop nano vim git bash\" >> data/install.log\necho \"Test data\" > data/test.txt\n\necho \"Setting permissions...\"\nchmod 755 data\nchmod 644 data/*\n\necho \"Verifying installation...\"\nls -la data/\ncat data/install.log\n\necho \"Testing installed packages...\"\nwhich curl && echo \"curl: OK\" || echo \"curl: FAILED\"\nwhich wget && echo \"wget: OK\" || echo \"wget: FAILED\"\nwhich git && echo \"git: OK\" || echo \"git: FAILED\"\n\necho \"Creating verification script...\"\ncat > verify.sh << EOF\n#!/bin/sh\necho \"Verification script running...\"\necho \"Files in data:\"\nls -la data/\necho \"Install log contents:\"\ncat data/install.log\necho \"Verification complete\"\nEOF\nchmod +x verify.sh\n\necho \"=== INSTALLATION COMPLETE ===\""
  }')

echo "Create response: $CREATE_RESPONSE"

# Extract container information
CONTAINER_ID=$(echo "$CREATE_RESPONSE" | jq -r '.data.container_id // empty')
CUSTOM_UUID=$(echo "$CREATE_RESPONSE" | jq -r '.data.custom_uuid // empty')

echo "Container ID: $CONTAINER_ID"
echo "Custom UUID: $CUSTOM_UUID"

if [ -z "$CONTAINER_ID" ] || [ "$CONTAINER_ID" = "null" ]; then
    echo "Failed to create container, exiting"
    exit 1
fi

# Test 2: Monitor installation progress
echo -e "\n2. Monitoring installation progress:"
for i in {1..30}; do
    STATUS_RESPONSE=$(curl -s "$BASE_URL/containers/$CONTAINER_ID/status")
    CONTAINER_STATUS=$(echo "$STATUS_RESPONSE" | jq -r '.data.status // "unknown"')
    echo "Status check $i: $CONTAINER_STATUS"
    
    if [ "$CONTAINER_STATUS" = "ready" ]; then
        echo "✓ Installation completed successfully"
        break
    elif [ "$CONTAINER_STATUS" = "install_failed" ]; then
        echo "✗ Installation failed"
        break
    elif [ "$CONTAINER_STATUS" = "failed" ]; then
        echo "✗ Container failed"
        break
    fi
    sleep 2
done

# Test 3: Get installation logs
echo -e "\n3. Getting installation logs:"
LOGS_RESPONSE=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/logs" \
  -H "Content-Type: application/json" \
  -d '{"follow": false, "tail": "100"}')

echo "=== INSTALLATION LOGS ==="
LOGS_DATA=$(echo "$LOGS_RESPONSE" | jq -r '.data // "No logs available"')
echo "$LOGS_DATA"
echo "=== END INSTALLATION LOGS ==="

# Check for installation markers in logs
INSTALL_START=$(echo "$LOGS_DATA" | grep -c "INSTALLATION START" || echo "0")
INSTALL_COMPLETE=$(echo "$LOGS_DATA" | grep -c "INSTALLATION COMPLETE" || echo "0")
PACKAGE_INSTALL=$(echo "$LOGS_DATA" | grep -c "Installing packages" || echo "0")

echo -e "\nInstallation log analysis:"
echo "- Installation start marker: $INSTALL_START"
echo "- Installation complete marker: $INSTALL_COMPLETE"
echo "- Package installation: $PACKAGE_INSTALL"

if [ "$INSTALL_START" -gt 0 ] && [ "$INSTALL_COMPLETE" -gt 0 ]; then
    echo "✓ Installation script executed successfully"
else
    echo "✗ Installation script may have failed"
fi

# Test 4: Verify installation by executing commands in container
if [ "$CONTAINER_STATUS" = "ready" ]; then
    echo -e "\n4. Verifying installation by executing commands in container:"
    
    # Test file system changes
    echo "Testing file system changes..."
    EXEC_RESPONSE=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/exec" \
      -H "Content-Type: application/json" \
      -d '{"command": ["ls", "-la", "data/"]}')
    
    echo "Directory listing response:"
    echo "$EXEC_RESPONSE" | jq -r '.data // "No response"'
    
    # Test installed packages
    echo -e "\nTesting installed packages..."
    CURL_TEST=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/exec" \
      -H "Content-Type: application/json" \
      -d '{"command": ["which", "curl"]}')
    
    echo "Curl test response:"
    echo "$CURL_TEST" | jq -r '.data // "No response"'
    
    # Run verification script
    echo -e "\nRunning verification script..."
    VERIFY_RESPONSE=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/exec" \
      -H "Content-Type: application/json" \
      -d '{"command": ["./verify.sh"]}')
    
    echo "Verification script response:"
    echo "$VERIFY_RESPONSE" | jq -r '.data // "No response"'
    
    # Check install log contents
    echo -e "\nChecking install log contents..."
    LOG_CONTENTS=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/exec" \
      -H "Content-Type: application/json" \
      -d '{"command": ["cat", "data/install.log"]}')
    
    echo "Install log contents:"
    echo "$LOG_CONTENTS" | jq -r '.data // "No log file found"'
fi

# Test 5: Test update functionality
echo -e "\n5. Testing update functionality:"
UPDATE_RESPONSE=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/update" \
  -H "Content-Type: application/json" \
  -d '{
    "update_content": "#!/bin/sh\necho \"=== UPDATE START ===\"\necho \"Running update at $(date)\"\napk add tree jq\necho \"Update completed at $(date)\" >> data/install.log\necho \"Updated packages: tree jq\" >> data/install.log\necho \"=== UPDATE COMPLETE ===\""
  }')

echo "Update response: $UPDATE_RESPONSE"
UPDATE_SUCCESS=$(echo "$UPDATE_RESPONSE" | jq -r '.success')

if [ "$UPDATE_SUCCESS" = "true" ]; then
    echo "✓ Update initiated successfully"
    
    # Wait for update to complete
    echo "Waiting for update to complete..."
    for i in {1..20}; do
        STATUS_RESPONSE=$(curl -s "$BASE_URL/containers/$CONTAINER_ID/status")
        CONTAINER_STATUS=$(echo "$STATUS_RESPONSE" | jq -r '.data.status // "unknown"')
        echo "Update status check $i: $CONTAINER_STATUS"
        
        if [ "$CONTAINER_STATUS" = "ready" ]; then
            echo "✓ Update completed successfully"
            break
        elif [ "$CONTAINER_STATUS" = "update_failed" ]; then
            echo "✗ Update failed"
            break
        fi
        sleep 2
    done
    
    # Verify update
    if [ "$CONTAINER_STATUS" = "ready" ]; then
        echo -e "\nVerifying update..."
        UPDATE_LOG=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/exec" \
          -H "Content-Type: application/json" \
          -d '{"command": ["tail", "-5", "data/install.log"]}')
        
        echo "Updated install log:"
        echo "$UPDATE_LOG" | jq -r '.data // "No log available"'
        
        # Test new packages
        TREE_TEST=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/exec" \
          -H "Content-Type: application/json" \
          -d '{"command": ["which", "tree"]}')
        
        if [[ $(echo "$TREE_TEST" | jq -r '.data') == *"/usr/bin/tree"* ]]; then
            echo "✓ Update packages installed successfully"
        else
            echo "✗ Update packages not found"
        fi
    fi
else
    echo "✗ Update failed to initiate"
    echo "Error: $(echo "$UPDATE_RESPONSE" | jq -r '.message')"
fi

# Test 6: Test filesystem functionality
echo -e "\n6. Testing filesystem functionality:"

if [ "$CONTAINER_STATUS" = "ready" ]; then
    # Test directory listing
    echo "Testing directory listing..."
    FILES_RESPONSE=$(curl -s "$BASE_URL/containers/$CONTAINER_ID/files?path=/")
    echo "Files response: $FILES_RESPONSE"
    
    FILES_SUCCESS=$(echo "$FILES_RESPONSE" | jq -r '.success')
    if [ "$FILES_SUCCESS" = "true" ]; then
        echo "✓ Directory listing successful"
        
        # Show directory contents
        echo "Directory contents:"
        echo "$FILES_RESPONSE" | jq -r '.data.files[] | "\(.name) (\(.is_directory | if . then "dir" else "file" end)) - \(.permissions)"'
        
        # Test file content reading
        echo -e "\nTesting file content reading..."
        FILE_CONTENT=$(curl -s "$BASE_URL/containers/$CONTAINER_ID/files/content/data/install.log")
        echo "File content response:"
        echo "$FILE_CONTENT" | jq -r '.data // "No content"'
        
        # Test file writing
        echo -e "\nTesting file writing..."
        WRITE_RESPONSE=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/files/write" \
          -H "Content-Type: application/json" \
          -d '{
            "path": "/test_write.txt",
            "content": "This is a test file created via API\nLine 2\nLine 3"
          }')
        
        echo "Write response: $WRITE_RESPONSE"
        WRITE_SUCCESS=$(echo "$WRITE_RESPONSE" | jq -r '.success')
        
        if [ "$WRITE_SUCCESS" = "true" ]; then
            echo "✓ File write successful"
            
            # Verify file was written
            echo "Verifying written file..."
            WRITTEN_CONTENT=$(curl -s "$BASE_URL/containers/$CONTAINER_ID/files/content/test_write.txt")
            echo "Written file content:"
            echo "$WRITTEN_CONTENT" | jq -r '.data // "No content"'
        else
            echo "✗ File write failed"
        fi
        
        # Test directory creation
        echo -e "\nTesting directory creation..."
        MKDIR_RESPONSE=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/files/mkdir" \
          -H "Content-Type: application/json" \
          -d '{"path": "/test_directory"}')
        
        echo "Directory creation response: $MKDIR_RESPONSE"
        MKDIR_SUCCESS=$(echo "$MKDIR_RESPONSE" | jq -r '.success')
        
        if [ "$MKDIR_SUCCESS" = "true" ]; then
            echo "✓ Directory creation successful"
            
            # Verify directory was created
            echo "Verifying directory listing after creation..."
            UPDATED_FILES=$(curl -s "$BASE_URL/containers/$CONTAINER_ID/files?path=/")
            echo "Updated directory contents:"
            echo "$UPDATED_FILES" | jq -r '.data.files[] | select(.name == "test_directory") | "\(.name) - \(.is_directory)"'
        else
            echo "✗ Directory creation failed"
        fi
        
        # Test file deletion
        echo -e "\nTesting file deletion..."
        DELETE_RESPONSE=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/files/delete" \
          -H "Content-Type: application/json" \
          -d '{"path": "/test_write.txt"}')
        
        echo "Delete response: $DELETE_RESPONSE"
        DELETE_SUCCESS=$(echo "$DELETE_RESPONSE" | jq -r '.success')
        
        if [ "$DELETE_SUCCESS" = "true" ]; then
            echo "✓ File deletion successful"
        else
            echo "✗ File deletion failed"
        fi
        
        # Test nested directory listing
        echo -e "\nTesting nested directory listing..."
        DATA_FILES=$(curl -s "$BASE_URL/containers/$CONTAINER_ID/files?path=/data")
        echo "Data directory contents:"
        echo "$DATA_FILES" | jq -r '.data.files[] | "\(.name) - \(.size // "N/A") bytes"'
        
    else
        echo "✗ Directory listing failed"
        echo "Error: $(echo "$FILES_RESPONSE" | jq -r '.message')"
    fi
else
    echo "Skipping filesystem tests - container not ready"
fi

# Test 7: Check daemon state
echo -e "\n7. Checking daemon state:"
if [ -f "./storage/states.json" ]; then
    echo "✓ States file exists"
    echo "Container state in daemon:"
    cat ./storage/states.json | jq ".[\"$CUSTOM_UUID\"] // \"Container not found in states\""
else
    echo "✗ States file not found"
fi

# Cleanup
echo -e "\n8. Cleanup:"
echo "Stopping container..."
STOP_RESPONSE=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/stop")
echo "Stop response: $(echo "$STOP_RESPONSE" | jq -r '.success')"

sleep 2

echo "Removing container..."
#REMOVE_RESPONSE=$(curl -s -X DELETE "$BASE_URL/containers/$CONTAINER_ID")
echo "Remove response: $(echo "$REMOVE_RESPONSE" | jq -r '.success')"

echo -e "\n=== Installation Test Complete ==="