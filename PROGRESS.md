# Progress

Canonical implementation tracker. Newest entries first. Milestones are defined in
[docs/ROADMAP.md](docs/ROADMAP.md).

## Current state (2026-10-22)

- **Milestone:** M1 Private Alpha is **feature-complete locally**. All M1
  slices are done and `npm run ci` is green. The first deploy is **blocked on
  owner approval** (see the readiness checklist in docs/ARCHITECTURE.md ›
  Deployment). **M2 JARVIS Alpha:** command layer ✅, presets ✅, `find` ✅,
  project commands ✅.
  Voice is blocked on owner approval. **M3:** search ✅, files ✅, library ✅,
  offline editing ✅, wiki ✅. **M4:** habits ✅, calendar ✅, dashboard ✅, automations ✅, music ✅.
- **Works locally** (verified by automated tests):
  - Passkey owner setup and sign-in, sessions, sign-out.
  - Device tokens for native clients and scripts.
  - Settings: passkeys and tokens.
  - Tasks with one level of subtasks: status, priority, due date, notes,
    Open/Done/All filters, and a project filter.
  - Offline editing (queued, synced on reconnect) for tasks, projects,
    notes, journal, and inbox capture/discard, with conflict choice for
    notes and journal.
  - Projects: create, rename, describe, archive, delete (tasks are kept),
    progress bars, and a link to the project's tasks.
  - Owner-scoped private file upload, list, download, and delete.
  - Notes saved to the account: autosave, pin, conflict detection across
    devices, and a one-time migration of legacy Scratchpad notes.
  - Wiki: `[[links]]` between notes, create a note from a missing link,
    and backlinks.
  - Habits: weekday schedules, daily check-ins, streaks (offline too).
  - Calendar: month and day views, events (timed, all-day, multi-day,
    repeating), and tasks that are due (offline too).
  - Journal: one entry per day with mood, autosave, month list, and day
    navigation.
  - Inbox: quick capture (on Home and in Inbox) and triage into a task, a
    note, or today's journal.
  - Links between tasks, notes, projects, and journal days, with a
    cross-module picker and deep links.
  - Full JSON data export from Settings, and restore from an export
    (merge-only).
  - Library of PDFs/ePubs with reading progress.
  - JARVIS text commands (tasks, projects, agenda, habit check-ins, notes,
    journal, capture, list, find) with preview,
    approve, results, history, and saved presets.
  - Universal search (`/` from anywhere), plus JARVIS `find`.
  - Automations: when a task is completed or a habit is checked in, run a
    saved command (journal, capture, note, add task).
  - Music: audio files, playlists, a player that keeps playing across
    modules, and tracks saved for offline listening.
  - PWA install and offline relaunch with the saved session.
  - Customizable Home dashboard (today, habits, recent notes, capture,
    status, modules), with the layout saved to the account.
- **Not deployed.** Nothing has run on Cloudflare yet; that needs owner approval
  (R2 activation asks for a payment method) and hostname vars.
- **Design:** no Paper handoff yet. All screens are provisional
  (docs/MODULES.md › Design status).

### Verification commands and latest results

| Command | Result (2026-10-22) |
| --- | --- |
| `npm run verify` (fmt, clippy `-D warnings`, tsc, unit tests, builds, deploy dry run) | ✅ pass |
| `cargo test -p directory-core` | ✅ 110 passed |
| Vitest | ✅ 72 passed |
| `npm run test:handoff` | ✅ 50 passed |
| `npm run smoke` (isolated wrangler dev, software authenticator) | ✅ all checks passed |
| `npm run test:browser` (headless Edge, virtual passkey) | ✅ 27/27 |

### Known issues and technical debt
- **CPU budget unmeasured:** ES256/RS256 verification in WASM has not been
  measured against the Workers Free 10 ms CPU limit. Check after first deploy;
  if needed, switch to `opt-level = 3` or Web Crypto verification.
- `rsa` 0.9 has advisory RUSTSEC-2023-0071 (Marvin timing). It affects
  private-key operations only; we only verify public-key signatures.
- Files: the quota is checked before upload, so concurrent uploads can briefly
  exceed it. A failed blob delete leaves an orphan object (no sweep yet).
