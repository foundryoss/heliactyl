#!/bin/bash

BASE_URL="http://localhost:8070"

echo "=== lightd Container Test ==="

# Function to check if port is available
check_port() {
    local port=$1
    if lsof -Pi :$port -sTCP:LISTEN -t >/dev/null 2>&1; then
        echo "Port $port is in use"
        return 1
    else
        echo "Port $port is available"
        return 0
    fi
}

# Test 1: Create nginx container with comprehensive installation script
echo "1. Creating nginx container with comprehensive installation script:"
CREATE_RESPONSE=$(curl -s -X POST "$BASE_URL/containers" \
  -H "Content-Type: application/json" \
  -d '{
    "image": "nginx:alpine",
    "name": "nginx-with-install",
    "description": "Test nginx container with comprehensive installation",
    "startup_command": ["nginx", "-g", "daemon off;"],
    "ports": {
      "80": "auto"
    },
    "limits": {
      "cpu": "0.5",
      "memory": "128m",
      "disk": "1g",
      "swap": "64m",
      "pids": 100,
      "threads": 50
    },
    "env": {
      "NGINX_HOST": "localhost",
      "NGINX_PORT": "80",
      "INSTALL_MODE": "comprehensive"
    },
    "install_content": "#!/bin/bash\necho \"=== COMPREHENSIVE INSTALLATION START ===\"\necho \"Container UUID: $HOSTNAME\"\necho \"Current user: $(whoami)\"\necho \"Current directory: $(pwd)\"\necho \"Available space: $(df -h /)\"\n\necho \"Updating package manager...\"\napk update\n\necho \"Installing essential packages...\"\napk add curl wget htop nano vim git\n\necho \"Creating custom directories...\"\nmkdir -p /app/logs /app/data /app/config\n\necho \"Setting up custom nginx configuration...\"\necho \"server { listen 80; location /health { return 200 \\\"healthy\\\"; } location /install { return 200 \\\"installation complete\\\"; } }\" > /etc/nginx/conf.d/custom.conf\n\necho \"Creating installation verification files...\"\necho \"Installation completed at $(date)\" > /usr/share/nginx/html/install.txt\necho \"Container UUID: $HOSTNAME\" >> /usr/share/nginx/html/install.txt\necho \"Installed packages: curl wget htop nano vim git\" >> /usr/share/nginx/html/install.txt\n\necho \"Setting up log rotation...\"\necho \"/app/logs/*.log { daily rotate 7 compress missingok notifempty }\" > /etc/logrotate.d/app\n\necho \"Creating startup script...\"\necho \"#!/bin/bash\" > /app/startup.sh\necho \"echo \\\"Container starting at \\$(date)\\\" >> /app/logs/startup.log\" >> /app/startup.sh\necho \"nginx -g \\\"daemon off;\\\"\" >> /app/startup.sh\nchmod +x /app/startup.sh\n\necho \"Installation verification...\"\nls -la /usr/share/nginx/html/\ncat /usr/share/nginx/html/install.txt\n\necho \"=== COMPREHENSIVE INSTALLATION COMPLETE ===\""
  }')

echo "Create response: $CREATE_RESPONSE"

# Extract container ID and allocated port
CONTAINER_ID=$(echo "$CREATE_RESPONSE" | jq -r '.data.container_id // empty')
ALLOCATED_PORT=$(echo "$CREATE_RESPONSE" | jq -r '.data.allocated_ports[0].host_port // empty')

echo "Container ID: $CONTAINER_ID"
echo "Allocated Port: $ALLOCATED_PORT"

if [ -z "$CONTAINER_ID" ] || [ "$CONTAINER_ID" = "null" ]; then
    echo "Failed to create container, exiting"
    exit 1
fi

# Test 2: Check container status during installation
echo -e "\n2. Checking container status during installation:"
sleep 2  # Give installation time to start
STATUS_RESPONSE=$(curl -s "$BASE_URL/containers/$CONTAINER_ID/status")
echo "Status response: $STATUS_RESPONSE"
CONTAINER_STATUS=$(echo "$STATUS_RESPONSE" | jq -r '.data.status // "unknown"')
echo "Container status: $CONTAINER_STATUS"

# Wait for installation to complete
echo "Waiting for installation to complete..."
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
    fi
    sleep 2
done

