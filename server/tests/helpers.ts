import { type ServerConfig } from '../../types/index';
import { createApp } from '../index';
import { loadConfig } from '../lib/config';

export async function makeTestConfig(): Promise<ServerConfig> {
  const cfg = await loadConfig();
  // Override DB/Redis for tests; keep real Pterodactyl config
  return {
    ...cfg,
    server: { ...cfg.server, port: 0 },
    security: { ...cfg.security, passwordHashing: { algorithm: 'bcrypt', bcryptRounds: 6 } },
    database: { url: 'sqlite://:memory:' },
    redis: { ...(cfg.redis as unknown as Record<string, unknown>), url: 'redis://127.0.0.1:6379', namespace: 'heliactyl:test', ...( { allowInMemoryFallback: true } as Record<string, unknown>) },
  };
}

export async function makeTestApp() {
  const cfg = await makeTestConfig();
  const { app } = await createApp({ config: cfg, serve: false });
  return { app, cfg };
}


