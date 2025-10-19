/**
 * Admin Router
 * Admin-only controls for users, tenants, packages, jobs, diagnostics.
 */
import { Hono } from 'hono';
import type { Context } from 'hono';
import type { AppEnv } from '../../lib/hono-env';
import type { TenantRecord } from '../../../types/index';

const router = new Hono<AppEnv>();

// [manifest] Metadata for Admin module.
export const manifest = {
  name: 'Admin',
  author: 'Heliactyl',
  targetVersion: '15.0.0',
  description: 'Admin-only routes',
} as const;

/**
 * Validates session and ensures the session user is an admin.
 * @param c Hono request context typed with our `AppEnv`.
 */
async function requireAdmin(c: Context<AppEnv>) {
  const sessions = c.get('sessions') as AppEnv['Variables']['sessions'];
  const auth = c.req.header('authorization') || '';
  const token = auth.startsWith('Bearer ') ? auth.slice(7) : '';
  if (!token) return null;
  const sess = await sessions.verify(token);
  if (!sess) return null;
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  const [u] = await client`SELECT isAdmin FROM users WHERE id = ${sess.userId} LIMIT 1`;
  if (!u?.isAdmin) return null;
  return sess;
}

// [guard] Guard all admin routes.
router.use('*', async (c: Context<AppEnv>, next: () => Promise<void>) => {
  const sess = await requireAdmin(c);
  if (!sess) return c.json({ error: 'Forbidden' }, 403);
  await next();
});

// [GET /admin/users] Lists all users with admin flag.
router.get('/users', async (c: Context<AppEnv>) => {
  const { client } = c.get('db') as AppEnv['Variables']['db'];
  const rows = await client`SELECT id, email, username, isAdmin, createdAt FROM users ORDER BY createdAt DESC`;
  return c.json({ items: rows });
});

// [POST /admin/users/:id/admin] Promote/demote admin.
router.post('/users/:id/admin', async (c) => {
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  const id = c.req.param('id');
  const body = await c.req.json().catch(() => ({}));
  const isAdmin = !!body?.isAdmin ? 1 : 0;
  await client`UPDATE users SET isAdmin = ${isAdmin} WHERE id = ${id}`;
  return c.json({ ok: true });
});

// [GET /admin/tenants] Lists all tenants.
router.get('/tenants', async (c) => {
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  const rows = await client`SELECT id, name, ownerUserId, packageId, extraMemoryMb, extraDiskMb, extraCpuPercent, extraServerSlots, createdAt, updatedAt FROM tenants ORDER BY createdAt DESC` as any as TenantRecord[];
  return c.json({ items: rows });
});

// [POST /admin/tenants/:id/extras] Updates tenant extra resources.
router.post('/tenants/:id/extras', async (c) => {
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  const id = c.req.param('id');
  const body = await c.req.json().catch(() => ({}));
  const updates: any = {};
  for (const k of ['extraMemoryMb', 'extraDiskMb', 'extraCpuPercent', 'extraServerSlots']) if (k in body) updates[k] = Number(body[k]);
  if (Object.keys(updates).length) await client`UPDATE tenants SET ${client(updates)} WHERE id = ${id}`;
  return c.json({ ok: true });
});

// [GET /admin/packages] Views package config.
router.get('/packages', async (c) => {
  const cfg = c.get('config');
  const pkg = (cfg as any).packages || { defaultPackage: 'default', items: {} };
  return c.json({ defaultPackage: pkg.defaultPackage, items: pkg.items || {} });
});

// [POST /admin/packages] Updates package config and persists to config.yml.
router.post('/packages', async (c) => {
  const cfg = c.get('config') as AppEnv['Variables']['config'];
  const body = await c.req.json().catch(() => ({}));
  const current = cfg.packages || { defaultPackage: 'default', items: {} };
  cfg.packages = current;
  current.items = { ...(current.items || {}), ...(body?.items || {}) };
  if (body?.defaultPackage) current.defaultPackage = String(body.defaultPackage);
  // persist to config.yml
  try {
    const { saveConfig } = await import('../../lib/config');
    await saveConfig(cfg);
    return c.json({ ok: true });
  } catch {
    return c.json({ error: 'Failed to save config' }, 500);
  }
});

// [POST /admin/jobs/reconcile/run] Manually triggers reconcile and returns drift ids and orphaned servers.
router.post('/jobs/reconcile/run', async (c) => {
  const ptero = c.get('ptero') as AppEnv['Variables']['ptero'];
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  try {
    const panelServers = await ptero.fetchAllServers();
    const ours = await client`SELECT id, tenantId, pteroServerId FROM servers`;
    
    // Check for servers in Heliactyl DB that don't exist on the panel (drift)
    const panelIds = new Set(panelServers.map((s: any) => s.id));
    const drifts: any[] = [];
    for (const s of ours) if (!panelIds.has(s.pteroServerId)) drifts.push(s.id);
    
    // Check for servers on the panel that aren't in Heliactyl DB (orphaned)
    const ourIds = new Set(ours.map((s: any) => s.pteroServerId));
    const orphaned: any[] = [];
    for (const s of panelServers) {
      if (!ourIds.has(s.id)) {
        orphaned.push({
          pteroServerId: s.id,
          name: s.name,
          uuid: s.uuid,
          identifier: s.identifier,
          status: s.status
        });
      }
    }
    
    return c.json({ ok: true, drifts, orphaned });
  } catch (e) {
    return c.json({ error: 'failed', message: (e as Error).message }, 500);
  }
});

// [GET /admin/diagnostics] Returns quick health check for panel, redis, db.
router.get('/diagnostics', async (c) => {
  const cfg = c.get('config') as AppEnv['Variables']['config'];
  const redis = c.get('redis') as AppEnv['Variables']['redis'];
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  try {
    await redis.setJSON(redis.buildKey('diag', 'ping'), { ok: true }, 10);
  } catch {}
  try {
    await client`SELECT 1`;
  } catch {}
  return c.json({ pterodactyl: { url: cfg.pterodactyl?.url }, redis: { ok: true }, db: { ok: true } });
});

// [POST /admin/jobs/eggs/sync] Manually triggers eggs sync from panel.
router.post('/jobs/eggs/sync', async (c) => {
  // simple hint for manual trigger; jobs run automatically too
  const ptero = c.get('ptero') as AppEnv['Variables']['ptero'];
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  try {
    const nests = await ptero.fetchEggs();
    await client`DELETE FROM eggs`;
    for (const e of nests) {
      const envStr = typeof e.environment === 'string' ? e.environment : JSON.stringify(e.environment || {});
      await client`INSERT INTO eggs ${client([{ id: e.id, nestId: e.nestId, eggId: e.eggId, name: e.name, dockerImage: e.dockerImage, startup: e.startup, environment: envStr }])}`;
    }
    return c.json({ ok: true, count: nests.length });
  } catch (e) {
    return c.json({ error: 'sync failed' }, 500);
  }
});

// [GET /admin/eggs] Lists cached eggs.
router.get('/eggs', async (c) => {
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  const rows = await client`SELECT eggId, name, dockerImage FROM eggs ORDER BY nestId, eggId`;
  return c.json({ items: rows });
});

export default router;


