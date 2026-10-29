/**
 * The unified planner engine (DECISIONS D-047): one list of calendar items
 * built from events, time blocks, and task deadlines, shared by the month,
 * week, and day views; overlap layout; per-task time allocation; conflict
 * detection; and deterministic free-slot suggestions. Pure functions, so
 * every view computes the same thing and it all works offline.
 */
import type { TimeBlock } from "./blocks";
import { addDays, dayNumber, isFixed, occurrences, type CalendarEvent } from "./calendar";
import { isOpen, type Task, type TaskPriority } from "./tasks";
import { clockLabel, convert, deviceZone, durationLabel, elapsed, toMinutes } from "./time";

export type ItemKind = "event" | "block" | "deadline";

export interface CalItem {
  /** Unique per rendered item (an occurrence, or one day of a multi-day item). */
  key: string;
  kind: ItemKind;
  /** Event, block, or task id. */
  id: string;
  title: string;
  /** Display date, in this device's zone. */
  date: string;
  /** Minutes since midnight on `date`; null = all day (or a deadline). */
  start: number | null;
  end: number | null;
  fixed: boolean;
  /** The series occurrence this came from (repeating events). */
  occurrence?: string;
  repeating: boolean;
  /** A finished session, or a closed task's deadline. */
  done: boolean;
  /** A skipped session: shown faded, never a conflict. */
  skipped: boolean;
  taskId: string | null;
  projectId: string | null;
  priority?: TaskPriority;
  /** Started on an earlier day / continues onto the next. */
  fromPrev: boolean;
  toNext: boolean;
  /** Times were converted from another zone. */
  zone?: string;
  event?: CalendarEvent;
  block?: TimeBlock;
  task?: Task;
}

export const isTimed = (i: CalItem): i is CalItem & { start: number; end: number } => i.start !== null && i.end !== null;

/** Monday-first dates of the week containing `date`. */
export function weekDates(date: string): string[] {
  const lead = (new Date(`${date}T00:00:00Z`).getUTCDay() + 6) % 7;
  return Array.from({ length: 7 }, (_, i) => addDays(date, i - lead));
}

/** 0 = Monday … 6 = Sunday. */
export const weekday = (date: string) => (new Date(`${date}T00:00:00Z`).getUTCDay() + 6) % 7;

const dates = (from: string, to: string) => Array.from({ length: Math.max(0, dayNumber(to) - dayNumber(from) + 1) }, (_, i) => addDays(from, i));

/**
 * Split a zoned span into per-day pieces in this device's zone.
 * `end` is on `endDate` (multi-day timed events) and may be 1440.
 */
function pieces(
  startDate: string,
  start: number,
  endDate: string,
  end: number,
  zone: string | null | undefined,
  here: string,
): { date: string; start: number; end: number; fromPrev: boolean; toNext: boolean }[] {
  const s = zone ? convert(startDate, start, zone, here) : { date: startDate, minutes: start };
  let e = zone ? convert(endDate, end, zone, here) : { date: endDate, minutes: end };
  // `24:00` is midnight at the end of the day, not the start of the next.
  if (e.minutes === 0 && dayNumber(e.date) > dayNumber(s.date)) e = { date: addDays(e.date, -1), minutes: 1440 };
  if (e.date < s.date || (e.date === s.date && e.minutes <= s.minutes)) e = { date: s.date, minutes: Math.min(1440, s.minutes + 15) };
  return dates(s.date, e.date).map((d) => ({
    date: d,
    start: d === s.date ? s.minutes : 0,
    end: d === e.date ? e.minutes : 1440,
    fromPrev: d !== s.date,
    toNext: d !== e.date,
  }));
}

export interface ItemSources {
  events: CalendarEvent[];
  blocks: TimeBlock[];
  tasks: Task[];
  from: string;
  to: string;
  /** Defaults to this device's zone. */
  zone?: string;
}