# Test 3: Check if nginx is responding and installation artifacts exist
if [ -n "$ALLOCATED_PORT" ] && [ "$ALLOCATED_PORT" != "null" ]; then
    echo -e "\n3. Testing nginx response on port $ALLOCATED_PORT:"
    sleep 2  # Give nginx time to be ready
    
    HTTP_RESPONSE=$(curl -s -o /dev/null -w "%{http_code}" "http://localhost:$ALLOCATED_PORT" || echo "000")
    if [ "$HTTP_RESPONSE" = "200" ]; then
        echo "✓ Nginx is responding correctly (HTTP $HTTP_RESPONSE)"
        
        # Check if installation artifact exists
        echo "Checking installation artifacts..."
        INSTALL_CHECK=$(curl -s "http://localhost:$ALLOCATED_PORT/install.txt" || echo "not found")
        if [[ "$INSTALL_CHECK" == *"Custom nginx setup complete"* ]]; then
            echo "✓ Installation artifacts found - custom setup completed"
        else
            echo "✗ Installation artifacts not found"
        fi
    else
        echo "✗ Nginx not responding (HTTP $HTTP_RESPONSE)"
    fi
fi

# Test 4: List containers to see status
echo -e "\n4. Listing containers:"
LIST_RESPONSE=$(curl -s "$BASE_URL/containers")
echo "$LIST_RESPONSE" | jq '.data[] | select(.name | contains("nginx-auto")) | {id: .id[0:12], name: .name, state: .state, ports: .ports}'

# Test 5: Update the container with new content
echo -e "\n5. Updating container with new content:"
UPDATE_RESPONSE=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/update" \
  -H "Content-Type: application/json" \
  -d '{
    "update_content": "#!/bin/bash\necho \"Running update...\"\napk add htop\necho \"Update completed - htop installed\" > /usr/share/nginx/html/update.txt\necho \"Container updated successfully!\""
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
    
    # Check if update artifacts exist
    if [ -n "$ALLOCATED_PORT" ] && [ "$ALLOCATED_PORT" != "null" ]; then
        echo "Checking update artifacts..."
        sleep 2
        UPDATE_CHECK=$(curl -s "http://localhost:$ALLOCATED_PORT/update.txt" || echo "not found")
        if [[ "$UPDATE_CHECK" == *"Update completed - htop installed"* ]]; then
            echo "✓ Update artifacts found - update completed successfully"
        else
            echo "✗ Update artifacts not found"
        fi
    fi
else
    echo "✗ Update failed to initiate"
    echo "Error: $(echo "$UPDATE_RESPONSE" | jq -r '.message')"
fi

# Test 6: Get comprehensive container logs to verify installation
echo -e "\n6. Getting comprehensive container logs to verify installation:"
LOGS_RESPONSE=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/logs" \
  -H "Content-Type: application/json" \
  -d '{"follow": false, "tail": "50"}')
echo "=== INSTALLATION LOGS ==="
echo "$LOGS_RESPONSE" | jq -r '.data' | head -30
echo "=== END INSTALLATION LOGS ==="

# Check if installation was successful by looking for specific markers in logs
INSTALL_SUCCESS=$(echo "$LOGS_RESPONSE" | jq -r '.data' | grep -c "COMPREHENSIVE INSTALLATION COMPLETE" || echo "0")
if [ "$INSTALL_SUCCESS" -gt 0 ]; then
    echo "✓ Installation script executed successfully (found completion marker)"
else
    echo "✗ Installation script may have failed (no completion marker found)"
fi

# Test 7: Verify installation artifacts via HTTP
if [ -n "$ALLOCATED_PORT" ] && [ "$ALLOCATED_PORT" != "null" ] && [ "$CONTAINER_STATUS" = "ready" ]; then
    echo -e "\n7. Verifying installation artifacts via HTTP:"
    
    # Test health endpoint
    HEALTH_RESPONSE=$(curl -s "http://localhost:$ALLOCATED_PORT/health" || echo "failed")
    if [[ "$HEALTH_RESPONSE" == *"healthy"* ]]; then
        echo "✓ Health endpoint responding correctly"
    else
        echo "✗ Health endpoint not responding: $HEALTH_RESPONSE"
    fi
    
    # Test install endpoint
    INSTALL_ENDPOINT=$(curl -s "http://localhost:$ALLOCATED_PORT/install" || echo "failed")
    if [[ "$INSTALL_ENDPOINT" == *"installation complete"* ]]; then
        echo "✓ Install endpoint responding correctly"
    else
        echo "✗ Install endpoint not responding: $INSTALL_ENDPOINT"
    fi
    
    # Test installation verification file
    INSTALL_FILE=$(curl -s "http://localhost:$ALLOCATED_PORT/install.txt" || echo "failed")
    if [[ "$INSTALL_FILE" == *"Installation completed"* ]]; then
        echo "✓ Installation verification file found"
        echo "Installation details:"
        echo "$INSTALL_FILE" | head -5
    else
        echo "✗ Installation verification file not found: $INSTALL_FILE"
    fi
