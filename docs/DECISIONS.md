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
