# Manhattan Web

Frontend for the Manhattan backend.

## Run

1. Start backend (root):

```bash
bun run dev
```

2. Start frontend (in `web/`):

```bash
bun run watch
```

App loads config from `/config.json` (apiBaseUrl). Update it to point to your backend origin.

## Notes

- Auth is Bearer-token based; token is stored in localStorage.
- Navbar appears only when authenticated.
- Pages: Login, Register, Dashboard, Tenant, Account, Admin.
- Minimal styles on purpose.

