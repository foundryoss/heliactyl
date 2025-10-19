import { Hono } from 'hono';
import { cors } from 'hono/cors';
import { loadConfig } from './lib/config';
import { createHeliaDB } from './db';
import { createRedis } from './lib/redis';
import { createSessionService } from './lib/session';
import { loadModules, registerModules } from './lib/modules';
import type { ModuleRegisterContext, ServerConfig } from '../types/index';
import type { AppEnv } from './lib/hono-env';
import { createDiscordProvider } from './lib/oauth/discord';
import { createAuditService } from './lib/audit';
import { createPteroClient } from './lib/ptero';
import { createCryptoService } from './lib/crypto';
import { createMQTTService } from './lib/mqtt';
import { createEmbeddedBroker, type EmbeddedMQTTBroker } from './lib/mqtt-broker';

export async function createApp(options?: { config?: ServerConfig; serve?: boolean }) {
  const config = options?.config ?? (await loadConfig());
  const { client, adapter, migrate } = await createHeliaDB(config);
  await migrate();
  const redis = await createRedis(config);
  const sessions = createSessionService(config, redis, { client });
  const audit = createAuditService({ client }, adapter);
  const ptero = createPteroClient(config);
  const crypto = createCryptoService(config.security.encryptionKey);
  
  // Start embedded MQTT broker
  const mqttBroker = await createEmbeddedBroker(config);
  await mqttBroker.start();
  
  // Create MQTT service using embedded broker
  const brokerConfig = mqttBroker.getConfig();
  const mqttConfig = {
    url: `mqtt://localhost:${brokerConfig.mqttPort}`,
    username: brokerConfig.credentials.server.username,
    password: brokerConfig.credentials.server.password,
    encryptionKey: brokerConfig.security.encryptionKey,
    signingKey: brokerConfig.security.signingKey
  };
  
  const mqtt = createMQTTService({ ...config, mqtt: mqttConfig });

  // Connect to embedded MQTT broker
  if (mqtt) {
    try {
      await mqtt.connect();
      console.log('[HeliaMQTT] Connected to embedded broker successfully');
    } catch (error) {
      console.error('[HeliaMQTT] Failed to connect to embedded broker:', error);
    }
  }

  const app = new Hono<AppEnv>();
  const api = new Hono<AppEnv>();

  if (config.server.cors) {
    app.use(
      '*',
      cors({
        origin: (origin) => {
          const allowAll = config.server.cors?.origins?.includes('*');
          if (allowAll) return '*';
          if (!origin) return '';
          return config.server.cors!.origins.includes(origin) ? origin : '';
        },
        allowMethods: config.server.cors.allowMethods,
        allowHeaders: config.server.cors.allowHeaders,
        credentials: config.server.cors.allowCredentials,
      })
    );
  }

  // Bind core services on both the root app and the API sub-app
  const bindContext = async (c: any, next: () => Promise<void>) => {
    c.set('config', config);
    c.set('db', { client, adapter });
    c.set('redis', redis);
    c.set('sessions', sessions);
    const discord = createDiscordProvider(config);
    c.set('oauthProviders', { discord });
    c.set('audit', audit);
    c.set('ptero', ptero);
    c.set('crypto', crypto);
    c.set('mqtt', mqtt);
    c.set('mqttBroker', mqttBroker);
    await next();
  };
  app.use('*', bindContext);
  api.use('*', bindContext);

  const { modules } = await loadModules();
  const ctx: ModuleRegisterContext = {
    app: api as unknown,
    config,
    db: { client, adapter },
    redis,
  };
  await registerModules(modules, ctx);

  // Mount API under /api and keep simple root info route
  app.route('/api', api);
  app.get('/', (c) => c.json({ name: config.app.name, version: config.app.version }));

  // Also expose API routes at root for tests and clients expecting non-prefixed paths
  // Forward specific top-level groups to the API sub-app without shadowing '/'
  app.all('/auth/*', (c) => api.fetch(c.req.raw));
  app.all('/tenants/*', (c) => api.fetch(c.req.raw));
  app.all('/admin/*', (c) => api.fetch(c.req.raw));
  app.all('/user/*', (c) => api.fetch(c.req.raw));
  app.all('/core/*', (c) => api.fetch(c.req.raw));

  // Background jobs: egg sync and server reconcile
  async function syncEggsJob() {
    try {
      const eggs = await ptero.fetchEggs();
      // naive replace strategy
      await client`DELETE FROM eggs`;
      for (const e of eggs) {
        const envStr = typeof e.environment === 'string' ? e.environment : JSON.stringify(e.environment || {});
        await client`INSERT INTO eggs ${client([{ id: e.id, nestId: e.nestId, eggId: e.eggId, name: e.name, dockerImage: e.dockerImage, startup: e.startup, environment: envStr }])}`;
      }
      console.log('[Jobs] Eggs synced:', eggs.length);
    } catch (e) {
      console.warn('[Jobs] Egg sync failed:', (e as Error).message);
    }
  }

  async function reconcileServersJob() {
    try {
      const panelServers = await ptero.fetchAllServers();
      const ours = await client`SELECT id, tenantId, pteroServerId FROM servers`;
      const panelIds = new Set(panelServers.map((s: any) => s.id));
      for (const s of ours) {
        if (!panelIds.has(s.pteroServerId)) {
          // audit drift; do not auto-delete
          await audit.log('00000000-0000-0000-0000-000000000000', 'account:update', 'system', { metadata: { serverMissingOnPanel: s.id } });
        }
      }
      console.log('[Jobs] Reconcile complete');
    } catch (e) {
      console.warn('[Jobs] Reconcile failed:', (e as Error).message);
    }
  }

  const jobsEnabled = config.jobs?.enabled ?? true;
  const startupRun = config.jobs?.startupRun ?? true;
  const eggIntervalMin = config.jobs?.eggSyncIntervalMinutes ?? config.pterodactyl?.eggSyncIntervalMinutes ?? 10;
  const reconcileIntervalMin = config.jobs?.reconcileIntervalMinutes ?? config.pterodactyl?.reconcileIntervalMinutes ?? 30;
  let eggTimer: any = null;
  let reconcileTimer: any = null;
  if (jobsEnabled) {
    if (startupRun) {
      syncEggsJob();
      reconcileServersJob();
    }
    eggTimer = setInterval(syncEggsJob, eggIntervalMin * 60 * 1000);
    reconcileTimer = setInterval(reconcileServersJob, reconcileIntervalMin * 60 * 1000);
  }

  let server: ReturnType<typeof Bun.serve> | null = null;
  if (options?.serve) {
    const port = config.server.port;
    const host = config.server.host;
    console.log(`[Heliactyl] Starting ${config.app.name} on ${host}:${port}`);
    server = Bun.serve({ port, hostname: host, fetch: app.fetch });
  }

  return { app, config, db: { client, adapter }, redis, sessions, server };
}

if (import.meta.main) {
  createApp({ serve: true }).catch((err) => {
    console.error('Failed to start server:', err);
    process.exit(1);
  });
}


