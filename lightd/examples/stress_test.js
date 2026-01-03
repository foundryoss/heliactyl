#!/usr/bin/env node

/**
 * Comprehensive stress test for lightd daemon
 * Tests power actions, filesystem operations, and concurrent load
 * 
 * Usage:
 *   node stress_test.js                    # Read-only tests
 *   ENABLE_POWER=true node stress_test.js  # Include power actions
 *   ENABLE_FILES=true node stress_test.js  # Include file operations
 *   ENABLE_ALL=true node stress_test.js    # All tests
 */

const http = require('http');

const DAEMON_HOST = process.env.DAEMON_HOST || 'localhost';
const DAEMON_PORT = process.env.DAEMON_PORT || 8070;
const AUTH_TOKEN = process.env.AUTH_TOKEN || 'lightd-secret-key-2024-ru-deduct';

// Test configuration
const config = {
    enablePowerTests: process.env.ENABLE_POWER === 'true' || process.env.ENABLE_ALL === 'true',
    enableFileTests: process.env.ENABLE_FILES === 'true' || process.env.ENABLE_ALL === 'true',
    powerTestContainerCount: 30000000, // How many containers to test power actions on
    concurrentPowerActions: 50000000,
    concurrentFileOps: 1000000,
};

// Colors
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
    power: { total: 0, success: 0, failed: 0, times: [] },
    files: { total: 0, success: 0, failed: 0, times: [] },
    overall: { total: 0, success: 0, failed: 0, times: [] },
};

// Make HTTP request
function makeRequest(method, path, body = null, timeout = 10000) {
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
                const duration = Date.now() - startTime;
                
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
            return result.data.data.map(c => ({
                id: c.id || c.container_id,
                uuid: c.uuid,
                name: c.name || 'unknown',
                state: c.state || 'unknown'
            })).filter(c => c.id);
        }
        return [];
    } catch (e) {
        console.error(`${colors.red}Failed to get containers: ${e.message}${colors.reset}`);
        return [];
    }
}

// Wait for container to reach state
async function waitForState(uuid, targetState, maxWait = 30000) {
    const startTime = Date.now();
    
    while (Date.now() - startTime < maxWait) {
        try {
            const result = await makeRequest('GET', `/containers/uuid/${uuid}`);
            if (result.data?.data?.state === targetState) {
                return true;
            }
        } catch (e) {
            // Ignore errors during polling
        }
        await new Promise(resolve => setTimeout(resolve, 500));
    }
    
    return false;
}

