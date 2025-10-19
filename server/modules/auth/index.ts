import { Hono } from 'hono';
import { z } from 'zod';
import type { HeliaModule, ModuleRegisterContext, UserRecord } from '../../../types/index';
import { createPasswordService } from '../../lib/password';
import discordRouter from './discord';
import type { AppEnv } from '../../lib/hono-env';
import { randomUUID, randomBytes } from 'node:crypto';
import sessionsRouter from './sessions';
import accountRouter, { accountAdminRoutes } from './account';
import auditRouter from './audit';
import { createRateLimiter } from '../../lib/ratelimit';
import tenantsRouter from '../tenants';
import adminRouter from '../admin';
import serversRouter from '../servers';

const manifest = {
  name: 'Auth',
  author: 'Heliactyl',
  targetVersion: '15.0.0',
  description: 'Authentication (email/username/password) and bearer sessions',
  dependencies: ['Core'],
} as const;

const router = new Hono<AppEnv>();

const registerSchema = z.object({
  email: z.string().email(),
  username: z.string().min(3).max(32),
  password: z.string().min(8).max(128),
});

const loginSchema = z.object({
  identifier: z.string().min(3), // email or username
  password: z.string().min(8),
});

router.post('/register', async (c) => {
  const cfg = c.get('config');
  const { client } = c.get('db') as AppEnv['Variables']['db'];
  const sessions = c.get('sessions') as AppEnv['Variables']['sessions'];
  const audit = c.get('audit') as AppEnv['Variables']['audit'];
  const passwords = createPasswordService(cfg);

  const body = await c.req.json().catch(() => ({}));
  const parse = registerSchema.safeParse(body);
  if (!parse.success) return c.json({ error: 'Invalid payload' }, 400);
  const { email, username, password } = parse.data;

  // Ensure uniqueness
  const existing = await client`
    SELECT id FROM users WHERE email = ${email} OR username = ${username} LIMIT 1
  `;
  if (existing.length) return c.json({ error: 'Email or username already in use' }, 409);

  const id = randomUUID();
  const passwordHash = await passwords.hashPassword(password);
  const [{ count }] = await client`SELECT COUNT(*) as count FROM users`;
  const isAdmin = Number(count) === 0 ? 1 : 0;
  await client`
    INSERT INTO users ${client([
      { id, email, username, passwordHash, discordId: null, isAdmin },
    ])}
  `;

  const session = await sessions.issue(id);
  await audit.log(id, 'auth:register', 'user', {
    ip: c.req.header('x-forwarded-for') || c.req.header('x-real-ip') || null,
    userAgent: c.req.header('user-agent') || null,
  });
  await audit.log(id, 'auth:success', 'user', {
    ip: c.req.header('x-forwarded-for') || c.req.header('x-real-ip') || null,
    userAgent: c.req.header('user-agent') || null,
  });
  await audit.log(id, 'session:issue', 'user', { metadata: { sessionId: session.id } });
  // Auto-create default tenant (internal, no HTTP)
  try {
    const ptero = c.get('ptero') as AppEnv['Variables']['ptero'];
    const crypto = c.get('crypto') as AppEnv['Variables']['crypto'];
    const pkgId = (c.get('config') as AppEnv['Variables']['config']).packages?.defaultPackage || 'default';
    const tenantId = randomUUID();
    await client`INSERT INTO tenants ${client([{ id: tenantId, name: 'Default', ownerUserId: id, packageId: pkgId, extraMemoryMb: 0, extraDiskMb: 0, extraCpuPercent: 0, extraServerSlots: 0 }])}`;
    await client`INSERT INTO tenant_members ${client([{ tenantId: tenantId, userId: id, role: 'owner' }])}`;
    const emailPat = `${tenantId}.${email}`;
    const usernamePat = `tenant_${tenantId.slice(0, 8)}`;
    const plain = randomBytes(14).toString('base64url').slice(0, 20);
    const created = await ptero.createUser({ email: emailPat, username: usernamePat, first_name: username || 'Owner', last_name: 'Tenant', password: plain });
    const encryptedPassword = crypto.encrypt(plain);
    await client`INSERT INTO tenant_ptero_accounts ${client([{ tenantId: tenantId, pteroUserId: created.id, email: emailPat, encryptedPassword }])}`;
  } catch {
    // best-effort; tenant can be created later
  }
  return c.json({ token: session.token, userId: id }, 201);
});

