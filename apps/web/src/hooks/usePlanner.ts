import { useCallback, useMemo, useRef, useState } from "react";
import type { BlockPatch, NewBlock, TimeBlock } from "../lib/blocks";
import { addDays, dayNumber, nthStart, type CalendarEvent, type NewEvent, type SplitScope } from "../lib/calendar";
import { allocations, conflicts, conflictText, DEFAULT_HOURS, type CalItem, type Hours } from "../lib/planner";
import { clientId } from "../lib/sync";
import { isOpen, repeats, successorId, type NewTask, type Task } from "../lib/tasks";
import { deviceZone, fromMinutes, toMinutes } from "../lib/time";
import { useBlocks } from "./useBlocks";
import { useEvents } from "./useEvents";
import { usePreferences } from "./usePreferences";
import { useProjects } from "./useProjects";
import { useTasks } from "./useTasks";

/** A message after a change, with a way to take it back. */
export interface Notice {
  id: number;
  text: string;
  tone: "ok" | "warn" | "bad";
  undo?: () => Promise<void>;
}

/** Where a timed entry goes: a date and minutes in this device's zone. */
export interface Spot {
  date: string;
  start: number;
  end: number;
}

const eventFields = (e: CalendarEvent): NewEvent => ({
  title: e.title,
  start_date: e.start_date,
  end_date: e.end_date,
  start_time: e.start_time,
  end_time: e.end_time,
  location: e.location,
  notes: e.notes,
  repeat: e.repeat,
  repeat_until: e.repeat_count ? null : e.repeat_until,
  fixed: e.fixed ?? true,
  timezone: e.timezone ?? null,
  repeat_interval: e.repeat_interval ?? 1,
  repeat_count: e.repeat_count ?? null,
});

const blockFields = (b: TimeBlock): NewBlock => ({
  task_id: b.task_id,
  title: b.title,
  date: b.date,
  start_time: b.start_time,
  end_time: b.end_time,
  timezone: b.timezone,
  fixed: b.fixed,
  notes: b.notes,
});

/**
 * The planner's data and actions (D-047): events, sessions, and tasks from
 * their offline-capable hooks, allocation per task, and edits that apply at
 * once, warn about overlaps (but allow them), and can be undone.
 */
