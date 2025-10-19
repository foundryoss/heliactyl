/**
 * MQTT Configuration API
 * Provides frontend with MQTT broker connection details
 */
import { Hono } from 'hono';
import type { Context } from 'hono';
import type { AppEnv } from '../../lib/hono-env';

const router = new Hono<AppEnv>();

/**
 * [GET /mqtt/config] Returns MQTT configuration for frontend
 */
router.get('/config', async (c: Context<AppEnv>) => {
  const mqttBroker = c.get('mqttBroker') as AppEnv['Variables']['mqttBroker'];
  
  if (!mqttBroker || !mqttBroker.running) {
    return c.json({ error: 'MQTT broker not available' }, 503);
  }

  const brokerConfig = mqttBroker.getConfig();
  
  // Return client configuration (not server credentials)
  return c.json({
    url: `ws://localhost:${brokerConfig.wsPort}/mqtt`,
    username: brokerConfig.credentials.client.username,
    password: brokerConfig.credentials.client.password,
    encryptionKey: brokerConfig.security.encryptionKey,
    signingKey: brokerConfig.security.signingKey
  });
});

export default router;