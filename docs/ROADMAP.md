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
| More commands: `find` ✅, projects ✅ (new, list, tasks `in project …`, also within one plan); entity links later | mostly done |
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
| PDF/ePub library with manual reading progress | ✅ done; in-app reader (pdf.js/epub.js) **needs a dependency decision** |
| Offline editing: tasks, projects, inbox capture (D-017); notes, journal with stored conflicts (D-018) | ✅ done; links and pinning stay online-only |
| Wiki: `[[links]]` between notes, create-from-link, backlinks (D-019) | ✅ done |

## M4 Personal OS (P4)

| Slice | Status |
| --- | --- |
| Habits: weekday schedules, check-ins, streaks, offline (D-020) | ✅ done |
| Calendar | next |
| Automation engine | planned |
| Music library, playlists, streaming and offline media | planned (R2 storage and quota implications need review) |
| Customizable dashboard | planned |

## M5 1.0 (P5)
Security review, performance, accessibility audit, backup/restore, error
recovery, monitoring, mobile (iOS) verification, storage quotas, full E2E suite.
