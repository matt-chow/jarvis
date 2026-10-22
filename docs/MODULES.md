# Modules

A module is a self-contained capability with a stable id. Module ids live in
`apps/web/src/lib/modules.ts` and must stay stable, because future clients and
JARVIS commands will refer to them.

| Id | Name | Stage | Offline | Backend | Notes |
| --- | --- | --- | --- | --- | --- |
| `home` | Home | ready | yes | `GET /api/v1/health` | Status dashboard; health cached in IndexedDB. |
| `tasks` | Tasks | preview | read-only | `/api/v1/tasks` | Tasks and one-level subtasks, status, priority, due date, notes. Last list cached for offline viewing. |
| `projects` | Projects | preview | read-only | `/api/v1/projects` | Group top-level tasks; progress, archive, and links into Tasks filtered by project. |
| `notes` | Scratchpad | preview | yes | none yet | Device-local notes in IndexedDB. Server notes are an M1 slice. |
| `files` | Files | preview | no | `/api/v1/storage/*` | Owner-scoped private R2 storage (10 MB cap, no folders yet). |
| `jarvis` | JARVIS | planned | — | — | Status screen only; M2. |
| `settings` | Settings | ready | no | `/api/v1/auth/*` | Passkeys, device tokens, sign-out. |

Identity is not a navigable module. The auth gate (`components/AuthScreen.tsx`)
wraps the shell, and core `identity` provides `Principal` to every module API.

## Design status

No Paper design handoff (`docs/DESIGN_HANDOFF.md`) exists yet. Every screen is
provisional (DECISIONS D-013): built from the shared `Panel`, `Button`, and
`StatusChip` components, with logic kept in `lib/` and hooks.

| Screen | File | Status |
| --- | --- | --- |
| App shell (sidebar, header, tab bar) | `App.tsx` | provisional (Sprint 00 style) |
| Sign-in / first-run setup | `components/AuthScreen.tsx` | awaiting design integration |
| Home | `modules/Home.tsx` | provisional |
| Tasks | `modules/Tasks.tsx` | awaiting design integration |
| Projects | `modules/Projects.tsx` | awaiting design integration |
| Scratchpad | `modules/Notes.tsx` | provisional |
| Files | `modules/Files.tsx` | awaiting design integration |
| Settings | `modules/Settings.tsx` | awaiting design integration |
| JARVIS | `modules/Jarvis.tsx` | placeholder: no functionality until M2 |

## Adding a module

1. Add metadata to `MODULES` in `apps/web/src/lib/modules.ts`.
2. Create `apps/web/src/modules/<Name>.tsx` (it receives `ModuleProps`) and
   register it in `VIEWS` in `App.tsx`.
3. Put domain rules in `crates/directory-core/src/<module>.rs`, with a
   repository trait for persistence and an in-memory implementation for tests.
4. Implement the trait in `crates/directory-worker/src/d1/`, and add routes
   under `/api/v1/<module>/…` that resolve a `Principal` with
   `http::auth::authenticate` and scope every query by `principal.user_id`.
5. Add a D1 migration if needed, and bump `EXPECTED_SCHEMA_VERSION` when it
   updates `system_meta`.
6. Add core unit tests, web tests, and smoke checks (plus a browser e2e step for
   key UI flows). Update this file, ROADMAP.md, and PROGRESS.md.
