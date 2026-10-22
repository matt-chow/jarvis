# Progress

Canonical implementation tracker. Newest entries first. Milestones are defined in
[docs/ROADMAP.md](docs/ROADMAP.md).

## Current state (2026-10-21)

- **Milestone:** M1 Private Alpha, in progress. Auth ✅, rate limiting ✅,
  Tasks ✅, Projects ✅, Notes ✅, Journal ✅, Inbox ✅, Links ✅, Export ✅.
  Next: **M1 exit review**.
- **Works locally** (verified by automated tests):
  - Passkey owner setup and sign-in, sessions, sign-out.
  - Device tokens for native clients and scripts.
  - Settings: passkeys and tokens.
  - Tasks with one level of subtasks: status, priority, due date, notes,
    Open/Done/All filters, a project filter, and a read-only offline copy.
  - Projects: create, rename, describe, archive, delete (tasks are kept),
    progress bars, and a link to the project's tasks.
  - Owner-scoped private file upload, list, download, and delete.
  - Notes saved to the account: autosave, pin, conflict detection across
    devices, and a one-time migration of legacy Scratchpad notes.
  - Journal: one entry per day with mood, autosave, month list, and day
    navigation.
  - Inbox: quick capture (on Home and in Inbox) and triage into a task, a
    note, or today's journal.
  - Links between tasks, notes, projects, and journal days, with a
    cross-module picker and deep links.
  - Full JSON data export from Settings.
  - PWA install and offline relaunch with the saved session.
  - Health dashboard.
- **Not deployed.** Nothing has run on Cloudflare yet; that needs owner approval
  (R2 activation asks for a payment method) and hostname vars.
- **Design:** no Paper handoff yet. All screens are provisional
  (docs/MODULES.md › Design status).

### Verification commands and latest results

| Command | Result (2026-10-21) |
| --- | --- |
| `npm run verify` (fmt, clippy `-D warnings`, tsc, unit tests, builds, deploy dry run) | ✅ pass. Worker upload 1,070 KiB / 355 KiB gzipped |
| `cargo test -p directory-core` | ✅ 60 passed |
| Vitest | ✅ 26 passed (9 files) |
| `npm run smoke` (isolated wrangler dev, software authenticator) | ✅ 24/24 |
| `npm run test:browser` (headless Edge 155, virtual passkey) | ✅ 14/14 |

### Known issues and technical debt
- **CPU budget unmeasured:** ES256/RS256 verification in WASM has not been
  measured against the Workers Free 10 ms CPU limit. Check after first deploy;
  if needed, switch to `opt-level = 3` or Web Crypto verification.
- `rsa` 0.9 has advisory RUSTSEC-2023-0071 (Marvin timing). It affects
  private-key operations only; we only verify public-key signatures.
- Files: whole bodies are buffered (10 MB cap); no folders, metadata table, or
  pagination cursor.
- Offline editing (tasks, notes, projects) is read-only until M3 sync; edits
  made while offline are not queued.
- `wrangler dev` must be restarted after rebuilding `apps/web/dist`.
- The repo lives under OneDrive, which slows down and locks `node_modules` and
  `target`.
- No CI pipeline yet. `npm run verify`, smoke, and browser e2e are CI-ready.
- iOS Safari is not verified on a device.

### Design dependencies
- Paper designs or `docs/DESIGN_HANDOFF.md` for the shell, auth, Tasks, Files,
  Projects, Notes, Journal, Inbox, and Settings.

### Unresolved decisions (need the owner)
- Approve the first Cloudflare deploy: R2 activation and the production
  hostname for `DIRECTORY_RP_ID`/`DIRECTORY_ORIGINS`.
- Whether to move the repo out of OneDrive.

### Next task
**M1 exit review**, then M2 (JARVIS) foundations, which don't depend on
deployment:
- A deploy readiness checklist, plus a single `npm run ci` gate (verify +
  smoke + browser e2e).
- A security pass over M1 (authz on every route, input limits, headers).
- Then M2: a command registry with typed, validated commands per module, and
  text commands with a deterministic parser (no AI provider without approval).

