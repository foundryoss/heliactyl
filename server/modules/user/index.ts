import { Hono } from 'hono';
import type { Context } from 'hono';
import type { HeliaModule, ModuleRegisterContext } from '../../../types/index';
import type { AppEnv } from '../../lib/hono-env';

const manifest = {
  name: 'User',
  author: 'Heliactyl',
  targetVersion: '15.0.0',
  description: 'User profile routes',
  dependencies: ['Core'],
} as const;

const router = new Hono<AppEnv>();

router.get('/me', async (c: Context<AppEnv>) => {
  const { client } = c.get('db') as AppEnv['Variables']['db'];
  const sessions = c.get('sessions') as AppEnv['Variables']['sessions'];
  const auth = c.req.header('authorization') || '';
  const token = auth.startsWith('Bearer ') ? auth.slice(7) : '';
  if (!token) return c.json({ user: null }, 401);
  const sess = await sessions.verify(token as string);
  if (!sess) return c.json({ user: null }, 401);
  const [user] = await client`SELECT id, email, username, discordId, isAdmin, createdAt, updatedAt FROM users WHERE id = ${sess.userId} LIMIT 1`;
  if (!user) return c.json({ user: null }, 404);
  // Frontend expects isAdmin optionally present
  return c.json({ user: { id: user.id, email: user.email, username: user.username, discordId: user.discordId, isAdmin: !!user.isAdmin, createdAt: user.createdAt, updatedAt: user.updatedAt } });
});

const mod: HeliaModule = {
  manifest,
  register: ({ app }: ModuleRegisterContext) => {
    (app as any).route('/user', router);
  },
};

export default mod;