- Search uses SQLite `LIKE`, which is case-insensitive for ASCII only, and
  reads whole note and journal bodies for snippets. Move to FTS5 when data
  grows.
- Offline: links, pinning, journal delete, and inbox triage need a
  connection. A new edit queued in the instant between a replay finishing
  and the open editor learning the new version can show a spurious
  conflict (resolved with "keep mine"). A project deleted offline leaves
  its tasks showing the old project until the next sync refresh.
- `wrangler dev` must be restarted after rebuilding `apps/web/dist`.
- The repo lives under OneDrive, which slows down and locks `node_modules` and
  `target`.
- No CI pipeline yet. `npm run verify`, smoke, and browser e2e are CI-ready.
- iOS Safari is not verified on a device.

### Design dependencies
- Paper designs or `docs/DESIGN_HANDOFF.md` for the shell, auth, Tasks, Files,
  Projects, Notes, Journal, Inbox, and Settings.

### Unresolved decisions (need the owner)
- **Blocker for the M1 release:** approve the first Cloudflare deploy. This
  needs `wrangler login`, R2 activation (asks for a payment method; free
  within limits), and the production hostname for
  `DIRECTORY_RP_ID`/`DIRECTORY_ORIGINS`.
- **M2:** which AI provider (if any) JARVIS may call, and its budget. Until
  then, JARVIS uses a deterministic command parser only.
- **M2 voice:** may JARVIS use the browser's speech recognition? In
  Chrome/Edge it sends audio to the vendor's cloud. It would be off by
  default, opt-in per device, and need `Permissions-Policy: microphone=(self)`.
- Whether to move the repo out of OneDrive.

### Next task
1. P5: "forget this device" (optional IndexedDB wipe on sign-out; see
   docs/SECURITY.md open items).
2. Search over events and habits.
3. Measure Worker CPU after the first deploy (owner approval needed).
4. Remaining owner decisions: deploy approval, voice, AI provider, reader
   dependency.

---

## Log

### 2026-10-22 — M5 slice: Lazy-loaded modules
- `App.tsx` loads every module except Home with `React.lazy` inside
  `Suspense`. Initial JS went from 387 KB (111 KB gzip) to about 290 KB
  (including shared chunks); modules are 3–11 KB each and all precached.
- The e2e `setFile` helper waits for its input (modules mount after their
  chunk loads).
- **Tests:** browser e2e 27/27, including the offline relaunch with the
  server stopped (lazy chunks come from the precache).

### 2026-10-22 — M5 slice: Security review
- **docs/SECURITY.md:** the model (owner-only, passkeys, sessions, CSRF,
  tokens, R2 sandboxing, JARVIS and automation limits, input caps,
  headers), what's checked automatically, a review log, and open items.
- **Smoke:**
  - A route sweep reads every route from the Worker router. Non-public
    routes must answer 401 anonymously (also in remote mode), and
    cookie writes from a foreign Origin must get 403.
  - App-shell security headers are checked locally and remotely.
- **Fix:** the CSP had no `media-src`, so music saved offline (a blob: URL)
  couldn't play. Added `media-src 'self' blob:`. The browser e2e now plays
  the saved copy; it failed before the fix.

### 2026-10-22 — M5 slice: Error recovery and diagnostics (D-027)
- **Migration 0019:** `client_errors`. Schema version 19.
- **Core:** `diagnostics` module (clean and cap fields, prune to 200,
  list, clear).
- **Worker:** `/api/v1/diagnostics/errors` (32 KiB limit; logs a warning).
- **Web:**
  - A per-module `ErrorBoundary` (try again / reload app).
  - Global reporting of uncaught errors and unhandled rejections, with
    throttling and dedupe (`lib/errors.ts`).
  - A Settings › diagnostics panel (list, expand stack, refresh, clear).
- **Tests:**
  - core 110; web 72 (throttle, describe).
  - smoke: validate, clean, order, user agent, clear, 401.
  - browser e2e 27/27: date formatting is made to throw so Notes crashes;
    the shell survives, "try again" recovers, and both the crash and an
    unhandled rejection reach diagnostics.