---

## Log

### 2026-10-21 — M1 slice: Data export
- **Core:** `export` module: versioned document, an `ExportSource` trait,
  `LinkRecord`, and a test asserting no owner or credential fields.
- **Worker:** `D1ExportSource` composes the existing repositories
  (`all()` added for notes, journal, and links). `GET /api/v1/export` is
  served as an attachment.
- **Web:** a Settings "export my data" download.
- **Tests:** a core export test, a smoke export check (completeness and a
  secret scan), and a browser export step.

### 2026-10-21 — M1 slice: Entity linking
- **Core:** `links` module: `EntityRef` (task, note, project, journal day),
  normalized pairs, idempotent symmetric linking, both ends validated as
  existing and owned, and lazy cleanup of dangling links.
- **Worker:** `d1/links.rs` (titles resolved per kind; note titles via the
  core summary derivation), `http/links.rs`. Migration `0009_links.sql`
  (schema v9).
- **Web:** `LinkedSection` (list, unlink, and a picker over tasks, notes,
  projects, and dates) in all four editors. Deep links with `useHashParam` in
  Tasks, Notes, Journal, and Projects.
- **Tests:** 3 core link tests, web route and filter tests, a smoke links
  check, and a browser e2e step (picker, deep link, back-link).

### 2026-10-21 — M1 slice: Universal inbox
- **Core:** `inbox` module: capture validation, and triage into a task
  (first line becomes the title, the rest becomes notes, nothing dropped), a
  note, or a journal append, via the target module services (D-015). Notes and
  journal in-memory repositories are shared for tests.
- **Worker:** `d1/inbox.rs`, `http/inbox.rs` (triage builds task, note, and
  journal repositories). Migration `0008_inbox.sql` (schema v8).
- **Web:** `useInbox` hook, a `CaptureBox` component (Enter to capture), the
  Inbox module with triage actions and result links, and a Home capture panel
  with the inbox count. The Sprint 00 welcome copy was removed.
- **Tests:** 4 core inbox tests, a smoke inbox check (all three targets plus
  validation), and a browser e2e capture-and-triage step.

### 2026-10-21 — M1 slice: Journal
- **Core:** `journal` module: entries keyed by `(owner, date)`, mood 1–5,
  upsert with optimistic concurrency, a create race resolved as conflict, and
  range validation (≤ 366 days, leap-aware).
- **Worker:** `d1/journal.rs` (`ON CONFLICT DO NOTHING` insert, conditional
  update), `http/journal.rs`, and routes. Migration `0007_journal.sql`
  (schema v7).
- **Web:**
  - New `useAutosaveDoc` hook (shared load, offline fallback, debounced
    versioned saves, conflicts, flush on unmount), adopted by Notes and
    Journal, plus a shared `ConflictBanner`.
  - Journal module: month list with mood meter, day editor, mood picker, and
    day and month navigation.
- **Tests:** 4 core journal tests, web date tests, a smoke journal check, and
  a browser e2e Journal step.

### 2026-10-21 — M1 slice: Notes
- **Core:** `notes` module: summaries with derived titles and excerpts,
  validation (100k chars), and optimistic concurrency (`NoteError::Conflict`
  carries the winner; versions strictly increase).
- **Worker:**
  - `d1/notes.rs` (list returns only `substr(body, 1, 400)`; conditional
    `update_if`) and `http/notes.rs` (409 body includes the current note).
  - New `read_json_limited` (512 KiB for notes).
  - Migration `0006_notes.sql` (schema v6).
- **Web:**
  - Notes module rewritten for the server: list, editor with 800 ms debounced
    autosave and save-state indicator, flush on switch, pin, delete, a
    conflict banner ("use theirs" / "keep mine"), and an offline read-only
    cache per note.
  - `useNotes` migrates legacy IndexedDB Scratchpad notes once.
  - `ApiError.body`; `useCachedList` now returns a stable `reload`.
- **Tests:** 3 core notes tests, web summary tests, a smoke notes check
  (including 409), and browser e2e for legacy migration, autosave, and offline
  cached notes and tasks.

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
