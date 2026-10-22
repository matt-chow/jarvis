import { useCallback, useEffect, useRef, useState } from "react";
import { api, ApiError } from "../lib/api";
import { localDb } from "../lib/db";
import { conflictKey, replay, type Conflict, type SyncResult } from "../lib/sync";
import { reportIfUnauthorized } from "./useSession";

/** Fired after something is queued in the outbox. */
export const OUTBOX_EVENT = "directory:outbox";
/** Fired after a replay changed server data; lists should reload. */
export const SYNCED_EVENT = "directory:synced";

/** `SYNCED_EVENT` detail, for open editors. */
export interface SyncedDetail {
  /** New server versions of saved documents, by document key (`note:<id>`). */
  versions: Record<string, number>;
  /** Documents whose queued save conflicted (see `conflictKey`). */
  conflicts: string[];
}

function describeConflict(c: Conflict): string {
  const what = c.doc.startsWith("journal:") ? `the journal entry for ${c.doc.slice("journal:".length)}` : "a note";
  return `An offline edit to ${what} conflicts with a change made elsewhere. Open it to choose which version to keep.`;
}

export interface SyncState {
  pending: number;
  syncing: boolean;
  /** Messages for changes the server rejected (dropped from the outbox). */
  rejected: string[];
  dismiss: () => void;
}

/** Replays the offline outbox on load, on reconnect, and when something is queued. */
export function useSync(online: boolean): SyncState {
  const [pending, setPending] = useState(0);
  const [syncing, setSyncing] = useState(false);
  const [rejected, setRejected] = useState<string[]>([]);
  const running = useRef(false);
  const again = useRef(false);

  const run = useCallback(async (): Promise<void> => {
    if (running.current) {
      // Something was queued (or edited in place) mid-replay: go round once more.
      again.current = true;
      return;
    }
    running.current = true;
    again.current = false;
    try {
      const db = await localDb();
      setPending((await db.outbox()).length);
      if (!navigator.onLine) return;
      setSyncing(true);
      const r: SyncResult = await replay(db, api);
      if (r.stopped === "unauthorized") reportIfUnauthorized(new ApiError(401, "unauthorized"));
      for (const c of r.conflicts) await db.setValue(conflictKey(c.doc), c);
      const messages = [...r.rejected.map((x) => x.reason), ...r.conflicts.map(describeConflict)];
      if (messages.length) setRejected((cur) => [...cur, ...messages]);
      setPending((await db.outbox()).length);
      if (r.applied > 0 || messages.length > 0) {
        const detail: SyncedDetail = { versions: r.versions, conflicts: r.conflicts.map((c) => c.doc) };
        window.dispatchEvent(new CustomEvent(SYNCED_EVENT, { detail }));
      }
    } catch {
      /* IndexedDB unavailable: nothing to sync */
    } finally {
      running.current = false;
      setSyncing(false);
    }
    if (again.current) return run();
  }, []);

  useEffect(() => {
    void run();
  }, [run, online]);

  useEffect(() => {
    const onQueued = () => void run();
    window.addEventListener(OUTBOX_EVENT, onQueued);
    return () => window.removeEventListener(OUTBOX_EVENT, onQueued);
  }, [run]);

  return { pending, syncing, rejected, dismiss: () => setRejected([]) };
}
