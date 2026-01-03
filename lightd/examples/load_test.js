#!/usr/bin/env node

/**
 * Load testing script for lightd daemon
 * Tests concurrent requests to various endpoints
 */

const http = require('http');

const DAEMON_HOST = process.env.DAEMON_HOST || 'localhost';
const DAEMON_PORT = process.env.DAEMON_PORT || 8070;
const AUTH_TOKEN = process.env.AUTH_TOKEN || 'lightd-secret-key-2024-ru-deduct';

// Test configuration
const config = {
    concurrentRequests: 2000,
    requestsPerEndpoint: 500,
    delayBetweenBatches: 10, // ms
    enableDestructiveTests: 'true', // Power actions and file operations
};

// Colors for output
const colors = {
    reset: '\x1b[0m',
    red: '\x1b[31m',
    green: '\x1b[32m',
    yellow: '\x1b[33m',
    blue: '\x1b[34m',
    magenta: '\x1b[35m',
    cyan: '\x1b[36m',
};

// Statistics
const stats = {
    total: 0,
    success: 0,
    failed: 0,
    timeouts: 0,
    totalTime: 0,
    minTime: Infinity,
    maxTime: 0,
    times: [],
};

// Make HTTP request
function makeRequest(method, path, body = null, timeout = 5000) {
    return new Promise((resolve, reject) => {
        const startTime = Date.now();
        
        const options = {
            hostname: DAEMON_HOST,
            port: DAEMON_PORT,
            path: path,
            method: method,
            headers: {
                'Authorization': `Bearer ${AUTH_TOKEN}`,
                'Content-Type': 'application/json',
            },
            timeout: timeout,
        };

        const req = http.request(options, (res) => {
            let data = '';
            
            res.on('data', (chunk) => {
                data += chunk;
            });
            
            res.on('end', () => {
                const endTime = Date.now();
                const duration = endTime - startTime;
                
                try {
                    const parsed = JSON.parse(data);
                    resolve({
                        status: res.statusCode,
                        data: parsed,
                        duration: duration,
                    });
                } catch (e) {
                    resolve({
                        status: res.statusCode,
                        data: data,
                        duration: duration,
                    });
                }
            });
        });

        req.on('timeout', () => {
            req.destroy();
            reject(new Error('Request timeout'));
        });

        req.on('error', (err) => {
            reject(err);
        });

        if (body) {
            req.write(JSON.stringify(body));
        }
        
        req.end();
    });
}

// Get list of containers
async function getContainers() {
    try {
        const result = await makeRequest('GET', '/containers');
        if (result.data && result.data.data) {
            return result.data.data.map(c => c.id || c.container_id).filter(Boolean);
        }
        return [];
    } catch (e) {
        console.error(`${colors.red}Failed to get containers: ${e.message}${colors.reset}`);
        return [];
    }
}

// Test endpoint with concurrent requests
async function testEndpoint(name, requestFn, count = config.requestsPerEndpoint) {
    console.log(`\n${colors.cyan}Testing ${name}...${colors.reset}`);
    console.log(`Sending ${count} requests with ${config.concurrentRequests} concurrent...`);
    
    const localStats = {
        success: 0,
        failed: 0,
        timeouts: 0,
        times: [],
    };

    const batches = Math.ceil(count / config.concurrentRequests);
    
    for (let batch = 0; batch < batches; batch++) {
        const batchSize = Math.min(config.concurrentRequests, count - (batch * config.concurrentRequests));
        const promises = [];
        
        for (let i = 0; i < batchSize; i++) {
            promises.push(
                requestFn()
                    .then(result => {
                        stats.success++;
                        localStats.success++;
                        stats.totalTime += result.duration;
                        stats.times.push(result.duration);
                        localStats.times.push(result.duration);
                        stats.minTime = Math.min(stats.minTime, result.duration);
                        stats.maxTime = Math.max(stats.maxTime, result.duration);
                        return result;
                    })
                    .catch(err => {
                        if (err.message === 'Request timeout') {
                            stats.timeouts++;
                            localStats.timeouts++;
                        } else {
                            stats.failed++;
                            localStats.failed++;
                        }
                        return null;
                    })
            );
        }
        
        await Promise.all(promises);
        stats.total += batchSize;
        
        // Progress indicator
        const progress = Math.round(((batch + 1) / batches) * 100);
        process.stdout.write(`\r${colors.yellow}Progress: ${progress}%${colors.reset}`);
        
        // Small delay between batches
        if (batch < batches - 1) {
            await new Promise(resolve => setTimeout(resolve, config.delayBetweenBatches));
        }
    }
    
    console.log(''); // New line after progress
    
    // Calculate stats
    const avgTime = localStats.times.length > 0 
        ? localStats.times.reduce((a, b) => a + b, 0) / localStats.times.length 
        : 0;
    
    const sortedTimes = localStats.times.sort((a, b) => a - b);
    const p50 = sortedTimes[Math.floor(sortedTimes.length * 0.5)] || 0;
    const p95 = sortedTimes[Math.floor(sortedTimes.length * 0.95)] || 0;
    const p99 = sortedTimes[Math.floor(sortedTimes.length * 0.99)] || 0;
    
    console.log(`${colors.green}✓ Success: ${localStats.success}${colors.reset}`);
    console.log(`${colors.red}✗ Failed: ${localStats.failed}${colors.reset}`);
    console.log(`${colors.yellow}⏱ Timeouts: ${localStats.timeouts}${colors.reset}`);
    console.log(`${colors.blue}Avg: ${avgTime.toFixed(2)}ms | P50: ${p50}ms | P95: ${p95}ms | P99: ${p99}ms${colors.reset}`);
}