/** Every calendar item overlapping `from..to`, in the viewer's zone. */
export function buildItems({ events, blocks, tasks, from, to, zone = deviceZone() }: ItemSources): CalItem[] {
  const out: CalItem[] = [];
  const inRange = (d: string) => d >= from && d <= to;
  const taskById = new Map(tasks.map((t) => [t.id, t]));
  const projectOf = (t: Task | undefined) => (t ? (t.project_id ?? (t.parent_id ? (taskById.get(t.parent_id)?.project_id ?? null) : null)) : null);

  // A day of padding: converting zones can move an entry across midnight.
  for (const o of occurrences(events, addDays(from, -1), addDays(to, 1))) {
    const e = o.event;
    const base = {
      kind: "event" as const,
      id: e.id,
      title: e.title,
      fixed: isFixed(e),
      occurrence: o.start_date,
      repeating: e.repeat !== "none",
      done: false,
      skipped: false,
      taskId: null,
      projectId: null,
      event: e,
    };
    if (!e.start_time) {
      for (const d of dates(o.start_date, o.end_date).filter(inRange)) {
        out.push({ ...base, key: `e:${e.id}:${o.start_date}:${d}`, date: d, start: null, end: null, fromPrev: d !== o.start_date, toNext: d !== o.end_date });
      }
      continue;
    }
    const start = toMinutes(e.start_time);
    const multi = o.end_date !== o.start_date;
    const end = e.end_time ? toMinutes(e.end_time) : multi ? 1440 : start + 60;
    const here = zone;
    const tz = e.timezone && e.timezone !== here ? e.timezone : null;
    for (const p of pieces(o.start_date, start, e.end_time || multi ? o.end_date : o.start_date, Math.min(end, 1440), tz, here)) {
      if (inRange(p.date)) out.push({ ...base, key: `e:${e.id}:${o.start_date}:${p.date}`, ...p, zone: tz ?? undefined });
    }
  }

  for (const b of blocks) {
    const task = b.task_id ? taskById.get(b.task_id) : undefined;
    const tz = b.timezone && b.timezone !== zone ? b.timezone : null;
    for (const p of pieces(b.date, toMinutes(b.start_time), b.date, toMinutes(b.end_time), tz, zone)) {
      if (!inRange(p.date)) continue;
      out.push({
        key: `b:${b.id}:${p.date}`,
        kind: "block",
        id: b.id,
        title: b.title || task?.title || "Work session",
        ...p,
        fixed: b.fixed,
        repeating: false,
        done: b.status === "done",
        skipped: b.status === "skipped",
        taskId: b.task_id,
        projectId: projectOf(task),
        priority: task?.priority,
        zone: tz ?? undefined,
        block: b,
        task,
      });
    }
  }

  for (const t of tasks) {
    if (!t.due_date || !inRange(t.due_date)) continue;
    out.push({
      key: `d:${t.id}`,
      kind: "deadline",
      id: t.id,
      title: t.title,
      date: t.due_date,
      start: null,
      end: null,
      fixed: true,
      repeating: (t.repeat ?? "none") !== "none",
      done: !isOpen(t),
      skipped: false,
      taskId: t.id,
      projectId: projectOf(t),
      priority: t.priority,
      fromPrev: false,
      toNext: false,
      task: t,
    });
  }
  return out.sort(compareItems);
}

/** All-day first, then by start time, longer first, then title. */
export function compareItems(a: CalItem, b: CalItem): number {
  return (
    a.date.localeCompare(b.date) ||
    Number(a.start !== null) - Number(b.start !== null) ||
    (a.start ?? 0) - (b.start ?? 0) ||
    (b.end ?? 0) - (a.end ?? 0) ||
    a.title.localeCompare(b.title)
  );
}

export interface Filters {
  events: boolean;
  /** Fixed commitments (events and blocks). */
  fixed: boolean;
  /** Flexible blocks and events. */
  flexible: boolean;
  deadlines: boolean;
  /** Finished sessions and closed tasks' deadlines. */
  completed: boolean;
  /** A project id, or "" for all. Events have no project and stay visible. */
  project: string;
}

export const ALL_FILTERS: Filters = { events: true, fixed: true, flexible: true, deadlines: true, completed: true, project: "" };

export function applyFilters(items: CalItem[], f: Filters): CalItem[] {
  return items.filter((i) => {
    if (i.kind === "event" && !f.events) return false;
    if (i.kind === "deadline") {
      if (!f.deadlines) return false;
    } else if (i.fixed ? !f.fixed : !f.flexible) return false;
    if (i.done && !f.completed) return false;
    if (f.project && i.kind !== "event" && i.projectId !== f.project) return false;
    return true;
  });
}

export interface Placed {
  item: CalItem & { start: number; end: number };
  /** Column within its overlap group, and how many columns the group has. */
  col: number;
  cols: number;
}

