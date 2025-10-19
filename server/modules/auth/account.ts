import { Hono } from 'hono';
import type { Context } from 'hono';
import type { AppEnv } from '../../lib/hono-env';
import { z } from 'zod';
import { createPasswordService } from '../../lib/password';

const router = new Hono<AppEnv>();

const updateSchema = z.object({
  email: z.string().email().optional(),
  username: z.string().min(3).max(32).optional(),
});

const passwordSchema = z.object({
  currentPassword: z.string().min(8),
  newPassword: z.string().min(8),
});

router.patch('/', async (c: Context<AppEnv>) => {
  const cfg = c.get('config') as AppEnv['Variables']['config'];
  const { client } = c.get('db') as AppEnv['Variables']['db'];
  const sessions = c.get('sessions') as AppEnv['Variables']['sessions'];
  const audit = c.get('audit') as AppEnv['Variables']['audit'];
  const auth = c.req.header('authorization') || '';
  const token = auth.startsWith('Bearer ') ? auth.slice(7) : '';
  if (!token) return c.json({ error: 'Unauthorized' }, 401);
  const sess = await sessions.verify(token as string);
  if (!sess) return c.json({ error: 'Unauthorized' }, 401);

  const body = await c.req.json().catch(() => ({}));
  const parse = updateSchema.safeParse(body);
  if (!parse.success) return c.json({ error: 'Invalid payload' }, 400);
  const { email, username } = parse.data;

  const updates: any = {};
  if (email) updates.email = email;
  if (username) updates.username = username;
  if (!Object.keys(updates).length) return c.json({ ok: true });

  // Check conflicts
  if (email) {
    const [e] = await client`SELECT id FROM users WHERE email = ${email} AND id <> ${sess.userId} LIMIT 1`;
    if (e) return c.json({ error: 'Email already in use' }, 409);
  }
  if (username) {
    const [u] = await client`SELECT id FROM users WHERE username = ${username} AND id <> ${sess.userId} LIMIT 1`;
    if (u) return c.json({ error: 'Username already in use' }, 409);
  }

  await client`UPDATE users SET ${client(updates)} WHERE id = ${sess.userId}`;
  await audit.log(sess.userId, 'account:update', 'user', { metadata: updates });
  return c.json({ ok: true });
});

router.post('/password', async (c: Context<AppEnv>) => {
  const cfg = c.get('config');
  const { client } = c.get('db') as AppEnv['Variables']['db'];
  const sessions = c.get('sessions');
  const audit = c.get('audit');
  const auth = c.req.header('authorization') || '';
  const token = auth.startsWith('Bearer ') ? auth.slice(7) : '';
  if (!token) return c.json({ error: 'Unauthorized' }, 401);
  const sess = await sessions.verify(token as string);
  if (!sess) return c.json({ error: 'Unauthorized' }, 401);

  const body = await c.req.json().catch(() => ({}));
  const parse = passwordSchema.safeParse(body);
  if (!parse.success) return c.json({ error: 'Invalid payload' }, 400);
  const { currentPassword, newPassword } = parse.data;

  const [u] = await client`SELECT passwordHash FROM users WHERE id = ${sess.userId} LIMIT 1`;
  if (!u?.passwordHash) return c.json({ error: 'Not allowed' }, 400);

  const passwords = createPasswordService(cfg);
  const ok = await passwords.verifyPassword(currentPassword, u.passwordHash);
  if (!ok) return c.json({ error: 'Invalid current password' }, 401);

  const newHash = await passwords.hashPassword(newPassword);
  await client`UPDATE users SET passwordHash = ${newHash} WHERE id = ${sess.userId}`;
  await audit.log(sess.userId, 'account:update', 'user', { metadata: { changed: ['password'] } });
  return c.json({ ok: true });
});

export default router;

// Delete Heliactyl account (only if no tenants)
export const accountAdminRoutes = new Hono<AppEnv>();

accountAdminRoutes.delete('/delete', async (c: Context<AppEnv>) => {
  const { client } = c.get('db') as any;
  const sessions = c.get('sessions');
  const auth = c.req.header('authorization') || '';
  const token = auth.startsWith('Bearer ') ? auth.slice(7) : '';
  if (!token) return c.json({ error: 'Unauthorized' }, 401);
  const sess = await sessions.verify(token as string);
  if (!sess) return c.json({ error: 'Unauthorized' }, 401);
  const [{ count }] = await client`SELECT COUNT(*) as count FROM tenant_members WHERE userId = ${sess.userId}`;
  if (Number(count) > 0) return c.json({ error: 'Delete tenants first' }, 400);
  await client`DELETE FROM users WHERE id = ${sess.userId}`;
  await sessions.revoke(token);
  return c.json({ ok: true });
});


