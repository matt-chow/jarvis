/**
 * Calendar types and helpers (mirrors directory-core `calendar`). Dates and
 * times are local wall-clock values: `YYYY-MM-DD`, `HH:MM`.
 */

export type Repeat = "none" | "daily" | "weekly" | "monthly" | "yearly";

export interface CalendarEvent {
  id: string;
  title: string;
  start_date: string;
  end_date: string;
  /** `null` = all day. */
  start_time: string | null;
  end_time: string | null;
  location: string;
  notes: string;
  repeat: Repeat;
  repeat_until: string | null;
  created_at: number;
  updated_at: number;
}

export interface NewEvent {
  title: string;
  start_date: string;
  end_date?: string;
  start_time?: string | null;
  end_time?: string | null;
  location?: string;
  notes?: string;
  repeat?: Repeat;
  repeat_until?: string | null;
}

export type EventPatch = Partial<Omit<CalendarEvent, "id" | "created_at" | "updated_at">>;

export interface Occurrence {
  event: CalendarEvent;
  start_date: string;
  end_date: string;
}

export const REPEAT_LABELS: Record<Repeat, string> = {
  none: "Does not repeat",
  daily: "Every day",
  weekly: "Every week",
  monthly: "Every month",
  yearly: "Every year",
};

const DAY_MS = 86_400_000;

/** Days since 1970-01-01. */
export function dayNumber(date: string): number {
  const [y, m, d] = date.split("-").map(Number);
  return Math.round(Date.UTC(y, m - 1, d) / DAY_MS);
}

export function fromDayNumber(n: number): string {
  return new Date(n * DAY_MS).toISOString().slice(0, 10);
}

export const addDays = (date: string, days: number) => fromDayNumber(dayNumber(date) + days);

/** `y-m-d` if that day exists. */
function realDate(y: number, m: number, d: number): string | undefined {
  const dt = new Date(Date.UTC(y, m - 1, d));
  return dt.getUTCMonth() === m - 1 && dt.getUTCDate() === d ? dt.toISOString().slice(0, 10) : undefined;
}

/** Start dates of an event's occurrences that overlap `from..to` (inclusive). */
export function occurrenceStarts(e: CalendarEvent, from: string, to: string): string[] {
  const span = dayNumber(e.end_date) - dayNumber(e.start_date);
  const earliest = addDays(from, -span);
  const last = e.repeat_until && e.repeat_until < to ? e.repeat_until : to;
  const out: string[] = [];
  const push = (d: string) => {
    if (d >= earliest && d <= last && d >= e.start_date) out.push(d);
  };
  const start = dayNumber(e.start_date);
  switch (e.repeat) {
    case "none":
      push(e.start_date);
      break;
    case "daily":
    case "weekly": {
      const step = e.repeat === "daily" ? 1 : 7;
      const skip = Math.max(dayNumber(earliest) - start, 0);
      for (let n = start + Math.ceil(skip / step) * step; n <= dayNumber(last); n += step) push(fromDayNumber(n));
      break;
    }
    case "monthly":
    case "yearly": {
      const [sy, sm, sd] = e.start_date.split("-").map(Number);
      const [ey, em] = earliest.split("-").map(Number);
      const [ly, lm] = last.split("-").map(Number);
      const months = (y: number, m: number) => y * 12 + (m - 1);
      for (let k = Math.max(months(ey, em), months(sy, sm)); k <= months(ly, lm); k++) {
        const [y, m] = [Math.floor(k / 12), (k % 12) + 1];
        if (e.repeat === "yearly" && m !== sm) continue;
        const d = realDate(y, m, sd);
        if (d) push(d);
      }
      break;
    }
  }
  return out;
}

/** Occurrences overlapping `from..to`: by start date, all-day first, then by time. */
export function occurrences(events: CalendarEvent[], from: string, to: string): Occurrence[] {
  const out = events.flatMap((event) => {
    const span = dayNumber(event.end_date) - dayNumber(event.start_date);
    return occurrenceStarts(event, from, to).map((start) => ({ event, start_date: start, end_date: addDays(start, span) }));
  });
  return out.sort(
    (a, b) =>
      a.start_date.localeCompare(b.start_date) ||
      Number(a.event.start_time !== null) - Number(b.event.start_time !== null) ||
      (a.event.start_time ?? "").localeCompare(b.event.start_time ?? ""),
  );
}

/** Whether an occurrence covers `date`. */
export const covers = (o: Occurrence, date: string) => o.start_date <= date && date <= o.end_date;

/** Monday-first weeks covering `month` (`YYYY-MM`), as dates. */
export function monthGrid(month: string): string[][] {
  const first = `${month}-01`;
  const lead = (new Date(`${first}T00:00:00Z`).getUTCDay() + 6) % 7;
  const start = addDays(first, -lead);
  const [y, m] = month.split("-").map(Number);
  const daysInMonth = new Date(Date.UTC(y, m, 0)).getUTCDate();
  const weeks = Math.ceil((lead + daysInMonth) / 7);
  return Array.from({ length: weeks }, (_, w) => Array.from({ length: 7 }, (_, d) => addDays(start, w * 7 + d)));
}

/** "All day", "09:30", or "09:30–10:15". */
export function timeLabel(e: Pick<CalendarEvent, "start_time" | "end_time">): string {
  if (!e.start_time) return "All day";
  return e.end_time ? `${e.start_time}–${e.end_time}` : e.start_time;
}
