/**
 * Tenants Router
 * Tenant lifecycle, membership, resources, and Pterodactyl account management.
 */
import { Hono } from 'hono';
import type { Context } from 'hono';
import type { AppEnv } from '../../lib/hono-env';
import type { TenantRecord } from '../../../types/index';
import { randomUUID, randomBytes } from 'node:crypto';
import { getPackageResources, remainingResources, sumUsedResources } from '../../lib/resources';
import { parsePageParams, buildPageMeta } from '../../lib/pagination';

const router = new Hono<AppEnv>();

// [manifest] Metadata for the Tenants module.
export const manifest = {
  name: 'Tenants',
  author: 'Heliactyl',
  targetVersion: '15.0.0',
  description: 'Tenant management routes',
} as const;

/**
 * Verify bearer session and return the session record or null.
 */
/**
 * Verify bearer session and return the session record or null.
 */
async function requireAuth(c: Context<AppEnv>) {
  const sessions = c.get('sessions');
  const auth = c.req.header('authorization') || '';
  const token = auth.startsWith('Bearer ') ? auth.slice(7) : '';
  if (!token) return null;
  const sess = await sessions.verify(token as string);
  return sess;
}

// [GET /tenants] Lists tenants for the current user.
router.get('/', async (c: Context<AppEnv>) => {
  const sess = await requireAuth(c);
  if (!sess) return c.json({ error: 'Unauthorized' }, 401);
  const { client } = c.get('db') as AppEnv['Variables']['db'];
  const rows = await client`SELECT t.* , tm.role FROM tenants t JOIN tenant_members tm ON tm.tenantId = t.id WHERE tm.userId = ${sess.userId} ORDER BY t.createdAt DESC`;
  return c.json({ items: rows });
});

// [POST /tenants] Creates a tenant and corresponding Pterodactyl user.
router.post('/', async (c) => {
  const cfg = c.get('config') as AppEnv['Variables']['config'];
  const ptero = c.get('ptero') as AppEnv['Variables']['ptero'];
  const crypto = c.get('crypto') as AppEnv['Variables']['crypto'];
  const audit = c.get('audit') as AppEnv['Variables']['audit'];
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  const sess = await requireAuth(c);
  if (!sess) return c.json({ error: 'Unauthorized' }, 401);
  const body = await c.req.json().catch(() => ({}));
  const name = String(body?.name || 'New Tenant').slice(0, 64);
  const packageId = String(body?.packageId || cfg.packages?.defaultPackage);

  const maxTenants = cfg.packages?.maxTenantsDefault ?? 2;
  const [{ count }] = await client`SELECT COUNT(*) as count FROM tenant_members WHERE userId = ${sess.userId}` as any;
  if (Number(count) >= maxTenants) return c.json({ error: 'Tenant limit reached' }, 403);

  const id = randomUUID();
  await client`INSERT INTO tenants ${client([{ id, name, ownerUserId: sess.userId, packageId, extraMemoryMb: 0, extraDiskMb: 0, extraCpuPercent: 0, extraServerSlots: 0 }])}`;
  await client`INSERT INTO tenant_members ${client([{ tenantId: id, userId: sess.userId, role: 'owner' }])}`;

  // Publish live update
  const mqtt = c.get('mqtt') as AppEnv['Variables']['mqtt'];
  if (mqtt) {
    await mqtt.publishLiveUpdate({
      type: 'tenant_created',
      tenantId: id,
      data: { id, name, packageId, ownerUserId: sess.userId },
      userId: sess.userId
    });
  }

  // Create Pterodactyl user for tenant
  const owner = (await client`SELECT email, username FROM users WHERE id = ${sess.userId} LIMIT 1`)[0];
  const email = `${id}.${owner.email}`;
  const username = `tenant_${id.slice(0, 8)}`;
  const first_name = owner.username || 'Owner';
  const last_name = 'Tenant';
  const plain = randomBytes(14).toString('base64url').slice(0, 20);
  try {
    const created = await ptero.createUser({ email, username, first_name, last_name, password: plain });
    const encryptedPassword = crypto.encrypt(plain);
    await client`INSERT INTO tenant_ptero_accounts ${client([{ tenantId: id, pteroUserId: created.id, email, encryptedPassword }])}`;
  } catch (e) {
    return c.json({ error: 'Pterodactyl unavailable. Try again later.' }, 502);
  }

  await audit.log(sess.userId, 'account:update', 'user', { tenantId: id, metadata: { tenantCreated: id } });
  return c.json({ id, name, packageId });
});