// Main test suite
async function runTests() {
    console.log(`${colors.magenta}╔════════════════════════════════════════╗${colors.reset}`);
    console.log(`${colors.magenta}║   Lightd Daemon Load Test Suite       ║${colors.reset}`);
    console.log(`${colors.magenta}╚════════════════════════════════════════╝${colors.reset}`);
    console.log(`\nTarget: ${DAEMON_HOST}:${DAEMON_PORT}`);
    console.log(`Concurrent requests: ${config.concurrentRequests}`);
    console.log(`Requests per endpoint: ${config.requestsPerEndpoint}`);
    console.log(`Destructive tests: ${config.enableDestructiveTests ? colors.yellow + 'ENABLED' + colors.reset : colors.green + 'DISABLED' + colors.reset}`);
    
    // Get container IDs for testing
    console.log(`\n${colors.cyan}Fetching container list...${colors.reset}`);
    const containers = await getContainers();
    
    if (containers.length === 0) {
        console.log(`${colors.yellow}⚠ No containers found. Some tests will be skipped.${colors.reset}`);
    } else {
        console.log(`${colors.green}✓ Found ${containers.length} containers${colors.reset}`);
    }
    
    // Test 1: Health check (lightweight)
    await testEndpoint(
        'Health Check',
        () => makeRequest('GET', '/health'),
        config.requestsPerEndpoint * 2 // More requests for lightweight endpoint
    );
    
    // Test 2: List containers
    await testEndpoint(
        'List Containers',
        () => makeRequest('GET', '/containers'),
        config.requestsPerEndpoint
    );
    
    if (containers.length > 0) {
        // Test 3: Get container by UUID (random selection)
        await testEndpoint(
            'Get Container by UUID',
            () => {
                const randomContainer = containers[Math.floor(Math.random() * containers.length)];
                return makeRequest('GET', `/containers/uuid/${randomContainer}`);
            },
            config.requestsPerEndpoint
        );
        
        // Test 4: Get container logs (random selection)
        await testEndpoint(
            'Get Container Logs',
            () => {
                const randomContainer = containers[Math.floor(Math.random() * containers.length)];
                return makeRequest('POST', `/containers/${randomContainer}/logs`, { tail: '10' });
            },
            Math.floor(config.requestsPerEndpoint / 2) // Fewer requests for heavier endpoint
        );
        
        // Test 5: Get container stats (random selection)
        await testEndpoint(
            'Get Container Stats',
            () => {
                const randomContainer = containers[Math.floor(Math.random() * containers.length)];
                return makeRequest('GET', `/containers/${randomContainer}/stats`);
            },
            Math.floor(config.requestsPerEndpoint / 2)
        );
        
        // Test 6: Filesystem operations (if enabled)
        if (config.enableDestructiveTests) {
            await testEndpoint(
                'List Directory',
                () => {
                    const randomContainer = containers[Math.floor(Math.random() * containers.length)];
                    return makeRequest('POST', `/containers/${randomContainer}/files/list`, { path: '/' });
                },
                Math.floor(config.requestsPerEndpoint / 2)
            );
            
            await testEndpoint(
                'Read File',
                () => {
                    const randomContainer = containers[Math.floor(Math.random() * containers.length)];
                    return makeRequest('POST', `/containers/${randomContainer}/files/read`, { path: '/etc/hostname' });
                },
                Math.floor(config.requestsPerEndpoint / 2)
            );
            
            // Test 7: Power actions (careful - this actually stops/starts containers!)
            console.log(`\n${colors.yellow}⚠ WARNING: Testing power actions (will stop/start containers)${colors.reset}`);
            
            // Pick a few containers for power testing
            const powerTestContainers = containers.slice(0, Math.min(3, containers.length));
            
            for (const containerId of powerTestContainers) {
                console.log(`\n${colors.cyan}Power testing container: ${containerId.slice(0, 12)}${colors.reset}`);
                
                // Stop
                await testEndpoint(
                    `Stop Container ${containerId.slice(0, 12)}`,
                    () => makeRequest('POST', `/containers/uuid/${containerId}/stop`),
                    1
                );
                
                // Wait a bit
                await new Promise(resolve => setTimeout(resolve, 2000));
                
                // Start
                await testEndpoint(
                    `Start Container ${containerId.slice(0, 12)}`,
                    () => makeRequest('POST', `/containers/uuid/${containerId}/start`),
                    1
                );
                
                // Wait a bit
                await new Promise(resolve => setTimeout(resolve, 2000));
            }
            
            // Test restart under load
            await testEndpoint(
                'Restart Containers (Concurrent)',
                () => {
                    const randomContainer = powerTestContainers[Math.floor(Math.random() * powerTestContainers.length)];
                    return makeRequest('POST', `/containers/uuid/${randomContainer}/restart`);
                },
                5
            );
        }
        
        // Test 8: Mixed workload (simulate real usage)
        await testEndpoint(
            'Mixed Workload',
            () => {
                const randomContainer = containers[Math.floor(Math.random() * containers.length)];
                const endpoints = [
                    () => makeRequest('GET', '/health'),
                    () => makeRequest('GET', '/containers'),
                    () => makeRequest('GET', `/containers/uuid/${randomContainer}`),
                    () => makeRequest('POST', `/containers/${randomContainer}/logs`, { tail: '5' }),
                    () => makeRequest('GET', `/containers/${randomContainer}/stats`),
                ];
                
                if (config.enableDestructiveTests) {
                    endpoints.push(
                        () => makeRequest('POST', `/containers/${randomContainer}/files/list`, { path: '/' })
                    );
                }
                
                const randomEndpoint = endpoints[Math.floor(Math.random() * endpoints.length)];
                return randomEndpoint();
            },
            config.requestsPerEndpoint * 2
        );
    }
    
    // Print final statistics
    console.log(`\n${colors.magenta}╔════════════════════════════════════════╗${colors.reset}`);
    console.log(`${colors.magenta}║         Final Statistics               ║${colors.reset}`);
    console.log(`${colors.magenta}╚════════════════════════════════════════╝${colors.reset}`);
    console.log(`\nTotal requests: ${stats.total}`);
    console.log(`${colors.green}✓ Successful: ${stats.success} (${((stats.success / stats.total) * 100).toFixed(2)}%)${colors.reset}`);
    console.log(`${colors.red}✗ Failed: ${stats.failed} (${((stats.failed / stats.total) * 100).toFixed(2)}%)${colors.reset}`);
    console.log(`${colors.yellow}⏱ Timeouts: ${stats.timeouts} (${((stats.timeouts / stats.total) * 100).toFixed(2)}%)${colors.reset}`);
    
    if (stats.times.length > 0) {
        const avgTime = stats.totalTime / stats.times.length;
        const sortedTimes = stats.times.sort((a, b) => a - b);
        const p50 = sortedTimes[Math.floor(sortedTimes.length * 0.5)];
        const p95 = sortedTimes[Math.floor(sortedTimes.length * 0.95)];
        const p99 = sortedTimes[Math.floor(sortedTimes.length * 0.99)];
        
        console.log(`\n${colors.blue}Response Times:${colors.reset}`);
        console.log(`  Min: ${stats.minTime}ms`);
        console.log(`  Avg: ${avgTime.toFixed(2)}ms`);
        console.log(`  P50: ${p50}ms`);
        console.log(`  P95: ${p95}ms`);
        console.log(`  P99: ${p99}ms`);
        console.log(`  Max: ${stats.maxTime}ms`);
    }
    
    // Performance assessment
    console.log(`\n${colors.magenta}Performance Assessment:${colors.reset}`);
    const successRate = (stats.success / stats.total) * 100;
    const avgTime = stats.totalTime / stats.times.length;
    
    if (successRate >= 99 && avgTime < 100) {
        console.log(`${colors.green}★★★★★ Excellent! Daemon is performing very well.${colors.reset}`);
    } else if (successRate >= 95 && avgTime < 200) {
        console.log(`${colors.green}★★★★☆ Good! Daemon is performing well.${colors.reset}`);
    } else if (successRate >= 90 && avgTime < 500) {
        console.log(`${colors.yellow}★★★☆☆ Fair. Some optimization recommended.${colors.reset}`);
    } else if (successRate >= 80) {
        console.log(`${colors.yellow}★★☆☆☆ Poor. Optimization needed.${colors.reset}`);
    } else {
        console.log(`${colors.red}★☆☆☆☆ Critical! Daemon is struggling under load.${colors.reset}`);
    }
    
    console.log('');
}

// Run tests
runTests().catch(err => {
    console.error(`${colors.red}Fatal error: ${err.message}${colors.reset}`);
    process.exit(1);
});
