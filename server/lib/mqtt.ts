/**
 * MQTT Service for Live Updates
 * Provides secure real-time updates for tenants, servers, and resources
 */
import { connect, MqttClient } from 'mqtt';
import { randomBytes, createHmac } from 'node:crypto';
import type { ServerConfig } from '../../types/index';

export interface MQTTConfig {
  url: string;
  username?: string;
  password?: string;
  clientId?: string;
  encryptionKey: string; // 32 bytes hex
  signingKey: string; // 32 bytes hex
}

export interface MQTTMessage {
  type: string;
  timestamp: string;
  data: any;
  signature: string;
  tenantId?: string;
}

export interface LiveUpdateData {
  type: 'tenant_created' | 'tenant_updated' | 'tenant_deleted' | 
        'server_created' | 'server_deleted' | 'server_updated' |
        'resources_updated' | 'member_added' | 'member_removed';
  tenantId?: string;
  data: any;
  userId?: string;
}

export class MQTTService {
  private client: MqttClient | null = null;
  private config: MQTTConfig;
  private isConnected = false;

  constructor(config: MQTTConfig) {
    this.config = config;
  }

  async connect(): Promise<void> {
    if (this.client) {
      await this.disconnect();
    }

    const clientId = this.config.clientId || `heliactyl-server-${randomBytes(4).toString('hex')}`;
    
    this.client = connect(this.config.url, {
      clientId,
      username: this.config.username,
      password: this.config.password,
      clean: true,
      reconnectPeriod: 5000,
      connectTimeout: 30000,
      will: {
        topic: 'heliactyl/server/status',
        payload: JSON.stringify({ status: 'offline', clientId, timestamp: new Date().toISOString() }),
        qos: 1,
        retain: false
      }
    });

    return new Promise((resolve, reject) => {
      if (!this.client) return reject(new Error('Failed to create MQTT client'));

      this.client.on('connect', () => {
        console.log('[HeliaMQTT] Connected to broker');
        this.isConnected = true;
        
        // Publish server online status
        this.publishServerStatus('online');
        resolve();
      });

      this.client.on('error', (error) => {
        console.error('[HeliaMQTT] Connection error:', error);
        this.isConnected = false;
        reject(error);
      });

      this.client.on('disconnect', () => {
        console.log('[HeliaMQTT] Disconnected from broker');
        this.isConnected = false;
      });

      this.client.on('reconnect', () => {
        console.log('[HeliaMQTT] Reconnecting to broker...');
      });
    });
  }

  async disconnect(): Promise<void> {
    if (this.client) {
      this.publishServerStatus('offline');
      await new Promise<void>((resolve) => {
        this.client!.end(false, {}, () => {
          this.isConnected = false;
          resolve();
        });
      });
      this.client = null;
    }
  }

  private publishServerStatus(status: 'online' | 'offline'): void {
    if (!this.client || !this.isConnected) return;
    
    const message = {
      status,
      timestamp: new Date().toISOString(),
      clientId: this.client.options.clientId
    };

    this.client.publish('heliactyl/server/status', JSON.stringify(message), { qos: 1 });
  }

  /**
   * Encrypt message data using simple XOR + Base64 (Bun compatible)
   * For production, consider using Web Crypto API for better security
   */
  private encryptData(data: any): { encrypted: string; iv: string; tag: string } {
    const key = this.config.encryptionKey.slice(0, 64); // Use first 64 chars as key
    const plaintext = JSON.stringify(data);
    const iv = randomBytes(16).toString('hex');
    
    // Simple XOR encryption (for compatibility)
    let encrypted = '';
    for (let i = 0; i < plaintext.length; i++) {
      const keyChar = key[i % key.length];
      const encryptedChar = String.fromCharCode(plaintext.charCodeAt(i) ^ keyChar.charCodeAt(0));
      encrypted += encryptedChar;
    }
    
    // Base64 encode the result
    const encodedEncrypted = Buffer.from(encrypted, 'binary').toString('base64');
    
    // Generate integrity tag
    const tag = createHmac('sha256', key).update(encodedEncrypted + iv).digest('hex').slice(0, 32);
    
    return {
      encrypted: encodedEncrypted,
      iv,
      tag
    };
  }

  /**
   * Decrypt message data using simple XOR + Base64 (Bun compatible)
   */
  private decryptData(encrypted: string, iv: string, tag: string): any {
    const key = this.config.encryptionKey.slice(0, 64); // Use first 64 chars as key
    
    // Verify integrity tag first
    const expectedTag = createHmac('sha256', key).update(encrypted + iv).digest('hex').slice(0, 32);
    if (tag !== expectedTag) {
      throw new Error('Message integrity check failed');
    }
    
    // Base64 decode
    const binaryData = Buffer.from(encrypted, 'base64').toString('binary');
    
    // Simple XOR decryption
    let decrypted = '';
    for (let i = 0; i < binaryData.length; i++) {
      const keyChar = key[i % key.length];
      const decryptedChar = String.fromCharCode(binaryData.charCodeAt(i) ^ keyChar.charCodeAt(0));
      decrypted += decryptedChar;
    }
    
    return JSON.parse(decrypted);
  }

  /**
   * Sign message with HMAC-SHA256
   */
  private signMessage(message: Omit<MQTTMessage, 'signature'>): string {
    const key = Buffer.from(this.config.signingKey, 'hex');
    const data = JSON.stringify(message);
    return createHmac('sha256', key).update(data).digest('hex');
  }

  /**
   * Verify message signature
   */
  private verifyMessage(message: MQTTMessage): boolean {
    const { signature, ...messageWithoutSig } = message;
    const expectedSignature = this.signMessage(messageWithoutSig);
    return signature === expectedSignature;
  }

