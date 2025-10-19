/**
 * MQTT Service Integration Tests
 */
import { describe, it, expect, beforeAll, afterAll, beforeEach } from 'bun:test';
import { MQTTService, type LiveUpdateData } from '../lib/mqtt';
import { EmbeddedMQTTBroker, generateBrokerConfig } from '../lib/mqtt-broker';
import { connect, MqttClient } from 'mqtt';
import type { ServerConfig } from '../../types/index';

describe('MQTTService', () => {
  let broker: EmbeddedMQTTBroker;
  let mqttService: MQTTService;
  let testClient: MqttClient;
  let config: ServerConfig;
  let brokerConfig: any;

  beforeAll(async () => {
    config = {
      app: { name: 'Test', version: '1.0.0' },
      server: { host: 'localhost', port: 3100 }, // Different port to avoid conflicts
      security: { sessionTtlSeconds: 3600, passwordHashing: { algorithm: 'bcrypt', bcryptRounds: 10 } },
      database: { url: 'sqlite://test.db' },
      redis: { url: 'redis://localhost:6379' }
    };

    brokerConfig = generateBrokerConfig(config);
    broker = new EmbeddedMQTTBroker(brokerConfig);
    await broker.start();

    // Create MQTT service
    const mqttConfig = {
      url: `mqtt://localhost:${brokerConfig.mqttPort}`,
      username: brokerConfig.credentials.server.username,
      password: brokerConfig.credentials.server.password,
      encryptionKey: brokerConfig.security.encryptionKey,
      signingKey: brokerConfig.security.signingKey
    };

    mqttService = new MQTTService(mqttConfig);
    await mqttService.connect();
  });

  afterAll(async () => {
    if (testClient) {
      testClient.end();
    }
    if (mqttService) {
      await mqttService.disconnect();
    }
    if (broker) {
      await broker.stop();
    }
  });

  beforeEach(() => {
    if (testClient) {
      testClient.end();
    }
  });

  it('should connect to embedded broker successfully', () => {
    expect(mqttService.connected).toBe(true);
  });

  it('should publish live updates with encryption and signatures', async () => {
    // Create test client to listen for messages
    testClient = connect(`mqtt://localhost:${brokerConfig.mqttPort}`, {
      clientId: 'test-subscriber',
      username: brokerConfig.credentials.client.username,
      password: brokerConfig.credentials.client.password,
      connectTimeout: 2000
    });

    await new Promise<void>((resolve) => {
      testClient.on('connect', resolve);
    });

    const receivedMessages: any[] = [];
    
    testClient.on('message', (topic, payload) => {
      const message = JSON.parse(payload.toString());
      receivedMessages.push({ topic, message });
    });

    // Subscribe to test topic
    await new Promise<void>((resolve, reject) => {
      testClient.subscribe('heliactyl/tenant/test-tenant/servers/updates', (err) => {
        if (err) reject(err);
        else resolve();
      });
    });

    // Publish update via MQTT service
    const testUpdate: LiveUpdateData = {
      type: 'server_created',
      tenantId: 'test-tenant',
      data: { id: 'server-123', name: 'Test Server', memoryMb: 1024 },
      userId: 'user-123'
    };

    await mqttService.publishLiveUpdate(testUpdate);

    // Wait for message to be received
    await new Promise(resolve => setTimeout(resolve, 100));

    expect(receivedMessages).toHaveLength(1);
    
    const { message } = receivedMessages[0];
    expect(message.type).toBe('server_created');
    expect(message.tenantId).toBe('test-tenant');
    expect(message.signature).toBeDefined();
    expect(message.data.encrypted).toBeDefined();
    expect(message.data.iv).toBeDefined();
    expect(message.data.tag).toBeDefined();
  });

  it('should route messages to correct topics based on update type', async () => {
    testClient = connect(`mqtt://localhost:${brokerConfig.mqttPort}`, {
      clientId: 'test-router',
      username: brokerConfig.credentials.client.username,
      password: brokerConfig.credentials.client.password,
      connectTimeout: 2000
    });

    await new Promise<void>((resolve) => {
      testClient.on('connect', resolve);
    });

    const receivedTopics: string[] = [];
    
    testClient.on('message', (topic) => {
      receivedTopics.push(topic);
    });

    // Subscribe to multiple topics
    const topics = [
      'heliactyl/tenant/test/servers/updates',
      'heliactyl/tenant/test/members/updates',
      'heliactyl/tenant/test/resources/updates',
      'heliactyl/global/tenants/updates'
    ];

    for (const topic of topics) {
      await new Promise<void>((resolve, reject) => {
        testClient.subscribe(topic, (err) => {
          if (err) reject(err);
          else resolve();
        });
      });
    }

    // Test different update types
    const updates: LiveUpdateData[] = [
      { type: 'server_created', tenantId: 'test', data: {}, userId: 'user' },
      { type: 'member_added', tenantId: 'test', data: {}, userId: 'user' },
      { type: 'resources_updated', tenantId: 'test', data: {}, userId: 'user' },
      { type: 'tenant_created', tenantId: 'test', data: {}, userId: 'user' }
    ];

    for (const update of updates) {
      await mqttService.publishLiveUpdate(update);
    }

    // Wait for messages
    await new Promise(resolve => setTimeout(resolve, 200));

    expect(receivedTopics).toContain('heliactyl/tenant/test/servers/updates');
    expect(receivedTopics).toContain('heliactyl/tenant/test/members/updates');
    expect(receivedTopics).toContain('heliactyl/tenant/test/resources/updates');
    expect(receivedTopics).toContain('heliactyl/global/tenants/updates');
  });

  it('should handle subscription callbacks correctly', async () => {
    const receivedUpdates: LiveUpdateData[] = [];
    
    const callback = (update: LiveUpdateData) => {
      receivedUpdates.push(update);
    };

    await mqttService.subscribeToTenant('callback-test', callback);

    // Publish test update
    const testUpdate: LiveUpdateData = {
      type: 'server_deleted',
      tenantId: 'callback-test',
      data: { id: 'server-456' },
      userId: 'user-456'
    };

    await mqttService.publishLiveUpdate(testUpdate);

    // Wait for callback
    await new Promise(resolve => setTimeout(resolve, 200));

    expect(receivedUpdates).toHaveLength(1);
    expect(receivedUpdates[0].type).toBe('server_deleted');
    expect(receivedUpdates[0].tenantId).toBe('callback-test');
    expect(receivedUpdates[0].data.id).toBe('server-456');
  });

  it('should generate unique encryption keys', () => {
    const keys1 = MQTTService.generateKeys();
    const keys2 = MQTTService.generateKeys();

    expect(keys1.encryptionKey).not.toBe(keys2.encryptionKey);
    expect(keys1.signingKey).not.toBe(keys2.signingKey);
    expect(keys1.encryptionKey).toHaveLength(64);
    expect(keys1.signingKey).toHaveLength(64);
  });
});