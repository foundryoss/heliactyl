import type { RedisClient } from './redis';

export interface RateLimitOptions {
  limit: number; // max requests
  windowSeconds: number; // per window
}

export function createRateLimiter(redis: RedisClient, keyPrefix: string, opts: RateLimitOptions) {
  const { limit, windowSeconds } = opts;
  return async function check(key: string): Promise<{ allowed: boolean; remaining: number; resetSeconds: number }> {
    const nowSec = Math.floor(Date.now() / 1000);
    const bucket = String(Math.floor(nowSec / windowSeconds));
    const windowKey = redis.buildKey('rl', keyPrefix, key, bucket);

    let current: number | null = null;
    try {
      if (redis.raw && typeof redis.raw.incr === 'function') {
        current = await redis.raw.incr(windowKey);
        if (current === 1 && typeof redis.raw.expire === 'function') {
          await redis.raw.expire(windowKey, windowSeconds);
        }
      } else {
        const val = await redis.getJSON<number>(windowKey);
        current = (val ?? 0) + 1;
        await redis.setJSON(windowKey, current, windowSeconds);
      }
    } catch {
      // as a last resort, allow but do not rate limit
      return { allowed: true, remaining: limit, resetSeconds: windowSeconds };
    }

    const remaining = Math.max(0, limit - current);
    return { allowed: current <= limit, remaining, resetSeconds: windowSeconds - (nowSec % windowSeconds) };
  };
}


