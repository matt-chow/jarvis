# Progress

Canonical implementation tracker. Newest entries first. Milestones are defined in
[docs/ROADMAP.md](docs/ROADMAP.md).

## Current state (2026-10-24)

- **Milestone:** M1 Private Alpha is **feature-complete locally**. All M1
  slices are done and `npm run ci` is green. The first deploy is **blocked on
  owner approval** (see the readiness checklist in docs/ARCHITECTURE.md ›
  Deployment). **M2 JARVIS Alpha:** command layer ✅, presets ✅, `find` ✅,
  project commands ✅, planner `priorities`/`suggest` and task updates ✅
  (D-041), iPhone via Apple Shortcuts `/jarvis/ask` + JARVIS-only tokens ✅
  (D-042, on-device check needs staging), optional free OpenRouter AI ✅ (D-043,
  off until a key is set). **Staging deployed ✅** (2026-10-24,
  https://directory-staging.mchowcl.workers.dev); on-iPhone Shortcut check pending.
  **JARVIS Ask** ✅ locally (D-044): the web conversation is now the
  primary iPhone path; not yet deployed or tried on an iPhone. **M3:** search ✅ (offline too), files ✅ (read-only offline), library ✅ (offline too),
  offline editing ✅, wiki ✅. **M4:** habits ✅, calendar ✅, dashboard ✅, automations ✅, music ✅.
- **Works locally** (verified by automated tests):
  - Passkey owner setup and sign-in, sessions, sign-out.
  - Device tokens for native clients and scripts, full or JARVIS-only.
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
  - Library of PDFs/ePubs with reading progress, and an in-app ePub reader.
  - JARVIS Ask (`#/ask`): full-screen phone conversation over
    `/jarvis/ask` with the session cookie; Confirm and Cancel; keyboard
    dictation fallback; optional voice reply.
  - JARVIS text commands (tasks, projects, agenda, habit check-ins, links,
    notes, journal, capture, list, find) with preview,
    approve, results, history, and saved presets.
  - Universal search (`/` from anywhere), plus JARVIS `find`.
  - Automations: when a task is completed or a habit is checked in, run a
    saved command (journal, capture, note, add task).
  - Music: audio files, playlists, a player that keeps playing across
    modules, and tracks saved for offline listening.
  - PWA install and offline relaunch with the saved session.
  - Customizable Home dashboard (today, habits, recent notes, capture,
    status, modules), with the layout saved to the account.
- **Staging is deployed** (owner-approved, 2026-10-24). Owner setup, the
  iPhone passkey, Windows sign-in, and both device tokens are done, and the
  remote smoke exited 0 (see the log entry for what that proves).
  **Production is not deployed** (separate decision).
- **Design:** no Paper handoff yet. All screens are provisional
  (docs/MODULES.md › Design status).

### Verification commands and latest results

| Command | Result (2026-10-23) |
| --- | --- |
| `npm run verify` (fmt, clippy `-D warnings`, tsc, unit tests, builds, deploy dry run) | ✅ pass |
| `cargo test -p directory-core` | ✅ 117 passed |
| Vitest | ✅ 94 passed |
| `npm run test:handoff` | ✅ 50 passed |
| `npm run smoke` (isolated wrangler dev, software authenticator) | ✅ all checks passed |
| `npm run test:browser` (headless Edge, virtual passkey) | ✅ 31/31 |
| `npm run ci` after JARVIS Ask (2026-10-24) | ✅ core 133, Vitest 118, handoff 50, smoke all passed, browser e2e 33/33 |
| Staging: `BASE_URL=… npm run smoke` with tokens (owner, 2026-10-24) | ✅ exit 0; per-check lines not recorded |
| Staging: tokenless remote checks (2026-10-24) | ✅ health, anonymous 401s, app shell, security headers |

### Known issues and technical debt
- **CPU budget unmeasured:** ES256/RS256 verification in WASM has not been
  measured against the Workers Free 10 ms CPU limit. Check after first deploy;
  if needed, switch to `opt-level = 3` or Web Crypto verification.
- `rsa` 0.9 has advisory RUSTSEC-2023-0071 (Marvin timing). It affects
  private-key operations only; we only verify public-key signatures.
- Files: concurrent uploads can exceed the quota for the moment between
  insert and re-check; the one over is then rolled back (D-033). Orphaned
  blobs are retried only when their owner uploads or deletes again.
- Search: queries with non-ASCII letters are narrowed by their ASCII part
  and matched in Rust (D-038). Queries with no ASCII part scan the 100
  most recent rows per kind. Snippets read whole bodies. Move to FTS5
  when data grows.
- Offline: a link made offline shows only on the item it was made from
  until it syncs (D-037). An item triaged offline appears in
  Tasks, Notes, or Journal only after it syncs (D-036). A new edit queued in the instant between a replay finishing
  and the open editor learning the new version can show a spurious
  conflict (resolved with "keep mine"). A project deleted offline leaves
  its tasks showing the old project until the next sync refresh.
- `wrangler dev` must be restarted after rebuilding `apps/web/dist`.
- No CI pipeline yet. `npm run verify`, smoke, and browser e2e are CI-ready.
- iOS Safari is not verified on a device.

### Design dependencies
- Paper designs or `docs/DESIGN_HANDOFF.md` for the shell, auth, Tasks, Files,
  Projects, Notes, Journal, Inbox, and Settings.

### Unresolved decisions (need the owner)
- **Production deploy:** separate approval, after staging passes.
- **AI key (optional):** create a $0 OpenRouter key, then set
  `OPENROUTER_API_KEY` and a `:free` `JARVIS_AI_MODEL` (D-043).
  JARVIS works without it.
- **Mail feasibility** (docs/MAIL_FEASIBILITY.md): create the Google OAuth
  client (Testing → In production, unverified), and try consent on the
  school Microsoft account. Both need a staging host for the redirect URI.
- **In-page speech recognition for JARVIS Ask** (D-044): change
  `Permissions-Policy` to `microphone=(self)`. Security policy, owner
  approval. The browser vendor's speech service (Apple on iPhone) hears
  the audio; Directory gets text only. Until then, Ask uses keyboard
  dictation.
- **Deploy JARVIS Ask to staging** (`npm run deploy:staging`), then the
  iPhone steps in docs/JARVIS_ASK.md.
- **M4 time-based automations:** these need a Workers Cron Trigger (free
  tier) that runs saved commands with no signed-in request behind them.
  That is a new way of acting as the owner, so it is a security-policy
  change (AGENTS.md). Also needed: per-owner time zones (the client can
  report its IANA zone, converted with `Intl` in the Worker).
- **In-app PDF reader:** pdf.js, or serving PDFs inline (CSP change).
- Whether to delete the retired OneDrive copy of the repo (D-040).

### Next task
00. **JARVIS 2.0 (D-048) is on staging** since 2026-10-29 (owner-approved):
   migration 0024 applied, Worker version
   029a3cfb-abd1-451f-9f26-4115e6efea5a, health reports schema v24, and
   `/jarvis/chat`, `/jarvis/plan`, execute, and `/preferences` refuse
   anonymous callers (401). The security headers are unchanged. Restore
   point before the migration: bookmark
   `00000017-00000000-00005113-52464e90f66d589ea6e914d3e1f27fe8`.
   Owner next: create a device token in Settings and run
   `BASE_URL=https://directory-staging.mchowcl.workers.dev
   DIRECTORY_API_TOKEN=dir_… npm run smoke`, then docs/JARVIS_ASK.md ›
   iPhone test steps 5–7a. Free-form requests need `OPENROUTER_API_KEY` and
   a `:free` `JARVIS_AI_MODEL` (docs/STAGING.md §4.3); until then Ask
   handles direct commands and says so.
0. Owner: run docs/PLANNER.md › Manual acceptance on a computer and an
   iPhone. The unified planner (D-047, d5a5bc7) is on staging since
   2026-10-29: migration 0023 applied, Worker version
   45ce805f-a341-4e20-a1bb-a400509824fe, health reports schema v23, and the
   new routes refuse anonymous callers (401). Restore point before the
   migration: bookmark
   `00000014-00000000-00005113-b2354a34b6ac8dd86f02a7d3e5fabbc0`.
1. Owner: approve and deploy JARVIS Ask to staging, then run the iPhone
   steps in docs/JARVIS_ASK.md (they include a confirmed "yes" on staging).
   Decide on `microphone=(self)` (D-044). Report any phrase JARVIS
   misunderstands; it becomes a parser test. After deploying D-045,
   check the Home Screen launch (docs/JARVIS_ASK.md › Unverified).
2. Optional: the Apple Shortcut (docs/IPHONE_SHORTCUT.md), and
   docs/STAGING.md §8 by hand and §9.
3. Mail OAuth feasibility, once the owner has created the clients.
4. Watch item: the intermittent "Offline edits" failure. Its likely cause
   is fixed (D-039); not proven.
5. Measure Worker CPU on staging (`/auth/login/verify`).

---

## Log

### 2026-10-29 — JARVIS 2.0, slice 4: eval suite, offline audit, docs (D-048)
- **`npm run eval:jarvis`** (`scripts/jarvis-eval.mjs`): a phrase table
  run on an isolated Worker with fixtures (coursework, an overdue task, a
  fixed weekly class, a flexible session, and a note with injected orders).
  It's a dry run: plans are proposed and checked, every pending plan is
  cancelled, and the run fails if any task, event, session, note, project,
  or preference changed. Result: parser 10/10, free-form 8/8 honest
  fallback (AI off), no data changed. `JARVIS_EVAL_LIVE=1` with a `:free`
  model runs the same table against the real provider. Model quality is
  reported there; safety is enforced. The live run hasn't been done (no
  key here).
