import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import YAML from 'yaml';
import type { ServerConfig } from '../../types/index';

const CONFIG_PATH = resolve(process.cwd(), 'config.yml');

let cachedConfig: ServerConfig | null = null;

export async function loadConfig(): Promise<ServerConfig> {
  if (cachedConfig) return cachedConfig;
  const file = await readFile(CONFIG_PATH, 'utf8');
  const parsed = YAML.parse(file) as ServerConfig;
  validateConfig(parsed);
  cachedConfig = parsed;
  return parsed;
}

function validateConfig(cfg: ServerConfig): void {
  if (!cfg?.app?.name || !cfg?.app?.version) {
    throw new Error('config.app.name and app.version are required');
  }
  if (!cfg?.server?.port) {
    throw new Error('config.server.port is required');
  }
  if (!cfg?.database?.url) {
    throw new Error('config.database.url is required');
  }
  if (!cfg?.redis?.url) {
    throw new Error('config.redis.url is required');
  }
  if (cfg.security?.passwordHashing?.algorithm !== 'bcrypt') {
    throw new Error('Only bcrypt password hashing is currently supported');
  }
}

export async function saveConfig(cfg: ServerConfig): Promise<void> {
  validateConfig(cfg);
  const yaml = YAML.stringify(cfg);
  await writeFile(CONFIG_PATH, yaml, 'utf8');
  cachedConfig = cfg;
}