### 2026-10-22 — M5 slice: Accessibility (D-026)
- **Audit:** `scripts/lib/a11y.mjs` (CDP accessibility tree). The browser
  e2e audits all 14 modules: no unnamed controls were found. The audit is
  validated against a planted unnamed button.
- **Contrast:** `lib/theme.test.ts` checks all text tokens in both themes.
  The light accent (4.10:1, below AA) is now `#9a560b`; ok and warn were
  darkened too. Primary-button text went from 4.18:1 to 5.44:1.
- **Keyboard:** inputs keep the focus ring; a "Skip to content" button
  moves focus to `<main>` (tested with real Tab/Enter key events).
- **Fix:** quick dashboard edits could overtake each other (stale render
  plus concurrent PATCHes). Preference changes are now computed from the
  latest state and sent in order.
- **Tests:** web 70, browser e2e 26/26.

### 2026-10-22 — M5 slice: Backup restore (D-025)
- **Core:**
  - `backup` module: `ImportTargets` (the module repositories),
    per-kind import with the same validation as creates, ids and
    timestamps kept, skip-if-present, caps, and an export header check.
  - Exported structs derive `Deserialize`.
  - Test fakes for links, preferences, and music are reachable from other
    modules' tests.
- **Worker:** `POST /api/v1/import/{kind}`, a 4 MiB body limit, and D1
  targets.
- **Web:** `lib/backup.ts` plans the batches (dependency order, subtasks
  after parents, ≤ 100 records and ≤ 1 MB each). Settings › your data ›
  restore shows file contents, progress, and a per-kind summary; it says
  files need re-uploading and that it's safe to run again.
- **Fix:** the offline browser step waited only for "pending" to clear;
  it now also waits for "syncing…" (it was a flaky pass).
- **Tests:**
  - core 109: dependency order, no overwrite, malformed and invalid
    records, owner reassignment, links, preferences, batch and header
    checks.
  - web 56: batch planning.
  - smoke: export, delete, import round trip on D1 (ids, timestamps,
    subtasks, check-ins, links); a second run only skips; 401.
  - browser e2e 25/25: restore a file from Settings.

### 2026-10-22 — Tooling: Claude Code ↔ Codex handoff safeguards
- **Scripts (bash, read-only unless noted):**
  - `scripts/session-start.sh`: resume briefing.
  - `scripts/handoff-check.sh`: size of uncommitted work, 300 lines / 10
    files thresholds, no double-counting; `--strict`, `--hook-json`.
  - `scripts/pre-compact.sh`: warning plus a snapshot in gitignored
    `.handoff/`.
  - `scripts/handoff.sh --to codex|claude`: validation, with append-only
    `--record`.
  - `scripts/test-handoff.sh`: 50 checks in throwaway repos, clean and
    dirty trees; part of `npm run verify`.
- **Claude hooks:** `.claude/settings.json` runs SessionStart, PreCompact,
  and Stop.
- **Docs:** an identical handoff-protocol section in AGENTS.md and
  CLAUDE.md (CLAUDE.md also imports AGENTS.md), and docs/HANDOFF.md
  (switching both ways, safe WIP restoration, HANDOVER.md template).
- Nothing pushes, deploys, resets, commits, or stashes automatically.

### 2026-10-22 — M4 slice: Music
- **Migration 0018:** `playlists`. Schema version 18.
- **Core:** `music` module: audio detection (type or extension), library,
  playlists validated against your audio files (≤ 500 tracks, ≤ 100
  playlists). Deleted files are skipped on read. Playlists are included in
  the export.
- **Worker:** `/api/v1/music` and playlist routes. Playback streams from
  file content with Range support.
- **Web:**
  - Music screen: upload into `Music`, play all or one track, add to a
    playlist, save or remove offline.
  - Playlists: create, play, reorder, remove, delete.
  - App-wide `PlayerProvider` and bar: play/pause, next/previous, seek,
    and Media Session keys. It prefers saved copies.
  - IndexedDB v3 `media` store.
- **Fix:** the Home notes widget no longer runs the legacy-notes migration
  (a race hid its message on the Notes screen).
