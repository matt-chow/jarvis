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
| Domain core | `crates/directory-core` | `serde`, pure-Rust crypto (`p256`, `rsa`, `sha2`, `base64`) | Identity, every module's domain rules (tasks, projects, notes, journal, inbox, links, files, search, export, JARVIS), health, and schema version. Defines a repository/source trait per module plus `Entropy`. |
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
| POST | `/api/v1/jarvis/plan` | session or token | `{ input, today }` → plan with typed actions. Read-only plans come back `executed`; writes are `pending` (10-minute expiry). Errors: `400 not_understood { message, examples }`, `422 unresolved`. |
| POST | `/api/v1/jarvis/plans/{id}/execute` | session or token | Approve: atomic `pending → running` claim, so a plan runs at most once (`409 not_pending`). |
| POST | `/api/v1/jarvis/plans/{id}/cancel` | session or token | |
| GET | `/api/v1/jarvis/history` | session or token | Last 50 plans with results |
| GET / POST | `/api/v1/jarvis/presets` | session or token | Saved commands `{ name, input }` (≤ 50, validated by parsing). Running one is a normal `plan` call. |
| DELETE | `/api/v1/jarvis/presets/{id}` | session or token | |
| GET | `/api/v1/search?q=` | session or token | 2–100 chars. Case-insensitive substring match across tasks, notes, projects, and journal; LIKE wildcards escaped. Title matches first, then newest; snippets cut in core. |
| GET | `/api/v1/export` | session or token | One JSON download (`format: "directory-export"`, `format_version: 1`): user, tasks, projects, notes with bodies, journal, inbox, links, habits with check-ins, events, playlists, preferences, reading progress, and file metadata. No credential material. |
| GET / POST / DELETE | `/api/v1/diagnostics/errors` | session or token | Client error reports: list the newest 50, report `{ message, module?, stack?, app_version? }` (≤ 32 KiB; cleaned and capped; newest 200 kept), or clear (D-027) |
| POST | `/api/v1/import/{kind}` | session or token | Restore one batch `{ records: [...] }` (≤ 100; body ≤ 4 MiB) of `projects`, `tasks`, `notes`, `journal`, `inbox`, `habits`, `events`, `playlists`, `links`, or `preferences` from an export → `{ inserted, skipped, rejected: [{ id, reason }] }`. Merge-only (D-025). |
| GET | `/api/v1/links?kind=task|note|project|journal&id=` | session or token | Items linked to an entity, with resolved titles. Links whose other end was deleted are dropped and removed. |
| POST | `/api/v1/links` | session or token | `{ from: {kind,id}, to: {kind,id} }` → `201 { link_id, kind, id, title }`. Idempotent and symmetric; both ends must exist and be owned. |
| DELETE | `/api/v1/links/{id}` | session or token | Unlink |
| GET / POST | `/api/v1/automations` | session or token | List (≤ 20), or create `{ name, trigger: { kind: task_completed, project_id? } or { kind: habit_checked, habit_id? }, command, enabled? }`. The command is validated (allowlisted kinds, ≤ 3 steps, known placeholders). |
| PATCH / DELETE | `/api/v1/automations/{id}` | session or token | Partial update (re-validated) / delete. `PATCH /tasks/{id}` and `POST /jarvis/plans/{id}/execute` accept `?today=` (local date) for the automations they fire. |
| GET | `/api/v1/music` | session or token | `{ tracks, playlists }`: audio files (by content type or extension) and playlists, without tracks whose files were deleted |
| POST / PATCH / DELETE | `/api/v1/music/playlists[/{id}]` | session or token | `{ name, tracks? }` / `{ name?, tracks? }` (tracks replace the order; each must be your audio file; ≤ 500; ≤ 100 playlists) / delete |
| GET / PATCH | `/api/v1/preferences` | session or token | Owner preferences (`{ dashboard?: { widgets: [...] } }`). PATCH is partial; `null` resets a field. Widgets are validated against a known list. |
| GET / POST | `/api/v1/events` | session or token | Every event (repeats not expanded, at most 2,000), or create `{ id?, title, start_date, end_date?, start_time?, end_time?, location?, notes?, repeat?, repeat_until? }` → `201` (`409 exists` on a repeated client id). Times are `HH:MM` local wall-clock; no start time = all day. |
| GET | `/api/v1/events/agenda?from=&to=` | session or token | Occurrences overlapping the range (≤ 400 days), repeats expanded, all-day first then by time |
| PATCH / DELETE | `/api/v1/events/{id}` | session or token | Partial update (`null` clears times / `repeat_until`; moving `start_date` keeps the length) / delete |
| GET | `/api/v1/habits?today=YYYY-MM-DD&days=N` | session or token | Habits (archived too) with check-in dates from the last `days` (1–366, default 28) and the current `streak`. `today` is the caller's local date. |
| POST | `/api/v1/habits` | session or token | `{ id?, name, days? }` (`days`: weekdays 0 = Monday … 6, default all) → `201`; a repeated client id → `409 exists` |
| PATCH / DELETE | `/api/v1/habits/{id}` | session or token | `{ name?, days?, archived? }`; delete removes its check-ins |
| PUT / DELETE | `/api/v1/habits/{id}/checkins/{YYYY-MM-DD}` | session or token | Check a day / clear it → `204`. Idempotent. Dates after the server's UTC date + 1 day are rejected. |
| GET / POST | `/api/v1/inbox` | session or token | List (oldest first), or capture `{ text, id? }` (≤ 2,000 chars; an optional client-chosen 32-hex `id` makes offline replays idempotent: a repeat → `409 exists`) |
| DELETE | `/api/v1/inbox/{id}` | session or token | Discard |
| POST | `/api/v1/inbox/{id}/triage` | session or token | `{ to: task|note|journal, date? }` → `{ kind, task|note|entry }`. Goes through the target module's service; the target is created before the item is removed. |
| GET | `/api/v1/journal?from=&to=` | session or token | Summaries (`date, excerpt, mood`) for a range of at most 366 days, newest first |
| GET / PUT / DELETE | `/api/v1/journal/{YYYY-MM-DD}` | session or token | PUT upserts `{ body?, mood? (1–5 or null), expected_updated_at? }`. A stale version or a same-day create race gets `409 { entry }`. `expected_updated_at: 0` means create only (an existing entry → 409). |
| GET | `/api/v1/notes` | session or token | Summaries `{ id, title, excerpt, pinned, … }`, without bodies. Pinned first, then newest. |
| GET | `/api/v1/notes/{id}/backlinks` | session or token | Summaries of notes whose body has `[[<this note's title>]]` (alias form too; case-insensitive). A LIKE pre-filter over at most 50 candidates, then an exact parse in core. |
| POST | `/api/v1/notes` | session or token | `{ id?, title?, body?, pinned? }` → `201` (optional client id; a repeat → `409 exists`). Body ≤ 100k chars (512 KiB request cap). |
| GET / PATCH / DELETE | `/api/v1/notes/{id}` | session or token | PATCH `title, body, pinned, expected_updated_at?`. A stale version gets `409 { error: "conflict", note }`. |
| GET | `/api/v1/projects[?include=archived]` | session or token | Owner's projects (active only by default) |
| POST | `/api/v1/projects` | session or token | `{ id?, name, description? }` → `201` (optional client id; a repeat → `409 exists`) |
| GET / PATCH / DELETE | `/api/v1/projects/{id}` | session or token | PATCH `name, description, status`. DELETE keeps the project's tasks and unassigns them. |
| GET | `/api/v1/tasks?status=all|open|closed` | session or token | Owner's tasks and subtasks (max 1000) |
| POST | `/api/v1/tasks` | session or token | `{ title, notes?, priority?, due_date?, parent_id?, project_id? }` → `201`. Subtasks are one level deep and can't set `project_id`. |
| GET / PATCH / DELETE | `/api/v1/tasks/{id}` | session or token | PATCH: any of `title, notes, status, priority, due_date, project_id` (`null` clears the due date or project). DELETE cascades to subtasks. |
| GET | `/api/v1/library` | session or token | PDF/ePub files with reading progress (reading, want, unread, finished) |
| PUT / DELETE | `/api/v1/library/{file_id}/progress` | session or token | PUT `{ status?, current_page?, total_pages?, position? }`. Percent is derived from pages; reaching the last page finishes the document. DELETE resets it. |
| GET | `/api/v1/files?folder=` | session or token | `{ folder, folders, files }` from D1 metadata (no R2 list operations) |
| POST | `/api/v1/files?folder=&name=` | session or token | Raw body with `Content-Length` (411 if missing). Streamed to R2 at `u/<owner>/f/<id>`. Limits: 95 MiB per file (413), owner quota (507; default 1 GiB, `DIRECTORY_QUOTA_BYTES`), duplicate name (409). |
| GET | `/api/v1/files/usage` | session or token | `{ used, quota, max_file }` |
| GET | `/api/v1/files/{id}/content` | session or token | Streamed from R2 as an attachment (RFC 6266 filename), CSP `sandbox`. Supports a single `Range: bytes=…` (206 with `Content-Range`; 416 past the end), so media can seek and downloads resume. |
| PATCH / DELETE | `/api/v1/files/{id}` | session or token | PATCH `{ name?, folder? }` (metadata only). DELETE removes metadata first, then the blob. |