// [POST /:tenantId/members] Adds a member (userId) to the tenant as role 'user'. Owner only.
router.post('/:tenantId/members', async (c) => {
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  const audit = c.get('audit') as AppEnv['Variables']['audit'];
  const cfg = c.get('config') as AppEnv['Variables']['config'];
  const sess = await requireAuth(c);
  if (!sess) return c.json({ error: 'Unauthorized' }, 401);
  const tenantId = c.req.param('tenantId');
  const body = await c.req.json().catch(() => ({}));
  const userId = String(body?.userId || '');
  const [m] = await client`SELECT role FROM tenant_members WHERE tenantId = ${tenantId} AND userId = ${sess.userId} LIMIT 1`;
  if (!m || m.role !== 'owner') return c.json({ error: 'Forbidden' }, 403);
  // Add member as user
  await client`INSERT OR IGNORE INTO tenant_members ${client([{ tenantId, userId, role: 'user' }])}`;
  
  // Publish live update
  const mqtt = c.get('mqtt') as AppEnv['Variables']['mqtt'];
  if (mqtt) {
    await mqtt.publishLiveUpdate({
      type: 'member_added',
      tenantId,
      data: { userId, role: 'user' },
      userId: sess.userId
    });
  }
  
  await audit.log(sess.userId, 'account:update', 'user', { tenantId, metadata: { tenantMemberAdded: userId } });
  return c.json({ ok: true });
});

// [GET /:tenantId/members] Lists members and roles for a tenant.
router.get('/:tenantId/members', async (c) => {
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  const sess = await requireAuth(c);
  if (!sess) return c.json({ error: 'Unauthorized' }, 401);
  const tenantId = c.req.param('tenantId');
  const [m] = await client`SELECT role FROM tenant_members WHERE tenantId = ${tenantId} AND userId = ${sess.userId} LIMIT 1`;
  if (!m) return c.json({ error: 'Forbidden' }, 403);
  const rows = await client`SELECT tm.userId, tm.role, u.email, u.username FROM tenant_members tm JOIN users u ON u.id = tm.userId WHERE tm.tenantId = ${tenantId}`;
  return c.json({ items: rows });
});

router.get('/:tenantId/audit', async (c) => {
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  const audit = c.get('audit') as AppEnv['Variables']['audit'];
  const sessions = c.get('sessions') as AppEnv['Variables']['sessions'];
  const tenantId = c.req.param('tenantId');
  const auth = c.req.header('authorization') || '';
  const token = auth.startsWith('Bearer ') ? auth.slice(7) : '';
  if (!token) return c.json({ error: 'Unauthorized' }, 401);
  const sess = await sessions.verify(token as string);
  if (!sess) return c.json({ error: 'Unauthorized' }, 401);
  const [member] = await client`SELECT role FROM tenant_members WHERE tenantId = ${tenantId} AND userId = ${sess.userId} LIMIT 1`;
  if (!member) return c.json({ error: 'Forbidden' }, 403);

  const { page, pageSize } = parsePageParams(new URL(c.req.url).searchParams);
  const result = await audit.listForTenant(tenantId, page, pageSize);
  const items = result.items.map((item) => ({
    id: item.id,
    action: item.action,
    actorType: item.actorType,
    ip: item.ip,
    userAgent: item.userAgent,
    metadata: item.metadata,
    createdAt: item.createdAt,
    tenantId: item.tenantId,
    actorEmail: item.actorEmail,
    actorUsername: item.actorUsername,
  }));
  return c.json({ items, meta: buildPageMeta(items, page, pageSize, result.total) });
});

// [POST /:tenantId/members/by-email] Adds a member to the tenant by email. Owner only.
router.post('/:tenantId/members/by-email', async (c) => {
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  const audit = c.get('audit') as AppEnv['Variables']['audit'];
  const sess = await requireAuth(c);
  if (!sess) return c.json({ error: 'Unauthorized' }, 401);
  const tenantId = c.req.param('tenantId');
  const [m] = await client`SELECT role FROM tenant_members WHERE tenantId = ${tenantId} AND userId = ${sess.userId} LIMIT 1`;
  if (!m || m.role !== 'owner') return c.json({ error: 'Forbidden' }, 403);
  const body = await c.req.json().catch(() => ({}));
  const email = String(body?.email || '').toLowerCase();
  const [u] = await client`SELECT id FROM users WHERE email = ${email} LIMIT 1`;
  if (!u) return c.json({ error: 'User not found' }, 404);
  await client`INSERT OR IGNORE INTO tenant_members ${client([{ tenantId, userId: u.id, role: 'user' }])}`;
  await audit.log(sess.userId, 'account:update', 'user', { tenantId, metadata: { tenantMemberAdded: u.id } });
  return c.json({ ok: true });
});

