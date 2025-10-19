/**
 * Servers Router
 * Tenant-scoped server lifecycle (list/create/delete) using Pterodactyl Application API.
 */
import { Hono } from 'hono';
import type { Context } from 'hono';
import type { AppEnv } from '../../lib/hono-env';
import type { ServerModelRecord, EggRecord } from '../../../types/index';
import { getPackageResources, remainingResources, sumUsedResources } from '../../lib/resources';
import { randomUUID } from 'node:crypto';

const router = new Hono<AppEnv>();

// [manifest] Basic router metadata for consistency with other modules.
export const manifest = {
  name: 'Servers',
  author: 'Heliactyl',
  targetVersion: '15.0.0',
  description: 'Tenant-scoped server management routes',
} as const;

/**
 * Ensure the request's bearer session is a member of `tenantId`.
 * Returns the verified session or null when unauthorized.
 */
async function requireMember(c: Context<AppEnv>, tenantId: string) {
  const sessions = c.get('sessions') as AppEnv['Variables']['sessions'];
  const auth = c.req.header('authorization') || '';
  const token = auth.startsWith('Bearer ') ? auth.slice(7) : '';
  if (!token) return null;
  const sess = await sessions.verify(token as string);
  if (!sess) return null;
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  const [m] = await client`SELECT 1 FROM tenant_members WHERE tenantId = ${tenantId} AND userId = ${sess.userId} LIMIT 1`;
  if (!m) return null;
  return { sess };
}

// [GET /:tenantId/servers] Lists servers for the given tenant.
router.get('/:tenantId/servers', async (c: Context<AppEnv>) => {
  const { client } = c.get('db') as AppEnv['Variables']['db'];
  const tenantId = c.req.param('tenantId');
  const r = await requireMember(c, tenantId);
  if (!r) return c.json({ error: 'Forbidden' }, 403);
  const rows = await client`SELECT id, tenantId, pteroServerId, pteroIdentifier, name, eggId, dockerImage, memoryMb, diskMb, cpuPercent, locationId, createdAt FROM servers WHERE tenantId = ${tenantId} ORDER BY createdAt DESC`;
  return c.json({ items: rows as ServerModelRecord[] });
});

// [POST /:tenantId/servers] Creates a server in Pterodactyl and records it locally after resource checks.
router.post('/:tenantId/servers', async (c: Context<AppEnv>) => {
  const cfg = c.get('config') as AppEnv['Variables']['config'];
  const ptero = c.get('ptero') as AppEnv['Variables']['ptero'];
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  const audit = c.get('audit') as AppEnv['Variables']['audit'];
  const tenantId = c.req.param('tenantId');
  const r = await requireMember(c, tenantId);
  if (!r) return c.json({ error: 'Forbidden' }, 403);

  const body = await c.req.json().catch(() => ({} as Record<string, unknown>));
  const name = String((body as Record<string, unknown>)['name'] || '').trim();
  const eggId = Number((body as Record<string, unknown>)['eggId']);
  const memoryMb = Number((body as Record<string, unknown>)['memoryMb']);
  const diskMb = Number((body as Record<string, unknown>)['diskMb']);
  const cpuPercent = Number((body as Record<string, unknown>)['cpuPercent']);
  const location = String((body as Record<string, unknown>)['location'] || '');
  if (!name || !eggId || !memoryMb || !diskMb || !cpuPercent || !location) return c.json({ error: 'Invalid payload' }, 400);

  const [tenant] = await client`SELECT * FROM tenants WHERE id = ${tenantId} LIMIT 1`;
  const pkg = getPackageResources(cfg, tenant.packageId);
  const current = await client`SELECT memoryMb, diskMb, cpuPercent FROM servers WHERE tenantId = ${tenantId}`;
  const used = sumUsedResources(current);
  const rem = remainingResources(pkg, { memoryMb: tenant.extraMemoryMb, diskMb: tenant.extraDiskMb, cpuPercent: tenant.extraCpuPercent, serverSlots: tenant.extraServerSlots }, { memoryMb: used.memoryMb, diskMb: used.diskMb, cpuPercent: used.cpuPercent, servers: current.length });
  if (memoryMb > rem.memoryMb || diskMb > rem.diskMb || cpuPercent > rem.cpuPercent || rem.serverSlots < 1) {
    return c.json({ error: 'Insufficient resources' }, 400);
  }

  const loc = cfg.locations?.find((l: any) => l.slug === location || l.id === Number(location));
  if (!loc) return c.json({ error: 'Invalid location' }, 400);
  if (loc.lockedPackage && loc.lockedPackage !== tenant.packageId) return c.json({ error: 'Location locked to another package' }, 403);

  const [acct] = await client`SELECT pteroUserId FROM tenant_ptero_accounts WHERE tenantId = ${tenantId} LIMIT 1`;
  const egg = (await client`SELECT id, nestId, eggId, name, dockerImage, startup, environment FROM eggs WHERE eggId = ${eggId} LIMIT 1`)[0] as any as EggRecord | undefined;
  if (!egg) return c.json({ error: 'Unknown egg' }, 400);

  // Parse environment if it's a string
  let environment = {};
  if (typeof egg.environment === 'string') {
    try {
      environment = JSON.parse(egg.environment);
    } catch {
      environment = {};
    }
  } else {
    environment = egg.environment || {};
  }

  // Add default environment variables for common eggs if missing
  if (Object.keys(environment).length === 0) {
    // Common Minecraft defaults - these can be overridden by users later
    if (egg.name.toLowerCase().includes('minecraft') || egg.name.toLowerCase().includes('paper') || egg.name.toLowerCase().includes('spigot')) {
      environment = {
        SERVER_JARFILE: 'server.jar',
        MC_VERSION: 'latest',
        BUILD_TYPE: 'recommended',
        BUILD_NUMBER: 'latest'
      };
    }
  }

  const serverSpec = {
    name,
    user: acct.pteroUserId,
    egg: egg.eggId,
    docker_image: egg.dockerImage,
    startup: egg.startup,
    environment,
    limits: { memory: memoryMb, swap: -1, disk: diskMb, io: 500, cpu: cpuPercent },
    feature_limits: { databases: 4, backups: 4, allocations: 10 },
    deploy: { locations: [Number(loc.id)], dedicated_ip: false, port_range: [] },
  };

  console.log('[Server Creation] Spec:', JSON.stringify(serverSpec, null, 2));

  // Test API access first
  try {
    console.log('[Server Creation] Testing API access...');
    const testServers = await ptero.fetchAllServers();
    console.log('[Server Creation] API test successful, found', testServers.length, 'servers');
  } catch (apiTest) {
    console.error('[Server Creation] API test failed:', apiTest);
    return c.json({ error: 'API access test failed' }, 500);
  }

  try {
    const created = await ptero.createServer(serverSpec);
    const id = randomUUID();
    await client`INSERT INTO servers ${client([{ id, tenantId, pteroServerId: created.id, pteroIdentifier: created.identifier, name, eggId: egg.eggId, dockerImage: egg.dockerImage, memoryMb, diskMb, cpuPercent, locationId: Number(loc.id) }])}`;
    
    // Publish live update
    const mqtt = c.get('mqtt') as AppEnv['Variables']['mqtt'];
    if (mqtt) {
      await mqtt.publishLiveUpdate({
        type: 'server_created',
        tenantId,
        data: { id, pteroServerId: created.id, name, memoryMb, diskMb, cpuPercent },
        userId: r.sess.userId
      });
    }
    
    await audit.log(r.sess.userId, 'account:update', 'user', { tenantId, metadata: { serverCreated: id } });
    return c.json({ id, pteroServerId: created.id, name });
  } catch (e) {
    console.error('[Server Creation Error]:', e);
    let errorMessage = 'Unknown error';
    
    if (e instanceof Error) {
      // Try to parse Pterodactyl API error format
      const match = e.message.match(/Ptero POST \/servers failed: \d+ - (.+)/);
      if (match) {
        try {
          const errorData = JSON.parse(match[1]);
          if (errorData.errors && errorData.errors[0] && errorData.errors[0].detail) {
            errorMessage = errorData.errors[0].detail;
          } else {
            errorMessage = e.message;
          }
        } catch {
          errorMessage = e.message;
        }
      } else {
        errorMessage = e.message;
      }
    }
    
    return c.json({ error: errorMessage }, 502);
  }
});