/** Side-by-side columns for overlapping timed items on one day. */
export function layoutDay(items: CalItem[]): Placed[] {
  const timed = items.filter(isTimed).sort((a, b) => a.start - b.start || b.end - a.end || a.key.localeCompare(b.key));
  const out: Placed[] = [];
  let group: Placed[] = [];
  let colEnds: number[] = [];
  let groupEnd = -1;
  const close = () => {
    for (const p of group) p.cols = colEnds.length;
    group = [];
    colEnds = [];
  };
  for (const item of timed) {
    if (item.start >= groupEnd) close();
    let col = colEnds.findIndex((end) => end <= item.start);
    if (col < 0) {
      col = colEnds.length;
      colEnds.push(item.end);
    } else colEnds[col] = item.end;
    // Very short items still take visual space; treat them as 15 minutes.
    groupEnd = Math.max(groupEnd, item.end, item.start + 15);
    const p = { item, col, cols: 1 };
    group.push(p);
    out.push(p);
  }
  close();
  return out;
}

export interface Allocation {
  /** The task's estimate, if any. */
  estimate: number | null;
  /** Minutes of planned or finished sessions (skipped ones don't count). */
  scheduled: number;
  /** Minutes worked: recorded actual time, or the session's length when none was recorded. */
  actual: number;
  /** Estimate not yet covered by sessions (0 when fully scheduled). */
  remaining: number | null;
  /** Scheduled minutes on or before the due date. */
  beforeDue: number;
  sessions: number;
  state: "unestimated" | "none" | "under" | "met" | "over";
}

/** Real minutes of a block (DST-aware in its zone). */
export function blockMinutes(b: TimeBlock, zone = deviceZone()): number {
  return elapsed(b.date, toMinutes(b.start_time), toMinutes(b.end_time), b.timezone || zone);
}

/** Estimated vs scheduled vs actual time for every task with an estimate or a session. */
export function allocations(tasks: Task[], blocks: TimeBlock[], zone = deviceZone()): Map<string, Allocation> {
  const byTask = new Map<string, TimeBlock[]>();
  for (const b of blocks) if (b.task_id) byTask.set(b.task_id, [...(byTask.get(b.task_id) ?? []), b]);
  const out = new Map<string, Allocation>();
  for (const t of tasks) {
    const sessions = (byTask.get(t.id) ?? []).filter((b) => b.status !== "skipped");
    const estimate = t.estimate_minutes ?? null;
    let scheduled = 0;
    let actual = 0;
    let beforeDue = 0;
    for (const b of sessions) {
      const m = blockMinutes(b, zone);
      scheduled += m;
      if (b.status === "done") actual += b.actual_minutes ?? m;
      if (t.due_date && b.date <= t.due_date) beforeDue += m;
    }
    const state: Allocation["state"] =
      estimate === null ? "unestimated" : scheduled === 0 ? "none" : scheduled < estimate ? "under" : scheduled === estimate ? "met" : "over";
    out.set(t.id, {
      estimate,
      scheduled,
      actual,
      remaining: estimate === null ? null : Math.max(0, estimate - scheduled),
      beforeDue,
      sessions: sessions.length,
      state,
    });
  }
  return out;
}

/** An open task whose sessions before the deadline don't cover its estimate. */
export function atRisk(t: Task, a: Allocation | undefined): boolean {
  return isOpen(t) && !!t.due_date && a?.estimate != null && a.beforeDue < a.estimate;
}

/** "1 h of 3 h scheduled", "3 h scheduled · 30 min over", "No estimate · 1 h scheduled". */
export function allocationLabel(a: Allocation): string {
  if (a.estimate === null) return a.scheduled ? `No estimate · ${durationLabel(a.scheduled)} scheduled` : "No estimate";
  if (a.state === "over") return `${durationLabel(a.scheduled)} scheduled · ${durationLabel(a.scheduled - a.estimate)} over`;
  return `${durationLabel(a.scheduled)} of ${durationLabel(a.estimate)} scheduled`;
}

export type InboxSort = "due" | "priority" | "project" | "remaining";

const PRIORITY_RANK: Record<TaskPriority, number> = { high: 0, medium: 1, low: 2, none: 3 };