// [POST /:tenantId/password/rotate] Rotates the Pterodactyl account password for the tenant. Owner only.
router.post('/:tenantId/password/rotate', async (c) => {
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  const ptero = c.get('ptero') as AppEnv['Variables']['ptero'];
  const crypto = c.get('crypto') as AppEnv['Variables']['crypto'];
  const audit = c.get('audit') as AppEnv['Variables']['audit'];
  const sess = await requireAuth(c);
  if (!sess) return c.json({ error: 'Unauthorized' }, 401);
  const tenantId = c.req.param('tenantId');
  const [m] = await client`SELECT role FROM tenant_members WHERE tenantId = ${tenantId} AND userId = ${sess.userId} LIMIT 1`;
  if (!m || m.role !== 'owner') return c.json({ error: 'Forbidden' }, 403);
  const [acct] = await client`SELECT pteroUserId FROM tenant_ptero_accounts WHERE tenantId = ${tenantId} LIMIT 1`;
  if (!acct) return c.json({ error: 'No tenant account' }, 400);
  const plain = randomBytes(16).toString('base64url').slice(0, 24);
  try {
    await ptero.updateUser(acct.pteroUserId, { password: plain });
    const encryptedPassword = crypto.encrypt(plain);
    await client`UPDATE tenant_ptero_accounts SET encryptedPassword = ${encryptedPassword} WHERE tenantId = ${tenantId}`;
    await audit.log(sess.userId, 'account:update', 'user', { tenantId, metadata: { tenantPasswordRotated: tenantId } });
    return c.json({ password: plain });
  } catch (e) {
    return c.json({ error: 'Pterodactyl unavailable' }, 502);
  }
});

// [GET /:tenantId/resources] Returns package, extra, used, and remaining resources for a tenant.
router.get('/:tenantId/resources', async (c) => {
  const cfg = c.get('config') as AppEnv['Variables']['config'];
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  const sess = await requireAuth(c);
  if (!sess) return c.json({ error: 'Unauthorized' }, 401);
  const tenantId = c.req.param('tenantId');
  const [m] = await client`SELECT 1 FROM tenant_members WHERE tenantId = ${tenantId} AND userId = ${sess.userId} LIMIT 1`;
  if (!m) return c.json({ error: 'Forbidden' }, 403);
  const [t] = await client`SELECT id, name, ownerUserId, packageId, extraMemoryMb, extraDiskMb, extraCpuPercent, extraServerSlots, createdAt, updatedAt FROM tenants WHERE id = ${tenantId} LIMIT 1` as any as TenantRecord[];
  const pkg = getPackageResources(cfg, t.packageId);
  const current = await client`SELECT memoryMb, diskMb, cpuPercent FROM servers WHERE tenantId = ${tenantId}`;
  const used = sumUsedResources(current);
  const rem = remainingResources(pkg, { memoryMb: t.extraMemoryMb, diskMb: t.extraDiskMb, cpuPercent: t.extraCpuPercent, serverSlots: t.extraServerSlots }, { memoryMb: used.memoryMb, diskMb: used.diskMb, cpuPercent: used.cpuPercent, servers: current.length });
  return c.json({ package: pkg, extra: { memoryMb: t.extraMemoryMb, diskMb: t.extraDiskMb, cpuPercent: t.extraCpuPercent, serverSlots: t.extraServerSlots }, used: { ...used, servers: current.length }, remaining: rem });
});

