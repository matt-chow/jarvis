// Reminders (DECISIONS D-049): the server's record, smart views, and the
// local mirror of its scheduling rules for offline edits. The server stays
// the authority: after a sync the list reloads from it.

import { localDate } from "./tasks";

export type Priority = "none" | "low" | "medium" | "high";
export type Alert = { kind: "before"; minutes: number } | { kind: "at"; time: string; days_before?: number };
export type Freq = "daily" | "weekly" | "monthly" | "yearly";
/** `weekdays`: 0 = Monday … 6 = Sunday (weekly only). */
export interface Rule {
  freq: Freq;
  interval: number;
  weekdays: number[];
  until: string | null;
  /** Set by the server from the due date. */
  anchor?: string;
}
export type Advance = "calendar" | "completion";
export type Missed = "each" | "one" | "skip";

export interface ChecklistItem {
  id: string;
  text: string;
  done: boolean;
}

export interface Reminder {
  id: string;
  title: string;
  notes: string;
  links: string[];
  /** The current occurrence's date (the series cursor). */
  due_date: string | null;
  due_time: string | null;
  timezone: string;
  list_id: string | null;
  category_ids: string[];
  priority: Priority;
  flagged: boolean;
  project_id: string | null;
  task_id: string | null;
  complete_with_task: boolean;
  alerts: Alert[];
  recurrence: Rule | null;
  advance: Advance;
  missed: Missed;
  checklist: ChecklistItem[];
  snoozed_until: number | null;
  completed_ahead: string[];
  backlog: number;
  completed_at: number | null;
  deleted_at: number | null;
  created_at: number;
  updated_at: number;
  version: number;
  field_versions: Record<string, number>;
  /** The current occurrence's start (due time, or local midnight). */
  due_at: number | null;
}

export interface NewReminder {
  title: string;
  notes?: string;
  links?: string[];
  due_date?: string | null;
  due_time?: string | null;
  timezone?: string;
  list_id?: string | null;
  category_ids?: string[];
  priority?: Priority;
  flagged?: boolean;
  project_id?: string | null;
  task_id?: string | null;
  complete_with_task?: boolean;
  /** Omitted: the default alert. `[]`: none. */
  alerts?: Alert[];
  recurrence?: Rule | null;
  advance?: Advance;
  missed?: Missed;
  checklist?: ChecklistItem[];
}

export type ReminderPatch = Partial<Omit<NewReminder, "alerts">> & {
  alerts?: Alert[];
  /** The version edited, for the server's field-level merge. */
  base_version?: number;
};

export interface ReminderList {
  id: string;
  name: string;
  color: string;
  created_at: number;
  updated_at: number;
}

export interface HistoryEntry {
  id: string;
  reminder_id: string;
  occurrence: string;
  action: "completed" | "uncompleted" | "snoozed" | "missed";
  at: number;
  until: number | null;
}

export interface AgendaItem {
  reminder_id: string;
  occurrence_id: string;
  date: string;
  time: string | null;
  at: number | null;
  title: string;
  state: "open" | "overdue" | "completed" | "missed";
  recurring: boolean;
  priority: Priority;
  flagged: boolean;
}

export const MAX_TITLE = 200;
export const MAX_ALERTS = 5;
export const MAX_CHECKLIST = 50;
export const MAX_LINKS = 5;
export const MAX_LIST_NAME = 60;

export const PRIORITY_LABELS: Record<Priority, string> = { none: "None", low: "Low", medium: "Medium", high: "High" };
export const PRIORITY_MARK: Record<Priority, string> = { none: "", low: "!", medium: "!!", high: "!!!" };
const PRIORITY_RANK: Record<Priority, number> = { high: 0, medium: 1, low: 2, none: 3 };
export const WEEKDAY_SHORT = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/** The browser's IANA zone (the server's default when none is given). */
export function deviceZone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
  } catch {
    return "UTC";
  }
}

// ---- Views ----

export type SmartView = "today" | "scheduled" | "all" | "flagged" | "completed" | "deleted";
export const SMART_VIEWS: { id: SmartView; label: string }[] = [
  { id: "today", label: "Today" },
  { id: "scheduled", label: "Scheduled" },
  { id: "all", label: "All" },
  { id: "flagged", label: "Flagged" },
  { id: "completed", label: "Completed" },
  { id: "deleted", label: "Deleted" },
];

export const isOpen = (r: Pick<Reminder, "completed_at" | "deleted_at">) => r.completed_at === null && r.deleted_at === null;