export function usePlanner(online: boolean) {
  const events = useEvents(online);
  const blocks = useBlocks(online);
  const tasks = useTasks(online);
  const projects = useProjects(online);
  const prefs = usePreferences(online);
  const zone = useMemo(deviceZone, []);
  const hours: Hours = prefs.prefs?.planner ?? DEFAULT_HOURS;
  const allocs = useMemo(() => allocations(tasks.tasks ?? [], blocks.blocks ?? [], zone), [tasks.tasks, blocks.blocks, zone]);

  const [notice, setNotice] = useState<Notice>();
  const undoStack = useRef<Notice[]>([]);
  const seq = useRef(0);
  const show = useCallback((text: string, tone: Notice["tone"] = "ok", undo?: () => Promise<void>) => {
    const n = { id: ++seq.current, text, tone, undo };
    if (undo) undoStack.current = [...undoStack.current.slice(-19), n];
    setNotice(n);
  }, []);
  const dismiss = useCallback(() => setNotice(undefined), []);

  /** Undo the newest change (Ctrl/⌘+Z, or the notice's Undo). */
  const undo = useCallback(async () => {
    const last = undoStack.current.pop();
    if (!last?.undo) return;
    await last.undo();
    setNotice({ id: ++seq.current, text: `Undone: ${last.text.split(".")[0].toLowerCase()}.`, tone: "ok" });
  }, []);

  /** " Overlaps …" for a spot, or "". */
  const overlap = useCallback((spot: Spot, items: CalItem[], ignore?: (i: CalItem) => boolean) => {
    const found = conflicts({ ...spot, ignore }, items);
    return found.length ? ` ${conflictText(found)}` : "";
  }, []);

  const failed = (what: string) => show(`Couldn't ${what}. Nothing was changed.`, "bad");

  // ---- Sessions (time blocks) ----

  const createBlock = useCallback(
    async (b: NewBlock, items: CalItem[], label: string) => {
      const id = clientId();
      const made = await blocks.create({ timezone: zone, ...b }, id);
      if (!made) return failed("schedule that");
      const spot = { date: b.date, start: toMinutes(b.start_time), end: toMinutes(b.end_time) };
      const warn = overlap(spot, items);
      show(`Scheduled ${label}.${warn}`, warn ? "warn" : "ok", async () => void (await blocks.remove(id)));
      return made;
    },
    [blocks, zone, overlap, show],
  );

  const updateBlock = useCallback(
    async (b: TimeBlock, patch: BlockPatch, label: string, items: CalItem[] = []) => {
      const before: BlockPatch = Object.fromEntries(Object.keys(patch).map((k) => [k, b[k as keyof TimeBlock]]));
      const done = await blocks.update(b.id, patch);
      if (!done) return failed(`update “${label}”`);
      const spot = { date: done.date, start: toMinutes(done.start_time), end: toMinutes(done.end_time) };
      const warn = patch.date || patch.start_time || patch.end_time ? overlap(spot, items, (i) => i.kind === "block" && i.id === b.id) : "";
      show(`Updated “${label}”.${warn}`, warn ? "warn" : "ok", async () => void (await blocks.update(b.id, before)));
      return done;
    },
    [blocks, overlap, show],
  );

  const deleteBlock = useCallback(
    async (b: TimeBlock, label: string) => {
      if (!(await blocks.remove(b.id))) return failed(`delete “${label}”`);
      show(`Deleted the session “${label}”.`, "ok", async () => {
        const back = await blocks.create(blockFields(b), b.id);
        if (back && (b.status !== "planned" || b.actual_minutes !== null)) await blocks.update(b.id, { status: b.status, actual_minutes: b.actual_minutes });
      });
    },
    [blocks, show],
  );

  // ---- Events ----

  const createEvent = useCallback(
    async (e: NewEvent, items: CalItem[]) => {
      const id = clientId();
      const made = await events.create({ timezone: e.start_time ? zone : null, ...e }, id);
      if (!made) return failed("add that event");
      const warn = e.start_time
        ? overlap({ date: e.start_date, start: toMinutes(e.start_time), end: e.end_time ? toMinutes(e.end_time) : toMinutes(e.start_time) + 60 }, items)
        : "";
      show(`Added “${made.title}”.${warn}`, warn ? "warn" : "ok", async () => void (await events.remove(id)));
      return made;
    },
    [events, zone, overlap, show],
  );

  /** Save an edited event: the whole series, or one occurrence / this and following. */
  const saveEvent = useCallback(
    async (e: CalendarEvent, fields: NewEvent, scope: "all" | SplitScope = "all", occurrence?: string) => {
      if (scope === "all" || e.repeat === "none" || !occurrence) {
        const before = eventFields(e);
        const done = await events.update(e.id, { ...fields, skip_dates: fields.repeat === "none" ? [] : e.skip_dates });
        if (!done) return failed(`save “${e.title}”`);
        show(`Saved “${done.title}”.`, "ok", async () => void (await events.update(e.id, { ...before, skip_dates: e.skip_dates ?? [] })));
        return done;
      }
      if (scope === "following" && occurrence === e.start_date) return saveEvent(e, fields, "all");
      const id = clientId();
      const replacement = { ...fields, id, ...(scope === "one" ? { repeat: "none" as const, repeat_until: null, repeat_count: null } : {}) };
      // A counted series keeps its total: the new part gets what's left.
      if (scope === "following" && e.repeat_count && replacement.repeat_count) {
        replacement.repeat_count = Math.max(1, e.repeat_count - countBefore(e, occurrence));
      }
      const r = await events.split(e.id, { date: occurrence, scope, replacement });
      if (!r) return failed(`save “${e.title}”`);
      show(scope === "one" ? `Changed this occurrence of “${e.title}”.` : `Changed “${e.title}” from ${occurrence} on.`, "ok", async () => {
        await events.remove(id);
        await events.update(e.id, { skip_dates: e.skip_dates ?? [], repeat_until: e.repeat_until, repeat_count: e.repeat_count ?? null });
      });
      return r.replacement ?? undefined;
    },
    [events, show],
  );

  /** Delete an event, one occurrence, or this and the following ones. */
  const deleteEvent = useCallback(
    async (e: CalendarEvent, scope: "all" | SplitScope = "all", occurrence?: string) => {
      if (scope === "all" || e.repeat === "none" || !occurrence || (scope === "following" && occurrence === e.start_date)) {
        if (!(await events.remove(e.id))) return failed(`delete “${e.title}”`);
        show(`Deleted “${e.title}”${e.repeat !== "none" ? " and every occurrence" : ""}.`, "ok", async () => {
          const back = await events.create(eventFields(e), e.id);
          if (back && e.skip_dates?.length) await events.update(e.id, { skip_dates: e.skip_dates });
        });
        return;
      }
      if (!(await events.split(e.id, { date: occurrence, scope }))) return failed(`change “${e.title}”`);
      show(scope === "one" ? `Cancelled “${e.title}” on ${occurrence}.` : `Ended “${e.title}” before ${occurrence}.`, "ok", async () => {
        await events.update(e.id, { skip_dates: e.skip_dates ?? [], repeat_until: e.repeat_until, repeat_count: e.repeat_count ?? null });
      });
    },
    [events, show],
  );

  // ---- Moving anything on the grid ----

  /**
   * Move or resize a timed item to `to` (this device's zone). A repeating
   * event's occurrence moves alone unless `scope` is "all". Overlaps are
   * reported, never prevented; nothing else moves.
   */
  const move = useCallback(
    async (item: CalItem, to: Spot, items: CalItem[], scope: "all" | "one" = "one") => {
      const label = item.title;
      const ignore = (i: CalItem) => i.kind === item.kind && i.id === item.id && (item.kind !== "event" || i.occurrence === item.occurrence);
      const warn = overlap(to, items, ignore);
      const verb = item.fixed ? "Moved the commitment" : "Moved";
      if (item.kind === "block" && item.block) {
        const b = item.block;
        const done = await blocks.update(b.id, { date: to.date, start_time: fromMinutes(to.start), end_time: fromMinutes(to.end), timezone: zone });
        if (!done) return failed(`move “${label}”`);
        show(`${verb} “${label}” to ${to.date} ${fromMinutes(to.start)}.${warn}`, warn ? "warn" : "ok", async () => {
          await blocks.update(b.id, { date: b.date, start_time: b.start_time, end_time: b.end_time, timezone: b.timezone });
        });
        return;
      }
      if (item.kind !== "event" || !item.event) return;
      const e = item.event;
      const times = { start_time: fromMinutes(to.start), end_time: fromMinutes(Math.min(to.end, 1439)), timezone: zone };
      if (e.repeat !== "none" && item.occurrence && scope === "one") {
        const id = clientId();
        const r = await events.split(e.id, {
          date: item.occurrence,
          scope: "one",
          replacement: { ...eventFields(e), ...times, id, start_date: to.date, end_date: to.date, repeat: "none", repeat_until: null, repeat_count: null },
        });
        if (!r) return failed(`move “${label}”`);
        show(`${verb} this occurrence of “${label}”.${warn}`, warn ? "warn" : "ok", async () => {
          await events.remove(id);
          await events.update(e.id, { skip_dates: e.skip_dates ?? [] });
        });
        return;
      }
      const shift = item.occurrence ? dayNumber(to.date) - dayNumber(item.occurrence) : 0;
      const before = { start_date: e.start_date, end_date: e.end_date, start_time: e.start_time, end_time: e.end_time, timezone: e.timezone ?? null };
      const done = await events.update(e.id, { ...times, start_date: addDays(e.start_date, shift), end_date: addDays(e.end_date, shift) });
      if (!done) return failed(`move “${label}”`);
      show(`${verb} “${label}”${e.repeat !== "none" ? " (every occurrence)" : ""}.${warn}`, warn ? "warn" : "ok", async () => void (await events.update(e.id, before)));
    },
    [blocks, events, zone, overlap, show],
  );

  // ---- Tasks ----

  const createTask = useCallback(
    async (t: NewTask, schedule?: Spot, items: CalItem[] = []) => {
      const made = await tasks.create(t);
      if (!made) return failed("add that task");
      if (!schedule) {
        show(`Added task “${made.title}”.`, "ok", async () => void (await tasks.remove(made.id)));
        return made;
      }
      const blockId = clientId();
      const b = await blocks.create({ task_id: made.id, date: schedule.date, start_time: fromMinutes(schedule.start), end_time: fromMinutes(schedule.end), timezone: zone }, blockId);
      const warn = overlap(schedule, items);
      show(`Added “${made.title}”${b ? " and scheduled it" : " (couldn't schedule it)"}.${warn}`, warn ? "warn" : b ? "ok" : "bad", async () => {
        await tasks.remove(made.id);
        blocks.forgetTask(made.id);
      });
      return made;
    },
    [tasks, blocks, zone, overlap, show],
  );

  /** Complete or reopen a task; its sessions and their history stay. */
  const setTaskDone = useCallback(
    async (t: Task, done: boolean) => {
      const status = done ? "done" : "todo";
      const r = await tasks.update(t.id, { status });
      if (!r) return failed(`update “${t.title}”`);
      const spawned = done && isOpen(t) && repeats(t);
      show(done ? `Completed “${t.title}”${spawned ? "; the next one is on your list" : ""}.` : `Reopened “${t.title}”.`, "ok", async () => {
        await tasks.update(t.id, { status: t.status });
        // The next occurrence of a repeating task goes too (unless it's been edited).
        if (spawned) {
          const next = (tasks.tasks ?? []).find((x) => x.id === successorId(t.id));
          if (!next || next.updated_at === next.created_at) await tasks.remove(successorId(t.id));
        }
      });
    },
    [tasks, show],
  );

  return {
    online,
    zone,
    events,
    blocks,
    tasks,
    projects,
    prefs,
    hours,
    allocs,
    notice,
    dismiss,
    show,
    undo,
    createBlock,
    updateBlock,
    deleteBlock,
    createEvent,
    saveEvent,
    deleteEvent,
    move,
    createTask,
    setTaskDone,
  };
}

/** Occurrences of a counted series before `date` (skipped ones count, as on the server). */
function countBefore(e: CalendarEvent, date: string): number {
  let n = 0;
  while (n < (e.repeat_count ?? 0)) {
    const d = nthStart(e, n + 1);
    if (!d || d >= date) break;
    n++;
  }
  return n;
}

export type Planner = ReturnType<typeof usePlanner>;
