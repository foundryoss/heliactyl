/**
 * End-to-End MQTT Live Updates Tests
 * Tests the full flow from server route publishing to client receiving updates
 */
import { describe, it, expect, beforeAll, afterAll } from 'bun:test';
import { createApp } from '../index';
import { connect, MqttClient } from 'mqtt';
import type { ServerConfig } from '../../types/index';

describe('MQTT Live Updates E2E', () => {
  let app: any;
  let server: any;
  let testClient: MqttClient;
  let mqttConfig: any;
  let authToken: string;
  let tenantId: string;

  beforeAll(async () => {
    // Create test config
    const config: ServerConfig = {
      app: { name: 'Test E2E', version: '1.0.0' },
      server: { host: 'localhost', port: 3200 },
      security: { 
        sessionTtlSeconds: 3600, 
        passwordHashing: { algorithm: 'bcrypt', bcryptRounds: 10 },
        encryptionKey: '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef'
      },
      database: { url: 'sqlite://./test-e2e.db' },
      redis: { url: 'redis://localhost:6379' }
    };

    // Start test server
    const result = await createApp({ config, serve: true });
    app = result.app;
    server = result.server;

    // Get MQTT configuration from the server
    const response = await fetch('http://localhost:3200/api/core/mqtt/config');
    if (!response.ok) {
      throw new Error('Failed to get MQTT config');
    }
    mqttConfig = await response.json();

    // Register test user
    const registerResponse = await fetch('http://localhost:3200/api/auth/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        email: 'test@example.com',
        username: 'testuser',
        password: 'testpassword123'
      })
    });

    if (registerResponse.ok) {
      const registerData = await registerResponse.json();
      authToken = registerData.token;
    } else {
      // Try login if user already exists
      const loginResponse = await fetch('http://localhost:3200/api/auth/login', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          identifier: 'test@example.com',
          password: 'testpassword123'
        })
      });
      const loginData = await loginResponse.json();
      authToken = loginData.token;
    }

    // Create test tenant
    const tenantResponse = await fetch('http://localhost:3200/api/tenants', {
      method: 'POST',
      headers: { 
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${authToken}`
      },
      body: JSON.stringify({ name: 'Test Tenant E2E' })
    });
    const tenantData = await tenantResponse.json();
    tenantId = tenantData.id;
  });

  afterAll(async () => {
    if (testClient) {
      testClient.end();
    }
    if (server) {
      server.stop();
    }
  });

  it('should receive live updates when creating a tenant', async () => {
    // Connect MQTT test client
    testClient = connect(mqttConfig.url, {
      clientId: 'e2e-test-client',
      username: mqttConfig.username,
      password: mqttConfig.password,
      connectTimeout: 5000
    });

    await new Promise<void>((resolve, reject) => {
      testClient.on('connect', resolve);
      testClient.on('error', reject);
      setTimeout(() => reject(new Error('MQTT connection timeout')), 6000);
    });

    const receivedUpdates: any[] = [];
    
    testClient.on('message', (topic, payload) => {
      try {
        const message = JSON.parse(payload.toString());
        receivedUpdates.push({ topic, message });
      } catch (error) {
        console.error('Failed to parse MQTT message:', error);
      }
    });

    // Subscribe to global tenant updates
    await new Promise<void>((resolve, reject) => {
      testClient.subscribe('heliactyl/global/tenants/updates', (err) => {
        if (err) reject(err);
        else resolve();
      });
    });

    // Create another tenant via API
    const response = await fetch('http://localhost:3200/api/tenants', {
      method: 'POST',
      headers: { 
        'Content-Type': 'application/json',
        'Authorization': `Bearer ${authToken}`
      },
      body: JSON.stringify({ name: 'Live Update Test Tenant' })
    });

    expect(response.ok).toBe(true);

    // Wait for MQTT message
    await new Promise(resolve => setTimeout(resolve, 500));

    expect(receivedUpdates.length).toBeGreaterThan(0);
    
    const tenantUpdate = receivedUpdates.find(update => 
      update.topic === 'heliactyl/global/tenants/updates' &&
      update.message.type === 'tenant_created'
    );
    
    expect(tenantUpdate).toBeDefined();
    expect(tenantUpdate.message.signature).toBeDefined();
    expect(tenantUpdate.message.data.encrypted).toBeDefined();
  });

  it('should receive live updates when adding tenant members', async () => {
    const receivedUpdates: any[] = [];
    
    testClient.removeAllListeners('message');
    testClient.on('message', (topic, payload) => {
      try {
        const message = JSON.parse(payload.toString());
        receivedUpdates.push({ topic, message });
      } catch (error) {
        console.error('Failed to parse MQTT message:', error);
      }
    });

    // Subscribe to tenant member updates
    await new Promise<void>((resolve, reject) => {
      testClient.subscribe(`heliactyl/tenant/${tenantId}/members/updates`, (err) => {
        if (err) reject(err);
        else resolve();
      });
    });

    // Create another user first
    const userResponse = await fetch('http://localhost:3200/api/auth/register', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        email: 'member@example.com',
        username: 'memberuser',
        password: 'memberpass123'
      })
    });

    let memberId: string;
    if (userResponse.ok) {
      const userData = await userResponse.json();
      memberId = userData.userId;
    } else {
      // User might exist, try to get their ID by email
      const response = await fetch('http://localhost:3200/api/admin/users', {
        headers: { 'Authorization': `Bearer ${authToken}` }
      });
      
      if (response.ok) {
        const users = await response.json();
        const member = users.items.find((u: any) => u.email === 'member@example.com');
        if (member) {
          memberId = member.id;
        }
      }
    }

    if (memberId) {
      // Add member to tenant
      const memberResponse = await fetch(`http://localhost:3200/api/tenants/${tenantId}/members/by-email`, {
        method: 'POST',
        headers: { 
          'Content-Type': 'application/json',
          'Authorization': `Bearer ${authToken}`
        },
        body: JSON.stringify({ email: 'member@example.com' })
      });

      expect(memberResponse.ok).toBe(true);

      // Wait for MQTT message
      await new Promise(resolve => setTimeout(resolve, 500));

      const memberUpdate = receivedUpdates.find(update => 
        update.topic === `heliactyl/tenant/${tenantId}/members/updates` &&
        update.message.type === 'member_added'
      );
      
      expect(memberUpdate).toBeDefined();
      expect(memberUpdate.message.signature).toBeDefined();
    }
  });

  it('should handle WebSocket MQTT connections', async () => {
    // Test WebSocket connection (if WebSocket URL is provided)
    if (mqttConfig.url.startsWith('ws')) {
      const wsClient = connect(mqttConfig.url, {
        clientId: 'e2e-ws-client',
        username: mqttConfig.username,
        password: mqttConfig.password,
        connectTimeout: 5000
      });

      await new Promise<void>((resolve, reject) => {
        wsClient.on('connect', () => {
          wsClient.end();
          resolve();
        });
        wsClient.on('error', reject);
        setTimeout(() => reject(new Error('WebSocket connection timeout')), 6000);
      });
    }
  });

  it('should auto-configure MQTT without manual setup', async () => {
    // Verify MQTT config endpoint returns valid configuration
    const response = await fetch('http://localhost:3200/api/core/mqtt/config');
    expect(response.ok).toBe(true);
    
    const config = await response.json();
    expect(config.url).toBeDefined();
    expect(config.username).toBeDefined();
    expect(config.password).toBeDefined();
    expect(config.encryptionKey).toBeDefined();
    expect(config.signingKey).toBeDefined();
    
    // Verify credentials work
    const verifyClient = connect(config.url, {
      clientId: 'verify-config-client',
      username: config.username,
      password: config.password,
      connectTimeout: 3000,
      reconnectPeriod: 0
    });

    await new Promise<void>((resolve, reject) => {
      verifyClient.on('connect', () => {
        verifyClient.end();
        resolve();
      });
      verifyClient.on('error', reject);
      setTimeout(() => reject(new Error('Config verification timeout')), 4000);
    });
  });
});