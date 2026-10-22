# Architecture

## Overview

```
 Browser / PWA (React)                     Cloudflare (free tier)
 ┌─────────────────────────┐        ┌──────────────────────────────────────┐
 │ App shell + modules     │        │ Worker "directory"                   │
 │ Service worker ─ cache  │  HTTPS │  ├─ Static Assets  (/*, SPA fallback)│
 │ IndexedDB (offline data)│ ─────▶ │  └─ Rust/WASM API  (/api/*)          │
 └─────────────────────────┘        │       ├─ D1 "directory-db"  (DB)     │
   native clients (later) ──Bearer─▶│       └─ R2 "directory-files" (FILES)│
                                    └──────────────────────────────────────┘
```

This is a modular monolith: a single Worker hosts both the UI and the API.
`run_worker_first = ["/api/*"]` means static files are served without invoking
the Worker, so they are free and unmetered. Only API calls run Rust code.

## Layers

| Layer | Location | Depends on | Responsibility |
| --- | --- | --- | --- |
| Domain core | `crates/directory-core` | `serde`, pure-Rust crypto (`p256`, `rsa`, `sha2`, `base64`) | Identity (WebAuthn verification, sessions, device tokens), owner-scoped storage rules, health, schema version. Defines repository traits (`IdentityRepository`, `ObjectStore`, `SystemRepository`) and `Entropy`. |
| Worker adapter | `crates/directory-worker` | `worker` crate, core | `http/` (routing, JSON, auth guard), `d1/` repositories, `r2.rs` store, `entropy.rs` (Web Crypto RNG), `bindings.rs`. No business rules. |
| Web app | `apps/web` | React, Tailwind | Shell, auth screens, modules, API client, WebAuthn glue, IndexedDB, service worker. |

Rule: Cloudflare types never appear in `directory-core`. Time (`now`) and
randomness (`Entropy`) are injected, so core logic is deterministic in tests. A
new backend, such as SQLite on a desktop client, implements the core traits.

## Identity and authorization

- **Owner sign-in uses passkeys (WebAuthn).** Requirements:
  - User verification is required.
  - Credentials must be discoverable (resident keys).
  - Supported algorithms are ES256 and RS256.
  - Attestation is `none`; trust instead comes from how a passkey gets
    registered (next point).

  The browser sends the SPKI public key (`getPublicKey()`) and the raw
  authenticator data, so the server needs no CBOR parsing.
- **First run:** while no owner exists, `/auth/setup/*` registers the first
  passkey when given the `DIRECTORY_SETUP_TOKEN` secret (at least 32 characters).
  Owner creation is atomic, so if two setups race, only one wins. After that,
  setup is closed (409). Further passkeys need an authenticated session.
- **Browser sessions:** a random 256-bit token is stored as SHA-256 in D1 and
  sent as `__Host-dir_session` (HttpOnly; Secure; SameSite=Strict). Expiry is
  30 days, sliding (refreshed at most daily), with a 90-day absolute cap.
  Logout deletes the session server-side.
- **Device tokens** (`dir_…` bearer tokens) are for native apps and scripts.
  They are stored hashed, revocable, and shown once at creation. They can use
  data APIs but can't manage passkeys or tokens (`403 session_required`).
- **CSRF:** cookie-authenticated non-GET requests must send an `Origin` listed in
  `DIRECTORY_ORIGINS` (`403 origin_not_allowed`), on top of SameSite=Strict.
- **Challenges** are single-use (`DELETE … RETURNING`), purpose-bound
  (setup/login/add-passkey), and expire after 5 minutes. Expired rows are purged
  on each insert.
- **Rate limiting:** setup and login endpoints allow 30 requests per client IP
  (`CF-Connecting-IP`) per 10-minute fixed window, stored in D1
  `rate_limits`. Over the limit they return `429 rate_limited` with
  `Retry-After`. If the counter store itself fails, requests are allowed
  through and the error is logged.
- **Fail closed:** if `DIRECTORY_RP_ID`/`DIRECTORY_ORIGINS` are missing, every
  auth flow returns `503 auth_not_configured`. A missing or weak setup token
  disables setup.
