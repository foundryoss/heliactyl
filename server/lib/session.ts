import type { ServerConfig, SessionRecord } from '../../types/index';
import { randomUUID, randomBytes } from 'node:crypto';
import type { RedisClient } from './redis';

/**
 * Session service backed by Redis; optionally persists sessions to DB for auditing.
 */

export interface SessionService {
  issue(userId: string): Promise<SessionRecord>;
  verify(token: string): Promise<SessionRecord | null>;
  revoke(token: string): Promise<void>;
}

export function createSessionService(cfg: ServerConfig, redis: RedisClient, db?: { client: any }): SessionService {
  const ttl = Number((cfg.security as any).sessionTtlSeconds) || 60 * 60 * 24 * 7;
  const keyFor = (token: string) => redis.buildKey('session', token);

  function generateToken(): string {
    const uuid = randomUUID();
    const hex = randomBytes(16).toString('hex');
    return `${uuid}.${hex}`;
  }

  async function issue(userId: string): Promise<SessionRecord> {
    const now = Math.floor(Date.now() / 1000);
    const token = generateToken();
    const record: SessionRecord = {
      id: randomUUID(),
      token,
      userId,
      createdAt: now,
      expiresAt: now + ttl,
    };
    await redis.setJSON(keyFor(token), record, ttl);
    if (db) {
      const expiresAt = new Date(record.expiresAt * 1000).toISOString();
      await db.client`INSERT INTO user_sessions ${db.client([{ id: record.id, userId: record.userId, token: record.token, createdAt: new Date().toISOString(), expiresAt }])}`;
    }
    return record;
  }

  async function verify(token: string): Promise<SessionRecord | null> {
    const rec = await redis.getJSON<SessionRecord>(keyFor(token));
    if (!rec) return null;
    // Optional: refresh TTL (sliding sessions). For now, keep fixed TTL.
    return rec;
  }

  async function revoke(token: string): Promise<void> {
    await redis.delete(keyFor(token));
    if (db) {
      await db.client`DELETE FROM user_sessions WHERE token = ${token}`;
    }
  }

  return { issue, verify, revoke };
}