/** Past its date (date-only) or its due time, and not done. Stays overdue until resolved. */
export function isOverdue(r: Reminder, now: number, today: string): boolean {
  if (!isOpen(r) || !r.due_date) return false;
  if (r.due_time && r.due_at !== null) return r.due_at <= now;
  return r.due_date < today;
}

export const isSnoozed = (r: Reminder, now: number) => isOpen(r) && r.snoozed_until !== null && r.snoozed_until > now;

export function inView(r: Reminder, view: SmartView, today: string): boolean {
  if (view === "deleted") return r.deleted_at !== null;
  if (r.deleted_at !== null) return false;
  if (view === "completed") return r.completed_at !== null;
  if (r.completed_at !== null) return false;
  switch (view) {
    case "today":
      return !!r.due_date && r.due_date <= today;
    case "scheduled":
      return !!r.due_date;
    case "flagged":
      return r.flagged;
    default:
      return true;
  }
}

/** Case-insensitive match on title, notes, and checklist text. */
export function matchesSearch(r: Reminder, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return [r.title, r.notes, ...r.checklist.map((c) => c.text)].some((s) => s.toLowerCase().includes(q));
}

export type SortKey = "due" | "priority" | "title" | "created";
export const SORTS: { id: SortKey; label: string }[] = [
  { id: "due", label: "Due date" },
  { id: "priority", label: "Priority" },
  { id: "title", label: "Title" },
  { id: "created", label: "Newest" },
];

/** A comparable due key: undated reminders last. */
const dueKey = (r: Reminder) => (r.due_date ? `${r.due_date} ${r.due_time ?? "~"}` : "￿");

export function compareReminders(sort: SortKey): (a: Reminder, b: Reminder) => number {
  const byDue = (a: Reminder, b: Reminder) => dueKey(a).localeCompare(dueKey(b));
  const byTitle = (a: Reminder, b: Reminder) => a.title.localeCompare(b.title, undefined, { sensitivity: "base" });
  switch (sort) {
    case "priority":
      return (a, b) => PRIORITY_RANK[a.priority] - PRIORITY_RANK[b.priority] || byDue(a, b) || byTitle(a, b);
    case "title":
      return (a, b) => byTitle(a, b) || byDue(a, b);
    case "created":
      return (a, b) => b.created_at - a.created_at;
    default:
      return (a, b) => byDue(a, b) || PRIORITY_RANK[a.priority] - PRIORITY_RANK[b.priority] || byTitle(a, b);
  }
}

export interface ViewQuery {
  view: SmartView;
  /** "" = every list, "none" = no list. */
  list: string;
  /** "" = any, "none" = no category, otherwise an id. */
  category: string;
  search: string;
  sort: SortKey;
}

/**
 * The reminders a view shows. A search looks through completed reminders too
 * (they're hidden by default but still findable).
 */
export function selectReminders(all: Reminder[], q: ViewQuery, today: string): Reminder[] {
  const searching = q.search.trim() !== "";
  return all
    .filter((r) => {
      if (searching && q.view !== "deleted") {
        if (r.deleted_at !== null) return false;
      } else if (!inView(r, q.view, today)) return false;
      if (q.list === "none" ? r.list_id !== null : q.list && r.list_id !== q.list) return false;
      if (q.category === "none" ? r.category_ids.length > 0 : q.category && !r.category_ids.includes(q.category)) return false;
      return matchesSearch(r, q.search);
    })
    .sort(compareReminders(q.sort));
}

/** Open reminders for the dashboard: overdue first, then the next `days` days. */
export function upcoming(all: Reminder[], now: number, today: string, days = 7): { overdue: Reminder[]; soon: Reminder[] } {
  const horizon = addDays(today, days);
  const open = all.filter((r) => isOpen(r) && r.due_date).sort(compareReminders("due"));
  return {
    overdue: open.filter((r) => isOverdue(r, now, today)),
    soon: open.filter((r) => !isOverdue(r, now, today) && r.due_date! <= horizon),
  };
}

// ---- Formatting ----

export function dueLabel(r: Reminder, today: string): string | undefined {
  if (!r.due_date) return undefined;
  const d = r.due_date;
  const day =
    d === today
      ? "Today"
      : d === addDays(today, 1)
        ? "Tomorrow"
        : d === addDays(today, -1)
          ? "Yesterday"
          : new Date(`${d}T12:00:00`).toLocaleDateString(undefined, {
              weekday: "short",
              month: "short",
              day: "numeric",
              ...(d.slice(0, 4) !== today.slice(0, 4) ? { year: "numeric" } : {}),
            });
  return r.due_time ? `${day} ${r.due_time}` : day;
}

