# Decisions

Architecture decision log. Add new entries at the bottom; supersede entries
rather than editing them.

## D-001 — One Worker for UI and API (modular monolith)
**Status:** accepted (Sprint 00)
A single Worker uses Static Assets for the PWA, and `run_worker_first` routes only
`/api/*` to Rust. This means one deploy, one origin (no CORS, simpler auth
cookies later), and static traffic that doesn't count against Worker requests.
Pages and separate API Workers were rejected as unnecessary services.

## D-002 — Rust core crate separate from the Workers adapter
**Status:** accepted
`directory-core` has no Cloudflare dependency and exposes async traits
(`ObjectStore`, `SystemRepository`). `directory-worker` implements them with
`workers-rs` 0.8 (built by `worker-build`). Core is tested natively with plain
`cargo test` and is reusable by Windows and iOS clients. The traits don't require
`Send`, because Workers is single-threaded.

## D-003 — No PWA framework; hand-written service worker
**Status:** accepted
A ~20-line Vite plugin injects the precache list into `sw/service-worker.js`.
This avoids Workbox and `vite-plugin-pwa` dependencies for a small,
well-understood caching policy. Revisit if background sync or complex runtime
caching is needed.

## D-004 — Sprint 00 auth: shared bearer token (temporary)
**Status:** superseded by D-009 (2026-10-21)
Private routes require `Authorization: Bearer <DIRECTORY_API_TOKEN>` (a Worker
secret, at least 32 characters, constant-time compare, fails closed). The
decision logic sits in core (`auth.rs`) so it can be swapped. Options for
Sprint 01:
(a) **Cloudflare Access** (Zero Trust free tier, up to 50 users), with the Worker
validating the Access JWT. No auth code to maintain, but native clients need
service tokens or an Access login flow.
(b) **Passkey (WebAuthn) + session cookie** stored in D1. Fully self-contained
and good on iPhone, but more code.
Recommendation: (a) for the web deploy, plus per-device API tokens for native
clients. **Needs owner decision.**

## D-005 — npm workspaces, pinned versions
**Status:** accepted
pnpm wasn't installed, and npm workspaces are sufficient. Exact versions are
pinned. npm 12 blocks install scripts by default, so `workerd` and `esbuild` are
allow-listed in `package.json#allowScripts`.

## D-006 — Tailwind v4 via `@tailwindcss/vite`, CSS-variable theme
**Status:** accepted
Design tokens are CSS variables redefined for `prefers-color-scheme: dark`. The
system font stack plus a monospace stack (Cascadia / SF Mono) avoids font
downloads and works offline.

## D-007 — Release profile without `strip`
**Status:** accepted
`strip = true` removes the wasm `target_features` section, and `wasm-bindgen`
then fails with "externref table required for catch wrappers". The release
profile uses `opt-level = "z"`, LTO, and one codegen unit; `wasm-opt` runs via
`worker-build`.

## D-008 — Hash-based client routing
**Status:** accepted
`#/module` routes need no router dependency and behave identically offline and
in installed PWAs. Revisit if deep links or shareable URLs need clean paths.

## D-009 — Owner auth: passkeys + D1 sessions + device tokens
**Status:** accepted (owner decision, 2026-10-21)
- **Sign-in:** WebAuthn passkeys with user verification required. They're
  verified in portable Rust: `p256` for ES256 and `rsa` for RS256, using
  stable crate lines (p256 0.13, rsa 0.9, sha2 0.10) to avoid mixed digest
  versions.
- **Bootstrap:** the first passkey is registered with a one-time setup secret.
- **Sessions and tokens:** browsers use an opaque session cookie, and native
  clients use revocable device tokens. Only SHA-256 hashes are stored.
- **Why passkeys over Cloudflare Access:** self-contained, testable locally, and
  cheap per request (a signature check versus password hashing, which could
  exceed the Workers Free 10 ms CPU limit). They also work with Windows Hello
  and Face ID.
- **Rejected:** Cloudflare Access (account setup, harder local dev) and
  passwords (CPU cost, phishing risk).
- **Known gap:** no rate limiting on login options/verify yet (tracked in
  PROGRESS).

