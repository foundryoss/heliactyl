import { Hono } from 'hono';
import type { Context } from 'hono';
import type { AppEnv } from '../../lib/hono-env';
import { parsePageParams, buildPageMeta } from '../../lib/pagination';

const router = new Hono<AppEnv>();

router.get('/', async (c: Context<AppEnv>) => {
  const { client } = c.get('db') as AppEnv['Variables']['db'];
  const sessions = c.get('sessions');
  const auth = c.req.header('authorization') || '';
  const token = auth.startsWith('Bearer ') ? auth.slice(7) : '';
  if (!token) return c.json({ error: 'Unauthorized' }, 401);
  const sess = await sessions.verify(token as string);
  if (!sess) return c.json({ error: 'Unauthorized' }, 401);

  const { page, pageSize } = parsePageParams(new URL(c.req.url).searchParams);
  const offset = (page - 1) * pageSize;
  const rows = await client`SELECT id, token, userId, createdAt, expiresAt, ip, userAgent FROM user_sessions WHERE userId = ${sess.userId} ORDER BY createdAt DESC LIMIT ${pageSize} OFFSET ${offset}`;
  const items = rows.map((r: any) => ({ id: r.id, createdAt: r.createdAt, expiresAt: r.expiresAt, isCurrent: r.token === token, ip: r.ip ?? null, userAgent: r.userAgent ?? null }));
  return c.json({ items, meta: buildPageMeta(items, page, pageSize) });
});

router.delete('/:id', async (c: Context<AppEnv>) => {
  const { client } = c.get('db') as AppEnv['Variables']['db'];
  const sessions = c.get('sessions');
  const audit = c.get('audit');
  const auth = c.req.header('authorization') || '';
  const token = auth.startsWith('Bearer ') ? auth.slice(7) : '';
  if (!token) return c.json({ error: 'Unauthorized' }, 401);
  const sess = await sessions.verify(token as string);
  if (!sess) return c.json({ error: 'Unauthorized' }, 401);
  const id = c.req.param('id');
  const [row] = await client`SELECT token FROM user_sessions WHERE id = ${id} AND userId = ${sess.userId} LIMIT 1`;
  if (!row) return c.json({ ok: true });
  await sessions.revoke(row.token);
  await audit.log(sess.userId, 'session:revoke', 'user', { metadata: { sessionId: id } });
  return c.json({ ok: true });
});

export default router;


