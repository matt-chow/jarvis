/**
 * What each screen shows (its selection, the day on the calendar), so
 * JARVIS can understand "this task" or "that session" (D-048). Screens
 * report it; App notes the last screen before Ask; Ask sends it as a hint
 * the server checks against the owner's data.
 */
import { useEffect } from "react";
import type { Screen } from "./chat";

type Shown = Omit<Screen, "route">;

const shown = new Map<string, Shown>();
let last: string | undefined;

/** Record what `route` shows (undefined fields clear). */
export function setShown(route: string, what: Shown) {
  shown.set(route, what);
}

/** App calls this on each screen change; Ask itself isn't a context. */
export function noteRoute(route: string) {
  if (route !== "ask") last = route;
}

/** The screen the owner came to Ask from, without empty fields. */
export function currentScreen(): Screen | undefined {
  if (!last) return undefined;
  const out: Screen = { route: last };
  for (const [k, v] of Object.entries(shown.get(last) ?? {}) as [keyof Shown, string | undefined][]) {
    if (v) out[k] = v;
  }
  return out;
}

/** The calendar item a planner key names (`b:id:date`, `e:id:…`, `d:taskId`). */
export function shownItem(key: string | undefined): Shown {
  const [kind, id] = key?.split(":") ?? [];
  if (!id) return {};
  if (kind === "b") return { block_id: id };
  if (kind === "e") return { event_id: id };
  if (kind === "d") return { task_id: id };
  return {};
}

/** For tests. */
export function resetFocus() {
  shown.clear();
  last = undefined;
}

/** Report what this screen shows while it's mounted and as it changes. */
export function useShown(route: string, what: Shown) {
  const key = JSON.stringify(what);
  useEffect(() => setShown(route, JSON.parse(key) as Shown), [route, key]);
}
