import type { AuditAction, AuditActorType, AuditLogRecord } from '../../types/index';
import { randomUUID } from 'node:crypto';

interface AuditLogMeta {
  tenantId?: string | null;
  ip?: string | null;
  userAgent?: string | null;
  metadata?: Record<string, unknown> | null;
}

export interface AuditLogWithActor extends AuditLogRecord {
  actorEmail: string | null;
  actorUsername: string | null;
}

export interface AuditService {
  log(userId: string, action: AuditAction, actorType: AuditActorType, meta?: AuditLogMeta): Promise<void>;
  listForUser(userId: string, page: number, pageSize: number): Promise<{ items: AuditLogWithActor[]; total: number }>;
  listForTenant(tenantId: string, page: number, pageSize: number): Promise<{ items: AuditLogWithActor[]; total: number }>;
}

/**
 * Create an AuditService backed by the provided DB client.
 * The implementation stores audit events in `audit_logs` and
 * serializes metadata for sqlite adapters.
 */
export function createAuditService(db: { client: any }, adapter: 'postgres' | 'mysql' | 'sqlite'): AuditService {
  const { client } = db;
  return {
    async log(userId, action, actorType, meta) {
      const id = typeof randomUUID === 'function' ? randomUUID() : String(Date.now()) + Math.random().toString().slice(2);
      const payload: any = {
        id,
        userId,
        tenantId: meta?.tenantId ?? null,
        action,
        actorType,
        ip: meta?.ip ?? null,
        userAgent: meta?.userAgent ?? null,
        metadata: meta?.metadata ?? null,
      };
      if (adapter === 'sqlite') {
        if (payload.metadata && typeof payload.metadata !== 'string') {
          payload.metadata = JSON.stringify(payload.metadata);
        }
      }
      await client`INSERT INTO audit_logs ${client([payload])}`;
    },
    async listForUser(userId, page, pageSize) {
      const offset = (page - 1) * pageSize;
      const rows = await client`SELECT l.id, l.userId, l.tenantId, l.action, l.actorType, l.ip, l.userAgent, l.metadata, l.createdAt, u.username as actorUsername, u.email as actorEmail FROM audit_logs l LEFT JOIN users u ON u.id = l.userId WHERE l.userId = ${userId} ORDER BY l.createdAt DESC LIMIT ${pageSize} OFFSET ${offset}`;
      const [{ count }] = await client`SELECT COUNT(*) as count FROM audit_logs WHERE userId = ${userId}` as Array<{ count: number }>;
      return {
        items: rows.map(mapRow(adapter)),
        total: Number(count) || 0,
      };
    },
    async listForTenant(tenantId, page, pageSize) {
      const offset = (page - 1) * pageSize;
      const rows = await client`SELECT l.id, l.userId, l.tenantId, l.action, l.actorType, l.ip, l.userAgent, l.metadata, l.createdAt, u.username as actorUsername, u.email as actorEmail FROM audit_logs l LEFT JOIN users u ON u.id = l.userId WHERE l.tenantId = ${tenantId} ORDER BY l.createdAt DESC LIMIT ${pageSize} OFFSET ${offset}`;
      const [{ count }] = await client`SELECT COUNT(*) as count FROM audit_logs WHERE tenantId = ${tenantId}` as Array<{ count: number }>;
      return {
        items: rows.map(mapRow(adapter)),
        total: Number(count) || 0,
      };
    },
  };
}

function mapRow(adapter: 'postgres' | 'mysql' | 'sqlite') {
  return (row: any): AuditLogWithActor => ({
    id: row.id,
    userId: row.userId,
    tenantId: row.tenantId,
    action: row.action,
    actorType: row.actorType,
    ip: row.ip ?? null,
    userAgent: row.userAgent ?? null,
    metadata: adapter === 'sqlite' && typeof row.metadata === 'string' ? safeParse(row.metadata) : row.metadata ?? null,
    createdAt: row.createdAt instanceof Date ? row.createdAt : new Date(row.createdAt),
    actorEmail: row.actorEmail ?? null,
    actorUsername: row.actorUsername ?? null,
  });
}

function safeParse(input: string) {
  try {
    return JSON.parse(input);
  } catch (err) {
    console.warn('[Audit] Failed to parse metadata JSON:', (err as Error).message);
    return null;
  }
}