router.post('/login', async (c) => {
  const cfg = c.get('config');
  const { client } = c.get('db') as AppEnv['Variables']['db'];
  const sessions = c.get('sessions');
  const audit = c.get('audit');
  const passwords = createPasswordService(cfg);

  const body = await c.req.json().catch(() => ({}));
  const parse = loginSchema.safeParse(body);
  if (!parse.success) return c.json({ error: 'Invalid payload' }, 400);
  const { identifier, password } = parse.data;

  const rows = await client`
    SELECT id, email, username, passwordHash, discordId, isAdmin, createdAt, updatedAt FROM users WHERE email = ${identifier} OR username = ${identifier} LIMIT 1
  `;
  const user = (rows?.[0] || null) as UserRecord | null;
  if (!user || !user.passwordHash) {
    await audit.log(user?.id || '00000000-0000-0000-0000-000000000000', 'auth:fail', 'user', {
      ip: c.req.header('x-forwarded-for') || c.req.header('x-real-ip') || null,
      userAgent: c.req.header('user-agent') || null,
      metadata: { reason: 'no_user_or_password' },
    });
    return c.json({ error: 'Invalid credentials' }, 401);
  }

  const ok = await passwords.verifyPassword(password, user.passwordHash);
  if (!ok) {
    await audit.log(user.id, 'auth:fail', 'user', {
      ip: c.req.header('x-forwarded-for') || c.req.header('x-real-ip') || null,
      userAgent: c.req.header('user-agent') || null,
      metadata: { reason: 'password_mismatch' },
    });
    return c.json({ error: 'Invalid credentials' }, 401);
  }

  const session = await sessions.issue(user.id);
  await audit.log(user.id, 'auth:success', 'user', {
    ip: c.req.header('x-forwarded-for') || c.req.header('x-real-ip') || null,
    userAgent: c.req.header('user-agent') || null,
  });
  await audit.log(user.id, 'session:issue', 'user', { metadata: { sessionId: session.id } });
  return c.json({ token: session.token, userId: user.id });
});

router.post('/logout', async (c) => {
  const sessions = c.get('sessions') as AppEnv['Variables']['sessions'];
  const audit = c.get('audit') as AppEnv['Variables']['audit'];
  const auth = c.req.header('authorization') || '';
  const token = auth.startsWith('Bearer ') ? auth.slice(7) : '';
  if (!token) return c.json({ ok: true });
  const sess = await sessions.verify(token);
  await sessions.revoke(token);
  if (sess) {
    await audit.log(sess.userId, 'auth:logout', 'user', {
      ip: c.req.header('x-forwarded-for') || c.req.header('x-real-ip') || null,
      userAgent: c.req.header('user-agent') || null,
    });
  }
  return c.json({ ok: true });
});

// Compatibility: expose /auth/me mirroring /user/me for existing clients/tests
router.get('/me', async (c) => {
  const { client } = c.get('db') as AppEnv['Variables']['db'];
  const sessions = c.get('sessions') as AppEnv['Variables']['sessions'];
  const auth = c.req.header('authorization') || '';
  const token = auth.startsWith('Bearer ') ? auth.slice(7) : '';
  if (!token) return c.json({ user: null }, 401);
  const sess = await sessions.verify(token as string);
  if (!sess) return c.json({ user: null }, 401);
  const [user] = await client`SELECT id, email, username, discordId, isAdmin, createdAt, updatedAt FROM users WHERE id = ${sess.userId} LIMIT 1`;
  if (!user) return c.json({ user: null }, 404);
  return c.json({ user: { id: user.id, email: user.email, username: user.username, discordId: user.discordId, isAdmin: !!user.isAdmin, createdAt: user.createdAt, updatedAt: user.updatedAt } });
});

const mod: HeliaModule = {
  manifest: { ...manifest, dependencies: ['Core'] as string[] },
  register: ({ app, config, db, redis }: ModuleRegisterContext) => {
    // Attach context bindings
    // @ts-expect-error
    (app as any).use((c, next) => {
      c.set('config', config);
      c.set('db', db);
      c.set('redis', redis);
      return next();
    });
    // Simple rate limit on auth endpoints
    //const limiter = createRateLimiter((redis as any), 'auth', { limit: 10, windowSeconds: 60 });
    /*(app as any).use('/auth/*', async (c, next) => {
      const ip = c.req.header('x-forwarded-for') || c.req.header('x-real-ip') || c.req.header('cf-connecting-ip') || c.req.header('x-client-ip') || c.req.header('x-appengine-user-ip') || c.req.header('x-forwarded') || c.req.header('x-cluster-client-ip') || c.req.header('remote-addr') || 'unknown';
      const { allowed, remaining, resetSeconds } = await limiter(ip);
      c.header('X-RateLimit-Limit', '10');
      c.header('X-RateLimit-Remaining', String(remaining));
      c.header('X-RateLimit-Reset', String(resetSeconds));
      if (!allowed) return c.json({ error: 'Too many requests' }, 429);
      await next();
    }); */
    // Sessions attached by bootstrap, available as c.get('sessions')
    (app as any).route('/auth', router);
    (app as any).route('/auth/discord', discordRouter);
    (app as any).route('/auth/sessions', sessionsRouter);
    (app as any).route('/auth/account', accountRouter);
    (app as any).route('/auth/account', accountAdminRoutes);
    (app as any).route('/auth/audit', auditRouter);
    (app as any).route('/tenants', tenantsRouter);
    (app as any).route('/tenants', serversRouter);
    (app as any).route('/admin', adminRouter);
  },
};

export default mod;


