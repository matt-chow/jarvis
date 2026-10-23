/**
 * Client error reporting (DECISIONS D-027): crashes and unhandled errors go to
 * the owner's diagnostics log, deduplicated and throttled so a loop can't
 * flood it. Reporting never throws.
 */

export interface ClientError {
  id: string;
  at: number;
  module: string;
  message: string;
  stack: string;
  user_agent: string;
  app_version: string;
}

export interface ErrorReport {
  message: string;
  module?: string;
  stack?: string;
  app_version?: string;
}

/** At most this many reports per minute… */
export const MAX_PER_MINUTE = 5;
/** …and the same message at most once per this window. */
export const DEDUPE_MS = 5 * 60_000;

export class Throttle {
  private sent: number[] = [];
  private seen = new Map<string, number>();

  /** Whether a report with this key may be sent at `now` (and record it if so). */
  allow(key: string, now = Date.now()): boolean {
    const last = this.seen.get(key);
    if (last !== undefined && now - last < DEDUPE_MS) return false;
    this.sent = this.sent.filter((t) => now - t < 60_000);
    if (this.sent.length >= MAX_PER_MINUTE) return false;
    this.sent.push(now);
    this.seen.set(key, now);
    return true;
  }
}

/** A readable message and stack from anything thrown. */
export function describeThrown(e: unknown): { message: string; stack?: string } {
  if (e instanceof Error) return { message: `${e.name}: ${e.message}`, stack: e.stack };
  if (typeof e === "string") return { message: e };
  try {
    return { message: JSON.stringify(e) ?? String(e) };
  } catch {
    return { message: String(e) };
  }
}