- **Tests:**
  - core 106; web 53 (media store, music helpers).
  - smoke: audio listing, playlist validation and order, deleted tracks
    skipped, export, 401.
  - browser e2e 24/24: upload a generated WAV, it plays, save offline,
    add to a playlist. The test browser allows autoplay
    (`--autoplay-policy`).

### 2026-10-22 — Files: HTTP Range support
- **Core:** `files::parse_range` (single range, suffix ranges, clamping;
  malformed, other units, and multi-range headers are ignored).
- **Worker:** file content answers `206` with `Content-Range` from an R2
  ranged read, `416` past the end, and always sends `Accept-Ranges: bytes`.
- **Tests:** core 105, smoke (prefix, suffix, and unsatisfiable ranges on
  local R2).

### 2026-10-22 — M4 slice: Automations (D-024)
- **Migration 0017:** `automations`. Schema version 17.
- **Core:**
  - `automations` module: rules (≤ 20) with triggers (task completed,
    optional project; habit checked, optional habit) and command templates.
  - Templates are parsed, then filled (injection-safe); only additive
    commands are allowed; `fire` records each rule's last run.
  - `JarvisService::run_unattended`. `ActionResult.changed` (not stored)
    marks state changes.
  - `HabitService::set_check` returns whether anything changed.
- **Worker:**
  - `/api/v1/automations` CRUD.
  - `fire` hooks in task update (open → done), habit check-in (new only),
    and JARVIS execute (changed completions and check-ins).
  - `?today=` local date on those routes.
- **Web:** an Automations panel on the JARVIS screen (create with trigger,
  filter, and command; on/off; delete; last-run status). Task updates and
  JARVIS execute send `today`.
- **Tests:**
  - core 104: templates, restrictions, injection, matching, end-to-end
    fire with a failing rule.
  - smoke: validation, fires once from routes and JARVIS, no injection,
    disabled rules, 401.
  - browser e2e 23/23: create a rule, it runs, its status shows, delete.

### 2026-10-22 — M4 slice: Customizable dashboard (D-023)
- **Migration 0016:** `preferences`. Schema version 16.
- **Core:** `preferences` module (typed document, validated widgets,
  lenient reads, partial PATCH with `null` reset), included in the export.
- **Worker:** `GET`/`PATCH /api/v1/preferences`.
- **Web:**
  - Home is now a dashboard of widgets: capture, today (events and due or
    overdue tasks, completable), habits (check off), recent notes, system
    status (with offline readiness), and modules.
  - Customize mode reorders, hides, adds, and resets widgets.
    `usePreferences` caches the layout and queues changes offline.
- **Tests:**
  - core 100: validation, reset, lenient reads.
  - web 51: layout helpers.
  - smoke: validate, patch, reset, export, 401.
  - browser e2e 22/22: hide and move widgets, persisted on the server
    and across a reload, then reset.

### 2026-10-22 — Hardening: no silent list truncation (D-022)
- **Bug:** the tasks list was `ORDER BY created_at LIMIT 1000`. Past 1,000
  tasks in total (including completed), new tasks would silently stop
  appearing. Other lists had the same latent problem at their limits.
- **Fix:**
  - Tasks list every open task plus the 500 most recently closed;
    creating more than 2,000 open tasks is refused.
  - Notes (2,000), events (2,000), projects (500), inbox (500), habits
    (200), and files per folder (1,000) refuse creates past their caps.
    Each repository gains a `count` / `count_open` / `count_in_folder`;
    D1 has a shared `count` helper.
  - The Tasks UI notes when Done/All shows only recent completions.
- **Tests:** core 98 (task list composition and cap, habit cap per owner,
  folder cap on upload and move).

### 2026-10-22 — M2 slice: JARVIS agenda and habit check-ins
- **Refactor:** `jarvis::CommandTargets` is now a trait with associated
  repository types instead of a struct with seven type parameters. The
  Worker's `Targets` and the test fixture implement it, and the service
  functions take `&C: CommandTargets`.
- **Commands:**
  - `Agenda { date }` (read): events (expanded), tasks due, overdue count,
    and habits scheduled but not checked.
  - `CheckHabit` (write): resolved by habit name; the result reports the
    streak.
  - Parser: `agenda [date]`, `what's on [date]`, `schedule [for] date`,
    `check in|check off <habit> [today|yesterday|YYYY-MM-DD]`.