## D-010 — Storage is owner-scoped in core
**Status:** accepted
`StorageService::new(store, owner_id)` maps every key to `u/<owner_id>/<key>`
and strips the prefix from results, so handlers can't address another
principal's objects even if multiple users exist later.

## D-011 — End-to-end tests drive real infrastructure locally
**Status:** accepted
- `npm run smoke` starts an isolated `wrangler dev` with a fresh D1/R2 state
  directory and generated secrets. It uses a Node software authenticator for
  the full identity lifecycle.
- `npm run test:browser` drives headless Chrome/Edge over CDP with a virtual
  WebAuthn authenticator, so it exercises real `navigator.credentials`.
- No Playwright or Puppeteer dependency.

## D-012 — Release milestones instead of a single "production" finish line
**Status:** accepted
Releases: M0 Foundation → M1 Private Alpha (auth + functional core) → M2 JARVIS
Alpha → M3 Knowledge & Offline Beta → M4 Personal OS → M5 1.0. Each milestone
has exit criteria in ROADMAP.md. A milestone ships to Cloudflare only after the
owner approves deployment.

## D-014 — Optimistic concurrency for long-form text
**Status:** accepted
Notes (and future long-form entities) use `updated_at` as a version. Clients
send `expected_updated_at`; the repository writes with
`… WHERE updated_at = ?`. A mismatch returns `409` with the current version,
and the user chooses "use theirs" or "keep mine" (a forced write with no
expected version). Versions strictly increase even within one millisecond.
Short fields (tasks, projects) stay last-write-wins until M3 sync.

## D-015 — Cross-module operations go through module services
**Status:** accepted
The inbox triages through `TaskService`, `NoteService`, and `JournalService`
(the same validation and ownership rules) via a `Targets` struct of
repositories, never through another module's tables. This is the pattern
JARVIS commands will follow in M2. There are no cross-module D1 transactions:
operations are ordered so that a failure can only leave a duplicate source
(e.g. an inbox item still present), never lose data.

## D-016 — JARVIS: typed commands, preview, approval, provider-independent
**Status:** accepted
- **Input:** text becomes typed `Command`s through an `IntentProvider`. Today
  that's a deterministic parser; an AI provider can implement the same trait
  later, which needs owner approval for any paid or external service.
- **Execution:** every command runs through the module services as the
  caller (D-015). JARVIS never touches tables directly.
- **Approval:** read-only plans execute immediately. Anything that writes is
  stored `pending` with a preview and a 10-minute expiry, and runs only after
  approval via an atomic `pending → running` claim (no double execution).
  Multi-step plans stop at the first failure (`failed`, later steps skipped).
- **Resolution:** name-based references ("complete dentist") are resolved
  during planning. Zero or several matches are rejected with a clear message
  rather than guessed.

## D-039 — A cached list ignores responses that newer changes overtook
**Status:** accepted
- `useCachedList` gives each load a token from `lib/gate.ts`. A local edit
  (`replace`, `mutate`) or a newer load makes older tokens stale, and a
  stale response (or its offline fallback) is dropped.
- Before this, a reload that started just before an offline edit could
  resolve after it and overwrite the list and its cached copy. The edit
  stayed queued and synced, but disappeared from view until then. Two
  overlapping reloads could also land out of order.
- A dropped response isn't retried. The list keeps the newer state, and
  the next reload (on sync or reconnect) brings it up to date.

## D-038 — Search ignores case for all letters, not just ASCII
**Status:** accepted
- SQLite `LIKE` folds case only for ASCII, so "über" missed "Über".
  Core now plans each query. An ASCII query uses `LIKE` as before. A
  query with other letters narrows `LIKE` by its longest ASCII run
  ("ber"), or by nothing when it has none (Greek, CJK), and fetches up to
  100 recent rows per kind.
- Every row is then matched in core, per character, with Unicode lower
  case, on the same fields as the SQL (journal: body only). The ranking
  and the 50 per kind and 50 overall caps are unchanged.
- No migration and no new dependency. FTS5 (`unicode61`) stays the
  option for larger data, behind `SearchSource`.

## D-037 — Links work offline
**Status:** accepted
- Each item's link list is cached (`links:<kind>:<id>`). The picker
  offers the cached tasks, notes, and projects when the server is
  unreachable.
