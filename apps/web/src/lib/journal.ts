/** Journal types and calendar helpers (dates are local `YYYY-MM-DD`). */
import { localDate } from "./tasks";

export interface JournalEntry {
  date: string;
  body: string;
  mood: number | null;
  created_at: number;
  updated_at: number;
}

export interface JournalSummary {
  date: string;
  excerpt: string;
  mood: number | null;
  updated_at: number;
}

export interface PutJournal {
  body?: string;
  mood?: number | null;
  expected_updated_at?: number;
}

export const MOODS = [
  { value: 1, label: "awful" },
  { value: 2, label: "bad" },
  { value: 3, label: "okay" },
  { value: 4, label: "good" },
  { value: 5, label: "great" },
] as const;

/** Five-dot meter, e.g. mood 4 → "●●●●○". */
export function moodMeter(mood: number): string {
  return "●".repeat(mood) + "○".repeat(5 - mood);
}

function parse(date: string): Date {
  const [y, m, d] = date.split("-").map(Number);
  return new Date(y, m - 1, d);
}

export function shiftDate(date: string, days: number): string {
  const d = parse(date);
  d.setDate(d.getDate() + days);
  return localDate(d);
}

/** `YYYY-MM` of a date. */
export function monthOf(date: string): string {
  return date.slice(0, 7);
}

export function shiftMonth(month: string, delta: number): string {
  const [y, m] = month.split("-").map(Number);
  return localDate(new Date(y, m - 1 + delta, 1)).slice(0, 7);
}

/** First and last day of a `YYYY-MM` month. */
export function monthRange(month: string): { from: string; to: string } {
  const [y, m] = month.split("-").map(Number);
  return { from: `${month}-01`, to: localDate(new Date(y, m, 0)) };
}

export function longDate(date: string): string {
  return parse(date).toLocaleDateString([], { weekday: "long", month: "long", day: "numeric", year: "numeric" });
}

export function monthLabel(month: string): string {
  return parse(`${month}-01`).toLocaleDateString([], { month: "long", year: "numeric" });
}