// Test power actions
async function testPowerActions(containers) {
    if (!config.enablePowerTests) {
        console.log(`\n${colors.yellow}⚠ Power tests disabled. Use ENABLE_POWER=true to enable.${colors.reset}`);
        return;
    }
    
    console.log(`\n${colors.magenta}╔════════════════════════════════════════╗${colors.reset}`);
    console.log(`${colors.magenta}║      Power Actions Stress Test         ║${colors.reset}`);
    console.log(`${colors.magenta}╚════════════════════════════════════════╝${colors.reset}`);
    
    const testContainers = containers
        .filter(c => c.state === 'running')
        .slice(0, config.powerTestContainerCount);
    
    if (testContainers.length === 0) {
        console.log(`${colors.yellow}No running containers found for power testing${colors.reset}`);
        return;
    }
    
    console.log(`\nTesting with ${testContainers.length} containers:`);
    testContainers.forEach(c => {
        console.log(`  - ${c.name} (${c.uuid.slice(0, 12)})`);
    });
    
    // Test 1: Sequential stop/start
    console.log(`\n${colors.cyan}Test 1: Sequential Stop/Start${colors.reset}`);
    for (const container of testContainers) {
        const startTime = Date.now();
        
        try {
            // Stop
            console.log(`  Stopping ${container.name}...`);
            await makeRequest('POST', `/containers/uuid/${container.uuid}/stop`);
            await waitForState(container.uuid, 'stopped', 15000);
            
            // Start
            console.log(`  Starting ${container.name}...`);
            await makeRequest('POST', `/containers/uuid/${container.uuid}/start`);
            await waitForState(container.uuid, 'running', 15000);
            
            const duration = Date.now() - startTime;
            stats.power.success++;
            stats.power.times.push(duration);
            console.log(`  ${colors.green}✓ Complete in ${duration}ms${colors.reset}`);
        } catch (e) {
            stats.power.failed++;
            console.log(`  ${colors.red}✗ Failed: ${e.message}${colors.reset}`);
        }
        stats.power.total++;
    }
    
    // Test 2: Concurrent restarts
    console.log(`\n${colors.cyan}Test 2: Concurrent Restarts${colors.reset}`);
    console.log(`  Restarting ${config.concurrentPowerActions} times per container...`);
    
    const restartPromises = [];
    for (const container of testContainers) {
        for (let i = 0; i < config.concurrentPowerActions; i++) {
            restartPromises.push(
                (async () => {
                    const startTime = Date.now();
                    try {
                        await makeRequest('POST', `/containers/uuid/${container.uuid}/restart`);
                        const duration = Date.now() - startTime;
                        stats.power.success++;
                        stats.power.times.push(duration);
                        return { success: true, duration };
                    } catch (e) {
                        stats.power.failed++;
                        return { success: false, error: e.message };
                    } finally {
                        stats.power.total++;
                    }
                })()
            );
        }
    }
    
    const results = await Promise.all(restartPromises);
    const successful = results.filter(r => r.success).length;
    console.log(`  ${colors.green}✓ ${successful}/${results.length} restarts successful${colors.reset}`);
    
    // Test 3: Rapid stop/start cycles
    console.log(`\n${colors.cyan}Test 3: Rapid Stop/Start Cycles${colors.reset}`);
    const cycleContainer = testContainers[0];
    console.log(`  Testing ${cycleContainer.name} with 5 rapid cycles...`);
    
    for (let i = 0; i < 5; i++) {
        const startTime = Date.now();
        try {
            await makeRequest('POST', `/containers/uuid/${cycleContainer.uuid}/stop`);
            await new Promise(resolve => setTimeout(resolve, 1000));
            await makeRequest('POST', `/containers/uuid/${cycleContainer.uuid}/start`);
            await new Promise(resolve => setTimeout(resolve, 1000));
            
            const duration = Date.now() - startTime;
            stats.power.success++;
            stats.power.times.push(duration);
            console.log(`  Cycle ${i + 1}: ${colors.green}✓ ${duration}ms${colors.reset}`);
        } catch (e) {
            stats.power.failed++;
            console.log(`  Cycle ${i + 1}: ${colors.red}✗ ${e.message}${colors.reset}`);
        }
        stats.power.total++;
    }
    
    // Print power test stats
    console.log(`\n${colors.blue}Power Actions Summary:${colors.reset}`);
    console.log(`  Total: ${stats.power.total}`);
    console.log(`  Success: ${stats.power.success} (${((stats.power.success / stats.power.total) * 100).toFixed(1)}%)`);
    console.log(`  Failed: ${stats.power.failed}`);
    if (stats.power.times.length > 0) {
        const avg = stats.power.times.reduce((a, b) => a + b, 0) / stats.power.times.length;
        const sorted = stats.power.times.sort((a, b) => a - b);
        console.log(`  Avg time: ${avg.toFixed(0)}ms`);
        console.log(`  P95: ${sorted[Math.floor(sorted.length * 0.95)]}ms`);
        console.log(`  Max: ${Math.max(...stats.power.times)}ms`);
    }
}