  /**
   * Publish live update to appropriate topic
   */
  async publishLiveUpdate(update: LiveUpdateData): Promise<void> {
    if (!this.client || !this.isConnected) {
      console.warn('[HeliaMQTT] Cannot publish - not connected');
      return;
    }

    const topic = this.getTopicForUpdate(update);
    const encryptedData = this.encryptData(update.data);
    
    const message: Omit<MQTTMessage, 'signature'> = {
      type: update.type,
      timestamp: new Date().toISOString(),
      data: encryptedData,
      tenantId: update.tenantId
    };

    const signature = this.signMessage(message);
    const finalMessage: MQTTMessage = { ...message, signature };

    try {
      await new Promise<void>((resolve, reject) => {
        this.client!.publish(topic, JSON.stringify(finalMessage), { qos: 1 }, (error) => {
          if (error) {
            console.error('[HeliaMQTT] Publish error:', error);
            reject(error);
          } else {
            console.log(`[HeliaMQTT] Published ${update.type} to ${topic}`);
            resolve();
          }
        });
      });
    } catch (error) {
      console.error('[HeliaMQTT] Failed to publish update:', error);
    }
  }

  /**
   * Subscribe to updates for a specific tenant
   */
  async subscribeToTenant(tenantId: string, callback: (update: LiveUpdateData) => void): Promise<void> {
    if (!this.client || !this.isConnected) {
      throw new Error('MQTT client not connected');
    }

    const topics = [
      `heliactyl/tenant/${tenantId}/servers/updates`,
      `heliactyl/tenant/${tenantId}/resources/updates`,
      `heliactyl/tenant/${tenantId}/members/updates`,
      `heliactyl/tenant/${tenantId}/updates`
    ];

    for (const topic of topics) {
      await new Promise<void>((resolve, reject) => {
        this.client!.subscribe(topic, { qos: 1 }, (error) => {
          if (error) {
            console.error(`[HeliaMQTT] Failed to subscribe to ${topic}:`, error);
            reject(error);
          } else {
            console.log(`[HeliaMQTT] Subscribed to ${topic}`);
            resolve();
          }
        });
      });
    }

    this.client.on('message', (topic, payload) => {
      try {
        const message: MQTTMessage = JSON.parse(payload.toString());
        
        if (!this.verifyMessage(message)) {
          console.warn('[HeliaMQTT] Invalid message signature, ignoring');
          return;
        }

        const decryptedData = this.decryptData(
          message.data.encrypted,
          message.data.iv,
          message.data.tag
        );

        const update: LiveUpdateData = {
          type: message.type as any,
          tenantId: message.tenantId,
          data: decryptedData
        };

        callback(update);
      } catch (error) {
        console.error('[HeliaMQTT] Failed to process message:', error);
      }
    });
  }

  /**
   * Subscribe to global admin updates
   */
  async subscribeToGlobal(callback: (update: LiveUpdateData) => void): Promise<void> {
    if (!this.client || !this.isConnected) {
      throw new Error('MQTT client not connected');
    }

    const topic = 'heliactyl/global/tenants/updates';

    await new Promise<void>((resolve, reject) => {
      this.client!.subscribe(topic, { qos: 1 }, (error) => {
        if (error) {
          console.error(`[HeliaMQTT] Failed to subscribe to ${topic}:`, error);
          reject(error);
        } else {
          console.log(`[HeliaMQTT] Subscribed to ${topic}`);
          resolve();
        }
      });
    });

    this.client.on('message', (topic, payload) => {
      if (topic !== 'heliactyl/global/tenants/updates') return;

      try {
        const message: MQTTMessage = JSON.parse(payload.toString());
        
        if (!this.verifyMessage(message)) {
          console.warn('[HeliaMQTT] Invalid message signature, ignoring');
          return;
        }

        const decryptedData = this.decryptData(
          message.data.encrypted,
          message.data.iv,
          message.data.tag
        );

        const update: LiveUpdateData = {
          type: message.type as any,
          tenantId: message.tenantId,
          data: decryptedData
        };

        callback(update);
      } catch (error) {
        console.error('[HeliaMQTT] Failed to process global message:', error);
      }
    });
  }

  private getTopicForUpdate(update: LiveUpdateData): string {
    switch (update.type) {
      case 'server_created':
      case 'server_deleted':
      case 'server_updated':
        return `heliactyl/tenant/${update.tenantId}/servers/updates`;
      
      case 'resources_updated':
        return `heliactyl/tenant/${update.tenantId}/resources/updates`;
      
      case 'member_added':
      case 'member_removed':
        return `heliactyl/tenant/${update.tenantId}/members/updates`;
      
      case 'tenant_created':
      case 'tenant_updated':
      case 'tenant_deleted':
        // Global admin topic for tenant lifecycle
        return 'heliactyl/global/tenants/updates';
      
      default:
        return `heliactyl/tenant/${update.tenantId}/updates`;
    }
  }

  /**
   * Generate secure keys for encryption and signing
   */
  static generateKeys(): { encryptionKey: string; signingKey: string } {
    return {
      encryptionKey: randomBytes(32).toString('hex'),
      signingKey: randomBytes(32).toString('hex')
    };
  }

  get connected(): boolean {
    return this.isConnected;
  }
}

/**
 * Create MQTT service from server config
 */
export function createMQTTService(config: ServerConfig & { mqtt?: MQTTConfig }): MQTTService | null {
  const mqttConfig = config.mqtt;
  
  if (!mqttConfig?.url) {
    console.log('[HeliaMQTT] No MQTT configuration found, live updates disabled');
    return null;
  }

  if (!mqttConfig.encryptionKey || !mqttConfig.signingKey) {
    console.error('[HeliaMQTT] Missing encryption or signing keys, live updates disabled');
    return null;
  }

  return new MQTTService(mqttConfig);
}