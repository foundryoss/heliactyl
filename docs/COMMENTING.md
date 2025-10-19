Project Commenting Standards
===========================

Purpose
-------
This document defines how and where to add comments inside the application to keep code readable, consistent, and machine-friendly.

General rules
-------------
- **Prefer concise JSDoc for functions and exported modules.** Use /** ... */ above functions or complex blocks to describe purpose, inputs, and outputs.
- **Avoid large SQL-style block comments** that restate what code does. Replace them with a short single-line or JSDoc summary describing intent.
- **Keep comments up-to-date.** If code changes, edit or remove comments that become misleading.
- **Use inline comments sparingly** for non-obvious algorithms or workarounds.

Router files
------------
- Each router should export a `manifest` object with `name`, `author`, `targetVersion`, and `description`.
- For middleware-like helpers (e.g. `requireAuth`, `requireAdmin`) prefer JSDoc that documents parameters and the return value.
- Add a short comment above each route documenting the HTTP method and path (e.g. `GET /:tenantId/servers`) and a one-line description.

Libs and utilities
-------------------
- Publicly exported helpers and services must have JSDoc describing their API shapes and any important behaviors (e.g. serialization for sqlite).
- Use type annotations for all exported types and interfaces.

SQL and DB access
-----------------
- Keep SQL queries as-is, but remove long block comments that duplicate the query. Instead, add a one-line note explaining intent (e.g., "fetch tenant members for auth check").
- When serializing metadata for sqlite, explain *why* (compatibility with sqlite TEXT columns) in a one-liner.

Examples
--------
```ts
/**
 * Verify bearer token and return session record or null.
 * @param c Hono context
 */
async function requireAuth(c: Context<AppEnv>) { ... }
```

When to use block comments
--------------------------
- Only for file-level module descriptions or when a multi-line explanation is truly required. Prefer JSDoc for function-level docs.

Formatting
----------
- Use `//` for short inline notes and `/** */` JSDoc for exported APIs.
- Keep line length reasonable (~120 chars).

Maintenance
-----------
- When editing a file that previously used block comments (`/* ... */`) refactor them into JSDoc or single-line comments and update the todo list accordingly.