- Linking queues `link.create { from, to }`, which the server handles
  idempotently per pair, so no client link id is needed. The local row
  gets a placeholder id (`local:…`) until the next sync reloads the list.
- Unlinking a synced link queues `link.delete { link_id }` (a 404 is
  done). Unlinking a placeholder removes its queued create from the
  outbox (`LocalDb.dropWhere`), so nothing reaches the server.
- The other item's cached list isn't updated locally (its own title isn't
  known there). The link shows on that side after the sync.

## D-036 — Inbox triage queues offline as one server-side operation
**Status:** accepted
- Triage offline queues `inbox.triage { id, to, date? }` and removes the
  item from the cached inbox. On replay the server does what it does
  online: it creates the task or note, or appends to the journal, then
  deletes the item. The journal date is today's date when queued.
- A 404 on replay means the item is gone (deleted or filed from another
  device), so the op is done. The new task or note appears after the sync,
  with no local preview, so ids and titles come from the server rules.
- Appending to a journal day bumps its version. An open editor with an
  older version then gets the usual conflict choice, so nothing is lost.

## D-035 — Journal days can be deleted offline
**Status:** accepted
- Delete queues a `journal.delete` op (a 404 on replay counts as done)
  and removes the day from the cached month and its `journal:<date>`
  copy.
- First it drops any queued coalesced save of that day
  (`LocalDb.dropMerged`). Otherwise writing the day again would update
  that older outbox slot in place, and it would replay before the delete,
  which would then wipe the new text. The new text is saved create-only
  (base 0) after the delete.

## D-034 — Pinning a note is part of its versioned save
**Status:** accepted
- `pinned` joins the note's autosaved fields, so the pin button edits
  the draft like typing does. Online it saves with `expected_updated_at`;
  offline it joins the coalesced `note.save` op. Each version holds one
  write, so a pin can't make a queued text save look like a conflict.
- A pin made on two devices is a conflict like any other edit, with the
  same "use theirs" or "keep mine" choice. Saves queued by older builds
  have no `pinned` field and leave it unchanged.

## D-033 — Quota re-checked after upload; failed blob deletes are retried
**Status:** accepted
- `commit_upload` checks usage again after inserting the metadata. If
  concurrent uploads pushed the owner over the quota, that upload's row
  and blob are removed and it gets `507 quota_exceeded`. Usage can never
  stay over the quota, though a burst may reject an upload that would have
  fitted on its own.
- A failed R2 delete (after a file delete, or a rolled-back upload) is
  recorded in `blob_orphans` (migration 0020). Each upload and delete then
  retries up to 5 of that owner's orphans. Keys outside the owner's
  `u/<id>/f/` prefix are dropped, never deleted. No cron is used.

## D-032 — Files are read-only offline
**Status:** accepted
- Each folder listing (and the storage usage) is cached at
  `files:<folder>` when it loads. Offline, or with the server
  unreachable, Files shows that copy with a notice. Upload, new folder,
  rename, move, and delete are disabled; queueing them offline would mean
  queueing file bytes, which isn't worth the risk.
- A file whose bytes are already on the device (a downloaded book or a
  saved track, in the `media` store) can be downloaded from that copy.
  Others are marked "not on this device".
- Listings are cached per folder only once opened; nothing is prefetched.

## D-031 — Offline search reads the device cache, labelled as partial
**Status:** accepted
- When the browser is offline, or the search request fails with a network
  error, Search matches the copies already in IndexedDB: the task,
  project, note, event, and habit lists, opened notes (`note:<id>`), and
  journal days and months. Nothing extra is downloaded for it.
- `lib/search.ts` `searchLocal` and `snippet` mirror directory-core: the
  same fields per kind (journal: body only), case-insensitive matching
  per character, title matches first then newest, at most 50 hits.
- Results are marked "from what this device has saved", because the
  cache holds only what this device has loaded (for example, note
  excerpts rather than bodies for notes never opened here).