fi

# Test 8: Create another container with specific port and installation script
echo -e "\n8. Creating second nginx container with specific port and installation:"
CREATE_RESPONSE2=$(curl -s -X POST "$BASE_URL/containers" \
  -H "Content-Type: application/json" \
  -d '{
    "image": "nginx:alpine", 
    "name": "nginx-specific-port",
    "description": "Test nginx with specific port binding and installation",
    "ports": {
      "80": "8081"
    },
    "limits": {
      "cpu": "1.0",
      "memory": "256m",
      "disk": "2g",
      "swap": "128m",
      "pids": 200,
      "threads": 100
    },
    "env": {
      "NGINX_HOST": "0.0.0.0",
      "NGINX_PORT": "80",
      "TEST_VAR": "specific-port-test"
    },
    "install_content": "#!/bin/bash\necho \"Installing for specific port container...\"\napk update\napk add nano vim\necho \"Specific port setup complete\" > /usr/share/nginx/html/specific.txt",
    "custom_uuid": "test-nginx-specific-' + $(date +%s) + '"
  }')

echo "Create response 2: $CREATE_RESPONSE2"
CONTAINER_ID2=$(echo "$CREATE_RESPONSE2" | jq -r '.data.container_id // empty')
CUSTOM_UUID2=$(echo "$CREATE_RESPONSE2" | jq -r '.data.custom_uuid // empty')

echo "Container ID 2: $CONTAINER_ID2"
echo "Custom UUID 2: $CUSTOM_UUID2"

if [ -n "$CONTAINER_ID2" ] && [ "$CONTAINER_ID2" != "null" ]; then
    echo "Waiting for second container installation to complete..."
    for i in {1..20}; do
        STATUS_RESPONSE2=$(curl -s "$BASE_URL/containers/$CONTAINER_ID2/status")
        CONTAINER_STATUS2=$(echo "$STATUS_RESPONSE2" | jq -r '.data.status // "unknown"')
        echo "Container 2 status check $i: $CONTAINER_STATUS2"
        
        if [ "$CONTAINER_STATUS2" = "ready" ]; then
            echo "✓ Second container installation completed"
            break
        elif [ "$CONTAINER_STATUS2" = "install_failed" ]; then
            echo "✗ Second container installation failed"
            break
        fi
        sleep 2
    done
    
    # Test port 8081
    if [ "$CONTAINER_STATUS2" = "ready" ]; then
        echo "Testing nginx on port 8081..."
        sleep 2
        HTTP_RESPONSE2=$(curl -s -o /dev/null -w "%{http_code}" "http://localhost:8081" || echo "000")
        if [ "$HTTP_RESPONSE2" = "200" ]; then
            echo "✓ Second nginx is responding correctly (HTTP $HTTP_RESPONSE2)"
            
            # Check installation artifacts
            SPECIFIC_CHECK=$(curl -s "http://localhost:8081/specific.txt" || echo "not found")
            if [[ "$SPECIFIC_CHECK" == *"Specific port setup complete"* ]]; then
                echo "✓ Second container installation artifacts found"
            else
                echo "✗ Second container installation artifacts not found"
            fi
        else
            echo "✗ Second nginx not responding (HTTP $HTTP_RESPONSE2)"
        fi
    fi
fi

# Test 9: Create a container with volumes and installation
echo -e "\n9. Creating container with volume mounts and installation:"
CREATE_RESPONSE3=$(curl -s -X POST "$BASE_URL/containers" \
  -H "Content-Type: application/json" \
  -d '{
    "image": "alpine:latest",
    "name": "alpine-with-volumes",
    "description": "Alpine container with volume mounts for testing",
    "startup_command": ["sleep", "300"],
    "volumes": [
      {
        "source": "/tmp/test-volume",
        "target": "/app/data",
        "read_only": false
      }
    ],
    "limits": {
      "cpu": "0.25",
      "memory": "64m",
      "swap": "32m",
      "pids": 50,
      "threads": 25
    },
    "env": {
      "TEST_MODE": "volume-test",
      "DATA_PATH": "/app/data"
    },
    "install_content": "#!/bin/bash\necho \"Setting up volume container...\"\nmkdir -p /app/data\necho \"Volume setup complete\" > /app/data/setup.txt\necho \"Alpine container ready\""
  }')

echo "Create response 3: $CREATE_RESPONSE3"
CONTAINER_ID3=$(echo "$CREATE_RESPONSE3" | jq -r '.data.container_id // empty')