- **Tests:** core 95 (parser cases; agenda message; check-in with streak;
  unresolved habit), smoke (agenda + check-in on D1).

### 2026-10-22 — M4 slice: Calendar (D-021)
- **Migration 0015:** `events`. Schema version 15.
- **Core:** `calendar` module:
  - Validation: dates, `HH:MM` times, end ≥ start, at most 366 days long.
  - Repeat expansion (`occurrence_starts`, `occurrences`) and `agenda`
    (≤ 400 days).
  - Create (client ids), partial update (moving the start keeps the
    length), and delete. Events are included in the export.
- **Worker:** `/api/v1/events` (list, create, agenda, patch, delete) and
  the D1 repository.
- **Web:**
  - `lib/calendar.ts` mirrors the expansion and builds a Monday-first
    month grid.
  - `useEvents` runs changes through the outbox, so they work offline.
  - Calendar module: month grid with events and due tasks, a day panel,
    and an event form (all-day/timed, multi-day, repeat + until, location,
    notes, delete).
- **Tests:**
  - core 93: expansion edge cases (31st, Feb 29), sorting, validation,
    move, scope, idempotency.
  - web 49: expansion parity, month grid, event ops.
  - smoke: validation, weekly agenda, move, all-day, export, 401, delete.
  - browser e2e 21/21: add an event today, see it in the cell and day,
    then delete it.

### 2026-10-22 — M4 slice: Habits (D-020)
- **Migration 0014:** `habits` and `habit_checkins`. Schema version 14.
- **Core:** `habits` module: create (client ids), update, archive, delete,
  idempotent `set_check`, list with a check-in window and streaks. Habits
  are included in the export with every check-in date.
- **Worker:** `/api/v1/habits` routes and the D1 repository. Delete removes
  check-ins explicitly.
- **Web:**
  - Habits module: add with day picker, a last-7-days check-in row,
    streak, rename/reschedule, archive/restore, and delete.
  - `useHabits` runs everything through the outbox, so it works offline.
    `lib/habits.ts` mirrors the streak rule.
  - Module registry: offline flags corrected (inbox, tasks, projects,
    notes, and journal work offline).
- **Tests:**
  - core 90: streak rules, lifecycle, validation, idempotency, export.
  - web 44: streak parity, toggles, habit ops replay.
  - smoke: validation, idempotent check-ins, streak, archive, export, 401,
    delete.
  - browser e2e 20/20: add a habit, check off today, streak on the server.

### 2026-10-22 — M2 slice: JARVIS project commands
- **Core:**
  - `CreateProject`, and `ListProjects`. Progress counts top-level,
    non-cancelled tasks, as on the Projects screen.
  - `CreateTask` gains an optional project (`project`/`project_id`). Serde
    defaults keep stored plans readable.
  - The project is resolved at plan time: an exact match first, then a
    unique partial match. If several match, the candidates are listed.
  - A project created by an earlier step of the same plan is resolved by its
    exact name when the plan runs.
  - `CommandTargets` gains `projects`, with shared `pick`/`ambiguity` helpers.
- **Parser:** `new project <name>`, `list projects`, and
  `add task <title> in|to|for project <name> [due …]`. The date may come
  before or after the project clause.
- **Tests:**
  - core 87: parser clauses, resolution, forward reference.
  - smoke: a project and its task created in one plan on D1, progress
    listing, and an unresolved project returning 422.

### 2026-10-22 — M3 slice: Wiki links and backlinks (D-019)
- **Core:** `notes::wiki_links` (targets of `[[Title]]` / `[[Title|label]]`),
  `display_title` (shared with summaries), `NoteService::backlinks`, and
  `NoteRepository::body_like` (LIKE pre-filter, exact check in core).
- **Worker:** `GET /api/v1/notes/{id}/backlinks`.
- **Web:** `lib/wiki.ts` (same parsing; resolves against the cached notes
  list, so outgoing links work offline). The note editor's wiki section
  opens linked notes, creates missing ones (titled, offline too), and lists
  backlinks (cached per note).