## D-030 — The library works offline: saved documents and queued progress
**Status:** accepted
- The library list is cached like other lists (`library:last`).
- "Download" saves a document's bytes in the IndexedDB `media` store,
  the same store music uses (D-028's "forget this device" clears it).
  The reader and the PDF opener use the saved copy first, then the
  server. Offline, only saved documents can be opened.
- Reading progress is an outbox op (`library.progress`). It applies
  locally with `applyProgress`, which mirrors the server's rules (percent
  from pages, status changes, finished pins the last page), and replays
  on reconnect. A 404 on replay means the file was deleted, so the op is
  dropped. The last write wins: progress has no version.
- Resetting progress stays online-only.

## D-029 — ePubs are read in-app without a dependency; PDFs wait for approval
**Status:** accepted
- An ePub is a zip of XHTML chapters. `lib/epub.ts` reads the zip with the
  browser's `DecompressionStream`, follows container → OPF → spine, and
  renders a chapter in `<iframe sandbox="">` (no scripts, opaque origin).
  It also strips scripts and inline handlers, inlines the book's styles,
  and maps images to `blob:` URLs. Links between chapters are made inert.
  Zip limits: 5,000 entries, 200 MB uncompressed.
- Progress is chapter n of N, so the server derives the percentage and
  the status as before.
- PDFs need either pdf.js (a large dependency) or serving PDF content
  inline. The latter loosens the user-content sandbox CSP, which is a
  security-policy change. Both need the owner. Until then, PDFs open in
  the browser's own viewer from a blob URL.

## D-028 — "Forget this device" deletes the browser's copy, after a warning
**Status:** accepted
- Settings › account offers "sign out and forget this device". It signs
  out, then deletes the whole `directory` IndexedDB database: caches, the
  saved session, the outbox, and tracks saved offline. If changes haven't
  synced, the confirmation says how many will be lost. Plain sign-out
  still keeps the offline copy.
- **Robustness:**
  - A closed shared connection (another tab upgrading, or a delete) is
    dropped, and the next caller reopens. Before, a stale handle made
    writes fail silently.
  - Upgrades create whatever stores are missing, keyed on what exists
    rather than the old version number, so a database something else
    created empty is completed rather than left broken.

## D-027 — Errors are contained per module and reported to the owner's own log
**Status:** accepted
- Each module renders inside an error boundary keyed by module. A crash
  shows "try again" and "reload app" while the shell and navigation keep
  working. Data lives on the server or in IndexedDB, so nothing is lost.
- Boundary crashes, uncaught errors, and unhandled rejections are posted
  to `/api/v1/diagnostics/errors` (authenticated). The client sends each
  message at most once per 5 minutes and at most 5 reports a minute. The
  server cleans and caps fields and keeps the newest 200 per owner. They
  show in Settings › diagnostics and are logged with `console_warn` for
  `wrangler tail`.
- No third-party monitoring service. Cloudflare's Workers Logs
  (`[observability]` in wrangler config) is free-tier, but enabling it is
  left for the first deploy so the owner can see it.

## D-026 — Accessibility is checked automatically
**Status:** accepted
- The browser e2e audits every module through the browser's computed
  accessibility tree. Interactive controls must have an accessible name,
  images alt text, and each page a heading. A planted unnamed button
  proves the audit works.
- A unit test checks every text colour token against both surfaces
  (panel, sunken) in both themes at WCAG AA (4.5:1), plus button text on
  the accent. To meet it, the light theme's accent, ok, and warn colours
  were darkened slightly (same hues). This is a provisional-token
  adjustment, not a new aesthetic.
- Inputs no longer suppress the global `:focus-visible` outline. A
  "Skip to content" button (first in tab order) moves focus to `<main>`;
  a link would change the hash route.
- Not automated: screen-reader announcements and real-device checks.

## D-025 — Restore is a merge-only import of the export, in small batches
**Status:** accepted
- `POST /api/v1/import/{kind}` takes at most 100 records of one kind. The
  client splits an export into batches (≤ 1 MB each) in dependency order:
  projects, tasks (parents first), notes, journal, inbox, habits, events,
  playlists, links, preferences. Small batches keep each request within
  the Workers Free CPU budget.
- Records keep their ids and timestamps and become the importing owner's.
  A record that already exists (same id, or a journal day with an entry,
  or a link between the same two items) is skipped, never overwritten. So
  restore is safe to repeat and to resume after a failure.
