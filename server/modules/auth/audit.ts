import { Hono } from 'hono';
import type { Context } from 'hono';
import type { AppEnv } from '../../lib/hono-env';
import { parsePageParams, buildPageMeta } from '../../lib/pagination';
import type { AuditService } from '../../lib/audit';

const router = new Hono<AppEnv>();

router.get('/', async (c: Context<AppEnv>) => {
  const sessions = c.get('sessions') as AppEnv['Variables']['sessions'];
  const audit = c.get('audit') as AuditService;
  const auth = c.req.header('authorization') || '';
  const token = auth.startsWith('Bearer ') ? auth.slice(7) : '';
  if (!token) return c.json({ error: 'Unauthorized' }, 401);
  const sess = await sessions.verify(token as string);
  if (!sess) return c.json({ error: 'Unauthorized' }, 401);

  const { page, pageSize } = parsePageParams(new URL(c.req.url).searchParams);
  const result = await audit.listForUser(sess.userId, page, pageSize);
  const items = result.items.map((row: any) => ({
    id: row.id,
    action: row.action,
    actorType: row.actorType,
    ip: row.ip,
    userAgent: row.userAgent,
    metadata: row.metadata,
    createdAt: row.createdAt,
    tenantId: row.tenantId,
    actorEmail: row.actorEmail,
    actorUsername: row.actorUsername,
  }));
  return c.json({ items, meta: buildPageMeta(items, page, pageSize, result.total) });
});

export default router;


