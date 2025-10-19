import type { ServerConfig } from '../../types/index';

export interface RedisClient {
  raw: any;
  buildKey: (...parts: string[]) => string;
  getJSON<T = unknown>(key: string): Promise<T | null>;
  setJSON(key: string, value: unknown, ttlSeconds?: number): Promise<'OK' | null>;
  delete(key: string): Promise<number>;
}

export async function createRedis(cfg: ServerConfig): Promise<RedisClient> {
  const prefix = cfg.redis.namespace ? cfg.redis.namespace + ':' : '';
  const buildKey = (...parts: string[]) => prefix + parts.join(':');

  // Try Bun's built-in Redis first
  let raw: any = null;
  try {
    const bunMod: any = await import('bun');
    if (bunMod && typeof bunMod.Redis === 'function') {
      raw = new bunMod.Redis(cfg.redis.url);
    }
  } catch {}

  // Fallback to node-redis if Bun.Redis is unavailable
  if (!raw) {
    try {
      const { createClient } = await import('redis');
      const client = createClient({ url: cfg.redis.url });
      await client.connect();
      raw = client;
    } catch (err) {
      const allowFallback = (cfg.redis as unknown as { allowInMemoryFallback?: boolean }).allowInMemoryFallback === true;
      if (!allowFallback) throw err;
      const store = new Map<string, { value: string; expires?: number }>();
      raw = {
        async get(key: string) {
          const entry = store.get(key);
          if (!entry) return null;
          if (entry.expires && entry.expires < Date.now()) {
            store.delete(key);
            return null;
          }
          return entry.value;
        },
        async set(key: string, value: string, opts?: { EX?: number }) {
          const exp = opts?.EX ? Date.now() + opts.EX * 1000 : undefined;
          store.set(key, { value, expires: exp });
          return 'OK';
        },
        async setEx(key: string, ttl: number, value: string) {
          store.set(key, { value, expires: Date.now() + ttl * 1000 });
          return 'OK';
        },
        async setex(key: string, ttl: number, value: string) {
          store.set(key, { value, expires: Date.now() + ttl * 1000 });
          return 'OK';
        },
        async del(key: string) {
          return store.delete(key) ? 1 : 0;
        },
      };
      console.warn('[Redis] Falling back to in-memory store (dev only).');
    }
  }

  async function getJSON<T = unknown>(key: string): Promise<T | null> {
    const val = await raw.get(key);
    if (val == null) return null;
    try {
      return JSON.parse(val) as T;
    } catch {
      return null;
    }
  }

  async function setJSON(key: string, value: unknown, ttlSeconds?: number): Promise<'OK' | null> {
    const payload = JSON.stringify(value);
    if (ttlSeconds && ttlSeconds > 0) {
      if (typeof raw.setex === 'function') {
        return (await raw.setex(key, ttlSeconds, payload)) as 'OK' | null;
      }
      if (typeof raw.setEx === 'function') {
        await raw.setEx(key, ttlSeconds, payload);
        return 'OK';
      }
      // node-redis set with EX option
      await raw.set(key, payload, { EX: ttlSeconds });
      return 'OK';
    }
    return (await raw.set(key, payload)) as 'OK' | null;
  }

  async function deleteKey(key: string): Promise<number> {
    return await raw.del(key);
  }

  return { raw, buildKey, getJSON, setJSON, delete: deleteKey };
}


