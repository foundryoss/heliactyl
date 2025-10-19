# Heliactyl Next 15 (Backend)

- Runtime: Bun
- Framework: Hono
- DB: Bun.SQL (Postgres / MySQL / SQLite)
- Sessions: Redis (Bearer tokens)
- Config: `server/config.yml`

## Quick Start

1. Install Bun: `https://bun.sh`
2. Configure `server/config.yml`
3. Install deps:
```bash
bun install
```
4. Start dev server (no build needed):
```bash
bun run server/index.ts
```
Or with watch:
```bash
bun --watch run server/index.ts
```

## Endpoints

- GET `/` → app info
- GET `/core/health` → health check
- GET `/core/info` → name + version
- POST `/auth/register` → `{ email, username, password }`
- POST `/auth/login` → `{ identifier, password }` (identifier is email or username)
- POST `/auth/logout` (Authorization: Bearer <token>)
- GET `/auth/me` (Authorization: Bearer <token>)
- GET `/auth/discord/login`
- GET `/auth/discord/callback?code=...&state=...`

## Schema

Edit `server/schema.yml` to define tables. On startup, HeliaDB applies `CREATE TABLE IF NOT EXISTS` and indexes. For destructive migrations, update manually.

## References

- Bun.SQL: https://bun.sh/docs/api/sql
- Redis: https://bun.sh/docs/api/redis
