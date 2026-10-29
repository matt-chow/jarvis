# Modules

A module is a self-contained capability with a stable id. Module ids live in
`apps/web/src/lib/modules.ts` and must stay stable, because future clients and
JARVIS commands will refer to them.

| Id | Name | Stage | Offline | Backend | Notes |
| --- | --- | --- | --- | --- | --- |
| `home` | Home | ready | yes | `/api/v1/health`, `/api/v1/preferences` | Customizable dashboard: capture, today (events, tasks due/overdue with complete), habits to check off, recent notes, system status, modules. Reorder/hide/add widgets; the layout is saved to the account (works offline, syncs). |
| `ask` | Ask | preview | no | `POST /api/v1/jarvis/ask` | JARVIS conversation, iPhone-first and full screen on phones (D-044). Type, or speak where the browser offers speech recognition and the site allows the microphone (feature-detected; otherwise the mic button opens the keyboard for system dictation). Each answer shows its `speech` and status; changes show Confirm and Cancel, which send `yes`/`no` with the plan id, once. Optional spoken replies (on-device speech synthesis). Uses the signed-in session, no device token. |
| `search` | Search | preview | cached data | `/api/v1/search` | Debounced search over tasks, notes, projects, journal, calendar events (title, location, notes; opens that day), and habits, with highlighted titles and snippets, query in the URL (`#/search?q=`), and `/` to open from anywhere. Also the JARVIS `find …` command. Offline or with the server unreachable, it searches the copies cached on this device, ranked the same way (D-031). |
| `inbox` | Inbox | preview | yes | `/api/v1/inbox` | Quick capture (also on Home); triage into task (first line becomes the title), note, or today's journal. Capture, discard, and triage all work offline (queued; triage files the item when it syncs, D-036). |
| `tasks` | Tasks | preview | yes | `/api/v1/tasks` | Tasks and one-level subtasks, status, priority, due date, estimate, notes, repeat (completing a repeating task adds the next one, D-047), and scheduled time from the calendar. Quick add from the header on every screen (+ Task, Alt+N). Editable offline: changes queue and sync on reconnect. |
| `calendar` | Calendar | preview | yes | `/api/v1/events`, `/api/v1/blocks`, `/api/v1/tasks` | The unified planner (D-047): month, week, and day views over one set of items (events, work sessions, task deadlines). Week and day are a time grid with all-day row, overlap columns, a current-time line, and configurable visible hours. Create by selecting a range or tapping a slot (event, task with optional session, work session, focus block); drag to move, drag the bottom edge to resize, arrow keys too; Undo for every change. Fixed commitments vs flexible work; overlap warnings (allowed). Unscheduled inbox (drag onto the grid, or Place then tap), allocation per task (estimated / scheduled / worked / left before the due date), deterministic slot suggestions within working hours. Repeats every N days/weeks/months/years, until a date or N times; change or cancel one occurrence, this and following, or all. Filters. Deep link `#/calendar?date=&view=`. Works offline (D-017, D-021).
| `habits` | Habits | preview | yes | `/api/v1/habits` | Routines on chosen weekdays; tap a day in the last-7-days row to check it off; current streak; rename, reschedule, archive, delete (D-020). Everything works offline. |
| `projects` | Projects | preview | yes | `/api/v1/projects` | Group top-level tasks; progress, archive, and links into Tasks filtered by project. Editable offline, links included (D-037). |
| `notes` | Notes | preview | yes | `/api/v1/notes` | Server notes with debounced autosave, pinning, and conflict detection (409, then "use theirs" or "keep mine"). Create, edit, and delete work offline; conflicts found on sync use the same choice. Pinning is saved with the text, so it works offline too (D-034), and so do links (D-037). Wiki: `[[Title]]` (or with a `\|label`) links to other notes (missing ones can be created from the link) and a backlinks list (D-019). Migrates legacy Scratchpad notes once. |
| `journal` | Journal | preview | yes | `/api/v1/journal` | One entry per day with mood (1–5), autosave, month list, day navigation, and deep links (`#/journal?date=`). Editable offline (a new day saves create-only), including delete (D-035) and links (D-037). |
| `library` | Library | preview | saved docs | `/api/v1/library` | PDFs/ePubs from Files with status and page progress. ePubs open in an in-app reader (sandboxed; chapter progress saved automatically, D-029). PDFs open in a new tab. Offline: cached list, "download" keeps a document on the device, progress queues and syncs (D-030). |
| `music` | Music | preview | saved tracks | `/api/v1/music` | Audio files from Files (any folder; upload here into `Music`), playlists (create, add, reorder, remove, delete), an app-wide player that keeps playing across modules (seek via Range requests, media keys), and per-track "save offline" into IndexedDB. |
| `files` | Files | preview | read-only | `/api/v1/files` | Folders (`#/files?folder=`), streamed multi/drag-drop upload, download, rename/move, delete, usage bar against the quota. Offline: the last listing of each opened folder, read-only; files saved on the device (books, tracks) download from there (D-032). |
| `jarvis` | JARVIS | preview | no | `/api/v1/jarvis/*` | Text command console: preview → approve → results (with links to items), clickable examples, history, and presets (save from any plan, run from chips). Automations panel: rules that run a command when a task is completed or a habit is checked in (D-024). Commands: tasks (add, optionally `in project …`; complete; list), projects (new, list with progress), agenda (`what's on [date]`: events, tasks due, overdue, habits left), habit check-ins (`check in <habit> [yesterday]`), links (`link note <name> to task <name>`), note, journal, capture, find. Deterministic parser; no AI provider yet. |
| `settings` | Settings | ready | no | `/api/v1/auth/*` | Passkeys, device tokens, sign-out. |

