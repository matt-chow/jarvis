# Roadmap

Delivery is organized into release milestones (DECISIONS D-012). Each milestone
is a usable product with explicit exit criteria. Security, tests, and docs are
part of every slice, not deferred to the end.

| Milestone | Theme | Status |
| --- | --- | --- |
| **M0 Foundation** | Monorepo, shell, Rust Worker, D1/R2, PWA, CI-ready checks | ✅ done (local) |
| **M1 Private Alpha** | Owner auth + functional core (P1) | ✅ feature-complete locally; deploy awaits owner approval |
| M2 JARVIS Alpha | Command layer and assistant (P2) | 🟡 in progress |
| M3 Knowledge & Offline Beta | Wiki, library, files, search, sync (P3) | 🟡 core slices done; reader awaits a dependency decision |
| M4 Personal OS | Habits, calendar, automations, music, dashboards (P4) | 🟡 in progress |
| M5 1.0 | Production hardening (P5) | planned |

## M1 Private Alpha: P1 functional core

| Slice | Status |
| --- | --- |
| Owner auth: passkeys, sessions, device tokens, settings | ✅ done |
| App shell (auth gate, offline session, module registry) | ✅ done (provisional design) |
| Security: auth rate limiting | ✅ done |
| Tasks: CRUD, status, due date, priority, subtasks (one level) | ✅ done |
| Projects (group tasks; progress) | ✅ done |
| Universal inbox (quick capture → triage into task/note/journal) | ✅ done |
| Notes: server persistence, autosave, conflict detection, legacy migration | ✅ done |
| Journal (dated entries, mood) | ✅ done |
| Entity linking (task ↔ note ↔ project ↔ journal) | ✅ done |
| Data export (JSON) | ✅ done |
| M1 exit review (deploy readiness checklist, `npm run ci`) | ✅ done; first deploy blocked on owner approval |

**Exit criteria:**
- All P1 slices are done to the Definition of Done in AGENTS.md.
- `npm run verify`, `npm run smoke`, and `npm run test:browser` pass.
- A data export (JSON) exists for the owner.
- First Cloudflare deploy, after owner approval (R2 activation, hostname vars),
  with a remote smoke test passing.

## M2 JARVIS Alpha (P2)

| Slice | Status |
| --- | --- |
| Command layer: typed commands, deterministic parser, preview/approve/execute-once, multi-step, history | ✅ done |
| Command presets (saved commands, one-tap run) | ✅ done |
| Voice input (Web Speech API where available; text fallback) | **needs owner approval**: browser speech services send audio to the vendor |
| More commands: `find` ✅, projects ✅ (new, list, tasks `in project …`, also within one plan), agenda ✅, habit check-ins ✅, links ✅ | ✅ done |
| AI intent provider | needs owner approval (provider, budget) |

- A command registry where modules expose typed, validated commands.
- Text command input, then voice (Web Speech API).
- Permission and approval workflow with action previews; multi-step plans.
- Presets and execution history.
- A provider-independent AI adapter: no provider is enabled without approval.
  JARVIS never touches the database directly, only module commands.

## M3 Knowledge & Offline Beta (P3)

| Slice | Status |
| --- | --- |
| Private file management (folders, streamed content, quota) | ✅ done |
| Universal search (substring across text modules) | ✅ done; full-text index later, behind `SearchSource` |
| PDF/ePub library with reading progress | ✅ done |
| In-app ePub reader, dependency-free, with automatic progress (D-029) | ✅ done |
| Offline library: cached list, downloaded documents, queued reading progress (D-030) | ✅ done |
| In-app PDF reader | **needs owner approval**: either a pdf.js dependency or serving PDFs inline (a change to the user-content sandbox policy); PDFs open in the browser's viewer meanwhile |
| Offline editing: tasks, projects, inbox capture (D-017); notes, journal with stored conflicts (D-018) | ✅ done; links and pinning stay online-only |
| Wiki: `[[links]]` between notes, create-from-link, backlinks (D-019) | ✅ done |

## M4 Personal OS (P4)

| Slice | Status |
| --- | --- |
| Habits: weekday schedules, check-ins, streaks, offline (D-020) | ✅ done |
| Calendar: month/day views, timed/all-day/multi-day/repeating events, tasks due, offline (D-021) | ✅ done |
| Automations: event-driven rules running pre-approved JARVIS commands (D-024) | ✅ done; time-based triggers need Cron + time zones |
| Music: audio from Files, playlists, app-wide player with seeking, offline saved tracks | ✅ done (uses the existing per-owner storage quota) |
| Customizable dashboard (widgets, layout saved to the account) | ✅ done |

## M5 1.0 (P5)

| Slice | Status |
| --- | --- |
| List caps enforced at create, no silent truncation (D-022) | ✅ done |
| Backup restore: merge-only import of the export (D-025) | ✅ done (file contents need re-upload) |
| HTTP Range for file content | ✅ done |
| Accessibility: names on every control (audited per module), AA contrast tokens (tested), visible input focus, skip link | ✅ done (automated checks; a screen-reader pass on real devices is still owed) |
| Security review: route sweep (auth + CSRF on every route), header checks, docs/SECURITY.md | ✅ done; fixed a CSP gap that blocked offline music |
| Performance: modules lazy-loaded (initial JS 387 → ~290 KB; modules 3–11 KB each, all precached) | ✅ done; Worker CPU needs a deploy to measure |
| Error recovery (per-module boundary) and monitoring (client error reports in Settings › diagnostics, Worker logs) | ✅ done (D-027) |
| Mobile (iOS) verification | needs a device |