/** Open top-level tasks with no session, or fewer minutes scheduled than estimated. */
export function unscheduled(tasks: Task[], allocs: Map<string, Allocation>, sort: InboxSort = "due", projectNames = new Map<string, string>()): Task[] {
  const list = tasks.filter((t) => {
    if (!isOpen(t) || t.parent_id) return false;
    const a = allocs.get(t.id);
    return !a || a.sessions === 0 || (a.remaining ?? 0) > 0;
  });
  const due = (t: Task) => t.due_date ?? "9999-12-31";
  const cmp: Record<InboxSort, (a: Task, b: Task) => number> = {
    due: (a, b) => due(a).localeCompare(due(b)) || PRIORITY_RANK[a.priority] - PRIORITY_RANK[b.priority],
    priority: (a, b) => PRIORITY_RANK[a.priority] - PRIORITY_RANK[b.priority] || due(a).localeCompare(due(b)),
    project: (a, b) =>
      (projectNames.get(a.project_id ?? "") ?? "￿").localeCompare(projectNames.get(b.project_id ?? "") ?? "￿") || due(a).localeCompare(due(b)),
    remaining: (a, b) => (allocs.get(b.id)?.remaining ?? b.estimate_minutes ?? 0) - (allocs.get(a.id)?.remaining ?? a.estimate_minutes ?? 0),
  };
  return list.sort((a, b) => cmp[sort](a, b) || a.created_at - b.created_at);
}

/** Timed items on the same day that overlap `target` (skipped sessions never conflict). */
export function conflicts(target: { date: string; start: number; end: number; ignore?: (i: CalItem) => boolean }, items: CalItem[]): CalItem[] {
  return items.filter(
    (i) => isTimed(i) && !i.skipped && i.date === target.date && i.start < target.end && target.start < i.end && !(target.ignore?.(i) ?? false),
  );
}

/** "Overlaps “Standup” (commitment) and “Report” (session)." */
export function conflictText(found: CalItem[]): string {
  const name = (i: CalItem) => `“${i.title}” (${i.kind === "block" ? (i.fixed ? "fixed session" : "session") : i.fixed ? "commitment" : "flexible event"})`;
  const names = found.slice(0, 3).map(name);
  const more = found.length > 3 ? ` and ${found.length - 3} more` : "";
  return `Overlaps ${names.join(", ")}${more}.`;
}

export interface Hours {
  /** `HH:MM` */
  day_start: string;
  day_end: string;
  /** 0 = Monday … 6 = Sunday. */
  work_days: number[];
  session_minutes: number;
}

export const DEFAULT_HOURS: Hours = { day_start: "09:00", day_end: "17:00", work_days: [0, 1, 2, 3, 4], session_minutes: 60 };

/** Merged busy intervals of timed, non-skipped items on `date`. */
export function busy(items: CalItem[], date: string): [number, number][] {
  const spans = items
    .filter((i) => isTimed(i) && !i.skipped && i.date === date)
    .map((i) => [i.start!, i.end!] as [number, number])
    .sort((a, b) => a[0] - b[0]);
  const out: [number, number][] = [];
  for (const [s, e] of spans) {
    const last = out.at(-1);
    if (last && s <= last[1]) last[1] = Math.max(last[1], e);
    else out.push([s, e]);
  }
  return out;
}

/** Free gaps within `window` on `date`. */
export function freeGaps(items: CalItem[], date: string, window: [number, number]): [number, number][] {
  const gaps: [number, number][] = [];
  let at = window[0];
  for (const [s, e] of busy(items, date)) {
    if (e <= at) continue;
    if (s >= window[1]) break;
    if (s > at) gaps.push([at, Math.min(s, window[1])]);
    at = Math.max(at, e);
  }
  if (at < window[1]) gaps.push([at, window[1]]);
  return gaps;
}

export interface Slot {
  date: string;
  start: number;
  end: number;
  /** Why this slot: plain words for the person choosing. */
  reason: string;
  /** Shorter than the requested length (the best gap that day). */
  short: boolean;
  /** After the task's due date. */
  late: boolean;
}

const dayName = (date: string) => new Date(`${date}T12:00:00Z`).toLocaleDateString([], { weekday: "short", month: "short", day: "numeric", timeZone: "UTC" });

/**
 * Up to `limit` free slots of `minutes` within working hours, earliest first,
 * at most one per day while there are enough days. Every timed item counts as
 * busy (fixed commitments and other flexible blocks alike), so a suggestion
 * never moves anything. Slots after `due` come only when nothing fits before.
 * Nothing is scheduled here: the caller asks the person to confirm.
 */
