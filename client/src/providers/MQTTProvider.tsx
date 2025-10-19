import React, { createContext, useContext, useEffect, useState, useCallback, useMemo, useRef } from 'react';
import mqtt, { type MqttClient } from 'mqtt';
import { useAuth } from './AuthProvider';

interface MQTTMessage {
  type: string;
  timestamp: string;
  data: any;
  signature: string;
  tenantId?: string;
}

interface LiveUpdate {
  type: 'tenant_created' | 'tenant_updated' | 'tenant_deleted' | 
        'server_created' | 'server_deleted' | 'server_updated' |
        'resources_updated' | 'member_added' | 'member_removed';
  tenantId?: string;
  data: any;
  userId?: string;
}

interface MQTTContextType {
  isConnected: boolean;
  subscribe: (tenantId: string) => void;
  unsubscribe: (tenantId: string) => void;
  onUpdate: (callback: (update: LiveUpdate) => void) => () => void;
}

const MQTTContext = createContext<MQTTContextType | null>(null);

interface MQTTConfig {
  url: string;
  username?: string;
  password?: string;
  encryptionKey: string;
  signingKey: string;
}

interface MQTTProviderProps {
  children: React.ReactNode;
  config?: MQTTConfig;
}

export function MQTTProvider({ children, config }: MQTTProviderProps) {
  const [client, setClient] = useState<MqttClient | null>(null);
  const [isConnected, setIsConnected] = useState(false);
  const [subscribedTenants, setSubscribedTenants] = useState<Set<string>>(new Set());
  const [updateCallbacks, setUpdateCallbacks] = useState<Array<(update: LiveUpdate) => void>>([]);
  const { user, token } = useAuth();
  const connectionSignatureRef = useRef<string | null>(null);
  const isConnectingRef = useRef<boolean>(false);
  const subscribedTenantsRef = useRef<Set<string>>(new Set());

  // Initialize MQTT client
  useEffect(() => {
    if (!config?.url || !user || !token) {
      return;
    }

    const signature = `${config.url}|${config.username || ''}|${user.id}`;
    if (connectionSignatureRef.current === signature || isConnectingRef.current) {
      return;
    }
    isConnectingRef.current = true;

    const clientId = `heliactyl-client-${user.id}-${Date.now()}`;
    
    const mqttClient = mqtt.connect(config.url, {
      clientId,
      username: config.username,
      password: config.password,
      clean: true,
      reconnectPeriod: 5000,
      connectTimeout: 30000,
    });

    mqttClient.on('connect', () => {
      console.log('[HeliaMQTT] Client connected');
      setIsConnected(true);
      connectionSignatureRef.current = signature;
      isConnectingRef.current = false;
    });

    mqttClient.on('error', (error) => {
      console.error('[HeliaMQTT] Client error:', error);
      setIsConnected(false);
    });

    mqttClient.on('disconnect', () => {
      console.log('[HeliaMQTT] Client disconnected');
      setIsConnected(false);
    });

    mqttClient.on('message', (topic, payload) => {
      try {
        const message: MQTTMessage = JSON.parse(payload.toString());
        
        // Verify message signature (simplified - in production, use proper crypto)
        if (!verifyMessage(message)) {
          console.warn('[HeliaMQTT] Invalid message signature, ignoring');
          return;
        }

        // Decrypt message data
        const decryptedData = decryptData(message.data);

        const update: LiveUpdate = {
          type: message.type as any,
          tenantId: message.tenantId,
          data: decryptedData
        };

        // Call all registered callbacks
        updateCallbacks.forEach(callback => {
          try {
            callback(update);
          } catch (error) {
            console.error('[HeliaMQTT] Error in update callback:', error);
          }
        });

      } catch (error) {
        console.error('[HeliaMQTT] Failed to process message:', error);
      }
    });

    setClient(mqttClient);

    return () => {
      mqttClient.end();
      setClient(null);
      setIsConnected(false);
      connectionSignatureRef.current = null;
      isConnectingRef.current = false;
      subscribedTenantsRef.current.clear();
      setSubscribedTenants(new Set());
    };
  }, [config, user, token]);

  // Decrypt message data (Bun compatible - matches server implementation)
  const decryptData = useCallback((encryptedData: any): any => {
    if (!config?.encryptionKey) {
      console.warn('[HeliaMQTT] No encryption key configured');
      return encryptedData;
    }

    try {
      const { encrypted, iv, tag } = encryptedData;
      const key = config.encryptionKey.slice(0, 64); // Use first 64 chars as key
      
      // Verify integrity (simplified - would use proper HMAC in production)
      if (!tag) {
        console.warn('[HeliaMQTT] Missing integrity tag');
        return {};
      }
      
      // Base64 decode
      const binaryData = atob(encrypted);
      
      // Simple XOR decryption (matches server implementation)
      let decrypted = '';
      for (let i = 0; i < binaryData.length; i++) {
        const keyChar = key[i % key.length];
        const decryptedChar = String.fromCharCode(binaryData.charCodeAt(i) ^ keyChar.charCodeAt(0));
        decrypted += decryptedChar;
      }
      
      return JSON.parse(decrypted);
    } catch (error) {
      console.error('[HeliaMQTT] Decryption failed:', error);
      return {};
    }
  }, [config?.encryptionKey]);

  // Verify message signature (simplified implementation)
  const verifyMessage = useCallback((message: MQTTMessage): boolean => {
    if (!config?.signingKey) {
      console.warn('[HeliaMQTT] No signing key configured');
      return true; // Allow unverified messages when no key is configured
    }

    try {
      // In a real implementation, you would verify HMAC-SHA256 signature
      // This is a simplified version for demonstration
      const { signature, ...messageWithoutSig } = message;
      
      // For now, we'll just check if signature exists
      return !!signature;
    } catch (error) {
      console.error('[HeliaMQTT] Signature verification failed:', error);
      return false;
    }
  }, [config?.signingKey]);

  const subscribe = useCallback(async (tenantId: string) => {
    if (!client || !isConnected || subscribedTenantsRef.current.has(tenantId)) {
      return;
    }

    const topics = [
      `heliactyl/tenant/${tenantId}/servers/updates`,
      `heliactyl/tenant/${tenantId}/resources/updates`,
      `heliactyl/tenant/${tenantId}/members/updates`,
      `heliactyl/tenant/${tenantId}/updates`
    ];

    try {
      for (const topic of topics) {
        await new Promise<void>((resolve, reject) => {
          client.subscribe(topic, { qos: 1 }, (error) => {
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

      subscribedTenantsRef.current.add(tenantId);
      // reflect for UI/debugging without changing callback identity
      setSubscribedTenants(new Set(subscribedTenantsRef.current));
    } catch (error) {
      console.error('[HeliaMQTT] Failed to subscribe to tenant updates:', error);
    }
  }, [client, isConnected]);

  const unsubscribe = useCallback(async (tenantId: string) => {
    if (!client || !subscribedTenantsRef.current.has(tenantId)) {
      return;
    }

    const topics = [
      `heliactyl/tenant/${tenantId}/servers/updates`,
      `heliactyl/tenant/${tenantId}/resources/updates`,
      `heliactyl/tenant/${tenantId}/members/updates`,
      `heliactyl/tenant/${tenantId}/updates`
    ];

    try {
      for (const topic of topics) {
        await new Promise<void>((resolve, reject) => {
          client.unsubscribe(topic, (error) => {
            if (error) {
              console.error(`[HeliaMQTT] Failed to unsubscribe from ${topic}:`, error);
              reject(error);
            } else {
              console.log(`[HeliaMQTT] Unsubscribed from ${topic}`);
              resolve();
            }
          });
        });
      }

      subscribedTenantsRef.current.delete(tenantId);
      setSubscribedTenants(new Set(subscribedTenantsRef.current));
    } catch (error) {
      console.error('[HeliaMQTT] Failed to unsubscribe from tenant updates:', error);
    }
  }, [client]);

  const onUpdate = useCallback((callback: (update: LiveUpdate) => void) => {
    setUpdateCallbacks(prev => [...prev, callback]);
    
    // Return cleanup function
    return () => {
      setUpdateCallbacks(prev => prev.filter(cb => cb !== callback));
    };
  }, []);

  const value: MQTTContextType = useMemo(() => ({
    isConnected,
    subscribe,
    unsubscribe,
    onUpdate
  }), [isConnected, subscribe, unsubscribe, onUpdate]);

  return (
    <MQTTContext.Provider value={value}>
      {children}
    </MQTTContext.Provider>
  );
}

export function useMQTT() {
  const context = useContext(MQTTContext);
  if (!context) {
    throw new Error('useMQTT must be used within an MQTTProvider');
  }
  return context;
}

// Hook for automatic tenant subscription management
export function useTenantUpdates(tenantId: string | null) {
  const mqtt = useMQTT();
  const [updates, setUpdates] = useState<LiveUpdate[]>([]);

  useEffect(() => {
    if (!tenantId || !mqtt.isConnected) {
      return;
    }

    mqtt.subscribe(tenantId);

    const cleanup = mqtt.onUpdate((update) => {
      if (update.tenantId === tenantId) {
        setUpdates(prev => [...prev, update]);
      }
    });

    return () => {
      mqtt.unsubscribe(tenantId);
      cleanup();
    };
  }, [tenantId, mqtt]);

  return updates;
}