- **Authorization:** every private handler resolves a `Principal` via
  `http::auth::authenticate`. Data is scoped by `principal.user_id`, and R2 keys
  are physically prefixed `u/<user_id>/`.

## HTTP API (v1)

| Method | Path | Auth | Notes |
| --- | --- | --- | --- |
| GET | `/api/v1/health` | none | `200` ok / `503` degraded. D1 schema version and binding presence; no data exposed. |
| GET | `/api/v1/auth/session` | optional | `{ authenticated, setupRequired, user? }` |
| POST | `/api/v1/auth/setup/options` | setup token | Body `{ setupToken, displayName }` → creation options. `409` once an owner exists. |
| POST | `/api/v1/auth/setup/verify` | — | Registration → `201` + session cookie |
| POST | `/api/v1/auth/login/options` | — | Request options (discoverable credentials) |
| POST | `/api/v1/auth/login/verify` | — | Assertion → `200` + session cookie |
| POST | `/api/v1/auth/logout` | session | Revokes the session and clears the cookie |
| GET / POST | `/api/v1/auth/passkeys`, `/passkeys/options`, `/passkeys/verify` | session only | List / add passkeys |
| DELETE | `/api/v1/auth/passkeys/{id}` | session only | `409 last_passkey` for the final one |
| GET / POST | `/api/v1/auth/tokens` | session only | List / create device token (secret returned once) |
| DELETE | `/api/v1/auth/tokens/{id}` | session only | Revoke |
| GET | `/api/v1/links?kind=task|note|project|journal&id=` | session or token | Items linked to an entity, with resolved titles. Links whose other end was deleted are dropped and removed. |
| POST | `/api/v1/links` | session or token | `{ from: {kind,id}, to: {kind,id} }` → `201 { link_id, kind, id, title }`. Idempotent and symmetric; both ends must exist and be owned. |
| DELETE | `/api/v1/links/{id}` | session or token | Unlink |
| GET / POST | `/api/v1/inbox` | session or token | List (oldest first), or capture `{ text }` (≤ 2,000 chars) |
| DELETE | `/api/v1/inbox/{id}` | session or token | Discard |
| POST | `/api/v1/inbox/{id}/triage` | session or token | `{ to: task|note|journal, date? }` → `{ kind, task|note|entry }`. Goes through the target module's service; the target is created before the item is removed. |
| GET | `/api/v1/journal?from=&to=` | session or token | Summaries (`date, excerpt, mood`) for a range of at most 366 days, newest first |
| GET / PUT / DELETE | `/api/v1/journal/{YYYY-MM-DD}` | session or token | PUT upserts `{ body?, mood? (1–5 or null), expected_updated_at? }`. A stale version or a same-day create race gets `409 { entry }`. |
| GET | `/api/v1/notes` | session or token | Summaries `{ id, title, excerpt, pinned, … }`, without bodies. Pinned first, then newest. |
| POST | `/api/v1/notes` | session or token | `{ title?, body?, pinned? }` → `201`. Body ≤ 100k chars (512 KiB request cap). |
| GET / PATCH / DELETE | `/api/v1/notes/{id}` | session or token | PATCH `title, body, pinned, expected_updated_at?`. A stale version gets `409 { error: "conflict", note }`. |
| GET | `/api/v1/projects[?include=archived]` | session or token | Owner's projects (active only by default) |
| POST | `/api/v1/projects` | session or token | `{ name, description? }` → `201` |
| GET / PATCH / DELETE | `/api/v1/projects/{id}` | session or token | PATCH `name, description, status`. DELETE keeps the project's tasks and unassigns them. |
| GET | `/api/v1/tasks?status=all|open|closed` | session or token | Owner's tasks and subtasks (max 1000) |
| POST | `/api/v1/tasks` | session or token | `{ title, notes?, priority?, due_date?, parent_id?, project_id? }` → `201`. Subtasks are one level deep and can't set `project_id`. |
| GET / PATCH / DELETE | `/api/v1/tasks/{id}` | session or token | PATCH: any of `title, notes, status, priority, due_date, project_id` (`null` clears the due date or project). DELETE cascades to subtasks. |
| GET | `/api/v1/storage/objects?prefix=&limit=` | session or token | List the owner's objects (max 1000) |
| PUT / GET / DELETE | `/api/v1/storage/objects/{key}` | session or token | Body ≤ 10 MB. GET is `attachment` with CSP `sandbox`. |