// Test filesystem operations
async function testFilesystemOps(containers) {
    if (!config.enableFileTests) {
        console.log(`\n${colors.yellow}⚠ Filesystem tests disabled. Use ENABLE_FILES=true to enable.${colors.reset}`);
        return;
    }
    
    console.log(`\n${colors.magenta}╔════════════════════════════════════════╗${colors.reset}`);
    console.log(`${colors.magenta}║    Filesystem Operations Stress Test   ║${colors.reset}`);
    console.log(`${colors.magenta}╚════════════════════════════════════════╝${colors.reset}`);
    
    const testContainers = containers.slice(0, 5);
    
    // Test 1: Concurrent directory listings
    console.log(`\n${colors.cyan}Test 1: Concurrent Directory Listings${colors.reset}`);
    console.log(`  ${config.concurrentFileOps} concurrent requests per container...`);
    
    const listPromises = [];
    for (const container of testContainers) {
        for (let i = 0; i < config.concurrentFileOps; i++) {
            listPromises.push(
                (async () => {
                    const startTime = Date.now();
                    try {
                        await makeRequest('POST', `/containers/${container.id}/files/list`, { path: '/' });
                        const duration = Date.now() - startTime;
                        stats.files.success++;
                        stats.files.times.push(duration);
                        return { success: true, duration };
                    } catch (e) {
                        stats.files.failed++;
                        return { success: false, error: e.message };
                    } finally {
                        stats.files.total++;
                    }
                })()
            );
        }
    }
    
    const listResults = await Promise.all(listPromises);
    const listSuccess = listResults.filter(r => r.success).length;
    console.log(`  ${colors.green}✓ ${listSuccess}/${listResults.length} successful${colors.reset}`);
    
    // Test 2: Read common files
    console.log(`\n${colors.cyan}Test 2: Read Common Files${colors.reset}`);
    const commonFiles = ['/etc/hostname', '/etc/hosts', '/etc/resolv.conf'];
    
    for (const file of commonFiles) {
        const readPromises = testContainers.map(container =>
            (async () => {
                const startTime = Date.now();
                try {
                    await makeRequest('POST', `/containers/${container.id}/files/read`, { path: file });
                    const duration = Date.now() - startTime;
                    stats.files.success++;
                    stats.files.times.push(duration);
                    return { success: true, duration };
                } catch (e) {
                    stats.files.failed++;
                    return { success: false };
                } finally {
                    stats.files.total++;
                }
            })()
        );
        
        const results = await Promise.all(readPromises);
        const success = results.filter(r => r.success).length;
        console.log(`  ${file}: ${colors.green}${success}/${results.length}${colors.reset}`);
    }
    
    // Test 3: Write/Read/Delete cycle
    console.log(`\n${colors.cyan}Test 3: Write/Read/Delete Cycles${colors.reset}`);
    const testContainer = testContainers[0];
    
    for (let i = 0; i < 5; i++) {
        const testFile = `/tmp/stress_test_${Date.now()}_${i}.txt`;
        const testContent = `Test content ${i} - ${new Date().toISOString()}`;
        
        try {
            // Write
            const writeStart = Date.now();
            await makeRequest('POST', `/containers/${testContainer.id}/files/write`, {
                path: testFile,
                content: Buffer.from(testContent).toString('base64')
            });
            const writeDuration = Date.now() - writeStart;
            
            // Read
            const readStart = Date.now();
            const readResult = await makeRequest('POST', `/containers/${testContainer.id}/files/read`, {
                path: testFile
            });
            const readDuration = Date.now() - readStart;
            
            // Delete
            const deleteStart = Date.now();
            await makeRequest('POST', `/containers/${testContainer.id}/files/delete`, {
                path: testFile
            });
            const deleteDuration = Date.now() - deleteStart;
            
            const totalDuration = writeDuration + readDuration + deleteDuration;
            stats.files.success += 3;
            stats.files.times.push(writeDuration, readDuration, deleteDuration);
            stats.files.total += 3;
            
            console.log(`  Cycle ${i + 1}: ${colors.green}✓ W:${writeDuration}ms R:${readDuration}ms D:${deleteDuration}ms${colors.reset}`);
        } catch (e) {
            stats.files.failed += 3;
            stats.files.total += 3;
            console.log(`  Cycle ${i + 1}: ${colors.red}✗ ${e.message}${colors.reset}`);
        }
    }
    
    // Print filesystem stats
    console.log(`\n${colors.blue}Filesystem Operations Summary:${colors.reset}`);
    console.log(`  Total: ${stats.files.total}`);
    console.log(`  Success: ${stats.files.success} (${((stats.files.success / stats.files.total) * 100).toFixed(1)}%)`);
    console.log(`  Failed: ${stats.files.failed}`);
    if (stats.files.times.length > 0) {
        const avg = stats.files.times.reduce((a, b) => a + b, 0) / stats.files.times.length;
        const sorted = stats.files.times.sort((a, b) => a - b);
        console.log(`  Avg time: ${avg.toFixed(0)}ms`);
        console.log(`  P95: ${sorted[Math.floor(sorted.length * 0.95)]}ms`);
        console.log(`  Max: ${Math.max(...stats.files.times)}ms`);
    }
}

