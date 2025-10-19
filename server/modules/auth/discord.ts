import { Hono } from 'hono';
import type { Context } from 'hono';
import type { AppEnv } from '../../lib/hono-env';
import { randomUUID } from 'node:crypto';

const router = new Hono<AppEnv>();

router.get('/login', async (c: Context<AppEnv>) => {
  const { client } = c.get('db') as AppEnv['Variables']['db'];
  const cfg = c.get('config') as AppEnv['Variables']['config'];
  const redis = c.get('redis') as AppEnv['Variables']['redis'];
  const providers = c.get('oauthProviders') as AppEnv['Variables']['oauthProviders'];
  const discord = providers.discord;
  if (!discord) return c.json({ error: 'Discord OAuth disabled' }, 400);

  const state = randomUUID();
  const key = redis.buildKey('oauth', 'state', state);
  await redis.setJSON(key, { provider: 'discord', createdAt: Date.now() / 1000 }, 600);

  const url = discord.getAuthorizationUrl(state);
  return c.json({ url });
});

router.get('/callback', async (c: Context<AppEnv>) => {
  const { client } = c.get('db') as AppEnv['Variables']['db'];
  const cfg = c.get('config') as AppEnv['Variables']['config'];
  const redis = c.get('redis') as AppEnv['Variables']['redis'];
  const sessions = c.get('sessions') as AppEnv['Variables']['sessions'];
  const providers = c.get('oauthProviders') as AppEnv['Variables']['oauthProviders'];
  const discord = providers.discord;
  if (!discord) return c.json({ error: 'Discord OAuth disabled' }, 400);

  const url = new URL(c.req.url);
  const code = url.searchParams.get('code');
  const state = url.searchParams.get('state');
  if (!code || !state) return c.json({ error: 'Invalid callback' }, 400);

  const key = redis.buildKey('oauth', 'state', state);
  const exists = await redis.getJSON(key);
  if (!exists) return c.json({ error: 'Invalid state' }, 400);
  await redis.delete(key);

  try {
    const { accessToken } = await discord.exchangeCode(code as string);
    const profile = await discord.fetchProfile(accessToken);
    // Upsert user by discordId
    const users = await client`SELECT id, email, username FROM users WHERE discordId = ${profile.id} LIMIT 1`;
    let user = users[0] as { id: string; email?: string | null; username?: string | null } | undefined;
    if (!user) {
      // Attempt link by email if available
      if (profile.email) {
        const byEmail = await client`SELECT id, email, username FROM users WHERE email = ${profile.email} LIMIT 1`;
        if (byEmail[0]) {
          user = byEmail[0] as { id: string; email?: string | null; username?: string | null };
          await client`UPDATE users SET discordId = ${profile.id}, updatedAt = ${new Date()} WHERE id = ${user.id}`;
        }
      }
    }
    if (!user) {
      const id = randomUUID();
      const username = profile.username || `dc_${(profile.id || '').toString().substring(0, 8)}`;
      await client`
        INSERT INTO users ${client([
          { id, email: profile.email, username, passwordHash: null, discordId: profile.id },
        ])}
      `;
      user = { id } as { id: string };
    }
    const session = await sessions.issue(user.id);
    return c.json({ token: session.token, userId: user.id });
  } catch (err) {
    return c.json({ error: 'OAuth failed' }, 400);
  }
});

export default router;