export function ruleSummary(rule: Rule | null): string | undefined {
  if (!rule) return undefined;
  const n = rule.interval;
  const unit = { daily: "day", weekly: "week", monthly: "month", yearly: "year" }[rule.freq];
  let s = n === 1 ? { daily: "Daily", weekly: "Weekly", monthly: "Monthly", yearly: "Yearly" }[rule.freq] : `Every ${n} ${unit}s`;
  if (rule.freq === "weekly" && rule.weekdays.length > 0) s += ` on ${[...rule.weekdays].sort().map((d) => WEEKDAY_SHORT[d]).join(", ")}`;
  if (rule.until) s += ` until ${rule.until}`;
  return s;
}

export function alertLabel(a: Alert): string {
  if (a.kind === "before") {
    if (a.minutes === 0) return "At due time";
    if (a.minutes % 1440 === 0) return `${a.minutes / 1440} day${a.minutes === 1440 ? "" : "s"} before`;
    if (a.minutes % 60 === 0) return `${a.minutes / 60} h before`;
    return `${a.minutes} min before`;
  }
  const days = a.days_before ?? 0;
  return days === 0 ? `At ${a.time}` : `At ${a.time}, ${days} day${days === 1 ? "" : "s"} before`;
}

export const ALERT_PRESETS: { label: string; alert: Alert }[] = [
  { label: "At due time", alert: { kind: "before", minutes: 0 } },
  { label: "10 min before", alert: { kind: "before", minutes: 10 } },
  { label: "1 h before", alert: { kind: "before", minutes: 60 } },
  { label: "1 day before", alert: { kind: "before", minutes: 1440 } },
  { label: "At 09:00 on the day", alert: { kind: "at", time: "09:00", days_before: 0 } },
  { label: "At 18:00 the day before", alert: { kind: "at", time: "18:00", days_before: 1 } },
];

export const sameAlert = (a: Alert, b: Alert) =>
  a.kind === "before" ? b.kind === "before" && a.minutes === b.minutes : b.kind === "at" && a.time === b.time && (a.days_before ?? 0) === (b.days_before ?? 0);

/** The alerts the server gives a reminder created without any (none when undated). */
export function defaultAlerts(r: Pick<NewReminder, "due_date" | "due_time">): Alert[] {
  if (!r.due_date) return [];
  return r.due_time ? [{ kind: "before", minutes: 0 }] : [{ kind: "at", time: "09:00", days_before: 0 }];
}

// ---- Dates and zones ----

export function addDays(date: string, n: number): string {
  const d = new Date(`${date}T00:00:00Z`);
  d.setUTCDate(d.getUTCDate() + n);
  return d.toISOString().slice(0, 10);
}

/** 0 = Monday … 6 = Sunday. */
export function weekday(date: string): number {
  return (new Date(`${date}T00:00:00Z`).getUTCDay() + 6) % 7;
}

const dayNo = (date: string) => Math.round(Date.parse(`${date}T00:00:00Z`) / 86_400_000);

function daysInMonth(y: number, m: number): number {
  return new Date(Date.UTC(y, m, 0)).getUTCDate();
}

/** `months` after y/m, on `day` or the month's last day. */
function clamped(y: number, m: number, months: number, day: number): string {
  const idx = y * 12 + (m - 1) + months;
  const yy = Math.floor(idx / 12);
  const mm = (((idx % 12) + 12) % 12) + 1;
  return `${String(yy).padStart(4, "0")}-${String(mm).padStart(2, "0")}-${String(Math.min(day, daysInMonth(yy, mm))).padStart(2, "0")}`;
}

const ymd = (d: string) => [Number(d.slice(0, 4)), Number(d.slice(5, 7)), Number(d.slice(8, 10))] as const;

/** Whether `date` is an occurrence of a rule anchored at `anchor` (mirrors the server). */
export function matches(rule: Rule, anchor: string, date: string): boolean {
  if (date < anchor || (rule.until && date > rule.until)) return false;
  const i = rule.interval;
  const [ay, am, ad] = ymd(anchor);
  switch (rule.freq) {
    case "daily":
      return (dayNo(date) - dayNo(anchor)) % i === 0;
    case "weekly": {
      const monday = (d: string) => dayNo(d) - weekday(d);
      const days = rule.weekdays.length ? rule.weekdays : [weekday(anchor)];
      return Math.trunc((monday(date) - monday(anchor)) / 7) % i === 0 && days.includes(weekday(date));
    }
    case "monthly": {
      const [y, m] = ymd(date);
      const k = y * 12 + m - (ay * 12 + am);
      return k % i === 0 && clamped(ay, am, k, ad) === date;
    }
    case "yearly": {
      const [y] = ymd(date);
      return (y - ay) % i === 0 && clamped(ay, am, (y - ay) * 12, ad) === date;
    }
  }
}

