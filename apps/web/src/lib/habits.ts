/** Habit types and helpers (mirrors directory-core `habits`). Dates are local `YYYY-MM-DD`. */
import { shiftDate as addDays } from "./journal";
import { localDate } from "./tasks";

/** 0 = Monday … 6 = Sunday. */
export function weekday(date: string): number {
  const [y, m, d] = date.split("-").map(Number);
  return (new Date(y, m - 1, d).getDay() + 6) % 7;
}

export interface Habit {
  id: string;
  name: string;
  /** Scheduled weekdays: 0 = Monday … 6 = Sunday, sorted. */
  days: number[];
  archived: boolean;
  created_at: number;
  updated_at: number;
}

/** A habit as listed: recent check-ins (oldest first) and the current streak. */
export interface HabitView extends Habit {
  checkins: string[];
  streak: number;
}

export interface NewHabit {
  name: string;
  days?: number[];
}

export type HabitPatch = Partial<Pick<Habit, "name" | "days" | "archived">>;

/** History the client asks for: enough to recompute streaks locally (server look-back). */
export const HISTORY_DAYS = 366;

export const WEEKDAY_LETTERS = ["M", "T", "W", "T", "F", "S", "S"] as const;
export const WEEKDAY_NAMES = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"] as const;
export const EVERY_DAY = [0, 1, 2, 3, 4, 5, 6];

/** Same rule as the server: a scheduled day without a check-in ends the streak, except today. */
export function streak(days: number[], checked: ReadonlySet<string>, today: string): number {
  let n = 0;
  for (let back = 0; back < HISTORY_DAYS; back++) {
    const d = addDays(today, -back);
    if (checked.has(d)) n++;
    else if (back > 0 && days.includes(weekday(d))) break;
  }
  return n;
}

/** The `count` dates ending at `today`, oldest first. */
export function lastDays(today: string, count: number): string[] {
  return Array.from({ length: count }, (_, i) => addDays(today, i - count + 1));
}

/** "Every day", "Weekdays", or e.g. "Mon, Wed, Fri". */
export function describeDays(days: number[]): string {
  const key = days.join("");
  if (key === "0123456") return "Every day";
  if (key === "01234") return "Weekdays";
  if (key === "56") return "Weekends";
  return days.map((d) => WEEKDAY_NAMES[d].slice(0, 3)).join(", ");
}

/** A habit with `date` checked or cleared, and its streak recomputed. */
export function withCheck(h: HabitView, date: string, done: boolean, today: string): HabitView {
  const set = new Set(h.checkins);
  if (done) set.add(date);
  else set.delete(date);
  const checkins = [...set].sort();
  return { ...h, checkins, streak: streak(h.days, new Set(checkins), today) };
}

/** How a day reads in a habit's grid: checked, a scheduled day that was missed, a rest day, today still open, or outside the habit's life. */
export type DayState = "done" | "missed" | "rest" | "today" | "future" | "before";

export function dayState(h: Pick<Habit, "days" | "created_at">, date: string, checked: ReadonlySet<string>, today: string): DayState {
  if (checked.has(date)) return "done";
  if (date > today) return "future";
  if (date < localDate(new Date(h.created_at))) return "before";
  if (date === today) return "today";
  return h.days.includes(weekday(date)) ? "missed" : "rest";
}

/** `weeks` columns of Monday-to-Sunday dates, the last holding `today`, oldest first. */
export function gridWeeks(today: string, weeks: number): string[][] {
  const start = addDays(today, -weekday(today) - (weeks - 1) * 7);
  return Array.from({ length: weeks }, (_, w) => Array.from({ length: 7 }, (_, d) => addDays(start, w * 7 + d)));
}

/** Scheduled days done out of scheduled days due (today counts once checked). */
export function completion(h: Pick<Habit, "days" | "created_at">, dates: string[], checked: ReadonlySet<string>, today: string): { done: number; due: number } {
  let done = 0;
  let due = 0;
  for (const d of dates) {
    const s = dayState(h, d, checked, today);
    if (s === "done") done++;
    if (s === "done" || s === "missed") due++;
  }
  return { done, due };
}

/** Streak lengths worth a quiet word. */
export const MILESTONES: Record<number, string> = { 7: "a full week", 30: "a month", 100: "a hundred days", 365: "a whole year" };
