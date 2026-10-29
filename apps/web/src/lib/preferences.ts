/** Owner preferences (mirrors directory-core `preferences`). */
import type { Hours } from "./planner";

export type WidgetId = "capture" | "today" | "habits" | "notes" | "status" | "modules";

export interface Preferences {
  dashboard?: { widgets: WidgetId[] };
  /** Working hours for slot suggestions (D-047). */
  planner?: Hours;
}

/** `null` resets a field to its default; omitted fields are unchanged. */
export interface PreferencesPatch {
  dashboard?: { widgets: WidgetId[] } | null;
  planner?: Hours | null;
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