- **Tests:** core 85, web 39, smoke (backlinks with alias/case, near misses
  excluded, 404), browser e2e 19/19 (create from link, follow backlink).

### 2026-10-22 — M3 slice: Offline notes and journal (D-018)
- **Core:** `NewNote.id` (client id; `NoteError::Exists` → 409). Journal
  `expected_updated_at: 0` = create only (tested).
- **Web:**
  - `db.ts`: outbox entries gain `doc`/`merge`/`rev`; `enqueueMerged`,
    `pendingFor`, `settle` (rebase if edited mid-flight), `deleteValue`.
  - `sync.ts`: `note.create/save/delete` and `journal.save` ops; replay
    reports new versions and conflicts.
  - `useSync` stores conflicts (`conflict:<doc>`), alerts, re-runs when work
    arrives mid-replay, and sends versions/conflicts with `SYNCED_EVENT`.
  - `useAutosaveDoc` queues offline (status "saved on this device"),
    restores queued edits and stored conflicts on open, and `discard()`
    before delete.
  - Notes and Journal stay editable offline; offline note create/delete.
- **Fix:** the journal deep-link regex had lost its backslashes
  (`/^d{4}-…/`), so `#/journal?date=` never matched. Likely cause: shell heredoc
  edits lose backslashes in this environment (reproduced this session), so
  edit code containing `\` with the Edit tool.
- **Tests:** core 83, web 37 (coalescing, versions, conflicts, mid-flight
  rebase), smoke (note client id, journal create-only), browser e2e 18/18
  (offline note and journal edits sync; a conflicting journal edit from
  another device surfaces; keep mine wins; editing with the server down).

### 2026-10-22 — M3 slice: Offline Projects
- **Core:** `NewProject.id` (optional client id; `ProjectError::Exists` →
  409), with a unit test.
- **Web:** `project.create/update/delete` outbox ops and
  `applyProjectLocally`. `useProjects` uses `runOrQueue` and reloads after
  sync. The Projects screen stays editable offline; links stay online-only.
- **Tests:** core 82, web 34, smoke (project client-id replay → 409),
  browser e2e 18/18 (the offline step also creates a project, then deletes
  it through the UI after sync).

### 2026-10-22 — M3 slice: Offline Inbox capture
- **Core:** `Capture.id` (optional client id; `InboxError::Exists` → 409).
- **Web:**
  - `hooks/offline.ts`: shared `runOrQueue` (server first, else apply
    locally and queue). `useTasks` was refactored onto it.
  - `lib/sync.ts`: `inbox.capture` / `inbox.delete` ops and
    `applyInboxLocally`.
  - `useInbox`: capture and discard queue offline; triage stays online-only.
    The Inbox and Home capture boxes stay enabled offline.
- **Tests:** core 81, web 33 (inbox replay test), smoke (inbox client-id
  replay → 409, discard replay → 404), browser e2e 18/18 (the offline step
  now also captures to the inbox and checks it reaches the server).

### 2026-10-22 — M3 slice: Offline edits for Tasks
- **Core:** `NewTask.id` (an optional client id; `TaskError::Exists` → 409).
- **Web:**
  - IndexedDB v2 with an `outbox` store (a version change closes old
    connections).
  - `lib/sync.ts`: replay with idempotency rules, plus `applyLocally` and
    `clientId`.
  - `useSync`: replays on load, on reconnect, and on enqueue; drives the
    pending chip and rejected alerts.
  - `useTasks` queues offline (or when earlier changes are queued) and
    reloads after sync. Tasks stay editable offline.
- **Tests:** web 32 (6 sync tests with fake-indexeddb), smoke 29/29
  (idempotent create), browser e2e 18/18 (offline via DevTools emulation:
  queue → reconnect → on server).

### 2026-10-22 — M3 slice: Document library
- **Core:** `library` module: documents are files by type or extension.
  Progress covers status, pages, a derived percent, auto reading/finished,
  and re-reading. The export includes `reading_progress`.
- **Worker:** migration `0013_reading_progress.sql` (schema v13),
  `d1/library.rs` (files via the files repository), `/api/v1/library`.
- **Web:** Library module (status and progress editor, progress bars,
  "currently reading"). PDFs open via a blob URL in a new tab; ePubs
  download.
- **Tests:** core 79, smoke 28/28, browser e2e 17/17.

### 2026-10-21 — M3 slice: Private file management
- **Core:** a `files` module (folder and name validation, quota 1 GiB
  default, 95 MiB per-file cap, metadata-only rename and move, an RFC 6266
  `content_disposition`). The legacy `storage` module was removed, and export
  now lists file metadata.
- **Worker:** migration `0012_files.sql` (schema v12) and `d1/files.rs`.
  `r2.rs` is now `R2Blobs`. `/api/v1/files` streams uploads from the request
  body into R2 and downloads via the runtime (no buffering). The
  `/api/v1/storage/*` routes were removed. `DIRECTORY_QUOTA_BYTES` var is
  optional.
- **Web:** Files module with breadcrumbs (`#/files?folder=`), new folder,
  multi/drag-drop upload, a usage bar, rename/move/delete, and download links.
- **Tests:** core 76, web 26, smoke 27/27, browser e2e 16/16.
- **Docs:** ARCHITECTURE (files API, data), MODULES, ROADMAP.

### 2026-10-21 — Universal search (+ JARVIS find)
- **Core:** `search` module: validation, LIKE escaping, Unicode-safe
  snippets, ranking (title match, then recency), and a `SearchSource` trait.
  JARVIS gains a read-only `Find` command (parser: find, search, look up),
  and its targets include a search source.
- **Worker:** `d1/search.rs` (per-kind escaped LIKE; derived note titles),
  `GET /api/v1/search`.
- **Web:** Search module (debounced, highlighted, linkable `#/search?q=`) and
  a global `/` shortcut. `lib/search.ts` holds the types (no lib → module
  imports).
- **Tests:** 4 core tests, a smoke search check (cross-kind ranking,
  literal `%`/`_`, JARVIS find), and a browser e2e search step.
- **Voice input** is deferred pending owner approval (privacy of browser
  speech services).

### 2026-10-21 — M2 slice: JARVIS presets
- **Core:** `jarvis::presets` (name and input validation, dry-parse on
  save, a 50-per-owner limit).
- **Worker:** `d1/presets.rs` and preset routes. Migration
  `0011_jarvis_presets.sql` (schema v11).
- **Web:** preset chips (run → preview → approve), delete, and "save as
  preset" on current and history plans.
- **Tests:** 2 core tests, a smoke presets check, and a browser e2e
  (save → chip → preview → cancel).

### 2026-10-21 — M2 slice: JARVIS command layer
- **Core:**
  - `jarvis` module: `Command` enum with risk and summaries, the
    `IntentProvider` trait, and `DeterministicParser`. The grammar covers
    tasks with due dates (today, tomorrow, weekdays, ISO), complete-by-name,
    note, journal, capture, list, and `;`/`then` chaining; it's ASCII-safe
    for any Unicode input.
  - `JarvisService`: plan, resolve, execute (claim), cancel, and history.
  - `JournalService::append` is shared with the inbox. New date arithmetic
    in `validate`.
- **Worker:** `d1/jarvis.rs` (JSON columns; atomic claim), `http/jarvis.rs`.
  Migration `0010_jarvis.sql` (schema v10).
- **Web:** the JARVIS console (preview cards, approve/cancel, results with
  deep links, example chips, history).
- **Tests:** 11 new core tests (parser and service), a smoke JARVIS check, and
  a browser e2e JARVIS step. Two e2e timing issues fixed (uppercase
  `innerText`, a remounted editor).

### 2026-10-21 — M1 exit review
- **Security pass:** every non-public handler resolves a `Principal`
  (audited mechanically); the public set is health, auth setup/login
  (throttled), and logout.
- **CI:** `npm run ci` runs verify, smoke, and browser e2e as one gate. The
  browser e2e `fill` now waits for editors to mount (fixed a flaky step).
- **Docs:** a deploy readiness checklist in docs/ARCHITECTURE.md; ROADMAP
  marks M1 feature-complete.

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
