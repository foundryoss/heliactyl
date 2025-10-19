![Alt text](https://i.ibb.co/XrC0Q1mN/Next15-17.png "Heliactyl")

## Heliactyl

> [!NOTE]  
> This is Heliactyl Next version 15.0.0-dev.

> [!IMPORTANT]  
> This is a very very early version of Next 15 and is only supported running in development mode at this time. Not much is really implemented, but things work.


### Repository

- `server` — API
- `client` — Frontend (Vite)

### Quick start

1) Install Bun: `https://bun.sh`

2) Copy and edit config:

```bash
cp server/config.yml.example server/config.yml
```

3) Install dependencies:

```bash
bun install
```

4) Run in development:

```bash
# Server (API)
bun run dev:server

# Client (UI)
bun run dev:client
```

### Configuration (`server/config.yml`)

Key sections:

- `app`: name and version metadata
- `server`: host, port, CORS, basePath
- `security`: session TTL, password hashing (bcrypt), `encryptionKey`
- `database.url`: database connection string
- `redis`: session/cache store URL and namespace
- `oauth.discord`: optional Discord OAuth settings
- `pterodactyl`: panel URL and API keys
- `packages`, `locations`: product packaging and region definitions

#### Database URL Protocols

Heliactyl has support for multiple databases and infers the adapter from the URL scheme:

- SQLite (recommended for local dev):
  - `sqlite://./server/data/heliactyl.db`
  - `file://./server/data/heliactyl.db`
  - `:memory:` (for tests I suppose)
- Postgres:
  - `postgres://user:pass@host:5432/dbname`
  - `postgresql://user:pass@host:5432/dbname`
- MySQL:
  - `mysql://user:pass@host:3306/dbname`
  - `mysql2://user:pass@host:3306/dbname`

Change the `database.url` to switch protocols; you don't need to do anything after that. On startup, HeliaDB 15 will create tables based on `server/schema.yml` using our nice and smart migrations system.

#### Redis

Set `redis.url` to your Redis instance. For local dev, `redis://127.0.0.1:6379` is probably what you'd use.

#### OAuth (Discord)

Set `enabled: true`, `clientId`, `clientSecret`, and `redirectUri` to enable Discord OAuth2.

#### Pterodactyl

Provide `url`, `apiKey`, and `clientKey`. Jobs reconcile servers and sync eggs at intervals. Heliactyl assumes that you are running Cryogenic, our Rust-based replacement for Wings.

> [!IMPORTANT]  
> If you aren't running Cryogenic, please ensure that in your Wings config that you have `allowed-origins` set to `*`. For example: `allowed-origins: ['*']`. If you use Cryogenic, the daemon automatically configures origins for you.

### Testing

Server tests:

```bash
cd server && bun run test
```

### Attribution Requirement

This project is licensed under the Foundry Open Source License. Public deployments must include visible attribution in the site footer.

### Copyright

Copyright (c) Foundry Technologies Inc. All rights reserved. A project by Matt James.


