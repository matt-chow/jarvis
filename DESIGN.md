# Design system

Directory's look: **a printed planner on a dark desk.** Calm ink-on-paper
surfaces, a mono "data" voice for numbers and dates, and colour kept for the
things that matter (today's items, a selected panel, a chart, JARVIS).
Dark (Night) first, with a complete Light theme. Decisions: D-046 (tokens),
D-050 (this redesign).

## Tokens (fixed — do not change values)

Defined in `apps/web/src/index.css` and used through Tailwind names.

| Token | Light | Night | Use |
|---|---|---|---|
| `canvas` | #f5f3ee | #12131f | page |
| `panel` | #fbfaf7 | #1b1d2d | cards, sidebar |
| `sunken` | #ecebe5 | #0d0e17 | wells, inputs, tracks |
| `ink` | #1c1e2e | #eeeff6 | text |
| `muted` | #5f6275 | #9a9db3 | secondary text (≥ 4.5:1) |
| `accent` | #3b3fa8 | #8e93f0 | links, primary buttons, focus, current |
| `ok` / `warn` / `bad` | #2c7445 / #8a5600 / #b23a32 | #7bd88f / #f2c14e / #ef6f6c | status only |
| `violet`, `periwinkle` | | | charts and the tidal gradient |

- **Type:** Geist (UI) and Geist Mono (data only: times, dates, amounts,
  counts, keys). Sentence case everywhere; no uppercase eyebrow labels.
- **Radii:** 8 (controls), 14 (cards), 22 (hero, dialogs).
- **`bg-tidal`** is reserved for JARVIS surfaces and the Home hero.
- **Texture:** a fixed feTurbulence grain behind everything
  (`--grain-opacity` 0.035 light / 0.05 night). Never over text blocks.
- **Density:** `html[data-density]` comfortable | compact sets
  `--pad-card`, `--gap-grid`, `--row` (Settings › This device).

## Motion

`--ease-out` cubic-bezier(0.23,1,0.32,1), `--ease-drawer`
cubic-bezier(0.32,0.72,0,1). Transform and opacity only, under 300 ms.
`.press` scales to 0.97 on press. Popovers (`.pop`), drawers (`.drawer`)
and dialogs enter with `@starting-style`. Keyboard-opened surfaces don't
animate. `prefers-reduced-motion` removes movement (the turntable stops
spinning; music keeps playing).

## Components

- `Panel` — card. Title "Name · data" renders the name in sans and the
  data part in mono muted.
- `Button` — `primary` (accent fill), default (outline), `ghost`, `danger`.
- `StatusChip` — dot + sentence + mono value.
- `Empty` — honest empty state with one next action.
- `Icon` (`components/icons.tsx`) — authored 24-px, 1.6-stroke line icons.
  No emoji or text glyphs as icons.
- Shell: grouped sidebar with icons, page header (title, sync status,
  quick task, JARVIS panel toggle), JARVIS slide-out panel (≥ 1024 px),
  phone tab bar (Home, Tasks, JARVIS, Calendar, Modules), shortcuts dialog
  (`?`), sync/offline `StatusIndicator` with detail on demand.

## Rules

- Colour concentrates in data and selected panels; chrome stays neutral.
- Every chart has a table equivalent.
- Interface sounds are off by default and separate from music.
- No inert controls; no sample data presented as real.
