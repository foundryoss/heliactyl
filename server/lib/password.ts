import type { ServerConfig } from '../../types/index';

export interface PasswordService {
  hashPassword(plain: string): Promise<string>;
  verifyPassword(plain: string, hash: string): Promise<boolean>;
}

export function createPasswordService(cfg: ServerConfig): PasswordService {
  const rounds = cfg.security.passwordHashing.bcryptRounds;

  async function hashPassword(plain: string): Promise<string> {
    return await Bun.password.hash(plain, { algorithm: 'bcrypt', cost: rounds });
  }

  async function verifyPassword(plain: string, hash: string): Promise<boolean> {
    return await Bun.password.verify(plain, hash);
  }

  return { hashPassword, verifyPassword };
}


