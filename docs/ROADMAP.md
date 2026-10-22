# Roadmap

Delivery is organized into release milestones (DECISIONS D-012). Each milestone
is a usable product with explicit exit criteria. Security, tests, and docs are
part of every slice, not deferred to the end.

| Milestone | Theme | Status |
| --- | --- | --- |
| **M0 Foundation** | Monorepo, shell, Rust Worker, D1/R2, PWA, CI-ready checks | ✅ done (local) |
| **M1 Private Alpha** | Owner auth + functional core (P1) | 🟡 in progress |
| M2 JARVIS Alpha | Command layer and assistant (P2) | planned |
| M3 Knowledge & Offline Beta | Wiki, library, files, search, sync (P3) | planned |
| M4 Personal OS | Habits, calendar, automations, music, dashboards (P4) | planned |
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
| Entity linking (task ↔ note ↔ project ↔ journal) | next |
| Data export (JSON) | planned |

**Exit criteria:**
- All P1 slices are done to the Definition of Done in AGENTS.md.
- `npm run verify`, `npm run smoke`, and `npm run test:browser` pass.
- A data export (JSON) exists for the owner.
- First Cloudflare deploy, after owner approval (R2 activation, hostname vars),
  with a remote smoke test passing.

## M2 JARVIS Alpha (P2)
- A command registry where modules expose typed, validated commands.
- Text command input, then voice (Web Speech API).
- Permission and approval workflow with action previews; multi-step plans.
- Presets and execution history.
- A provider-independent AI adapter: no provider is enabled without approval.
  JARVIS never touches the database directly, only module commands.

## M3 Knowledge & Offline Beta (P3)
- Wiki and PDF/ePub library on R2, with reading progress.
- File management: folders, metadata in D1, multipart uploads, quotas.
- Universal search.
- Offline sync: IndexedDB outbox, server timestamps, conflict policy.

## M4 Personal OS (P4)
Habits, calendar, automation engine, music library and playlists, streaming and
offline media, customizable dashboard.

## M5 1.0 (P5)
Security review, performance, accessibility audit, backup/restore, error
recovery, monitoring, mobile (iOS) verification, storage quotas, full E2E suite.
