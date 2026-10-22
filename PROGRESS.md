# Progress

Canonical implementation tracker. Newest entries first. Milestones are defined in
[docs/ROADMAP.md](docs/ROADMAP.md).

## Current state (2026-10-21)

- **Milestone:** M1 Private Alpha, in progress. Auth ✅, rate limiting ✅,
  Tasks ✅, Projects ✅. Next: **Notes (server-backed)**.
- **Works locally** (verified by automated tests):
  - Passkey owner setup and sign-in, sessions, sign-out.
  - Device tokens for native clients and scripts.
  - Settings: passkeys and tokens.
  - Tasks with one level of subtasks: status, priority, due date, notes,
    Open/Done/All filters, a project filter, and a read-only offline copy.
  - Projects: create, rename, describe, archive, delete (tasks are kept),
    progress bars, and a link to the project's tasks.
  - Owner-scoped private file upload, list, download, and delete.
  - Device-local Scratchpad.
  - PWA install and offline relaunch with the saved session.
  - Health dashboard.
- **Not deployed.** Nothing has run on Cloudflare yet; that needs owner approval
  (R2 activation asks for a payment method) and hostname vars.
- **Design:** no Paper handoff yet. All screens are provisional
  (docs/MODULES.md › Design status).

### Verification commands and latest results

| Command | Result (2026-10-21) |
| --- | --- |
| `npm run verify` (fmt, clippy `-D warnings`, tsc, unit tests, builds, deploy dry run) | ✅ pass. Worker upload 886 KiB / 298 KiB gzipped |
| `cargo test -p directory-core` | ✅ 45 passed |
| Vitest | ✅ 20 passed (6 files) |
| `npm run smoke` (isolated wrangler dev, software authenticator) | ✅ 19/19 |
| `npm run test:browser` (headless Edge 155, virtual passkey) | ✅ 10/10 |

### Known issues and technical debt
- **CPU budget unmeasured:** ES256/RS256 verification in WASM has not been
  measured against the Workers Free 10 ms CPU limit. Check after first deploy;
  if needed, switch to `opt-level = 3` or Web Crypto verification.
- `rsa` 0.9 has advisory RUSTSEC-2023-0071 (Marvin timing). It affects
  private-key operations only; we only verify public-key signatures.
- Files: whole bodies are buffered (10 MB cap); no folders, metadata table, or
  pagination cursor.
- Scratchpad is device-local only, pending the Notes slice.
- `wrangler dev` must be restarted after rebuilding `apps/web/dist`.
- The repo lives under OneDrive, which slows down and locks `node_modules` and
  `target`.
- No CI pipeline yet. `npm run verify`, smoke, and browser e2e are CI-ready.
- iOS Safari is not verified on a device.

### Design dependencies
- Paper designs or `docs/DESIGN_HANDOFF.md` for the shell, auth, Tasks, Files,
  Projects, Settings, and the upcoming Inbox, Journal, and Notes screens.

### Unresolved decisions (need the owner)
- Approve the first Cloudflare deploy: R2 activation and the production
  hostname for `DIRECTORY_RP_ID`/`DIRECTORY_ORIGINS`.
- Whether to move the repo out of OneDrive.

### Next task
**Notes slice (M1): server-backed notes that replace the device-only Scratchpad.**
- Core `notes` module: title (optional, derived from the first line), Markdown
  body (≤ 100 KB), pinned, and timestamps. Owner-scoped repository.
- Migration `0006`: `notes`.
- `/api/v1/notes` CRUD with a list that returns summaries (no body), sorted by
  `updated_at`.
- Web: a Notes module using the API, with debounced autosave and save-state
  indicators. Migrate existing device-local Scratchpad notes into the account
  (one-time, on first load, then clear them locally). Read-only offline cache.
- Then: Journal, Inbox, Linking.

---

## Log

### 2026-10-21 — M1 slice: Projects
- **Core:**
  - `projects` module (validation, archive, owner scoping).
  - Tasks gain `project_id`: top-level only, checked against owned projects
    via `TaskRepository::project_exists`, clearable by PATCH `null`.
- **Worker:** `d1/projects.rs` (delete unassigns tasks in the same batch),
  `http/projects.rs`, and a shared `auth::principal` guard. Migration
  `0005_projects.sql` (schema v5).
