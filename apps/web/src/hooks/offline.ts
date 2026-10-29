import { isNetworkError } from "../lib/api";
import { localDb } from "../lib/db";
import type { Op } from "../lib/sync";
import { describeError, type CachedList } from "./useCachedList";
import { reportIfUnauthorized } from "./useSession";
import { OUTBOX_EVENT } from "./useSync";

/**
 * Send a change to the server, or (offline, or while earlier changes are
 * still queued) apply it locally and queue it for replay (DECISIONS D-017).
 */
export async function runOrQueue<T, R>(opts: {
  list: CachedList<T>;
  noun: string;
  op: Op;
  /** The document the op touches (e.g. `note:<id>`), so its editor can find it. */
  doc?: string;
  server: () => Promise<R>;
  fromServer: (current: T[], result: R) => T[];
  applyLocal: (current: T[], op: Op) => T[];
  local: (next: T[]) => R;
}): Promise<R | undefined> {
  const { list } = opts;
  list.setError(undefined);
  const db = await localDb();
  const queued = (await db.outbox()).length > 0;
  if (navigator.onLine && !queued) {
    try {
      const r = await opts.server();
      list.replace(opts.fromServer(list.latest(), r));
      return r;
    } catch (e) {
      if (!isNetworkError(e)) {
        reportIfUnauthorized(e);
        list.setError(describeError(e, opts.noun));
        return undefined;
      }
    }
  }
  await db.enqueue(opts.op, opts.doc);
  const next = opts.applyLocal(list.latest(), opts.op);
  list.replace(next);
  window.dispatchEvent(new Event(OUTBOX_EVENT));
  return opts.local(next);
}

/**
 * Like `runOrQueue`, but the list changes at once, before the server answers
 * (drag, resize, and other planner edits; D-047). If the server refuses the
 * change, the list goes back to how it was and the error is shown; if
 * something else changed it meanwhile, it reloads from the server instead.
 * Offline, or behind queued changes, the change is queued and stays applied.
 * `fromServer` must replace the optimistic copy (upsert), not append.
 */
export async function runOptimistic<T, R>(opts: Parameters<typeof runOrQueue<T, R>>[0]): Promise<R | undefined> {
  const { list } = opts;
  list.setError(undefined);
  const before = list.latest();
  const optimistic = opts.applyLocal(before, opts.op);
  list.replace(optimistic);
  const db = await localDb();
  const queued = (await db.outbox()).length > 0;
  if (navigator.onLine && !queued) {
    try {
      const r = await opts.server();
      list.replace(opts.fromServer(list.latest(), r));
      return r;
    } catch (e) {
      if (!isNetworkError(e)) {
        reportIfUnauthorized(e);
        if (list.latest() === optimistic) list.replace(before);
        else list.reload();
        list.setError(describeError(e, opts.noun));
        return undefined;
      }
    }
  }
  await db.enqueue(opts.op, opts.doc);
  window.dispatchEvent(new Event(OUTBOX_EVENT));
  return opts.local(list.latest());
}

/** Replace the item with the same id, or append it. */
export function upsert<T extends { id: string }>(items: T[], item: T): T[] {
  return items.some((x) => x.id === item.id) ? items.map((x) => (x.id === item.id ? item : x)) : [...items, item];
}
