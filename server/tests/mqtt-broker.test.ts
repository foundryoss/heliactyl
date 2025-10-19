/**
 * MQTT Broker Tests
 */
import { describe, it, expect, beforeAll, afterAll } from 'bun:test';
import { EmbeddedMQTTBroker, generateBrokerConfig } from '../lib/mqtt-broker';
import { connect, MqttClient } from 'mqtt';
import type { ServerConfig } from '../../types/index';

describe('EmbeddedMQTTBroker', () => {
  let broker: EmbeddedMQTTBroker;
  let config: ServerConfig;

  beforeAll(() => {
    config = {
      app: { name: 'Test', version: '1.0.0' },
      server: { host: 'localhost', port: 3000 },
      security: { sessionTtlSeconds: 3600, passwordHashing: { algorithm: 'bcrypt', bcryptRounds: 10 } },
      database: { url: 'sqlite://test.db' },
      redis: { url: 'redis://localhost:6379' }
    };
  });

  afterAll(async () => {
    if (broker && broker.running) {
      await broker.stop();
    }
  });

  it('should generate broker configuration with correct ports', () => {
    const brokerConfig = generateBrokerConfig(config);
    
    expect(brokerConfig.mqttPort).toBe(4000); // 3000 + 1000
    expect(brokerConfig.wsPort).toBe(4001);   // 3000 + 1001
    expect(brokerConfig.credentials.server.username).toBe('heliactyl-server');
    expect(brokerConfig.credentials.client.username).toBe('heliactyl-client');
    expect(brokerConfig.security.encryptionKey).toHaveLength(64); // 32 bytes hex
    expect(brokerConfig.security.signingKey).toHaveLength(64);    // 32 bytes hex
  });

  it('should start and stop broker successfully', async () => {
    const brokerConfig = generateBrokerConfig(config);
    broker = new EmbeddedMQTTBroker(brokerConfig);
    
    expect(broker.running).toBe(false);
    
    await broker.start();
    expect(broker.running).toBe(true);
    
    await broker.stop();
    expect(broker.running).toBe(false);
  });

  it('should authenticate server credentials', async () => {
    const brokerConfig = generateBrokerConfig(config);
    broker = new EmbeddedMQTTBroker(brokerConfig);
    await broker.start();

    const serverClient = connect(`mqtt://localhost:${brokerConfig.mqttPort}`, {
      clientId: 'test-server',
      username: brokerConfig.credentials.server.username,
      password: brokerConfig.credentials.server.password,
      connectTimeout: 2000,
      reconnectPeriod: 0
    });

    await new Promise<void>((resolve, reject) => {
      serverClient.on('connect', () => {
        serverClient.end();
        resolve();
      });
      serverClient.on('error', reject);
      setTimeout(() => reject(new Error('Connection timeout')), 3000);
    });
  });

  it('should authenticate client credentials', async () => {
    const brokerConfig = generateBrokerConfig(config);
    if (!broker || !broker.running) {
      broker = new EmbeddedMQTTBroker(brokerConfig);
      await broker.start();
    }

    const clientClient = connect(`mqtt://localhost:${brokerConfig.mqttPort}`, {
      clientId: 'test-client',
      username: brokerConfig.credentials.client.username,
      password: brokerConfig.credentials.client.password,
      connectTimeout: 2000,
      reconnectPeriod: 0
    });

    await new Promise<void>((resolve, reject) => {
      clientClient.on('connect', () => {
        clientClient.end();
        resolve();
      });
      clientClient.on('error', reject);
      setTimeout(() => reject(new Error('Connection timeout')), 3000);
    });
  });

  it('should reject invalid credentials', async () => {
    const brokerConfig = generateBrokerConfig(config);
    if (!broker || !broker.running) {
      broker = new EmbeddedMQTTBroker(brokerConfig);
      await broker.start();
    }

    const invalidClient = connect(`mqtt://localhost:${brokerConfig.mqttPort}`, {
      clientId: 'test-invalid',
      username: 'invalid-user',
      password: 'invalid-password',
      connectTimeout: 2000,
      reconnectPeriod: 0
    });

    await new Promise<void>((resolve, reject) => {
      invalidClient.on('connect', () => {
        invalidClient.end();
        reject(new Error('Should not have connected with invalid credentials'));
      });
      invalidClient.on('error', (error) => {
        if (error.message.includes('Not authorized')) {
          resolve();
        } else {
          reject(error);
        }
      });
      setTimeout(() => resolve(), 3000); // Timeout = success (no connection)
    });
  });
});