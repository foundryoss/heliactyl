import type { ServerConfig } from '../../types/index';
import type { HeliaDBClient } from '../db';
import type { RedisClient } from './redis';
import type { SessionService } from './session';
import type { OAuthProvider } from './oauth/types';
import type { AuditService } from './audit';
import type { PteroClient } from './ptero';
import type { CryptoService } from './crypto';
import type { MQTTService } from './mqtt';
import type { EmbeddedMQTTBroker } from './mqtt-broker';

export type AppEnv = {
  Bindings: Record<string, never>;
  Variables: {
    config: ServerConfig;
    db: { client: HeliaDBClient; adapter: 'postgres' | 'mysql' | 'sqlite' };
    redis: RedisClient;
    sessions: SessionService;
    oauthProviders: { discord: OAuthProvider | null };
    audit: AuditService;
    ptero: PteroClient;
    crypto: CryptoService;
    mqtt: MQTTService | null;
    mqttBroker: EmbeddedMQTTBroker;
  };
};