- Every record is validated like an ordinary create. Caps (D-022) apply.
  Subtasks need their parent; links need both ends in the account. Tasks
  whose project is missing are kept, unassigned. Preferences apply only if
  the account has none.
- File contents aren't in exports, so files and reading progress aren't
  restored; the importer says how many files to upload again.

## D-024 — Automations: event-driven rules that run pre-approved JARVIS commands
**Status:** accepted (extends D-016)
- A rule is a trigger and a JARVIS command template. Triggers in v1: a task
  moved to done (optionally in one project), and a new habit check-in
  (optionally for one habit). They fire from the Tasks and Habits routes
  (including offline replays) and from JARVIS completions and check-ins.
- **Approval:** saving a rule approves its command in advance (it is
  validated by the parser then). Only additive commands may run unattended:
  add task, note, journal, and capture, at most 3 steps. Interactive JARVIS
  writes still need per-plan approval.
- **Safety:**
  - Templates are parsed first and placeholders (`{title}`, `{project}`,
    `{date}`) are substituted into the parsed text fields, so a value like
    `x; capture y` can't add steps.
  - Actions never fire rules, and the allowlist excludes completions and
    check-ins, so there are no chains or loops.
  - Repeats (an already-done task, a re-sent check-in) don't fire.
- **Execution:** rules run in the same request, after the triggering change
  has committed. Failures never fail that request; each rule records its
  last run (ok/failed + message).
- **Dates:** clients pass their local date as `?today=` on task updates and
  JARVIS execute; otherwise the UTC date is used.
- **Not yet:** scheduled (time-based) triggers. They need a Workers Cron
  Trigger and per-owner time zones; deferred.

## D-023 — Preferences are one validated JSON document per owner
**Status:** accepted
- Settings that should follow the owner across devices, starting with the
  dashboard layout, are stored in `preferences` as one JSON document. Core
  owns the typed shape and validation; unknown stored values are dropped
  when read, so old clients and removed widgets can't break it.
- PATCH is partial and last write wins. Offline changes queue in the outbox
  (`prefs.update`) and replay safely.
- Per-device conveniences (like a remembered filter) stay in the URL or
  IndexedDB, not here.

## D-022 — Lists are never silently cut: caps are enforced when creating
**Status:** accepted
- **Problem:** list queries had fixed `LIMIT`s (tasks 1,000 ordered oldest
  first), but creates didn't check them. Past the limit, new items silently
  disappeared from the app (still in D1 and the export).
- **Tasks:** the list is every open task plus the 500 most recently closed
  (by `completed_at`). Creating more than 2,000 open tasks is refused with
  a clear message. The Done/All views say when they show only recent
  completions.
- **Other modules:** a create past the list cap is refused: notes 2,000
  (raised from 1,000), events 2,000, projects 500, inbox 500, habits 200,
  and files 1,000 per folder (uploads and moves into a full folder).
  Presets already worked this way (50).
- Caps are sized to keep a list response well within the Workers Free CPU
  budget. Pagination is the path to larger datasets and would come with
  sync paging.
- Each check runs one `COUNT(*)` per create, on the existing owner indexes.

## D-021 — Calendar: local wall-clock events, repeats expanded on both sides
**Status:** accepted
- Events store local dates and `HH:MM` times with no time zone, which fits a
  single-owner personal calendar. If shared calendars or travel across time
  zones become requirements, add an IANA zone per event; this would be a
  migration, not a redesign.
- Repeats are a small enum (daily, weekly, monthly, yearly) with an optional
  last date, not RRULE. Monthly on the 31st skips shorter months, and yearly
  on Feb 29 waits for leap years. Deleting a repeating event deletes every
  occurrence; there are no per-occurrence exceptions yet.
- `GET /events` returns events unexpanded. The web client expands any range
  itself, so every month works offline. Core has the same expansion
  (`/events/agenda`, for JARVIS and other clients), and both sides are tested
  with the same cases.
- The calendar shows open tasks with due dates from the tasks cache; it
  doesn't store copies.

