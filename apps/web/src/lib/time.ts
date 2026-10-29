/**
 * Wall-clock time helpers. Events and blocks store a local date and `HH:MM`
 * times, plus (since D-047) the IANA zone they were made in. The calendar
 * shows everything in this device's zone; an entry made in another zone is
 * converted, DST included. Durations are real elapsed minutes in the entry's
 * zone (a 01:00–04:00 block on a spring-forward night is 2 hours).
 */
import { addDays } from "./calendar";

/** This device's IANA zone, e.g. `Europe/London`. */
export function deviceZone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";
  } catch {
    return "UTC";
  }
}

/** `HH:MM` → minutes since midnight (`24:00` = 1440). */
export function toMinutes(hhmm: string): number {
  const [h, m] = hhmm.split(":").map(Number);
  return h * 60 + m;
}

/** Minutes since midnight → `HH:MM` (1440 → `24:00`). */
export function fromMinutes(minutes: number): string {
  const m = Math.max(0, Math.min(1440, Math.round(minutes)));
  return `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
}

/** Round to the nearest `step` minutes. */
export const snap = (minutes: number, step = 15) => Math.round(minutes / step) * step;

const formats = new Map<string, Intl.DateTimeFormat>();

function format(zone: string): Intl.DateTimeFormat {
  let f = formats.get(zone);
  if (!f) {
    f = new Intl.DateTimeFormat("en-US", {
      timeZone: zone,
      hourCycle: "h23",
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    });
    formats.set(zone, f);
  }
  return f;
}

/** Wall-clock date and minutes of an instant in `zone`. */
export function wallClock(ms: number, zone: string): { date: string; minutes: number } {
  const p = Object.fromEntries(format(zone).formatToParts(new Date(ms)).map((x) => [x.type, x.value]));
  return { date: `${p.year}-${p.month}-${p.day}`, minutes: (Number(p.hour) % 24) * 60 + Number(p.minute) };
}

/** Minutes `zone` is ahead of UTC at an instant. */
function offset(ms: number, zone: string): number {
  const w = wallClock(ms, zone);
  const [y, m, d] = w.date.split("-").map(Number);
  return Math.round((Date.UTC(y, m - 1, d) + w.minutes * 60_000 - Math.floor(ms / 60_000) * 60_000) / 60_000);
}

/**
 * The instant of a wall-clock time in `zone`. A time skipped by a DST jump
 * resolves forward (02:30 → 03:30); a repeated one resolves to the first.
 */
export function instant(date: string, minutes: number, zone: string): number {
  if (minutes >= 1440) return instant(addDays(date, Math.floor(minutes / 1440)), minutes % 1440, zone);
  const [y, m, d] = date.split("-").map(Number);
  const guess = Date.UTC(y, m - 1, d) + minutes * 60_000;
  const first = guess - offset(guess, zone) * 60_000;
  const second = guess - offset(first, zone) * 60_000;
  const real = [first, second].filter((t) => {
    const w = wallClock(t, zone);
    return w.date === date && w.minutes === minutes;
  });
  // Ambiguous: the earlier instant. Skipped: the later one (forward).
  return real.length ? Math.min(...real) : Math.max(first, second);
}

/** Convert a wall-clock time between zones. */
export function convert(date: string, minutes: number, from: string, to: string): { date: string; minutes: number } {
  if (from === to) return { date, minutes };
  return wallClock(instant(date, minutes, from), to);
}

/** Real minutes between two wall-clock times on `date` in `zone` (DST-aware). */
export function elapsed(date: string, start: number, end: number, zone: string): number {
  return Math.round((instant(date, end, zone) - instant(date, start, zone)) / 60_000);
}

/** "9:30", "14:00" in the user's locale. */
export function clockLabel(minutes: number): string {
  const d = new Date(Date.UTC(2000, 0, 1, Math.floor(minutes / 60) % 24, minutes % 60));
  return d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit", timeZone: "UTC" });
}

/** "1 h 30 min", "45 min" (short duration). */
export function durationLabel(minutes: number): string {
  const m = Math.max(0, Math.round(minutes));
  const h = Math.floor(m / 60);
  if (h === 0) return `${m} min`;
  return m % 60 === 0 ? `${h} h` : `${h} h ${m % 60} min`;
}
