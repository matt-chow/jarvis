# Modules

A module is a self-contained capability with a stable id. Module ids live in
`apps/web/src/lib/modules.ts` and must stay stable, because future clients and
JARVIS commands will refer to them.

| Id | Name | Stage | Offline | Backend | Notes |
| --- | --- | --- | --- | --- | --- |
| `home` | Home | ready | yes | `/api/v1/health`, `/api/v1/preferences` | Customizable dashboard: capture, today (events, tasks due/overdue with complete), habits to check off, recent notes, system status, modules. Reorder/hide/add widgets; the layout is saved to the account (works offline, syncs). |
| `search` | Search | preview | cached data | `/api/v1/search` | Debounced search over tasks, notes, projects, journal, calendar events (title, location, notes; opens that day), and habits, with highlighted titles and snippets, query in the URL (`#/search?q=`), and `/` to open from anywhere. Also the JARVIS `find …` command. Offline or with the server unreachable, it searches the copies cached on this device, ranked the same way (D-031). |
| `inbox` | Inbox | preview | capture | `/api/v1/inbox` | Quick capture (also on Home); triage into task (first line becomes the title), note, or today's journal. Capture and discard work offline (queued); triage needs a connection. |
| `tasks` | Tasks | preview | yes | `/api/v1/tasks` | Tasks and one-level subtasks, status, priority, due date, notes. Editable offline: changes queue and sync on reconnect. |
| `calendar` | Calendar | preview | yes | `/api/v1/events` | Month grid (Monday first) with events and open tasks due; a day panel; add/edit/delete events (all-day or timed, multi-day, repeating daily/weekly/monthly/yearly with an optional end). Deep link `#/calendar?date=`. Repeats expand on the client, so every month works offline (D-021). |
| `habits` | Habits | preview | yes | `/api/v1/habits` | Routines on chosen weekdays; tap a day in the last-7-days row to check it off; current streak; rename, reschedule, archive, delete (D-020). Everything works offline. |
| `projects` | Projects | preview | yes | `/api/v1/projects` | Group top-level tasks; progress, archive, and links into Tasks filtered by project. Editable offline (links need a connection). |
| `notes` | Notes | preview | yes | `/api/v1/notes` | Server notes with debounced autosave, pinning, and conflict detection (409, then "use theirs" or "keep mine"). Create, edit, and delete work offline; conflicts found on sync use the same choice. Pinning and links need a connection. Wiki: `[[Title]]` (or with a `\|label`) links to other notes (missing ones can be created from the link) and a backlinks list (D-019). Migrates legacy Scratchpad notes once. |
| `journal` | Journal | preview | yes | `/api/v1/journal` | One entry per day with mood (1–5), autosave, month list, day navigation, and deep links (`#/journal?date=`). Editable offline (a new day saves create-only); deleting and links need a connection. |
| `library` | Library | preview | saved docs | `/api/v1/library` | PDFs/ePubs from Files with status and page progress. ePubs open in an in-app reader (sandboxed; chapter progress saved automatically, D-029). PDFs open in a new tab. Offline: cached list, "download" keeps a document on the device, progress queues and syncs (D-030). |
| `music` | Music | preview | saved tracks | `/api/v1/music` | Audio files from Files (any folder; upload here into `Music`), playlists (create, add, reorder, remove, delete), an app-wide player that keeps playing across modules (seek via Range requests, media keys), and per-track "save offline" into IndexedDB. |
| `files` | Files | preview | no | `/api/v1/files` | Folders (`#/files?folder=`), streamed multi/drag-drop upload, download, rename/move, delete, usage bar against the quota. |
| `jarvis` | JARVIS | preview | no | `/api/v1/jarvis/*` | Text command console: preview → approve → results (with links to items), clickable examples, history, and presets (save from any plan, run from chips). Automations panel: rules that run a command when a task is completed or a habit is checked in (D-024). Commands: tasks (add, optionally `in project …`; complete; list), projects (new, list with progress), agenda (`what's on [date]`: events, tasks due, overdue, habits left), habit check-ins (`check in <habit> [yesterday]`), links (`link note <name> to task <name>`), note, journal, capture, find. Deterministic parser; no AI provider yet. |
| `settings` | Settings | ready | no | `/api/v1/auth/*` | Passkeys, device tokens, sign-out. |

**Links** aren't a module either. The `LinkedSection` component appears in the
task, note, journal-day, and project editors. **Deep links** open an item
directly: `#/tasks?task=`, `#/notes?note=`, `#/journal?date=`,
`#/projects?project=` (`lib/links.ts › entityRoute`).

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
| Home dashboard and widgets | `modules/Home.tsx`, `modules/widgets.tsx` | awaiting design integration |
| Music and player bar | `modules/Music.tsx`, `components/Player.tsx` | awaiting design integration |
| JARVIS automations panel | `modules/AutomationsPanel.tsx` | awaiting design integration |
| Search | `modules/Search.tsx` | awaiting design integration |
| Inbox + capture box | `modules/Inbox.tsx`, `components/CaptureBox.tsx` | awaiting design integration |
| Tasks | `modules/Tasks.tsx` | awaiting design integration |
| Projects | `modules/Projects.tsx` | awaiting design integration |
| Habits | `modules/Habits.tsx` | awaiting design integration |
| Calendar | `modules/Calendar.tsx` | awaiting design integration |
| Notes | `modules/Notes.tsx` | awaiting design integration |
| Journal | `modules/Journal.tsx` | awaiting design integration |
| Library | `modules/Library.tsx` | awaiting design integration |
| Files | `modules/Files.tsx` | awaiting design integration |
| Settings | `modules/Settings.tsx` | awaiting design integration |
| JARVIS console | `modules/Jarvis.tsx` | awaiting design integration |

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
