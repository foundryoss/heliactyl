import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { MQTTProvider } from '@/providers/MQTTProvider';
import { useApp } from '@/providers/AppProvider';
import { useAuth } from '@/providers/AuthProvider';
import { useApi } from '@/api/client';

interface MQTTConfig {
  url: string;
  username: string;
  password: string;
  encryptionKey: string;
  signingKey: string;
}

interface MQTTWrapperProps {
  children: React.ReactNode;
}

export function MQTTWrapper({ children }: MQTTWrapperProps) {
  const { config } = useApp();
  const { token, user } = useAuth();
  const [mqttConfig, setMqttConfig] = useState<MQTTConfig | null>(null);
  const getToken = useCallback(() => token, [token]);
  const api = useApi(getToken);
  const fetchedKeyRef = useRef<string | null>(null);

  const sessionKey = useMemo(() => {
    if (!user || !token) return null;
    return `${user.id}:${token.slice(0, 8)}`;
  }, [user, token]);

  useEffect(() => {
    // Only fetch once per auth session
    if (!sessionKey) {
      setMqttConfig(null);
      fetchedKeyRef.current = null;
      return;
    }
    if (fetchedKeyRef.current === sessionKey) return;

    fetchedKeyRef.current = sessionKey;

    api.core.mqttConfig()
      .then((serverConfig) => {
        const nextKey = `${serverConfig.url}|${serverConfig.username}`;
        const currentKey = mqttConfig ? `${mqttConfig.url}|${mqttConfig.username}` : null;
        if (nextKey !== currentKey) {
          console.log('[HeliaMQTT] Using auto-configured embedded broker');
          setMqttConfig(serverConfig);
        }
      })
      .catch(() => {
        // Fallback to static config from config.json
        if (config.mqtt) {
          const nextKey = `${config.mqtt.url}|${config.mqtt.username}`;
          const currentKey = mqttConfig ? `${mqttConfig.url}|${mqttConfig.username}` : null;
          if (nextKey !== currentKey) {
            console.log('[HeliaMQTT] Embedded broker not available, using config.json');
            setMqttConfig(config.mqtt as MQTTConfig);
          }
        } else {
          console.log('[HeliaMQTT] No MQTT configuration available');
          setMqttConfig(null);
        }
      });
  }, [sessionKey, api, config.mqtt, mqttConfig]);

  return (
    <MQTTProvider config={mqttConfig}>
      {children}
    </MQTTProvider>
  );
}