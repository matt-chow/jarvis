import { useCallback, useEffect, useRef, useState } from "react";
import { api, ApiError } from "../lib/api";
import { localDb } from "../lib/db";
import { replay, type SyncResult } from "../lib/sync";
import { reportIfUnauthorized } from "./useSession";

/** Fired after something is queued in the outbox. */
export const OUTBOX_EVENT = "directory:outbox";
/** Fired after a replay changed server data; lists should reload. */
export const SYNCED_EVENT = "directory:synced";

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

  const run = useCallback(async () => {
    if (running.current) return;
    running.current = true;
    try {
      const db = await localDb();
      setPending((await db.outbox()).length);
      if (!navigator.onLine) return;
      setSyncing(true);
      const r: SyncResult = await replay(db, api);
      if (r.stopped === "unauthorized") reportIfUnauthorized(new ApiError(401, "unauthorized"));
      if (r.rejected.length) setRejected((cur) => [...cur, ...r.rejected.map((x) => x.reason)]);
      setPending((await db.outbox()).length);
      if (r.applied > 0 || r.rejected.length > 0) window.dispatchEvent(new Event(SYNCED_EVENT));
    } catch {
      /* IndexedDB unavailable: nothing to sync */
    } finally {
      running.current = false;
      setSyncing(false);
    }
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