/** The first occurrence strictly after `date` (calendar advance). */
export function nextAfter(rule: Rule, anchor: string, date: string): string | null {
  const from = date < anchor ? addDays(anchor, -1) : date;
  const i = rule.interval;
  const [ay, am, ad] = ymd(anchor);
  let next: string | undefined;
  if (rule.freq === "daily") {
    next = addDays(anchor, (Math.floor((dayNo(from) - dayNo(anchor)) / i) + 1) * i);
  } else if (rule.freq === "weekly") {
    for (let k = 1; k <= 7 * i + 7 && !next; k++) {
      const d = addDays(from, k);
      if (matches(rule, anchor, d)) next = d;
    }
  } else {
    const step = rule.freq === "monthly" ? i : 12 * i;
    const [y, m] = ymd(from);
    let k = Math.floor(Math.max(0, y * 12 + m - (ay * 12 + am)) / step) * step;
    for (;;) {
      const d = clamped(ay, am, k, ad);
      if (d > from) {
        next = d;
        break;
      }
      k += step;
    }
  }
  return next && (!rule.until || next <= rule.until) ? next : null;
}

/** The next occurrence counted from a completion on `completed` (completion advance). */
export function afterCompletion(rule: Rule, completed: string): string | null {
  const i = rule.interval;
  const [y, m, d] = ymd(completed);
  let next: string | undefined;
  switch (rule.freq) {
    case "daily":
      next = addDays(completed, i);
      break;
    case "weekly":
      if (rule.weekdays.length) {
        for (let k = 1; k <= 7 && !next; k++) if (rule.weekdays.includes(weekday(addDays(completed, k)))) next = addDays(completed, k);
      } else next = addDays(completed, 7 * i);
      break;
    case "monthly":
      next = clamped(y, m, i, d);
      break;
    case "yearly":
      next = clamped(y, m, 12 * i, d);
      break;
  }
  return next && (!rule.until || next <= rule.until) ? next : null;
}

export interface Occurrence {
  date: string;
  done: boolean;
}

/**
 * A dated reminder's occurrences in `from..=to`, for the calendar. Mirrors
 * core `schedule::occurrences`: the current one, then (calendar advance) the
 * next open ones, skipping those completed ahead, which show as done. A
 * completed one-time reminder shows done on its date. Past completions of a
 * series aren't cached on the device, so they aren't shown.
 */
export function occurrencesIn(r: Reminder, from: string, to: string, max = 500): Occurrence[] {
  if (r.deleted_at !== null || !r.due_date) return [];
  const inRange = (d: string) => from <= d && d <= to;
  const out: Occurrence[] = r.completed_ahead.filter(inRange).map((date) => ({ date, done: true }));
  if (r.completed_at !== null) return inRange(r.due_date) ? [...out, { date: r.due_date, done: true }] : out;
  const rule = r.advance === "calendar" ? r.recurrence : null;
  let d: string | null = r.due_date;
  for (let step = 0; d && d <= to && out.length < max && step < 5000; step++) {
    if (inRange(d)) out.push({ date: d, done: false });
    if (!rule) break;
    let next = nextAfter(rule, rule.anchor ?? r.due_date, d);
    while (next && r.completed_ahead.includes(next)) next = nextAfter(rule, rule.anchor ?? r.due_date, next);
    d = next;
  }
  return out.sort((a, b) => a.date.localeCompare(b.date));
}

/**
 * Open reminders related to a project: linked to it, or to one of its tasks.
 * Undated last; otherwise by due time.
 */
export function projectReminders(all: Reminder[], projectId: string, taskIds: Set<string>): Reminder[] {
  return all
    .filter((r) => isOpen(r) && (r.project_id === projectId || (r.task_id !== null && taskIds.has(r.task_id))))
    .sort(compareReminders("due"));
}