Keys are `segment(/segment)*` over `[A-Za-z0-9._-]`, with no `.`/`..` segments
and at most 512 bytes. Errors are JSON `{ "error": code, "message"? }`.
WebAuthn failures always return the generic `401 authentication_failed`; details
go to the server log only. All API responses are `Cache-Control: no-store`.
JSON bodies are capped at 64 KiB.

## Data

- **D1** migrations live in `migrations/NNNN_name.sql` and are append-only.
  `system_meta.schema_version` must equal `EXPECTED_SCHEMA_VERSION` (now **13**).
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
  - `0010`: `jarvis_plans` (actions and results as JSON; status lifecycle).
  - `0011`: `jarvis_presets`.
  - `0012`: `files` (unique `owner_id, folder, name`).
  - `0013`: `reading_progress` (keyed `owner_id, file_id`; cascades with the file).
  - `0014`: `habits` (weekday bit mask, Monday = bit 0) and `habit_checkins`
    (keyed `habit_id, date`).
  - `0015`: `events` (local dates/times, `repeat` + `repeat_until`).
  - `0016`: `preferences` (one validated JSON document per owner).
  - `0017`: `automations` (trigger JSON, command template, last-run status).
  - `0018`: `playlists` (ordered file ids as JSON).
  - `0019`: `client_errors` (bounded per owner).
  - `0020`: `blob_orphans` (R2 deletes to retry, D-033).