// Main
async function main() {
    console.log(`${colors.magenta}╔════════════════════════════════════════╗${colors.reset}`);
    console.log(`${colors.magenta}║   Lightd Comprehensive Stress Test     ║${colors.reset}`);
    console.log(`${colors.magenta}╚════════════════════════════════════════╝${colors.reset}`);
    console.log(`\nTarget: ${DAEMON_HOST}:${DAEMON_PORT}`);
    console.log(`Power tests: ${config.enablePowerTests ? colors.yellow + 'ENABLED' + colors.reset : colors.green + 'DISABLED' + colors.reset}`);
    console.log(`File tests: ${config.enableFileTests ? colors.yellow + 'ENABLED' + colors.reset : colors.green + 'DISABLED' + colors.reset}`);
    
    if (!config.enablePowerTests && !config.enableFileTests) {
        console.log(`\n${colors.yellow}⚠ All destructive tests disabled. Running in read-only mode.${colors.reset}`);
        console.log(`${colors.yellow}  Use ENABLE_ALL=true to enable all tests.${colors.reset}`);
    }
    
    // Get containers
    console.log(`\n${colors.cyan}Fetching container list...${colors.reset}`);
    const containers = await getContainers();
    
    if (containers.length === 0) {
        console.log(`${colors.red}✗ No containers found. Cannot run tests.${colors.reset}`);
        process.exit(1);
    }
    
    console.log(`${colors.green}✓ Found ${containers.length} containers${colors.reset}`);
    
    // Run tests
    await testPowerActions(containers);
    await testFilesystemOps(containers);
    
    // Final summary
    stats.overall.total = stats.power.total + stats.files.total;
    stats.overall.success = stats.power.success + stats.files.success;
    stats.overall.failed = stats.power.failed + stats.files.failed;
    stats.overall.times = [...stats.power.times, ...stats.files.times];
    
    console.log(`\n${colors.magenta}╔════════════════════════════════════════╗${colors.reset}`);
    console.log(`${colors.magenta}║         Overall Summary                ║${colors.reset}`);
    console.log(`${colors.magenta}╚════════════════════════════════════════╝${colors.reset}`);
    console.log(`\nTotal operations: ${stats.overall.total}`);
    console.log(`${colors.green}✓ Successful: ${stats.overall.success} (${((stats.overall.success / stats.overall.total) * 100).toFixed(1)}%)${colors.reset}`);
    console.log(`${colors.red}✗ Failed: ${stats.overall.failed} (${((stats.overall.failed / stats.overall.total) * 100).toFixed(1)}%)${colors.reset}`);
    
    if (stats.overall.times.length > 0) {
        const avg = stats.overall.times.reduce((a, b) => a + b, 0) / stats.overall.times.length;
        const sorted = stats.overall.times.sort((a, b) => a - b);
        console.log(`\n${colors.blue}Response Times:${colors.reset}`);
        console.log(`  Avg: ${avg.toFixed(0)}ms`);
        console.log(`  P50: ${sorted[Math.floor(sorted.length * 0.5)]}ms`);
        console.log(`  P95: ${sorted[Math.floor(sorted.length * 0.95)]}ms`);
        console.log(`  Max: ${Math.max(...stats.overall.times)}ms`);
    }
    
    console.log('');
}

main().catch(err => {
    console.error(`${colors.red}Fatal error: ${err.message}${colors.reset}`);
    process.exit(1);
});