/** The UTC instant of a local wall time in an IANA zone (DST-aware, like the server). */
export function zonedMs(date: string, minute: number, zone: string): number | null {
  let fmt: Intl.DateTimeFormat;
  try {
    fmt = new Intl.DateTimeFormat("en-US", {
      timeZone: zone,
      hourCycle: "h23",
      year: "numeric",
      month: "numeric",
      day: "numeric",
      hour: "numeric",
      minute: "numeric",
    });
  } catch {
    return null;
  }
  const offset = (ms: number) => {
    const p = Object.fromEntries(fmt.formatToParts(new Date(ms)).map((x) => [x.type, Number(x.value)]));
    return Date.UTC(p.year, p.month - 1, p.day, p.hour % 24, p.minute) - Math.floor(ms / 60_000) * 60_000;
  };
  const [y, m, d] = ymd(date);
  const wall = Date.UTC(y, m - 1, d, Math.floor(minute / 60), minute % 60);
  // As core's `local_to_utc`: try the offsets a day either side. A time that
  // occurs twice is the first; one skipped by spring-forward moves on by the gap.
  const offsets = [offset(wall - 86_400_000), offset(wall + 86_400_000)];
  const candidates = offsets.map((o) => wall - o);
  const valid = candidates.filter((u, i) => offset(u) === offsets[i]);
  return valid.length ? Math.min(...valid) : Math.max(...candidates);
}

const minuteOf = (time: string) => Number(time.slice(0, 2)) * 60 + Number(time.slice(3, 5));

/** `due_at` for a cursor date: its time, or local midnight. */
export function dueAt(date: string | null, time: string | null, zone: string): number | null {
  return date ? zonedMs(date, time ? minuteOf(time) : 0, zone) : null;
}

/** 09:00 tomorrow in the reminder's zone (the server's "tomorrow" snooze). */
export function tomorrowMorning(zone: string, now = Date.now()): number | null {
  let today: string;
  try {
    today = new Intl.DateTimeFormat("en-CA", { timeZone: zone, year: "numeric", month: "2-digit", day: "2-digit" }).format(new Date(now));
  } catch {
    return null;
  }
  return zonedMs(addDays(today, 1), 9 * 60, zone);
}

export type SnoozePreset = "10m" | "1h" | "tomorrow";
export const SNOOZE_PRESETS: { id: SnoozePreset; label: string }[] = [
  { id: "10m", label: "10 min" },
  { id: "1h", label: "1 hour" },
  { id: "tomorrow", label: "Tomorrow" },
];

/** The absolute end of a snooze preset (queued offline as an instant). */
export function snoozeUntil(preset: SnoozePreset, zone: string, now = Date.now()): number | null {
  if (preset === "10m") return now + 10 * 60_000;
  if (preset === "1h") return now + 60 * 60_000;
  return tomorrowMorning(zone, now);
}

/** The current occurrence's date, as complete and snooze name it. */
export const occurrenceOf = (r: Pick<Reminder, "due_date">) => r.due_date ?? undefined;

// ---- Local edits (optimistic and offline; the server's result replaces them) ----

export function newReminderFrom(id: string, n: NewReminder, now: number): Reminder {
  const timezone = n.timezone || deviceZone();
  const due_date = n.due_date ?? null;
  const due_time = due_date ? (n.due_time ?? null) : null;
  return {
    id,
    title: n.title.trim(),
    notes: n.notes ?? "",
    links: n.links ?? [],
    due_date,
    due_time,
    timezone,
    list_id: n.list_id ?? null,
    category_ids: n.category_ids ?? [],
    priority: n.priority ?? "none",
    flagged: n.flagged ?? false,
    project_id: n.project_id ?? null,
    task_id: n.task_id ?? null,
    complete_with_task: !!n.task_id && (n.complete_with_task ?? false),
    alerts: n.alerts ?? defaultAlerts({ due_date, due_time }),
    recurrence: due_date && n.recurrence ? { ...n.recurrence, anchor: due_date } : null,
    advance: n.advance ?? "calendar",
    missed: n.missed ?? "one",
    checklist: n.checklist ?? [],
    snoozed_until: null,
    completed_ahead: [],
    backlog: 0,
    completed_at: null,
    deleted_at: null,
    created_at: now,
    updated_at: now,
    version: 0,
    field_versions: {},
    due_at: dueAt(due_date, due_time, timezone),
  };
}

