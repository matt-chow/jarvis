# Architecture

## Overview

```
 Browser / PWA (React)                     Cloudflare (free tier)
 ┌─────────────────────────┐        ┌──────────────────────────────────────┐
 │ App shell + modules     │        │ Worker "directory"                   │
 │ Service worker ─ cache  │  HTTPS │  ├─ Static Assets  (/*, SPA fallback)│
 │ IndexedDB (offline data)│ ─────▶ │  └─ Rust/WASM API  (/api/*)          │
 └─────────────────────────┘        │       ├─ D1 "directory-db"  (DB)     │
                                    │       └─ R2 "directory-files" (FILES)│
                                    └──────────────────────────────────────┘
```

This is a modular monolith: a single Worker hosts both the UI and the API.
`run_worker_first = ["/api/*"]` means static files are served without invoking
the Worker, so they are free and unmetered. Only API calls run Rust code.

## Layers

| Layer | Location | Depends on | Responsibility |
| --- | --- | --- | --- |
| Domain core | `crates/directory-core` | `serde` only | Health aggregation, auth decision, storage key rules, size and content-type limits, schema-version check. Defines the `ObjectStore` and `SystemRepository` traits. |
| Worker adapter | `crates/directory-worker` | `worker` crate, core | HTTP routing (`http.rs`), D1 repo (`d1.rs`), R2 store (`r2.rs`), binding names (`bindings.rs`). No business rules. |
| Web app | `apps/web` | React, Tailwind | Shell, modules, API client, IndexedDB, service worker. |

Rule: Cloudflare types never appear in `directory-core`. A new backend, such as
SQLite on a desktop client or S3, implements the core traits.

## HTTP API (v1)

| Method | Path | Auth | Notes |
| --- | --- | --- | --- |
| GET | `/api/v1/health` | none | `200` ok / `503` degraded. Checks D1 schema version and binding presence; no data exposed. |
| GET | `/api/v1/storage/objects?prefix=&limit=` | Bearer | List (max 1000). |
| PUT | `/api/v1/storage/objects/{key}` | Bearer | Body ≤ 10 MB. Returns `201` + metadata. |
| GET | `/api/v1/storage/objects/{key}` | Bearer | `Content-Disposition: attachment`, CSP `sandbox`. |
| DELETE | `/api/v1/storage/objects/{key}` | Bearer | `204`. |

Keys are `segment(/segment)*` over `[A-Za-z0-9._-]`, with no `.`/`..` segments
and at most 512 bytes. Errors are JSON `{ "error": code }`. `401` includes
`WWW-Authenticate: Bearer`, and a missing or weak server secret returns
`503 auth_not_configured`. All API responses are `Cache-Control: no-store`.

## Data

- **D1** migrations live in `migrations/NNNN_name.sql` and are append-only.
  Wrangler tracks them in `d1_migrations`. `system_meta.schema_version` must equal
  `EXPECTED_SCHEMA_VERSION` in core, otherwise health reports degraded.
- **R2** bucket `directory-files` is private, with no r2.dev URL or custom
  domain. It is reachable only through the authenticated Worker routes.
- **IndexedDB** database `directory` (v1) has a `notes` store (keyPath `id`,
  index `updatedAt`) and a `kv` store (cached health report).

## Offline / PWA

- `vite.config.ts` emits `dist/sw.js` from `sw/service-worker.js`. It injects the
  list of hashed build files plus shell files as the precache manifest, and a
  content hash as the cache version. No Workbox is involved.
- Navigations are network-first with the cached `/` as fallback. Hashed assets
  are cache-first. `/api/*` is never cached by the service worker.
- Updates: a new SW waits, the app shows "Reload to update", and the page then
  posts `SKIP_WAITING` and reloads on `controllerchange`.
- Health falls back to the last report cached in IndexedDB.

## Security

- **Sprint 00 auth:** one shared bearer token (`DIRECTORY_API_TOKEN`, a Worker
  secret, at least 32 characters). It is compared in constant time and fails
  closed. In the UI it is held in `sessionStorage` only. This is a stopgap: see
  DECISIONS D-004.
- Static assets carry security headers from `public/_headers` (CSP,
  `X-Frame-Options`, `nosniff`, referrer and permissions policies).
- Backend error details are logged server-side, never returned.

## Deployment

Everything below is free-tier. No step enables a paid product.

1. `npx wrangler login`
2. `npx wrangler d1 create directory-db`. Copy the printed `database_id` into
   `wrangler.toml`.
3. `npx wrangler r2 bucket create directory-files`. Don't enable public access.
   (R2 requires the account's R2 subscription to be activated, which is free
   within limits but asks for a payment method; confirm before doing this.)
4. `npx wrangler secret put DIRECTORY_API_TOKEN`. Paste a random value of at
   least 32 characters:
   `node -e "console.log(require('crypto').randomBytes(32).toString('hex'))"`
5. `npm run db:migrate:remote`
6. `npm run deploy`
7. `BASE_URL=https://directory.<subdomain>.workers.dev DIRECTORY_API_TOKEN=… npm run smoke`

Don't store personal data in the deployed instance until Sprint 01 auth is in
place. The shared token protects the PoC, but it isn't owner identity.