if [ -n "$CONTAINER_ID3" ] && [ "$CONTAINER_ID3" != "null" ]; then
    echo "Waiting for alpine container installation to complete..."
    for i in {1..15}; do
        STATUS_RESPONSE3=$(curl -s "$BASE_URL/containers/$CONTAINER_ID3/status")
        CONTAINER_STATUS3=$(echo "$STATUS_RESPONSE3" | jq -r '.data.status // "unknown"')
        echo "Alpine container status check $i: $CONTAINER_STATUS3"
        
        if [ "$CONTAINER_STATUS3" = "ready" ]; then
            echo "✓ Alpine container installation completed"
            break
        elif [ "$CONTAINER_STATUS3" = "install_failed" ]; then
            echo "✗ Alpine container installation failed"
            break
        fi
        sleep 2
    done
fi

# Test 10: Create a resource-constrained container to test limits
echo -e "\n10. Creating resource-constrained container to test limits:"
CREATE_RESPONSE4=$(curl -s -X POST "$BASE_URL/containers" \
  -H "Content-Type: application/json" \
  -d '{
    "image": "alpine:latest",
    "name": "resource-test-container",
    "description": "Container with strict resource limits for testing",
    "startup_command": ["sh", "-c", "while true; do echo Resource test running; sleep 10; done"],
    "limits": {
      "cpu": "0.1",
      "memory": "32m",
      "swap": "16m",
      "pids": 10,
      "threads": 5
    },
    "env": {
      "TEST_TYPE": "resource-limits",
      "MAX_MEMORY": "32m"
    }
  }')

echo "Create response 4: $CREATE_RESPONSE4"
CONTAINER_ID4=$(echo "$CREATE_RESPONSE4" | jq -r '.data.container_id // empty')

if [ -n "$CONTAINER_ID4" ] && [ "$CONTAINER_ID4" != "null" ]; then
    echo "Starting resource-constrained container..."
    START_RESPONSE4=$(curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID4/start")
    echo "Start response 4: $(echo "$START_RESPONSE4" | jq -r '.success')"
    
    if [ "$(echo "$START_RESPONSE4" | jq -r '.success')" = "true" ]; then
        echo "✓ Resource-constrained container started successfully"
        
        # Get container stats to verify limits are applied
        sleep 2
        echo "Getting container stats to verify resource limits..."
        STATS_RESPONSE=$(curl -s "$BASE_URL/containers/$CONTAINER_ID4/stats")
        echo "Stats response: $(echo "$STATS_RESPONSE" | jq -r '.success')"
    fi
fi

# Test 11: Check daemon state persistence
echo -e "\n11. Checking daemon state (states.json should be created):"
if [ -f "./storage/states.json" ]; then
    echo "✓ States file exists"
    echo "States file content:"
    cat ./storage/states.json | jq '.'
else
    echo "✗ States file not found"
fi

# Cleanup
echo -e "\n12. Cleanup:"
echo "Stopping containers..."
curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID/stop" | jq -r '.success'
if [ -n "$CONTAINER_ID2" ]; then
    curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID2/stop" | jq -r '.success'
fi
if [ -n "$CONTAINER_ID3" ]; then
    curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID3/stop" | jq -r '.success'
fi
if [ -n "$CONTAINER_ID4" ]; then
    curl -s -X POST "$BASE_URL/containers/$CONTAINER_ID4/stop" | jq -r '.success'
fi

sleep 2

echo "Removing containers..."
REMOVE_RESPONSE=$(curl -s -X DELETE "$BASE_URL/containers/$CONTAINER_ID")
echo "Remove response 1: $(echo "$REMOVE_RESPONSE" | jq -r '.success')"

if [ -n "$CONTAINER_ID2" ]; then
    REMOVE_RESPONSE2=$(curl -s -X DELETE "$BASE_URL/containers/$CONTAINER_ID2")
    echo "Remove response 2: $(echo "$REMOVE_RESPONSE2" | jq -r '.success')"
fi

if [ -n "$CONTAINER_ID3" ]; then
    REMOVE_RESPONSE3=$(curl -s -X DELETE "$BASE_URL/containers/$CONTAINER_ID3")
    echo "Remove response 3: $(echo "$REMOVE_RESPONSE3" | jq -r '.success')"
fi

if [ -n "$CONTAINER_ID4" ]; then
    REMOVE_RESPONSE4=$(curl -s -X DELETE "$BASE_URL/containers/$CONTAINER_ID4")
    echo "Remove response 4: $(echo "$REMOVE_RESPONSE4" | jq -r '.success')"
fi

# Final state check
echo -e "\n13. Final state check:"
if [ -f "./storage/states.json" ]; then
    echo "Final states file content:"
    cat ./storage/states.json | jq '.'
else
    echo "States file not found after cleanup"
fi

echo -e "\n=== Test completed! ==="