export function patchReminder(r: Reminder, patch: ReminderPatch, now: number): Reminder {
  const { base_version: _base, ...fields } = patch;
  void _base;
  const next: Reminder = { ...r, ...(fields as Partial<Reminder>), updated_at: now };
  if (next.title !== r.title) next.title = next.title.trim();
  if (!next.due_date) {
    next.due_time = null;
    next.recurrence = null;
  }
  if (!next.task_id) next.complete_with_task = false;
  if (next.recurrence && next.due_date && (patch.due_date !== undefined || patch.recurrence !== undefined)) {
    next.recurrence = { ...next.recurrence, anchor: next.due_date };
  }
  if (patch.due_date !== undefined || patch.due_time !== undefined || patch.timezone !== undefined) {
    next.due_at = dueAt(next.due_date, next.due_time, next.timezone);
    next.snoozed_until = null;
  }
  return next;
}

/** Complete the current occurrence locally: a one-off closes, a series moves on. */
export function completeLocally(r: Reminder, now: number, today = localDate()): Reminder {
  if (!isOpen(r)) return r;
  const rule = r.recurrence;
  if (!rule || !r.due_date) return { ...r, completed_at: now, snoozed_until: null, updated_at: now };
  const anchor = rule.anchor || r.due_date;
  const next = r.advance === "completion" ? afterCompletion(rule, today > r.due_date ? today : r.due_date) : nextAfter(rule, anchor, r.due_date);
  if (!next) return { ...r, completed_at: now, snoozed_until: null, updated_at: now };
  return {
    ...r,
    due_date: next,
    due_at: dueAt(next, r.due_time, r.timezone),
    recurrence: r.advance === "completion" ? { ...rule, anchor: next } : rule,
    snoozed_until: null,
    updated_at: now,
  };
}

export function uncompleteLocally(r: Reminder, now: number): Reminder {
  // A series' undo needs its history; the server's answer replaces this.
  return r.completed_at !== null ? { ...r, completed_at: null, updated_at: now } : r;
}

/** Apply a list op locally; a deleted list leaves its reminders without one. */
export function withoutList(items: Reminder[], listId: string, now: number): Reminder[] {
  return items.map((r) => (r.list_id === listId ? { ...r, list_id: null, updated_at: now } : r));
}

export function withoutCategory(items: Reminder[], categoryId: string): Reminder[] {
  return items.map((r) => (r.category_ids.includes(categoryId) ? { ...r, category_ids: r.category_ids.filter((c) => c !== categoryId) } : r));
}

export function sortLists(lists: ReminderList[]): ReminderList[] {
  return [...lists].sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
}

export function listNameProblem(lists: ReminderList[] | undefined, name: string, self?: string): string | undefined {
  const n = name.trim();
  if (!n) return "Give the list a name.";
  if (n.length > MAX_LIST_NAME) return `At most ${MAX_LIST_NAME} characters.`;
  if ((lists ?? []).some((l) => l.id !== self && l.name.toLowerCase() === n.toLowerCase())) return "A list with that name exists.";
  return undefined;
}

const FIELD_LABELS: Record<string, string> = {
  due_date: "date",
  due_time: "time",
  list_id: "list",
  category_ids: "categories",
  project_id: "project",
  task_id: "task",
  complete_with_task: "complete with task",
  recurrence: "repeat",
  advance: "next date from",
  missed: "if missed",
};

/** A reminder field's name in words, for the sync-conflict view. */
export const fieldLabel = (field: string) => FIELD_LABELS[field] ?? field.replace(/_/g, " ");

/**
 * A field's value in words, for comparing two versions (D-049 M7). `names`
 * turns list and category ids into names where known.
 */
export function fieldValue(field: string, value: unknown, names: (id: string) => string | undefined = () => undefined): string {
  if (value === null || value === undefined || value === "") return "none";
  if (field === "recurrence") return ruleSummary(value as Rule) ?? "none";
  if (field === "priority") return PRIORITY_LABELS[value as Priority] ?? String(value);
  if (field === "alerts") return (value as Alert[]).map(alertLabel).join(", ") || "none";
  if (field === "checklist") {
    const items = value as ChecklistItem[];
    return items.length ? `${items.filter((i) => i.done).length}/${items.length} done` : "none";
  }
  if (typeof value === "boolean") return value ? "yes" : "no";
  if (Array.isArray(value)) return value.map((v) => names(String(v)) ?? (field.endsWith("_ids") ? "(removed)" : String(v))).join(", ") || "none";
  if (field.endsWith("_id")) return names(String(value)) ?? "linked";
  if (typeof value === "string") return value.length > 80 ? `${value.slice(0, 79)}…` : value;
  return String(value);
}
