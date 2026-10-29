# Design handoff (Paper → Directory)

Paper is the visual source of truth; the code is the functional source of
truth (DECISIONS D-046). Paper file: **Directory — Design**
(`01M5EBRMTA4PTJY25FMBJW28A9`), pages *Design System*, *Desktop Screens*,
*Mobile Screens*.

## Design system (implemented)

The existing token names were kept and remapped to Paper's values, so every
screen changed at once and there's one styling system (`src/index.css`).

| Paper token | Directory token / utility | Light | Night |
| --- | --- | --- | --- |
| `--color-paper` | `canvas` (app ground) | `#F5F3EE` | `#12131F` |
| `--color-sheet` | `panel` (surfaces) | `#FBFAF7` | `#1B1D2D` |
| (derived: pressed paper) | `sunken` (inputs, hover, active nav) | `#ECEBE5` | `#0D0E17` |
| `--color-line` | `line` | ink 12% | night ink 12% |
| `--color-ink` | `ink` | `#1C1E2E` | `#EEEFF6` |
| `--color-ink-muted` | `muted` | `#5F6275` | `#9A9DB3` |
| `--color-indigo` | `accent` (primary) | `#3B3FA8` | `#8E93F0` |
| `--color-violet`, `--color-periwinkle` | `violet`, `periwinkle`; `bg-tidal` | | |
| `--font-sans`, `--font-mono` | Geist, Geist Mono (self-hosted, `assets/fonts`, OFL) | | |
| `--radius-sm/md/lg` | `rounded-sm` 8, `rounded-md` 14, `rounded-lg` 22 | | |

- **Tidal gradient** (`bg-tidal`): JARVIS and hero surfaces only. Paper
  starts it at periwinkle `#8FA6E8`, which gives 2.4:1 under white text; the
  app starts at `#5A6BC8` so every stop is ≥ 4.5:1 (tested in
  `lib/theme.test.ts`). This is the one deliberate deviation.
- **Semantic colours** (`ok`, `warn`, `bad`) aren't in Paper; the previous
  values were kept, re-checked for AA on the new surfaces.
- **Night** follows the system, or the device's choice in Settings ›
  *this device* › Theme (`lib/theme.ts`, applied before the first render).

## Components (from the style board)

| Paper | Component | Notes |
| --- | --- | --- |
| Cards (sheet, 22 px, eyebrow) | `Panel` | Mono uppercase eyebrow title, no title bar. |
| "New task" / "Cancel" | `Button` `primary` / `default` | Pills. Primary is ink; labels render in sentence case. |
| Chips | `StatusChip` | Pill with a tone dot. |
| Task row with time, status squares | Home widgets | Round task checks, square habit marks, mono times. |
| JARVIS suggests | `modules/Suggest.tsx` | Fed by the planner (`GET /tasks/plan`). |
| Module grid | `components/ModuleGrid.tsx` | Home "modules" widget and the phone Modules tab. |

## Screens

| Paper artboard | Directory | Status |
| --- | --- | --- |
| Design System · Foundations, Style board | `index.css`, `components/ui.tsx` | Integrated |
| Desktop · Home · Light / Dark (sidebar) | `App.tsx` shell | Integrated: brand mark, grouped nav (main, Knowledge, Media, System), account footer → Settings, "Ask JARVIS ⌘J" chip. |
| Mobile · Home (tab bar) | `App.tsx` shell | Integrated: Home · Tasks · JARVIS button (Ask) · Projects · Modules. |
| Desktop · Home, Mobile · Home | `modules/Home.tsx`, `widgets.tsx`, `Suggest.tsx` | Integrated, with the existing customizable widgets (D-023). |
| Mobile · JARVIS · Voice | `modules/Ask.tsx` | Integrated: tidal orb, quoted turns, "Proposed" card with Confirm / Cancel. |
| Mobile · Modules, Desktop · Modules | `modules/Modules.tsx` | Built from the shared grid; the artboard itself wasn't read (see Blocked). |
| Desktop · JARVIS, JARVIS · Approval modal | `modules/Jarvis.tsx` | Inherits the system; layout not yet matched. |
| Tasks (desktop, mobile, Today and detail, Task schedule) | `modules/Tasks.tsx` | Inherits the system; layout not yet matched. |
| Projects (desktop, mobile) | `modules/Projects.tsx` | Inherits the system; layout not yet matched. |
| Calendar Month / Week / Day (light, night, mobile day) | `modules/Calendar.tsx` | Inherits the system; layout not yet matched. |
| Habits overview, Habit detail (light, night, mobile) | `modules/Habits.tsx` | Inherits the system; layout not yet matched. |

Screens with no Paper artboard (Search, Inbox, Notes, Journal, Library,
Music, Files, Settings, sign-in) use the shared components only; their
layouts are unchanged.

### Where Paper shows things the app doesn't have

Not built, to avoid inert controls: "Time blocks", the "Now playing" card on
Home, Wiki / Bookmarks / Tracker entries, the inbox count in the sidebar, and
"Hold to speak" on the capture card. Paper's "Review plan" became **Open
task** (it opens the suggested task), because there's no separate plan
review screen.

## Blocked

The Paper MCP hit its weekly limit on 2026-10-28 ("Weekly MCP limit
reached. It resets in 2 days."). Before that, these were read: the tokens,
the style board, Desktop · Home · Light, Mobile · Home, and Mobile · JARVIS ·
Voice. The Tasks, Projects, Calendar, Habits, JARVIS desktop, and Modules
artboards couldn't be read. They're the next slice once access resets (or
on Paper Pro, which is the owner's call).
