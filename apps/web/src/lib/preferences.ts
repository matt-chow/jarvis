/** Owner preferences (mirrors directory-core `preferences`). */
import type { Hours } from "./planner";

export type WidgetId = "capture" | "today" | "habits" | "notes" | "status" | "modules";

export interface Preferences {
  dashboard?: { widgets: WidgetId[] };
  /** Working hours for slot suggestions (D-047). */
  planner?: Hours;
  /** How JARVIS schedules sessions (D-048). */
  jarvis?: JarvisPrefs;
}

/** Mirrors directory-core `preferences::JarvisPrefs`. */
export interface JarvisPrefs {
  /** `HH:MM`: no session ends after this. */
  avoid_after?: string;
  /** Free minutes kept around a session, 0–120. */
  break_minutes: number;
  /** Assumed length of a task without an estimate, 15–480. */
  default_task_minutes?: number;
  /** Most session minutes planned on one day, 30–960. */
  max_daily_minutes?: number;
  /** Fill the earliest free time first instead of spreading across days. */
  earliest_first: boolean;
}

/** The JARVIS settings form: blank fields mean "no limit" / "the default". */
export interface JarvisForm {
  avoid_after: string;
  break_minutes: string;
  default_task_minutes: string;
  max_daily_minutes: string;
  earliest_first: boolean;
}

export function jarvisForm(j: JarvisPrefs | undefined): JarvisForm {
  return {
    avoid_after: j?.avoid_after ?? "",
    break_minutes: String(j?.break_minutes ?? 0),
    default_task_minutes: j?.default_task_minutes ? String(j.default_task_minutes) : "",
    max_daily_minutes: j?.max_daily_minutes ? String(j.max_daily_minutes) : "",
    earliest_first: j?.earliest_first ?? false,
  };
}

/** Validate the form as the server does (`check_jarvis`). */
export function parseJarvisForm(f: JarvisForm): { prefs: JarvisPrefs } | { error: string } {
  const num = (v: string, lo: number, hi: number, what: string): number | undefined | string => {
    if (!v.trim()) return undefined;
    const n = Number(v);
    return Number.isInteger(n) && n >= lo && n <= hi ? n : `${what} must be ${lo} to ${hi} minutes.`;
  };
  const brk = num(f.break_minutes || "0", 0, 120, "Break");
  const def = num(f.default_task_minutes, 15, 480, "Default task length");
  const max = num(f.max_daily_minutes, 30, 960, "Daily maximum");
  for (const x of [brk, def, max]) if (typeof x === "string") return { error: x };
  if (f.avoid_after && !/^(([01]\d|2[0-3]):[0-5]\d|24:00)$/.test(f.avoid_after)) return { error: "Use a time like 21:00." };
  const prefs: JarvisPrefs = { break_minutes: (brk as number | undefined) ?? 0, earliest_first: f.earliest_first };
  if (f.avoid_after) prefs.avoid_after = f.avoid_after;
  if (def !== undefined) prefs.default_task_minutes = def as number;
  if (max !== undefined) prefs.max_daily_minutes = max as number;
  return { prefs };
}

/** `null` resets a field to its default; omitted fields are unchanged. */
export interface PreferencesPatch {
  dashboard?: { widgets: WidgetId[] } | null;
  planner?: Hours | null;
  jarvis?: JarvisPrefs | null;
}

export const WIDGETS: readonly { id: WidgetId; label: string; summary: string }[] = [
  { id: "capture", label: "Capture", summary: "Quick capture to the inbox." },
  { id: "today", label: "Today", summary: "Today's events, and tasks due or overdue." },
  { id: "habits", label: "Habits", summary: "Today's habits, to check off." },
  { id: "notes", label: "Recent notes", summary: "The notes you edited last." },
  { id: "status", label: "System status", summary: "API, database, storage, and offline readiness." },
  { id: "modules", label: "Modules", summary: "Every module, with its offline support." },
];

export const DEFAULT_WIDGETS: WidgetId[] = ["capture", "today", "habits", "notes", "status", "modules"];

export const widgetsOf = (p: Preferences | undefined): WidgetId[] => p?.dashboard?.widgets ?? DEFAULT_WIDGETS;

export function applyPreferencesPatch(p: Preferences, patch: PreferencesPatch): Preferences {
  const next = { ...p };
  if (patch.dashboard === null) delete next.dashboard;
  else if (patch.dashboard) next.dashboard = patch.dashboard;
  if (patch.planner === null) delete next.planner;
  else if (patch.planner) next.planner = { ...patch.planner, work_days: [...new Set(patch.planner.work_days)].sort((a, b) => a - b) };
  if (patch.jarvis === null) delete next.jarvis;
  else if (patch.jarvis) next.jarvis = patch.jarvis;
  return next;
}

/** Move a widget up (-1) or down (+1) in the layout. */
export function moveWidget(widgets: WidgetId[], id: WidgetId, by: -1 | 1): WidgetId[] {
  const i = widgets.indexOf(id);
  const j = i + by;
  if (i < 0 || j < 0 || j >= widgets.length) return widgets;
  const next = [...widgets];
  [next[i], next[j]] = [next[j], next[i]];
  return next;
}