- **Web:**
  - Generic `useCachedList` hook (server list + offline cache), now used by
    `useTasks` and `useProjects`.
  - Projects module with progress bars.
  - Tasks: a project filter in the URL (`#/tasks?project=`), quick add into
    the filtered project, and a project picker in the detail panel.
  - The phone tab bar now scrolls horizontally.
- **Tests:** core project tests and task-project tests, web progress and hash
  tests, a smoke projects check, and a browser e2e Projects step.

### 2026-10-21 — M1 slice: Tasks
- **Core:**
  - `tasks` module: model, status/priority enums, `completed_at` bookkeeping,
    and one-level subtasks validated against the owner.
  - `validate` helpers (line, text, `YYYY-MM-DD` date, PATCH double-option).
  - `Entropy` moved to a shared `entropy` module with `new_id`/`is_id`.
- **Worker:** `d1/tasks.rs` (explicit cascade delete), `http/tasks.rs`, and
  routes. Migration `0004_tasks.sql` (schema v4).
- **Web:**
  - `lib/tasks.ts` (sorting, due labels, tree) with tests.
  - `useTasks` hook (server state with an IndexedDB read-only cache).
  - `modules/Tasks.tsx`: quick add, filters, completion checkbox, a detail
    editor (title, status, priority, due date, notes, subtasks), and a delete
    confirmation.
  - Shared `inputClass`.
- **Tests:** 9 new core tests, 4 web tests, a smoke task check, and a browser
  e2e Tasks step. Optional `SCREENSHOT_DIR` screenshots in the browser e2e.

### 2026-10-21 — Security: auth rate limiting
- Core `rate_limit` (fixed window, `RateLimitStore` trait).
- D1 store with an atomic upsert and purge (migration 0003, schema v3).
- Setup and login endpoints return 429 with `Retry-After` after 30 requests per
  IP per 10 minutes.
- Smoke check added; the sign-in screen shows a friendly message.

### 2026-10-21 — M1 slice: owner identity (passkeys)
- **Core:**
  - `identity` module: WebAuthn verification (ES256/RS256, UV required,
    sign-count clone detection), single-use purpose-bound challenges, sessions
    (30-day sliding, 90-day cap), and device tokens. Storage is SHA-256 only.
  - `IdentityRepository` and `Entropy` traits.
  - `encoding` helpers.
  - Storage is now owner-namespaced (`u/<user_id>/`).
  - Removed the shared-token `auth` module.
- **Worker:**
  - `d1/identity.rs` repository with an atomic owner create; `entropy.rs` uses
    Web Crypto.
  - `http/` split into `auth`, `storage`, and `health`, with the `authenticate`
    guard (bearer token > cookie, plus an Origin check on cookie writes).
  - Auth routes; device tokens can't manage credentials.
  - Migration `0002_identity.sql` (schema v2).
- **Web:**
  - `lib/webauthn.ts`; `Api` client (cookie or bearer).
  - `useSession`, which falls back to an offline cached session and re-checks on
    401.
  - `AuthScreen` (setup and sign-in) and the `Settings` module (passkeys,
    tokens, sign-out).
  - Files now use the session.
- **Tests:**
  - 16 new core tests: WebAuthn vectors including RS256, service flows, and
    storage isolation.
  - Web tests for the API client and WebAuthn conversions.
  - `smoke.mjs` rewritten around an isolated dev server and a software
    authenticator.
  - New `browser-e2e.mjs` with a CDP virtual authenticator (replaces
    `offline-check.mjs`).
- **Config:** `DIRECTORY_SETUP_TOKEN` secret, plus `DIRECTORY_RP_ID` and
  `DIRECTORY_ORIGINS` vars. `DIRECTORY_API_TOKEN` was removed.
- **Docs:** ARCHITECTURE (identity, API, deploy), DECISIONS D-009 to D-013,
  ROADMAP milestones, MODULES design status, AGENTS Definition of Done.

### 2026-10-21 — Audit of Sprint 00
- **Verified:** `npm run verify` passes.
- **Locally verified:** D1 migrations, the R2 proof of concept, and PWA offline.
- **Scaffolded:** auth was a shared token.
- **Unverified:** deployment.
- **Docs:** moved to `docs/`. Committed the baseline (`5c523d4`).

### 2026-10-21 — Sprint 00: Infrastructure foundation
- Monorepo, React/Vite/Tailwind PWA shell, portable Rust core with a Worker
  adapter, health endpoint, D1 migration 0001, R2 proof of concept, service
  worker and offline shell, smoke and offline checks, doc set.
