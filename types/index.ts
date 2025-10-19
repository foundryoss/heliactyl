// Heliactyl unified types (version 15)

export interface AppMetadata {
  name: string; // e.g., "Heliactyl Next 15"
  version: string; // e.g., "15.0.0"
}

export interface ServerConfig {
  app: AppMetadata;
  server: {
    host: string;
    port: number;
    basePath?: string;
    cors?: {
      origins: string[];
      allowCredentials?: boolean;
      allowHeaders?: string[];
      allowMethods?: string[];
    };
  };
  security: {
    sessionTtlSeconds: number; // TTL for bearer sessions in Redis
    passwordHashing: {
      algorithm: 'bcrypt';
      bcryptRounds: number; // cost factor
    };
    encryptionKey?: string; // AES-256 key for reversible encryption
  };
  database: {
    url: string; // e.g., sqlite://./server/data/heliactyl.db
  };
  redis: {
    url: string; // e.g., redis://localhost:6379
    namespace?: string; // key prefix, e.g., "heliactyl:v15"
    // Allow runtime option to fall back to in-memory store (dev/test).
    allowInMemoryFallback?: boolean;
  };
  pterodactyl?: {
    url: string;
    apiKey: string; // Application API key (admin functions)
    clientKey: string; // Client API key (user functions)
    reconcileIntervalMinutes?: number; // default 30
    eggSyncIntervalMinutes?: number; // default 10
  };
  jobs?: {
    enabled: boolean;
    startupRun?: boolean;
    eggSyncIntervalMinutes?: number;
    reconcileIntervalMinutes?: number;
  };
  packages?: {
    defaultPackage: string;
    items: Record<string, PackageResources>;
    maxTenantsDefault?: number; // default 2
  };
  locations?: Array<{ id: number; slug: string; name: string; lockedPackage?: string }>;
  oauth?: {
    discord?: OAuthProviderConfig;
    [provider: string]: OAuthProviderConfig | undefined;
  };
}

export interface OAuthProviderConfig {
  enabled: boolean;
  clientId: string;
  clientSecret: string;
  redirectUri: string; // absolute URL back to our server
  scopes?: string[];
}

export interface ModuleManifest {
  name: string; // e.g., "Auth"
  author: string; // e.g., "Your Name"
  targetVersion: string; // e.g., "15.0.0"
  description?: string;
  dependencies?: string[]; // other module names this depends on
}

export interface HeliaModule {
  manifest: ModuleManifest;
  register: (ctx: ModuleRegisterContext) => Promise<void> | void;
}

export interface ModuleRegisterContext {
  // Deliberately narrow surface shared with modules
  app: unknown; // Hono<AppEnv>
  config: ServerConfig;
  db: unknown; // Bun SQL client
  redis: unknown; // Redis client
}

export interface UserRecord {
  id: string; // UUID (or adapter-specific UID)
  email: string | null;
  username: string | null;
  passwordHash: string | null; // null for OAuth-only users
  discordId: string | null;
  isAdmin?: boolean;
  createdAt: Date;
  updatedAt: Date;
}

export interface SessionRecord {
  id: string; // session id (uuid)
  token: string; // opaque bearer
  userId: string;
  createdAt: number; // epoch seconds
  expiresAt: number; // epoch seconds
}

export type AuditActorType = 'user' | 'system';
export type AuditAction =
  | 'auth:success'
  | 'auth:fail'
  | 'auth:logout'
  | 'auth:register'
  | 'account:update'
  | 'session:revoke'
  | 'session:issue';

export interface AuditLogRecord {
  id: string;
  userId: string;
  tenantId: string | null;
  action: AuditAction;
  actorType: AuditActorType;
  ip: string | null;
  userAgent: string | null;
  metadata: Record<string, unknown> | null;
  createdAt: Date;
  actorEmail?: string | null;
  actorUsername?: string | null;
}

// ===== Heliactyl Pterodactyl/Tenant Types =====

export interface PackageResources {
  memoryMb: number; // MB
  diskMb: number; // MB
  cpuPercent: number; // 100 = 1 vCPU
  serverSlots: number;
}

export type TenantRole = 'owner' | 'user';

export interface TenantRecord {
  id: string;
  name: string;
  ownerUserId: string;
  packageId: string;
  extraMemoryMb: number;
  extraDiskMb: number;
  extraCpuPercent: number;
  extraServerSlots: number;
  createdAt: Date;
  updatedAt: Date;
}

export interface TenantMemberRecord {
  tenantId: string;
  userId: string;
  role: TenantRole; // 'owner' | 'user'
}

export interface TenantPteroAccountRecord {
  tenantId: string;
  pteroUserId: number;
  email: string;
  encryptedPassword: string; // AES-256-GCM payload (iv:tag:ciphertext)
}

export interface EggRecord {
  id: string; // uuid
  nestId: number;
  eggId: number;
  name: string;
  dockerImage: string;
  startup: string;
  environment: Record<string, string>;
}

export interface ServerModelRecord {
  id: string; // uuid local
  tenantId: string;
  pteroServerId: number;
  pteroIdentifier: string; // Short identifier used for websockets
  name: string;
  eggId: number;
  dockerImage: string;
  memoryMb: number;
  diskMb: number;
  cpuPercent: number;
  locationId: number;
  createdAt: Date;
}