export function suggestSlots(opts: {
  items: CalItem[];
  from: string;
  to: string;
  minutes: number;
  hours: Hours;
  now: { date: string; minutes: number };
  due?: string | null;
  limit?: number;
}): Slot[] {
  const { items, minutes, hours, now, due, limit = 5 } = opts;
  const window: [number, number] = [toMinutes(hours.day_start), toMinutes(hours.day_end)];
  const fits: Slot[] = [];
  const short: Slot[] = [];
  const from = opts.from < now.date ? now.date : opts.from;
  for (const date of dates(from, opts.to).slice(0, 62)) {
    if (!hours.work_days.includes(weekday(date))) continue;
    // Today: from the next quarter hour.
    const open: [number, number] = date === now.date ? [Math.max(window[0], Math.ceil((now.minutes + 1) / 15) * 15), window[1]] : window;
    if (open[1] - open[0] < 15) continue;
    const late = !!due && date > due;
    const gaps = freeGaps(items, date, open);
    const daysLeft = due ? dayNumber(due) - dayNumber(date) : null;
    const dueNote = daysLeft === null ? "" : daysLeft > 1 ? ` · ${daysLeft} days before it's due` : daysLeft === 1 ? " · the day before it's due" : daysLeft === 0 ? " · on the due date" : " · after the due date";
    const fit = gaps.find(([s, e]) => e - s >= minutes);
    if (fit) {
      const [s, e] = fit;
      const after = busy(items, date).filter(([, be]) => be <= s).length > 0;
      fits.push({
        date,
        start: s,
        end: s + minutes,
        short: false,
        late,
        reason: `${dayName(date)}: free ${clockLabel(s)}–${clockLabel(e)}${after ? " after your earlier plans" : ""}${dueNote}`,
      });
    } else {
      const best = gaps.filter(([s, e]) => e - s >= 30).sort((a, b) => b[1] - b[0] - (a[1] - a[0]))[0];
      if (best)
        short.push({
          date,
          start: best[0],
          end: best[1],
          short: true,
          late,
          reason: `${dayName(date)}: only ${durationLabel(best[1] - best[0])} free (${clockLabel(best[0])}–${clockLabel(best[1])})${dueNote}`,
        });
    }
  }
  const onTime = fits.filter((s) => !s.late);
  const pool = onTime.length ? onTime : fits;
  const picked = pool.slice(0, limit);
  if (picked.length < limit) picked.push(...short.filter((s) => !s.late).slice(0, limit - picked.length));
  if (!picked.length) picked.push(...short.slice(0, limit));
  return picked;
}

export interface DaySummary {
  commitments: number;
  commitmentMinutes: number;
  sessions: number;
  plannedMinutes: number;
  /** Estimated work still unscheduled for tasks due this day or worked on this day. */
  remainingMinutes: number;
  due: number;
  /** Free minutes within working hours (0 on non-working days). */
  freeMinutes: number;
}

export function daySummary(date: string, items: CalItem[], allocs: Map<string, Allocation>, hours: Hours): DaySummary {
  const today = items.filter((i) => i.date === date);
  const timed = today.filter(isTimed);
  const commitments = timed.filter((i) => i.kind === "event" && i.fixed);
  const sessions = timed.filter((i) => i.kind === "block" && !i.skipped);
  const tasks = new Set<string>();
  for (const i of today) if (i.taskId && (i.kind === "block" || (i.kind === "deadline" && !i.done))) tasks.add(i.taskId);
  const working = hours.work_days.includes(weekday(date));
  const window: [number, number] = [toMinutes(hours.day_start), toMinutes(hours.day_end)];
  const sum = (xs: CalItem[]) => xs.reduce((n, i) => n + (i.end! - i.start!), 0);
  return {
    commitments: commitments.length,
    commitmentMinutes: sum(commitments),
    sessions: sessions.length,
    plannedMinutes: sum(sessions),
    remainingMinutes: [...tasks].reduce((n, id) => n + (allocs.get(id)?.remaining ?? 0), 0),
    due: today.filter((i) => i.kind === "deadline" && !i.done).length,
    freeMinutes: working ? freeGaps(items, date, window).reduce((n, [s, e]) => n + e - s, 0) : 0,
  };
}