## D-020 — Habits: weekday schedules, idempotent check-ins, server-side streaks
**Status:** accepted
- A habit is scheduled on chosen weekdays (stored as a bit mask, Monday = bit
  0). A check-in is one row per habit and local date. `PUT` and `DELETE`
  are idempotent, so offline replays need no client ids beyond the habit's
  own.
- **Streak:** count back from today. A checked day counts. A scheduled day
  without a check-in ends the streak, except today, which isn't over yet.
  Unscheduled days are skipped, but a check-in on one still counts. The
  look-back is 366 days. Core computes it; the web client mirrors the rule to
  update offline, and the shared cases are tested on both sides.
- Dates are the owner's local dates. The server can't know the time zone, so
  it rejects dates more than one day past its UTC date.
- The client loads 366 days of history so it can recompute streaks locally.
  That's fine for a personal dataset; revisit if it grows.

## D-019 — The wiki is notes with `[[title]]` links, not a separate module
**Status:** accepted
- A wiki page is a note. `[[Title]]` or `[[Title|label]]` in a body links to
  the note whose display title (explicit, or the first body line) matches
  case-insensitively. Links can't span lines.
- Outgoing links resolve on the client against the cached notes list, so they
  work offline. A missing target offers "create", which makes a note with
  that title.
- Backlinks are computed on request (`GET /notes/{id}/backlinks`): a LIKE
  pre-filter over at most 50 candidates, then an exact parse in core. Nothing
  is stored, so renaming a note simply changes what links to it, and no
  migration was needed. LIKE is case-insensitive for ASCII only, so
  differently-cased non-ASCII titles can be missed. Move to an index if
  note counts grow.
- Explicit entity links (the `links` module) stay separate: they connect any
  module, while wiki links are text inside a note.

## D-018 — Offline document edits: coalesced, versioned saves and stored conflicts
**Status:** accepted (Notes, Journal)
- **Coalescing:** autosaved edits to one note or journal day made while
  offline (or while other changes are queued) update a single outbox entry
  in place (`merge` key = document key, `rev` bumped). That entry keeps the
  server version the edits started from (`base`).
- **Versioned replay:** saves send `expected_updated_at: base`. Notes created
  offline have no base (their first save overwrites the copy just created).
  A journal day that had no entry uses `0`, which the server treats as
  create only, so a day written on another device is never overwritten
  silently.
- **In-flight edits:** if the entry changed while its save was in flight, the
  replay keeps it and rebases it onto the version it just wrote, instead of
  deleting it. A replay that sees new work queued mid-run goes round again.
- **Conflicts:** a `409 conflict` during replay drops the entry and stores
  `conflict:<doc>` = `{ mine, theirs }` in IndexedDB. The app shows an alert,
  and the editor shows the existing "use theirs / keep mine" banner when the
  document is open (immediately, or the next time it's opened). Nothing is
  merged automatically.
- **Online-only:** pinning, links, journal delete, and inbox triage.

## D-017 — Offline edits through a replayable outbox
**Status:** accepted (Tasks, Projects, Notes list create/delete; Inbox capture and discard)
- **Queueing:** offline changes apply to the local copy immediately and are
  queued in an IndexedDB `outbox` (schema v2), then replayed in order on load,
  on reconnect, and after each enqueue. While anything is queued, new edits
  also queue, so order is preserved.
- **Safe replay:** every operation can be replayed without harm. Creates
  carry a client-generated 32-hex id (the server answers `409 exists` to a
  repeat, which counts as applied). Updates send only the changed fields
  (last write wins per field). Deletes treat 404 as done.
- **Failure handling:** network, 5xx, and 401 errors stop the replay and keep
  the queue. Other 4xx errors drop that change and show a visible "couldn't
  …" message.
- **Documents:** note and journal saves are versioned and coalesced (D-018).
  Inbox triage stays online-only because it creates items in other modules,
  and links need a connection.

## D-013 — Provisional UI until Paper designs land
**Status:** accepted
No `DESIGN_HANDOFF.md` exists yet. New screens reuse the existing `Panel`,
`Button`, and `StatusChip` components and tokens, are marked
`// Provisional screen: awaiting design integration`, and are listed in
MODULES.md › Design status. Business logic stays in `lib/` and hooks so designs
can replace presentation only.
