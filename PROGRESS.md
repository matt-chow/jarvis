# Progress

Canonical implementation tracker. Newest entries first. Milestones are defined in
[docs/ROADMAP.md](docs/ROADMAP.md).

## Current state (2026-10-21)

- **Milestone:** M1 Private Alpha, in progress. Auth slice ✅. Next: **Tasks**.
- **Works locally** (verified by automated tests):
  - Passkey owner setup and sign-in, sessions, sign-out.
  - Device tokens for native clients and scripts.
  - Settings: passkeys and tokens.
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
| `npm run verify` (fmt, clippy `-D warnings`, tsc, unit tests, builds, deploy dry run) | ✅ pass. Worker upload 795 KiB / 271 KiB gzipped |
| `cargo test -p directory-core` | ✅ 33 passed |
| Vitest | ✅ 14 passed (4 files) |
| `npm run smoke` (isolated wrangler dev, software authenticator) | ✅ 17/17 |
| `npm run test:browser` (headless Edge 155, virtual passkey) | ✅ 8/8 |

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
- Paper designs or `docs/DESIGN_HANDOFF.md` for the shell, auth, Files,
  Settings, and the upcoming Tasks, Projects, Inbox, Journal, and Notes screens.

### Unresolved decisions (need the owner)
- Approve the first Cloudflare deploy: R2 activation and the production
  hostname for `DIRECTORY_RP_ID`/`DIRECTORY_ORIGINS`.
- Whether to move the repo out of OneDrive.

### Next task
**Tasks slice (M1).**
- Core `tasks` module: model, validation, status transitions, and a repository
  trait with an in-memory test double.
- D1 migration `0003_tasks`.
- `/api/v1/tasks` CRUD, owner-scoped.
- Tasks UI: list, create, edit, complete, delete, with loading, empty, and error
  states.
- Smoke and browser e2e steps.
- Projects and subtasks build on this.

---

## Log

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