- IndexedDB v3 adds a `media` store: tracks the owner chose to save for
  offline playback (blobs, explicit download; the service worker still never
  caches `/api/*`).
- **R2** bucket `directory-files` is private, with no r2.dev URL or custom domain.
  File content lives at `u/<user_id>/f/<file_id>`; names and folders live in D1.
- **IndexedDB** database `directory` (v2, adds `outbox`) has a `kv` store with cached
  copies for offline use: health, the last verified session user,
  task, project, habit, event, note, and library lists, each opened note (`note:<id>`), and each
  journal month and day (`journal:<YYYY-MM>`, `journal:<date>`). Unresolved
  sync conflicts live at `conflict:<doc>` (D-018). Outbox entries carry an
  optional `doc` key, a `merge` key for coalesced document saves, and a `rev`
  counter. It also
  has a legacy `notes` store (Sprint 00 Scratchpad), which is migrated into the
  account once and then emptied.

## Offline / PWA

- Modules other than Home are `React.lazy` chunks. The service worker
  precaches every build chunk, so each module still opens offline after
  the first visit.

- `vite.config.ts` emits `dist/sw.js` from `sw/service-worker.js`. It injects the
  hashed build files plus shell files as the precache manifest, and a content
  hash as the cache version.
- Navigations are network-first with the cached `/` as fallback. Hashed assets
  are cache-first. `/api/*` is never cached by the service worker.
- Updates: a new SW waits, the app shows "Reload to update", and the page then
  posts `SKIP_WAITING` and reloads on `controllerchange`.
- **Offline edits (Tasks):** changes apply locally and queue in the IndexedDB
  `outbox`, then replay in order when the connection returns (D-017). The
  header shows `sync: N pending`; rejected changes appear as a dismissible
  alert. Task creates accept a client `id` (32 hex; a repeat gives
  `409 exists`).
- **Library offline (D-030):** downloaded documents live in the `media`
  store; reading progress queues as a `library.progress` outbox op.
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

### Readiness checklist (M1 exit)
- [x] `npm run ci` is green: verify, isolated-Worker smoke, and browser e2e.
- [x] Every data route resolves a `Principal` and scopes by `user_id`. The
  only public routes are health, auth setup and login (rate-limited), and
  logout (cookie plus origin check).
- [x] Secrets live only in Worker secrets. `.dev.vars` is gitignored, and
  the export contains no credential material.
- [x] Auth fails closed without `DIRECTORY_RP_ID`, `DIRECTORY_ORIGINS`, or a
  setup token (503).
- [x] Worker size is about 355 KiB gzipped (limit 3 MB).
- [ ] **Owner:** `wrangler login`, approve R2 activation (it asks for a
  payment method), and choose the hostname.
- [ ] After deploy: complete setup, create a device token, run the remote smoke
  test, and check Worker CPU time (Workers Free: 10 ms) in the dashboard,
  especially `/auth/login/verify`.
- [ ] After deploy: take an export from Settings as the first backup.

### Steps

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