- **Found by the eval and fixed:** "Find four hours for my essay" didn't
  match the task "Essay". The parser now drops a leading "my" or "the" (new
  parser test).
- **Offline audit (§12), no changes needed:** browser e2e already covers
  offline edits that are queued, survive a reload, and sync in order
  without duplicates (client ids, D-039), conflict choice for notes and
  journal, and the app opening from cache with the server stopped. JARVIS
  preferences save offline through the existing `prefs.update` outbox op.
  Ask needs a connection and says so ("tasks and notes still save
  offline"). There is no offline NL, and none is claimed.
- **Docs:** DECISIONS D-048, JARVIS_ASK (the conversation, tests, iPhone
  steps 5–7a), ARCHITECTURE (`/jarvis/chat`, execute body, schema v24),
  STAGING (v24, AI limits), README.

### 2026-10-29 — JARVIS 2.0, slice 3: Ask UI on /jarvis/chat, screen context, preferences (D-048)
- **Ask (`modules/Ask.tsx`, `lib/chat.ts`):** a conversation with JARVIS
  and you in separate bubbles. A plan shows as a card with one checkbox per
  step. Leaving a step out also leaves out the steps that need it, and
  confirm says how many will run. Questions offer their choices as buttons.
  Results show ✓/✗ per step with "open" links, a partial failure is labelled
  "partly done", and an executed plan has an undo button. Undo is still a
  previewed plan that you confirm. Confirm runs exactly the previewed plan
  (by id). A double tap or an expired plan says that nothing new ran. Other
  errors keep the plan and offer a retry. "new" cancels any pending plan and
  starts over. The composer says when the AI model is off. Voice input and
  spoken replies are unchanged.
- **Screen context (`lib/focus.ts`):** Tasks, Notes, Projects, and Calendar
  report what's selected, and the calendar also reports its day. Ask sends
  the screen you came from, and the server checks it against your data.
- **Core:** `ChatRequest.undo` proposes reversing one executed plan,
  owner-scoped (another user's plan is not_found).
- **Settings › jarvis scheduling:** no sessions after, break minutes,
  default task length, most minutes per day, and earliest-first. It shows
  the working hours it refines. The form validates like the server, and
  "use defaults" removes the preferences. It saves offline via the outbox.
- **Verified:** core 181 tests, web 169, handoff 50, verify ✅, smoke ✅,
  browser e2e all steps ✅. New e2e steps cover Ask confirm → undo → undone,
  "complete this task" from the Tasks screen, cancel, anonymous chat → 401,
  and Settings save, persist, and reset.

### 2026-10-29 — JARVIS 2.0, slice 2: conversation and model tool loop (D-048)
- **Core (`jarvis/chat.rs`, `conversation.rs`, `agent.rs`):** `chat` runs
  one turn of a conversation. The parser goes first. What it doesn't
  understand goes to the optional free model, which may run up to 6
  read-only, owner-scoped lookups (tasks, a task, calendar ≤14 days,
  projects, notes, a note, preferences) and then propose actions, ask one
  question, or answer. That's at most 3 model calls, each counted by the
  daily budget. A malformed reply gets one correction; outages, timeouts,
  and a spent budget fall back to direct commands and say so. The model
  sees short handles (`t3`, `b7`), never ids. Stored text reaches it only as
  quoted DATA (`<`, `>`, and `|` replaced). Its actions become ordinary
  commands, which are resolved, previewed, and approved as before.
- **Conversation state** (30 min, bounded): recent items, the list just
  shown ("the second one"), on-screen items (checked against the caller's
  data), the last 6 turns (clipped), the pending plan, and open questions.
  A typed "yes" or "no" decides the pending plan (with steps left out).
  Corrections ("move the second session to tomorrow afternoon", "leave
  Thursday free") replace the pending plan, and the old one can no longer
  run. "Start over" resets the conversation.
- **Worker:** `POST /api/v1/jarvis/chat` (session only).
  `execute` takes an optional `{skip, conversation_id}`. The D1
  conversation store is owner-scoped on read and write, and expired rows
  are pruned. The OpenRouter call has a timeout (`JARVIS_AI_TIMEOUT_MS`,
  default 12 s, aborted) and `max_tokens` 700.
- **Verified:** core 179 tests (11 chat tests with a scripted model:
  lookups → plan → move → keep-free → approve → undo, the §16 scenarios,
  injection in stored text, forged handles, malformed and outage replies,
  budget, model questions, cross-user screens and conversations, reset).
  The smoke test has a new chat check, and all checks pass. Clippy is
  clean (wasm too).
- **Next:** the Ask UI on `/jarvis/chat` (preview cards, step toggles,
  choices, undo), Settings for JARVIS preferences, browser e2e, and docs.

### 2026-10-29 — JARVIS 2.0, slice 1: scheduling plans, revalidation, undo (D-048)
- **Core (`jarvis/expand.rs`, `schedule.rs`, `undo.rs`):** "find 4 hours
  for the essay by Friday", "make time for my assignments this week",
  "rearrange my afternoon", "what's unscheduled this week", "when am I free
  tomorrow", "what did I forget yesterday", "undo that", and "from now on, no
  sessions after 9pm" become concrete, previewed steps (sessions with real
  times, moves, reversals). Sessions avoid events, other sessions, and the
  owner's working hours; fixed items never move. Steps carry dependencies
  and notes (overlaps, shortfalls, assumed durations); steps can be left out
  at approval unless a kept step needs them. Each step is checked again when
  it runs: a session or event that changed since, or a new clash, fails the
  step instead of overwriting. Results record reversals; undo refuses items
  edited since and is idempotent. Ambiguous names return choices.
- **Preferences:** optional `jarvis` block (avoid after, breaks, default
  task length, daily cap, earliest-first), validated in core.
- **Migration 0024 (additive):** `jarvis_plans.reversed_by`, `revises`;
  `jarvis_conversations` (used by the next slice). Schema v24.
- **Verified:** core 164 tests (13 new scheduling/undo/isolation tests, 3
  parser tests), web 155, handoff 50, clippy, build, deploy dry run.
- **Next:** conversation state and the model tool loop, the Ask UI, and
  Settings for the preferences.

### 2026-10-29 — Unified planner: tasks, events, and time blocks (D-047)
- **Model (migration 0023, additive):** `time_blocks` (work sessions, any
  number per task, or standalone focus blocks, with status and actual
  minutes). Events gained fixed/flexible (existing ones stay fixed), an IANA
  zone, every-N repeats, an occurrence limit, and skipped dates. Tasks gained
  repeat and an end date. Schema v23.
- **API:** `/api/v1/blocks` (CRUD; a linked task must be the caller's, also
  checked in the SQL), `POST /events/{id}/split` (one occurrence, or this
  and following: skip or end the series and create the replacement in one D1
  batch; replays answer 409), and working hours in preferences. Closing a
  repeating task creates the next one with a deterministic id
  (`INSERT OR IGNORE`), so it can't be created twice. Export and restore
  include blocks. Deleting a task deletes its sessions.
- **Calendar (`#/calendar`):** month, week, and day views over one item list
  (`lib/planner.ts`). The time grid has an all-day row, overlap columns, a
  current-time line, and configurable hours. You can create by range or tap
  (event, task with a session, session, focus block), drag to move, resize
  from the bottom edge, use arrow keys, and undo (Ctrl/⌘+Z). There's an
  Unscheduled inbox (drag in, or Place then tap), allocation per task,
  suggestions within working hours, overlap warnings, filters, a day
  summary, and a details sheet on phones. Edits apply at once and roll back
  if refused (`runOptimistic`); offline they queue (`block.*`,
  `event.split`). Times convert between zones with `Intl`, and durations
  are DST-aware.
- **Elsewhere:** Tasks shows scheduled time and a repeat setting. "+ Task"
  quick capture is in the header on every screen (Alt+N).
- **Tests:** core 142 (+9), web 155 (+27, including optimistic rollback), smoke "planner (D-047)", browser
  e2e "Planner (D-047)" (suggest → schedule, drag + undo, toolbar event,
  cancel one occurrence, day summary, quick add); 35/35.
- **Docs:** docs/PLANNER.md (model, use, manual acceptance, limitations);
  DECISIONS D-047; ARCHITECTURE (API, data); MODULES; ROADMAP; STAGING (v23).
- **Not verified:** touch dragging and the bottom sheet on a real iPhone
  (docs/PLANNER.md › Manual acceptance). Not deployed.

### 2026-10-28 — Local 502: unmigrated local D1
- **Cause:** the local D1 was at schema v1 (only `0001` applied), so
  `GET /api/v1/auth/session` failed with 502 `backend_unavailable`. The tests
  didn't catch it because each run makes its own migrated database.
- **Fix:** applied `0002`–`0022` locally. `npm run dev:worker` now runs
  `db:migrate:local` first, which does nothing when the database is current.
- **Staging:** already at `0022`. A one-off 7403 from `db:migrate:staging`
  came straight after a login refresh; the same calls succeeded on retry.

### 2026-10-28 — Paper design system, shell, Home, and Ask (D-046)
- **Paper:** read "Directory — Design": the tokens, the style board, Desktop ·
  Home, Mobile · Home, and Mobile · JARVIS · Voice. Then the Paper MCP
  weekly limit stopped access (docs/DESIGN_HANDOFF.md › Blocked).
- **Design system:** Directory's tokens were remapped to Paper's light and
  night values; Geist and Geist Mono are self-hosted; Paper radii; a tidal
  gradient (deepened for AA). `Panel`, `Button`, and `StatusChip` follow the
  style board. There's a per-device Light / Night / system theme.
- **Shell:** a grouped sidebar with a brand mark and account footer, and an
  "Ask JARVIS ⌘J" chip (Ctrl+J works too). Phone tab bar: Home · Tasks ·
  JARVIS button · Projects · Modules. The new `#/modules` launcher shares
  `ModuleGrid` with Home. On the phone, status chips show only when
  something needs attention.
- **Home:** a date eyebrow, a display greeting, and "JARVIS suggests" from
  the planner (1 of 3, Open task, Not now; loading, empty, offline, and
  error states). The widgets are restyled and still customizable.
- **Ask:** the tidal orb (it pulses while listening), quoted turns, a
  "Proposed" card for pending changes, and a composer with the mic orb. The
  confirmation logic is untouched.
- **Not built (no data behind them):** Paper's time blocks, now playing,
  bookmarks, and tracker.
- **Tests:** contrast for both themes and the gradient, theme choice, and
  greeting (unit). The browser e2e step covers the Paper shell (phone tab
  bar, Modules, Ctrl+J, Night, suggestions). Two existing expectations
  changed for sentence-case buttons and the new greeting.
- **Unverified on a real iPhone:** the tab bar orb with the home indicator,
  the Ask composer above the keyboard, and Night in the installed app.

### 2026-10-28 — The Home Screen app opens on Ask (D-045)
- **Problem:** the installed iPhone app opened on the dashboard. iOS uses
  the URL saved at install time and ignores manifest `shortcuts`.
- **Fix:** `lib/launch.ts`, called from `main.tsx` before the first render.
  An installed app's fresh, unrouted launch moves to `#/ask` with
  `replaceState`. Reloads, back/forward, chosen routes, and browser tabs
  are left alone; the launch is decided once per `sessionStorage`.
- **Setting:** Settings › *this device* › "Home Screen app opens on"
  (Ask or Home). Per device, in `localStorage`. It defaults to Ask on
  iOS and Home elsewhere.
- **Unchanged:** the manifest, service worker, auth, and backend. The
  existing install doesn't need to be reinstalled.
- **Tests:** `lib/launch.test.ts`, and a browser e2e step with emulated
  `navigator.standalone`. Gate: verify ✅ (core 133, web 124, handoff 50), smoke ✅, browser e2e 34/34 ✅.
- **Unverified on a real iPhone:** docs/JARVIS_ASK.md › Unverified.

### 2026-10-24 — JARVIS Ask: mobile-first web conversation (D-044)
- **New module `ask`** (`#/ask`, second in the tab bar and sidebar): a
  full-screen phone conversation over the existing `POST
  /api/v1/jarvis/ask`. No backend change; the parser, planner, and
  approval are unchanged. It uses the passkey session cookie (`ask`
  already accepted sessions, with the Origin check), so the page holds no
  device token.
- **Behaviour:**
  - editable text box (Enter or Send), example chips, conversation bubbles
    with the `speech` and status;
  - Confirm and Cancel send `yes`/`no` with the original `plan_id`,
    disabled while busy (a double tap sends once);
  - a new request drops an unconfirmed change;
  - actionable messages for network, 401 (then sign-in), 403, 413, 502,
    and 503;
  - optional voice reply (`speechSynthesis`, per-device toggle);
  - offline: send and mic disabled with a message.
- **Voice input:** `lib/speech.ts` feature-detects
  `webkitSpeechRecognition`, a secure context, and the Permissions-Policy.
  It handles denied permission, silence (engine or 8 s timeout), no mic,
  network, discard, and a throwing start. Our own `_headers` deny the
  microphone, so today the mic button falls back to focusing the box for
  iPhone keyboard dictation. Enabling in-page recognition
  (`microphone=(self)`) is proposed, awaiting approval. Apple has said
  Home Screen web apps lack recognition (WebKit bug 225298). No audio is
  ever recorded or uploaded by Directory.
- **PWA:** manifest `shortcuts` → `/#/ask` (Android/desktop; iOS ignores
  it). Already installable on iOS (standalone, apple metas, icon, safe
  areas). Proposed: an "open Ask at launch" option and an install hint.
- **Tests:** `lib/talk.test.ts` and `lib/speech.test.ts` (request body,
  no token, confirm/cancel/once, drop, retry, errors, unsupported,
  blocked, error mapping, silence, discard, synthesis). Browser e2e "Ask
  (phone)" at 390×844: full screen, no horizontal scroll, policy fallback,
  ask, confirm with a double tap → one task, cancel → none, anonymous →
  401. Ask added to the accessibility sweep.
- **Docs:** docs/JARVIS_ASK.md (new: how it works, iPhone test steps,
  implemented/tested/unverified, proposals), D-044, MODULES.md,
  SECURITY.md, and pointers in the Shortcut docs (now optional).
- **Unverified on a real iPhone:** safe-area layout with the keyboard,
  keyboard dictation in Safari and the Home Screen app, voice reply
  unlocking, in-page recognition (blocked by policy).

### 2026-10-24 — Staging deployed; iPhone Shortcut guide
- **Deployed** `directory-staging` to
  https://directory-staging.mchowcl.workers.dev (owner-approved). Fixed the
  staging D1 block in `wrangler.toml` (a pasted `[[d1_databases]]` header
  had emptied it; binding must be `DB`), added the real database id,
  hostname vars, and `DIRECTORY_QUOTA_BYTES` = 100 MB. Applied migrations
  0001–0022 remotely; `npm run verify` passed first.
- **Evidence:** health `ok`, `schema v22`, storage bound; anonymous
  `/api/v1/tasks` and `/api/v1/jarvis/ask` → 401; a bad Bearer token → 401;
  `/api/v1/auth/session` reports `setupRequired: false` (owner exists).
  R2 r2.dev URL disabled.
- **Owner-reported:** setup token set, first-run setup with an iPhone
  passkey, Windows sign-in, two device tokens (Full "Staging smoke",
  JARVIS-only "iPhone Shortcut"), and remote `npm run smoke` exit 0.
  Caveat: the remote smoke prints `skip` and still exits 0 when a token
  variable is missing, so exit 0 alone doesn't prove the file, task, and
  JARVIS checks ran. The tokenless checks were re-run and pass. The remote
  smoke covers JARVIS read + preview + "no", not a confirmed "yes".
- **Docs:** new docs/IPHONE_SHORTCUT.md (numbered Shortcut build, token
  handling, exact `/ask` contract, statuses, errors, commands).
  docs/JARVIS_SHORTCUTS.md now points there; its status table gained
  `expired`, `running`, `not_found`, and the non-200 errors. The old
  "booleans show as 1/0" step was replaced by checking `status` =
  `pending`. docs/STAGING.md status updated, plus a warning to read every
  smoke line.
- **Backend gaps for the Shortcut:** none blocking. Non-200 replies have no
  `speech` (the Shortcut speaks a fallback). No per-token rate limit on
  `/ask` (security-policy change; not proposed).
- **Not verified:** the Shortcut on a real iPhone, a confirmed write via
  `/ask` on staging, and Worker CPU for sign-in.
- **Note:** `package.json` now has `"wrangler": "^4.152.0"` (uncommitted,
  not made by this slice). AGENTS.md asks for exact pins; owner to decide.

### 2026-10-24 — Staging preparation: full guide, remote checks, Downloads cleanup
- **"STAGING.md missing":** it existed only at the active repo path
  (`C:\Users\mchow\dev\jarvis\docs\STAGING.md`, first committed in
  005250e). It wasn't in the retired OneDrive copy, which stops at 9b9d978,
  before the file was written. It wasn't on GitHub either: `main` is 10+
  commits ahead of `origin/main` (never pushed, by policy).
- **docs/STAGING.md** is rewritten as a complete Windows/PowerShell guide:
  - prerequisites and login;
  - dashboard actions;
  - staging-only D1/R2;
  - vars and secrets;
  - safe migrations;
  - deploy;
  - HTTPS and auth checks;
  - persistence and private-storage tests;
  - JARVIS tests;
  - iPhone prerequisites;
  - free tier and billing;
  - troubleshooting and rollback.

  Owner actions are marked separately from commands, and every command was
  checked against the repo's scripts and wrangler 4.151.3's flags.
- **Remote smoke** (`BASE_URL=… npm run smoke`) adds:
  - task persistence (create, read back, delete);
  - private storage (content refused without auth);
  - `/jarvis/ask` with `DIRECTORY_JARVIS_TOKEN`.

  `npm run smoke:remote-local` (scripts/remote-smoke-local.mjs) runs
  those remote checks against an isolated local Worker: all passed. They
  haven't run against Cloudflare.
- **Downloads cleanup:**
  - Found 75 `directory-export-*.json` files in `C:\Users\mchow\Downloads`.
  - All 75 are browser-e2e artifacts, not owner data:
    - format `directory-export`, schema v9–v22;
    - the test owner "Browser Owner", with the e2e fixture note;
    - 75 distinct user ids (a fresh test database per run);
    - no credential-like strings.
  - Cause: the e2e "export my data" step downloaded into the default
    Downloads folder.
  - All 75 were moved, hash-verified (SHA-256 before and after, no
    overwrite), to `C:\Users\mchow\Documents\Directory\Backups\e2e-test-exports\`,
    with a `MANIFEST-sha256.txt`.
  - That folder is the local Documents directory. The Windows "Documents"
    known folder points to OneDrive, so the OneDrive copy was avoided.
  - Nothing was deleted except each source file, after its copy verified.
  - The app doesn't depend on that location: export is a browser
    download, and restore uses a file picker.
  - Fix: the e2e browser downloads into its throwaway profile
    (`Browser.setDownloadBehavior`).
  - `.gitignore` ignores `directory-export-*.json`.
- **Tests:**
  - verify ✅ (core 133, web 96, handoff 50);
  - smoke ✅;
  - remote-mode smoke against a local Worker ✅;
  - staging dry run ✅ (directory-db-staging, directory-files-staging);
  - browser e2e 32/32, with no new files in Downloads.

### 2026-10-24 — Planner: plans can stay within one project (D-041)
- **JARVIS:**
  - `priorities` and `suggest` take "for|in|on project <name>",
    resolved to an active project (or a clear "no project matches").
  - The spoken answer says "in “Garden”".
  - The AI command set may name a project too, resolved the same way.
- **Web:** the Tasks plan panel follows the project filter.
- **Tests:** verify ✅, smoke (project plan via API and JARVIS) ✅, e2e 32/32; core 133 (parser clauses, service flow with resolution and
  ambiguity).

### 2026-10-24 — Optional AI intent parsing (D-043)
- **Core:** `jarvis::ai`:
  - `TextModel` and `AiBudget` traits; `ParserThenModel` provider.
  - Restricted `AiCommand` set with no ids; reply validation;
    `is_free_model`.
- **Worker:** `ai.rs`:
  - OpenRouter chat completion (one model, temperature 0, 400 tokens).
  - `DailyBudget` on the rate-limit table.
  - Used by `/jarvis/plan` and `/jarvis/ask`.
- **Config:** `OPENROUTER_API_KEY` (secret), `JARVIS_AI_MODEL`, and
  `JARVIS_AI_DAILY`, documented in wrangler.toml, .dev.vars.example,
  and docs/JARVIS_SHORTCUTS.md.
- **Tests:**
  - core 132 (free-model check; parser first; budget/model failure
    fallback; reply validation drops ids and rejects bad dates, unknown
    types, and too many steps);
  - smoke runs with a paid model id configured and asserts AI stays off;
  - verify ✅; browser e2e 32/32.
- **Not verified:** a live OpenRouter call. It needs the owner's key, and
  the request/response shape follows OpenRouter's chat-completions API.

### 2026-10-24 — JARVIS on iPhone: `/api/v1/jarvis/ask` + JARVIS-only tokens (D-042)
- **Core:**
  - `jarvis::voice`: spoken answers for plans and errors, and strict
    dictated yes/no.
  - Identity: `TokenScope` (`full`/`jarvis`) and
    `AuthMethod::JarvisToken`.
- **Worker:**
  - `POST /api/v1/jarvis/ask` (preview → "Say yes to confirm" → confirm
    or cancel once).
  - The data-route and auth-route guards refuse JARVIS-only tokens (403
    `token_scope`).
  - Migration 0022; schema v22.
- **Web:** Settings › device tokens has an access choice (Full / JARVIS
  only), shown in the list.
- **Docs:** docs/JARVIS_SHORTCUTS.md, a step-by-step Shortcut build (Dictate
  Text → Get Contents of URL → Speak Text, with the confirm loop; Siri,
  Lock Screen control, Action button).
- **Tests:**
  - core 129 (voice 3, identity scope);
  - web 96;
  - smoke: new "jarvis ask" check (scope denial on 5 routes, preview,
    unclear answer, yes once, repeat-safe, no cancels, not understood,
    revoke), all passed;
  - verify ✅; browser e2e 32/32.
- **Not verified on a real iPhone:** that needs a staging deploy (owner
  steps in docs/STAGING.md).
- Automations fired by voice use the phone's date (`today`), which the
  smoke test checks.

### 2026-10-24 — Planner and JARVIS task commands (D-041)
- **Core:**
  - `planner.rs`: scoring with reasons, subtask dependencies, and a time
    budget.
  - Tasks gain `estimate_minutes` (validated 1–1440; restore validates it
    too).
  - JARVIS:
    - New commands `priorities`, `suggest` (read-only), and
      `update task` (due date, clear due date, priority, estimate;
      needs approval).
    - `add task` takes "high priority" and "takes 20 minutes" clauses.
    - The parser understands dictation punctuation and durations ("an hour
      and a half", "45m").
- **Worker:** `GET /api/v1/tasks/plan`; migration 0021; schema v21.
- **Web:** Tasks has a provisional **plan** panel (priorities, or suggest
  for N minutes, with reasons; needs a connection) and an estimate field.
- **Tests:**
  - core 126 (planner 6, parser phrases, service flow);
  - web 96;
  - smoke: new planner check, all passed;
  - browser e2e: new "Tasks plan" step, all passed (32);
  - verify ✅.
- Staging config: `[env.staging]` + docs/STAGING.md (005250e).

### 2026-10-24 — Repo moved out of OneDrive (D-040)
- Copied to `C:\Users\mchow\dev\jarvis` with robocopy, excluding build
  output (`node_modules`, `target`, `dist`, the worker build, and the busy
  `.wrangler/hdr`). Gitignored local files (`.dev.vars`, `.handoff/`, and
  `.wrangler` state) came along.
- **Checks:** `git fsck --full` is clean, the `git ls-files -s` hash is
  identical, and `diff -rq` shows no differences. After `npm ci`: verify
  ✅ (core 117, web 94, handoff 50), smoke ✅, browser e2e ✅.
- The OneDrive copy is retired at 9b9d978 and not deleted (it needs owner
  approval). Agent memory was copied to the new project path.
- Mail feasibility research: docs/MAIL_FEASIBILITY.md.

### 2026-10-23 — Reliability: stale list loads can't overwrite edits (D-039)
- **Web:** `lib/gate.ts` (load tokens), used by `useCachedList` for loads,
  commits, and local replaces.
- **Why:** the inbox e2e flake showed out-of-order list responses, and
  the same race explains the long-standing "Offline edits" watch item.
- **Tests:** web 94 (gate ordering); browser e2e 31/31 three runs in a
  row.

### 2026-10-23 — Search: Unicode case-insensitive (D-038)
- **Core:** `search::plan` picks the LIKE pattern and row limit; rows are
  matched again with `find_ci`.
- **Tests:** core 117 (über and Über, CAFÉ, Greek with no ASCII part,
  journal body only); smoke has a new D1 check (über, Greek, and no
  false match on "überall"); browser e2e 31/31.

### 2026-10-23 — Links work offline (D-037)
- **Web:** `LinkedSection` uses a cached list, queued `link.create` and
  `link.delete` ops, placeholder ids for unsynced links
  (`lib/links.ts`), `LocalDb.dropWhere` to cancel an unsynced link, and
  picker candidates from the device cache when offline. Links are no
  longer disabled offline in Notes, Journal, or Projects.
- **Tests:** web 91 (link and unlink replay, cancelling a queued link);
  browser e2e 31/31, with a new step: unlink and relink offline, then
  the server has exactly one link after the sync.

### 2026-10-23 — Inbox: triage works offline (D-036); e2e harness fixes
- **Web:** the `inbox.triage` outbox op, `useInbox.triage` through
  `runOrQueue` (returns `"queued"`), and the Inbox says what will happen
  on reconnect.
- **Tests:** web 89 (a capture plus a triage replay in order; a 404 is
  done); browser e2e 30/30, with a new step: capture and triage offline,
  then the task is on the server and the inbox is empty.
- **Found while testing:** the new step failed one run in three. The
  Inbox shows its cached list before the server copy, and the test
  triaged a stale item (deleted earlier by an API call), which the app
  correctly treated as gone. The step now waits for the server copy. The
  intermittent "Offline edits" failure may be the same stale-cache race in
  the test.
- **Harness:** `scripts/lib/devserver.mjs` retries migrations (up to 3
  times, 3 s apart) on `SQLITE_READONLY`/`BUSY`. Back-to-back runs on
  Windows hit it while the previous run released its state.

### 2026-10-23 — Journal: delete works offline (D-035)
- **Web:** `journal.delete` outbox op, `LocalDb.dropMerged`, and the
  Journal screen deletes through `runOrQueue` (delete is no longer
  disabled offline).
- **Tests:** web 87 (delete, then rewrite, replays in order);
  browser e2e 29/29 (a new step: delete yesterday offline, write it
  again, reconnect, and the server has the rewrite).

### 2026-10-23 — Notes: pinning works offline (D-034)
- **Web:** `NoteFields.pinned`; the Notes editor saves the pin with the
  text through `useAutosaveDoc` instead of a separate request.
- **Tests:** web 86 (a queued pin replays with its base version);
  browser e2e 28/28 (pinning with the server stopped).

### 2026-10-23 — M5 slice: Upload quota race and orphaned blobs (D-033)
- **Core:** `FileService::commit_upload` re-checks usage and rolls back;
  `BlobDeleter`, `remove_blob`, and `sweep_orphans` (bounded, owner-scoped).
- **Worker:** migration 0020 `blob_orphans`, D1 repository methods,
  `R2Blobs` implements `BlobDeleter`, upload and delete routes record and
  sweep. Schema version 20.
- **Tests:** core 115 (concurrent uploads, retry and owner scoping);
  smoke: five concurrent 300 KB uploads against a 1 MB quota never leave
  usage over it; browser e2e 28/28.

### 2026-10-23 — M3 slice: Offline Files (D-032)
- **Web:** `modules/Files.tsx` caches each folder listing and usage
  (`files:<folder>`). Offline it shows them read-only with a notice,
  disables changes, and downloads files saved on the device from the
  `media` store. A folder never opened here says it needs a connection.
- **Tests:** browser e2e 28/28. With the server stopped, a cached folder
  lists its file as "not on this device", upload is disabled, and an
  unopened folder explains why it is empty. (Files has no unit-testable
  logic beyond the component.)

### 2026-10-23 — M3 slice: Offline search (D-031)
- **Web:**
  - `lib/search.ts`: `searchLocal` and `snippet`, mirroring core ranking
    and limits.
  - `lib/searchCache.ts`: reads cached lists and opened documents from
    IndexedDB (`LocalDb.entriesWithPrefix`), keeping the fullest copy.
  - `modules/Search.tsx`: falls back to the device when offline or on a
    network error, with a "results may be incomplete" notice.
- **Tests:**
  - web 85: snippet parity with core, ranking, journal body-only
    matching, the 50-hit cap, and the cache reader on fake-indexeddb.
  - browser e2e 28/28: with the server stopped, search finds a cached
    task and a cached note body.

### 2026-10-23 — M3 slice: Offline library (D-030)
- **Web:**
  - `hooks/useLibrary.ts`: cached library list, documents saved to the
    IndexedDB media store ("download" / "remove download"), and
    `documentBytes`, which prefers the saved copy.
  - `lib/library.ts` `applyProgress` mirrors the server rules; `lib/sync.ts`
    adds the `library.progress` outbox op (a 404 on replay drops it).
  - Library and the ePub reader use the saved copy and queue progress
    offline; reset stays online-only.
- **Tests:**
  - web: `applyProgress` parity cases and the outbox op.
  - browser e2e: download "Tiny", go offline, the book opens from the
    device at the saved chapter, a chapter change queues (1 pending),
    and it syncs on reconnect.
- **Gotcha:** e2e `click` matches text by substring, so the download
  button is labelled "download" rather than "save offline" (it clashed
  with "save").

### 2026-10-22 — M3 slice: In-app ePub reader (D-029)
- **Web:**
  - `lib/epub.ts`: zip reader (stored and deflate, size guards), OPF and
    spine parsing, and chapter documents (scripts and handlers stripped,
    CSS inlined, images as blob URLs).
  - `modules/EpubReader.tsx`: a sandboxed iframe, previous/next and arrow
    keys, resume at the saved chapter, debounced progress saves.
  - Library: a "read" button for ePubs.
- **Tests:**
  - web 79: real zips in tests (stored and deflated), container → OPF →
    spine, sanitizing, no spine.
  - browser e2e 28/28: a generated two-chapter ePub opens, the next
    chapter shows, progress is saved as 2 of 2 (100%), and the book's
    script never runs.

### 2026-10-22 — M2 slice: JARVIS link command
- **Parser:** `link <kind> <name> to|with <kind> <name>`. Kinds are task,
  note, project, and journal/day (a date). `to` inside a name is fine.
- **Core:** `Command::LinkItems` with `LinkTarget`. Both sides resolve at
  plan time (exact, then a unique partial match). It executes through
  `LinkService`, which re-checks ownership and existence.
  `CommandTargets` gains `links`.
- **Tests:** core 113 (parser cases, resolve and link, unresolved), smoke
  (the link on D1, 422 when a name doesn't match).

### 2026-10-22 — Search over events and habits
- **Core:** `search::SearchKind` adds `event` and `habit` (`entity()` maps
  the linkable kinds back). Event hits carry their start `date`. JARVIS
  `find` links only linkable hits.
- **Worker:** event (title, location, notes) and habit (name) queries.
- **Web:** `lib/search.ts` `hitRoute`: events open their day on the
  calendar, habits open Habits.
- **Tests:** core 111, web 75, smoke (event by title and location with its
  date; habit).

### 2026-10-22 — M5 slice: Forget this device (D-028)
- **Web:**
  - `deleteLocalData()` removes the `directory` database.
  - Settings › account has "sign out and forget this device". The
    confirmation warns how many changes would be lost unsynced.
- **Fixes found while testing:**
  - The shared IndexedDB connection is dropped once it's closed, so
    writes no longer fail silently on a stale handle.
  - Upgrades complete a database that was created empty.
  - The e2e legacy-notes step left a connection open, which blocked
    deletes.
- **Tests:**
  - web 74: forget, and completing an empty database (fails on the old
    upgrade).
  - browser e2e 27/27: forget wipes cached data; signing in saves the
    session again; the offline relaunch still works.

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
