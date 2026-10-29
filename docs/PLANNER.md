# Unified planner (Calendar)

The Calendar module (`#/calendar`) is where tasks, events, and time blocks
meet (DECISIONS D-047). This page covers how it works, what was tested, and
how to check it by hand on a computer and an iPhone.

## Model

| | What it is | Stored in | Defaults |
| --- | --- | --- | --- |
| **Task** | Work to do: title, notes, status, priority, due date, project, estimate (minutes), repeat | `tasks` | open, no estimate, doesn't repeat |
| **Event** | A commitment: start and end, all-day or timed, time zone, repeat | `events` | **fixed** |
| **Time block** | A work session on one day, for one task or standalone ("focus block"), with status and actual minutes | `time_blocks` | **flexible**, planned |

- A task can have any number of sessions. Finishing a session doesn't finish
  the task, and finishing the task keeps its sessions.
- Per task, the calendar computes the estimate, scheduled minutes (planned
  and done sessions; skipped ones don't count), minutes worked (recorded
  actual time, or the session's length), what's left unscheduled, and how
  much is scheduled before the due date. A task is **at risk** when that last
  number is below its estimate.
- **Due date ≠ scheduled time:** the due date is an all-day deadline marker
  (◆). Moving a session never changes the due date.
- **Fixed vs flexible:** fixed items (🔒︎, solid bar) are commitments;
  flexible ones (dashed) are work you can move. Suggestions treat everything
  on the calendar as busy, so they never move or overlap anything. You can
  move anything yourself; the notice says when you moved a commitment.
- **Overlaps** are allowed: the form and the notice after a change name what
  overlaps, as "commitment", "session", and so on.

## Views

- **Month:** a Monday-first grid with up to three items per day and "+N more".
  Tap a date for that day's list (and **+ new**), or double-click to open the
  day.
- **Week:** seven columns on one time grid: an all-day row (all-day events and
  deadlines), side-by-side columns for overlaps, today highlighted, a red
  current-time line, and it opens scrolled to the current time.
- **Day:** the same grid at full width, with richer blocks (project,
  priority, due date, scheduled vs estimated with a progress bar, notes) and
  a summary: commitments, planned work, work left, free time within working
  hours, and the number due.
- The side panel (a bottom sheet on phones) shows Unscheduled, details, the
  create and edit form, suggestions, or settings.

## Doing things

| To | Computer | Phone |
| --- | --- | --- |
| Create | Drag across empty time, or click a slot, or **+ new** | Tap a slot, or **+ new** |
| Choose what | Event · Task (optionally with a session now) · Work session (for a task) · Focus block | same |
| Move | Drag; arrow keys on a focused entry (↑↓ 15 min, ←→ a day) | Tap to select, then drag it; or −15 / +15 min, −1 / +1 day in details |
| Resize | Drag the bottom edge of the selected entry; Shift+↑↓ | Drag the bottom edge of the selected entry |
| Schedule a task | Drag it from Unscheduled onto the grid; or **Suggest times** | **Place on calendar**, then tap a time; or **Suggest times** |
| Finish a session | The round check on the block, or **session done** (with minutes worked) | same |
| Undo | **Undo** on the notice, or Ctrl/⌘+Z | **Undo** on the notice |
| Quick task | **+ Task** in the header, or Alt+N, on any screen | **+ Task** in the header |

**Repeating events:** every N days, weeks, months, or years; never ending, until
a date, or N times. Changing or deleting one occurrence asks which: this
occurrence only, this and following, or every occurrence. Dragging an
occurrence moves only that one. Cancelled occurrences are kept as skipped dates
(you can still undo).

**Repeating tasks:** set Repeat in the task (it needs a due date). Completing
it, or cancelling it to skip, adds the next one, due on the next date of the
pattern. Undo removes that next one again unless you've edited it.

**Suggestions:** free time within working hours (default Mon–Fri 09:00–17:00,
60-minute sessions; ⚙ planner settings), from now until the due date or over
the next 3, 7, or 14 days. They come one per day, earliest first, each with a
reason. Slots after the due date appear only when nothing fits before it, and
are flagged, as is a day that only has a shorter gap. Nothing is scheduled
until you choose.

**Filters:** events, commitments, flexible, deadlines, completed, and project.
Events have no project, so they stay visible under a project filter.

**Time zones:** entries remember the zone they were made in, and the calendar
shows them in this device's zone (marked ⌖, "Set in …"). Durations are real
minutes, so a 01:00–04:00 session on the night clocks go forward counts as 2
hours.

## Offline

Everything above works offline: changes apply at once, queue in the outbox,
and sync in order when the connection returns. If the server refuses a change
(for example, a task deleted on another device), the calendar goes back to
how it was and says why. Recurring tasks get their next occurrence when the
completion syncs.

## Tested automatically

- **Core (Rust):** intervals, counts, and skipped dates; split (skip, move
  one, end the series, replays, ownership, one-off events refused); sessions
  (many per task, flexible default, done with actual time, unlink needs a
  title, validation, someone else's task or block refused, idempotent
  creates); recurring tasks (the next one appears once across reopen and
  replay, cancel skips, the end date stops it, the same successor ids as the
  web client); working hours; restoring sessions from an export.
- **Web (Vitest):** items from all three sources, zone conversion across
  midnight, multi-day splitting, overlap columns, filters, allocation
  (including a DST night), the Unscheduled list and sorting, conflicts,
  suggestions (working days, before the due date, now, shorter gaps), the
  day summary, repeat expansion like the server, DST instants and durations,
  outbox replay of block and split ops, local split and block edits, and
  optimistic edits (shown before the server answers, rolled back when
  refused, reloaded when something else changed, queued when offline).
- **Smoke (API):** two sessions for one task, validation, a focus block,
  finishing a session leaves the task open, moving, split
  (skip / move / replay / end), a counted series, a recurring task completed,
  reopened, and completed again (one next occurrence), working hours,
  export, anonymous 401 on every route, and deleting a task deletes its
  sessions.
- **Browser e2e:** suggest and schedule (nothing scheduled before choosing),
  drag a session an hour later and undo it, add an event from the toolbar
  (fixed, with a zone), cancel one occurrence from the month view, the day
  summary, quick add from the header, and the accessibility audit of the
  calendar.

## Manual acceptance (needs a person)

### Desktop (Chrome, Edge, or Safari)
1. Open Calendar. Switch Month → Week → Day and back; the same entries
   appear in each, and ‹ › and **today** move by month, week, or day.
2. Week: drag across 10:00–11:30 on a weekday. Choose **Task**, title
   "Acceptance", estimate 180, due in three days, keep "Schedule a work
   session now", and **add and schedule**. The block appears; Unscheduled
   shows "1 h 30 min of 3 h scheduled".
3. Drag the block to the next day at 14:00. Drag its bottom edge 30 minutes
   down. Press Ctrl+Z twice; it goes back each time.
4. Drag "Acceptance" from Unscheduled onto Friday 09:00. It shows 3 h of 3 h
   and leaves the list.
5. **+ new** → Event "Standup", 09:00–09:30, More options → Every 1 week,
   After 4 times. It appears for four weeks. Drag next week's Standup to
   09:15: only that one moves. Open another occurrence → **cancel this one**
   → confirm; it disappears; **Undo** brings it back.
6. Create a focus block over Standup: the form says it overlaps "Standup"
   (commitment), and you can still save it.
7. Open the Acceptance task's deadline (◆) → **plan time**. Suggestions are
   inside working hours, before the due date, and around everything else.
   Pick one.
8. Filters: turn off Commitments, then Completed; choose a project.
9. ⚙: set working hours to 10:00–16:00 and visible hours to 07:00–20:00;
   suggestions and the grid follow.
10. Tasks: set "Acceptance" to repeat weekly and complete it. The next one
    appears, due a week later, with no sessions.
11. Turn the network off (DevTools › Offline), move a block, and add a task
    with + Task. Turn it back on: the header shows sync, then the server
    has both changes.

### iPhone (Safari and the Home Screen app, after deploying to staging)
1. Calendar opens in Day view. The bottom sheet and the toast sit above the
   tab bar and the home indicator; nothing is hidden behind the notch.
2. Tap an empty slot → the sheet opens with the time filled in. Add an
   event.
3. Tap a block to select it (it gets a ring), then drag it. Scrolling the
   day by swiping on other blocks must still scroll, not move them.
4. **unscheduled** → **Place on calendar** → tap a time: a session is
   created. **Suggest times** → **schedule** works too.
5. Use **−15 min / +1 day** in details to move a block without dragging.
6. Week view on a phone: seven narrow columns that stay readable; the month
   view taps through to a day list.
7. + Task in the header adds a task; it appears in Unscheduled at once.
8. Airplane Mode: make a change, then reconnect; it syncs.

Unverified until done by hand: touch dragging on iOS (pointer capture after
selection), sheet height with the keyboard open, and Safari's time pickers.

## Limitations

- No Paper design for the calendar screens yet (D-013 provisional layout,
  docs/DESIGN_HANDOFF.md).
- Repeats are not RRULE: no "every Monday and Wednesday" or "the second
  Tuesday". Editing one occurrence makes it a one-off event; there are no
  per-occurrence overrides that follow later series edits.
- Sessions don't repeat and can't cross midnight. Timed multi-day events
  can't be dragged (edit them instead).
- Suggestions are deterministic rules, not optimization: earliest free gap
  per working day, no splitting a task across several suggested sessions at
  once, and the dashboard's planner (D-041) doesn't use sessions yet.
- Estimates are capped at 1,440 minutes per task.
- At most 5,000 sessions per account (create refuses beyond that, D-022).
- No external calendars (Google, iCloud) and no notifications.