router.delete('/:tenantId/members/:userId', async (c) => {
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  const audit = c.get('audit') as AppEnv['Variables']['audit'];
  const sess = await requireAuth(c);
  if (!sess) return c.json({ error: 'Unauthorized' }, 401);
  const tenantId = c.req.param('tenantId');
  const userId = c.req.param('userId');
  const [m] = await client`SELECT role FROM tenant_members WHERE tenantId = ${tenantId} AND userId = ${sess.userId} LIMIT 1`;
  if (!m || m.role !== 'owner') return c.json({ error: 'Forbidden' }, 403);
  await client`DELETE FROM tenant_members WHERE tenantId = ${tenantId} AND userId = ${userId}`;
  
  // Publish live update
  const mqtt = c.get('mqtt') as AppEnv['Variables']['mqtt'];
  if (mqtt) {
    await mqtt.publishLiveUpdate({
      type: 'member_removed',
      tenantId,
      data: { userId },
      userId: sess.userId
    });
  }
  
  await audit.log(sess.userId, 'account:update', 'user', { tenantId, metadata: { tenantMemberRemoved: userId } });
  return c.json({ ok: true });
});

// [POST /:tenantId/transfer] Transfers ownership to a different user in the tenant.
router.post('/:tenantId/transfer', async (c) => {
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  const audit = c.get('audit') as AppEnv['Variables']['audit'];
  const ptero = c.get('ptero') as AppEnv['Variables']['ptero'];
  const sess = await requireAuth(c);
  if (!sess) return c.json({ error: 'Unauthorized' }, 401);
  const tenantId = c.req.param('tenantId');
  const body = await c.req.json().catch(() => ({}));
  const newOwnerUserId = String(body?.userId || '');
  const [m] = await client`SELECT role FROM tenant_members WHERE tenantId = ${tenantId} AND userId = ${sess.userId} LIMIT 1`;
  if (!m || m.role !== 'owner') return c.json({ error: 'Forbidden' }, 403);
  const [acct] = await client`SELECT pteroUserId FROM tenant_ptero_accounts WHERE tenantId = ${tenantId} LIMIT 1`;
  const [newOwner] = await client`SELECT email FROM users WHERE id = ${newOwnerUserId} LIMIT 1`;
  await client`UPDATE tenants SET ownerUserId = ${newOwnerUserId} WHERE id = ${tenantId}`;
  await client`UPDATE tenant_members SET role = 'user' WHERE tenantId = ${tenantId} AND userId = ${sess.userId}`;
  await client`UPDATE tenant_members SET role = 'owner' WHERE tenantId = ${tenantId} AND userId = ${newOwnerUserId}`;
  // Update ptero email to include new owner email
  const email = `${tenantId}.${newOwner.email}`;
  try { await ptero.updateUser(acct.pteroUserId, { email }); } catch {}
  await client`UPDATE tenant_ptero_accounts SET email = ${email} WHERE tenantId = ${tenantId}`;
  await audit.log(sess.userId, 'account:update', 'user', { tenantId, metadata: { tenantOwnerTransferredTo: newOwnerUserId } });
  return c.json({ ok: true });
});

// [DELETE /:tenantId] Deletes tenant servers, Pterodactyl account, then tenant. Owner only.
router.delete('/:tenantId', async (c) => {
  const { client } = (c.get('db') as AppEnv['Variables']['db']);
  const audit = c.get('audit') as AppEnv['Variables']['audit'];
  const ptero = c.get('ptero') as AppEnv['Variables']['ptero'];
  const sess = await requireAuth(c);
  if (!sess) return c.json({ error: 'Unauthorized' }, 401);
  const tenantId = c.req.param('tenantId');
  const [m] = await client`SELECT role FROM tenant_members WHERE tenantId = ${tenantId} AND userId = ${sess.userId} LIMIT 1`;
  if (!m || m.role !== 'owner') return c.json({ error: 'Forbidden' }, 403);
  const servers = await client`SELECT pteroServerId FROM servers WHERE tenantId = ${tenantId}`;
  for (const s of servers) { try { await ptero.deleteServer(s.pteroServerId); } catch {} }
  const [acct] = await client`SELECT pteroUserId FROM tenant_ptero_accounts WHERE tenantId = ${tenantId} LIMIT 1`;
  try { await ptero.deleteUser(acct.pteroUserId); } catch {}
  await client`DELETE FROM tenants WHERE id = ${tenantId}`;
  
  // Publish live update
  const mqtt = c.get('mqtt') as AppEnv['Variables']['mqtt'];
  if (mqtt) {
    await mqtt.publishLiveUpdate({
      type: 'tenant_deleted',
      tenantId,
      data: { tenantId },
      userId: sess.userId
    });
  }
  
  await audit.log(sess.userId, 'account:update', 'user', { tenantId, metadata: { tenantDeleted: tenantId } });
  return c.json({ ok: true });
});

export default router;