**Links** aren't a module either. The `LinkedSection` component appears in the
task, note, journal-day, and project editors. **Deep links** open an item
directly: `#/tasks?task=`, `#/notes?note=`, `#/journal?date=`,
`#/projects?project=` (`lib/links.ts › entityRoute`).

Identity is not a navigable module. The auth gate (`components/AuthScreen.tsx`)
wraps the shell, and core `identity` provides `Principal` to every module API.

## Design status

The Paper design system is integrated (docs/DESIGN_HANDOFF.md, DECISIONS
D-046). Every screen uses its tokens and the shared `Panel`, `Button`, and
`StatusChip` components. A screen is *integrated* once its layout matches
its Paper artboard; *design system* means it has the new look but its own
layout, because the artboard hasn't been read yet or doesn't exist.

| Screen | File | Status |
| --- | --- | --- |
| App shell (sidebar, header, tab bar) | `App.tsx` | integrated (Desktop · Home, Mobile · Home) |
| Sign-in / first-run setup | `components/AuthScreen.tsx` | design system |
| Home dashboard and widgets | `modules/Home.tsx`, `modules/widgets.tsx`, `modules/Suggest.tsx` | integrated |
| Music and player bar | `modules/Music.tsx`, `components/Player.tsx` | design system |
| JARVIS automations panel | `modules/AutomationsPanel.tsx` | design system |
| Search | `modules/Search.tsx` | design system |
| Inbox + capture box | `modules/Inbox.tsx`, `components/CaptureBox.tsx` | design system |
| Tasks | `modules/Tasks.tsx` | design system |
| Projects | `modules/Projects.tsx` | design system |
| Habits | `modules/Habits.tsx` | design system |
| Calendar | `modules/Calendar.tsx` | design system |
| Notes | `modules/Notes.tsx` | design system |
| Journal | `modules/Journal.tsx` | design system |
| Library | `modules/Library.tsx` | design system |
| Files | `modules/Files.tsx` | design system |
| Settings | `modules/Settings.tsx` | design system |
| JARVIS console | `modules/Jarvis.tsx` | design system |
| JARVIS Ask (conversation) | `modules/Ask.tsx` | integrated (Mobile · JARVIS · Voice) |
| Modules launcher (phone tab) | `modules/Modules.tsx`, `components/ModuleGrid.tsx` | design system |

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