// [GET /:tenantId/servers/:id/websocket] Get websocket credentials for server console
router.get('/:tenantId/servers/:id/websocket', async (c: Context<AppEnv>) => {
  const { client } = c.get('db') as AppEnv['Variables']['db'];
  const ptero = c.get('ptero') as AppEnv['Variables']['ptero'];
  const tenantId = c.req.param('tenantId');
  const r = await requireMember(c, tenantId);
  if (!r) return c.json({ error: 'Forbidden' }, 403);
  
  const id = c.req.param('id');
  const [srv] = await client`SELECT pteroServerId, pteroIdentifier FROM servers WHERE id = ${id} AND tenantId = ${tenantId} LIMIT 1`;
  if (!srv) return c.json({ error: 'Server not found' }, 404);
  
  try {
    const credentials = await ptero.getWebSocketCredentials(srv.pteroIdentifier);
    return c.json(credentials);
  } catch (e: any) {
    console.error('[WebSocket Credentials Error]:', e);
    return c.json({ error: 'Failed to get websocket credentials' }, 502);
  }
});

// [DELETE /:tenantId/servers/:id] Deletes a server in Pterodactyl and removes it locally.
router.delete('/:tenantId/servers/:id', async (c) => {
  const { client } = c.get('db') as any;
  const audit = c.get('audit');
  const ptero = c.get('ptero');
  const tenantId = c.req.param('tenantId');
  const r = await requireMember(c, tenantId);
  if (!r) return c.json({ error: 'Forbidden' }, 403);
  const id = c.req.param('id');
  const [srv] = await client`SELECT pteroServerId, pteroIdentifier FROM servers WHERE id = ${id} AND tenantId = ${tenantId} LIMIT 1`;
  if (!srv) return c.json({ ok: true });
  try { await ptero.deleteServer(srv.pteroServerId); } catch {}
  await client`DELETE FROM servers WHERE id = ${id}`;
  
  // Publish live update
  const mqtt = c.get('mqtt') as AppEnv['Variables']['mqtt'];
  if (mqtt) {
    await mqtt.publishLiveUpdate({
      type: 'server_deleted',
      tenantId,
      data: { id, pteroServerId: srv.pteroServerId },
      userId: r.sess.userId
    });
  }
  
  await audit.log(r.sess.userId, 'account:update', 'user', { tenantId, metadata: { serverDeleted: id } });
  return c.json({ ok: true });
});

export default router;