Keys are `segment(/segment)*` over `[A-Za-z0-9._-]`, with no `.`/`..` segments
and at most 512 bytes. Errors are JSON `{ "error": code, "message"? }`.
WebAuthn failures always return the generic `401 authentication_failed`; details
go to the server log only. All API responses are `Cache-Control: no-store`.
JSON bodies are capped at 64 KiB.

## Data

- **D1** migrations live in `migrations/NNNN_name.sql` and are append-only.
  `system_meta.schema_version` must equal `EXPECTED_SCHEMA_VERSION` (now **9**).
  - `0001`: `system_meta`
  - `0002`: `users`, `passkeys`, `auth_challenges`, `sessions`, `api_tokens`.
    Times are epoch milliseconds; secrets are stored only as SHA-256 hex.
  - `0003`: `rate_limits` (fixed-window counters).
  - `0004`: `tasks` (`owner_id`, `parent_id`, status, priority, `due_date`
    as `YYYY-MM-DD`, timestamps, `completed_at`).
  - `0005`: `projects`, plus `tasks.project_id` (top-level tasks only).
  - `0006`: `notes`. `updated_at` doubles as the optimistic-concurrency
    version; saves use `UPDATE … WHERE updated_at = expected`.
  - `0007`: `journal_entries` (primary key `owner_id, date`; creates use
    `ON CONFLICT DO NOTHING`).
  - `0008`: `inbox_items`.
  - `0009`: `links` (normalized pair `a < b`, unique per owner).
- **R2** bucket `directory-files` is private, with no r2.dev URL or custom domain.
  Object keys are `u/<user_id>/<key>`.
- **IndexedDB** database `directory` (v1) has a `kv` store with cached
  copies for read-only offline viewing: health, the last verified session user,
  task, project, and note lists, each opened note (`note:<id>`), and each
  journal month and day (`journal:<YYYY-MM>`, `journal:<date>`). It also
  has a legacy `notes` store (Sprint 00 Scratchpad), which is migrated into the
  account once and then emptied.

## Offline / PWA

- `vite.config.ts` emits `dist/sw.js` from `sw/service-worker.js`. It injects the
  hashed build files plus shell files as the precache manifest, and a content
  hash as the cache version.
- Navigations are network-first with the cached `/` as fallback. Hashed assets
  are cache-first. `/api/*` is never cached by the service worker.
- Updates: a new SW waits, the app shows "Reload to update", and the page then
  posts `SKIP_WAITING` and reloads on `controllerchange`.
- **Offline sessions:** if the session check can't reach the server, the app
  uses the last verified user cached in IndexedDB and says so in Settings.
  Device-local modules keep working. A `401` from any call re-checks the session
  and shows sign-in. This is UX only, not a security boundary: the server
  authorizes every request.

## Security headers

Static assets get headers from `public/_headers`: CSP, `X-Frame-Options`,
`nosniff`, and referrer and permissions policies. Backend error details are
logged server-side and never returned.

## Deployment

Everything below is free-tier. No step enables a paid product.

1. `npx wrangler login`
2. `npx wrangler d1 create directory-db`. Copy the `database_id` into
   `wrangler.toml`.
3. `npx wrangler r2 bucket create directory-files`. Don't enable public access.
   (R2 must be activated on the account. It's free within limits but asks for a
   payment method, so get the owner's approval first.)
4. In `wrangler.toml` `[vars]`, set `DIRECTORY_RP_ID` to the deployed hostname
   (e.g. `directory.<subdomain>.workers.dev`) and `DIRECTORY_ORIGINS` to its
   `https://` origin.
5. `npx wrangler secret put DIRECTORY_SETUP_TOKEN`. Paste a random value of at
   least 32 characters:
   `node -e "console.log(require('crypto').randomBytes(32).toString('hex'))"`
6. `npm run db:migrate:remote`, then `npm run deploy`.
7. Open the site, complete first-run setup with the setup token, and create a
   device token in Settings.
8. `BASE_URL=https://… DIRECTORY_API_TOKEN=dir_… npm run smoke`
