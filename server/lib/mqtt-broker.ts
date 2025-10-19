/**
 * Embedded MQTT Broker using Aedes
 * Provides zero-configuration MQTT broker with WebSocket support
 */
import Aedes from 'aedes';
import { createServer } from 'node:net';
import { WebSocketServer } from 'ws';
import { createServer as createHttpServer } from 'node:http';
import { randomBytes } from 'node:crypto';
import type { ServerConfig } from '../../types/index';

export interface BrokerConfig {
  mqttPort: number;
  wsPort: number;
  credentials: {
    server: { username: string; password: string };
    client: { username: string; password: string };
  };
  security: {
    encryptionKey: string;
    signingKey: string;
  };
}

export class EmbeddedMQTTBroker {
  private aedes: Aedes;
  private mqttServer: any = null;
  private wsServer: WebSocketServer | null = null;
  private httpServer: any = null;
  private config: BrokerConfig;
  private isRunning = false;

  constructor(config: BrokerConfig) {
    this.config = config;
    this.aedes = new Aedes({
      authenticate: this.authenticate.bind(this),
      authorizePublish: this.authorizePublish.bind(this),
      authorizeSubscribe: this.authorizeSubscribe.bind(this),
    });

    this.setupEventHandlers();
  }

  private setupEventHandlers() {
    this.aedes.on('client', (client) => {
      console.log(`[HeliaMQTT] Client ${client.id} connected`);
    });

    this.aedes.on('clientDisconnect', (client) => {
      console.log(`[HeliaMQTT] Client ${client.id} disconnected`);
    });

    this.aedes.on('publish', (packet, client) => {
      if (client) {
        console.log(`[HeliaMQTT] Message published to ${packet.topic} by ${client.id}`);
      }
    });

    this.aedes.on('subscribe', (subscriptions, client) => {
      console.log(`[HeliaMQTT] Client ${client.id} subscribed to:`, 
        subscriptions.map(s => s.topic).join(', '));
    });
  }

  private async authenticate(client: any, username: string, password: Buffer, callback: Function) {
    const passwordStr = password.toString();
    
    // Server authentication
    if (username === this.config.credentials.server.username && 
        passwordStr === this.config.credentials.server.password) {
      client.isServer = true;
      return callback(null, true);
    }
    
    // Client authentication  
    if (username === this.config.credentials.client.username && 
        passwordStr === this.config.credentials.client.password) {
      client.isClient = true;
      return callback(null, true);
    }

    console.log(`[HeliaMQTT] Authentication failed for ${username}`);
    callback(null, false);
  }

  private async authorizePublish(client: any, packet: any, callback: Function) {
    // Only server can publish updates
    if (client.isServer) {
      return callback(null);
    }

    // Clients can only publish to heartbeat/status topics
    if (packet.topic.startsWith('heliactyl/client/') && client.isClient) {
      return callback(null);
    }

    console.log(`[HeliaMQTT] Publish denied for ${client.id} to ${packet.topic}`);
    callback(new Error('Unauthorized publish'));
  }

  private async authorizeSubscribe(client: any, sub: any, callback: Function) {
    // Server can subscribe to anything
    if (client.isServer) {
      return callback(null, sub);
    }

    // Clients can only subscribe to heliactyl topics and server status
    if (client.isClient && (
      sub.topic.startsWith('heliactyl/') || 
      sub.topic.startsWith('$SYS/') ||
      sub.topic === 'heliactyl/server/status'
    )) {
      return callback(null, sub);
    }

    console.log(`[HeliaMQTT] Subscribe denied for ${client.id} to ${sub.topic}`);
    callback(new Error('Unauthorized subscription'));
  }

  async start(): Promise<void> {
    if (this.isRunning) {
      console.log('[HeliaMQTT] Already running');
      return;
    }

    try {
      // Start MQTT server
      this.mqttServer = createServer(this.aedes.handle);
      await new Promise<void>((resolve, reject) => {
        this.mqttServer.listen(this.config.mqttPort, (error: any) => {
          if (error) {
            reject(error);
          } else {
            console.log(`[HeliaMQTT] TCP server listening on port ${this.config.mqttPort}`);
            resolve();
          }
        });
      });

      // Start WebSocket server
      this.httpServer = createHttpServer();
      this.wsServer = new WebSocketServer({ 
        server: this.httpServer,
        path: '/mqtt'
      });

      this.wsServer.on('connection', (ws, req) => {
        const websocketStream = require('websocket-stream');
        const stream = websocketStream(ws);
        this.aedes.handle(stream);
      });

      await new Promise<void>((resolve, reject) => {
        this.httpServer.listen(this.config.wsPort, (error: any) => {
          if (error) {
            reject(error);
          } else {
            console.log(`[HeliaMQTT] WebSocket server listening on port ${this.config.wsPort}`);
            resolve();
          }
        });
      });

      this.isRunning = true;
      console.log('[HeliaMQTT] Embedded MQTT broker started successfully');

    } catch (error) {
      console.error('[HeliaMQTT] Failed to start:', error);
      throw error;
    }
  }

  async stop(): Promise<void> {
    if (!this.isRunning) {
      return;
    }

    try {
      if (this.wsServer) {
        this.wsServer.close();
        this.wsServer = null;
      }

      if (this.httpServer) {
        await new Promise<void>((resolve) => {
          this.httpServer.close(() => resolve());
        });
        this.httpServer = null;
      }

      if (this.mqttServer) {
        await new Promise<void>((resolve) => {
          this.mqttServer.close(() => resolve());
        });
        this.mqttServer = null;
      }

      await new Promise<void>((resolve) => {
        this.aedes.close(() => resolve());
      });

      this.isRunning = false;
      console.log('[HeliaMQTT] Embedded MQTT broker stopped');

    } catch (error) {
      console.error('[HeliaMQTT] Error stopping broker:', error);
      throw error;
    }
  }

  getConfig(): BrokerConfig {
    return this.config;
  }

  get running(): boolean {
    return this.isRunning;
  }
}

/**
 * Auto-generate MQTT broker configuration
 */
export function generateBrokerConfig(serverConfig: ServerConfig): BrokerConfig {
  const basePort = serverConfig.server.port;
  
  return {
    mqttPort: basePort + 1000,     // e.g., 4000 if server is 3000
    wsPort: basePort + 1001,       // e.g., 4001 if server is 3000
    credentials: {
      server: {
        username: 'heliactyl-server',
        password: randomBytes(32).toString('base64url')
      },
      client: {
        username: 'heliactyl-client', 
        password: randomBytes(32).toString('base64url')
      }
    },
    security: {
      encryptionKey: randomBytes(32).toString('hex'),
      signingKey: randomBytes(32).toString('hex')
    }
  };
}

/**
 * Create and configure embedded MQTT broker
 */
export async function createEmbeddedBroker(serverConfig: ServerConfig): Promise<EmbeddedMQTTBroker> {
  const brokerConfig = generateBrokerConfig(serverConfig);
  const broker = new EmbeddedMQTTBroker(brokerConfig);
  
  return broker;
}