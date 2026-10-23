import { useCallback, useEffect, useRef, useState } from "react";
import { ApiError, isNetworkError } from "../lib/api";
import { localDb } from "../lib/db";
import { createGate } from "../lib/gate";
import { reportIfUnauthorized } from "./useSession";

export interface CachedList<T> {
  items?: T[];
  loading: boolean;
  /** Showing the copy saved on this device because the server was unreachable. */
  stale: boolean;
  error?: string;
  reload: () => void;
  /** Run a server mutation and apply its result to the list (and the cache). */
  mutate: <R>(fn: () => Promise<R>, apply: (current: T[], result: R) => T[]) => Promise<R | undefined>;
  /** Current items (including local edits not yet rendered). */
  latest: () => T[];
  /** Replace items locally (and in the offline copy) without a server call. */
  replace: (next: T[]) => void;
  setError: (message: string | undefined) => void;
}

export function describeError(e: unknown, noun: string): string {
  if (e instanceof ApiError) {
    if (e.code === "invalid_input" && e.detail) return `Check ${e.detail.replace(":", " —")}.`;
    if (e.status === 404) return `That ${noun} no longer exists.`;
    return `Request failed (${e.code}).`;
  }
  if (isNetworkError(e)) return "Can't reach the server right now.";
  return String(e);
}

/**
 * A server-backed list with a read-only offline copy in IndexedDB under
 * `cacheKey`. Mutations go to the server first; the local list follows.
 */
export function useCachedList<T>(cacheKey: string, noun: string, fetchAll: () => Promise<T[]>, online: boolean): CachedList<T> {
  const [items, setItems] = useState<T[]>();
  const [loading, setLoading] = useState(true);
  const [stale, setStale] = useState(false);
  const [error, setError] = useState<string>();
  const latest = useRef<T[] | undefined>(undefined);
  const fetchRef = useRef(fetchAll);
  fetchRef.current = fetchAll;
  // A slow load must not overwrite a newer load or a local edit (D-039).
  const gate = useRef(createGate()).current;

  const commit = useCallback(
    (next: T[]) => {
      gate.bump();
      latest.current = next;
      setItems(next);
      setStale(false);
      void localDb()
        .then((db) => db.setValue(cacheKey, next))
        .catch(() => {});
    },
    [cacheKey, gate],
  );

  const reload = useCallback(async () => {
    setLoading(true);
    const token = gate.begin();
    try {
      const fresh = await fetchRef.current();
      if (!gate.current(token)) return;
      commit(fresh);
      setError(undefined);
    } catch (e) {
      reportIfUnauthorized(e);
      const cached = await localDb()
        .then((db) => db.getValue<T[]>(cacheKey))
        .catch(() => undefined);
      if (!gate.current(token)) return;
      if (cached) {
        latest.current = cached;
        setItems(cached);
        setStale(true);
      }
      setError(describeError(e, noun));
    } finally {
      setLoading(false);
    }
  }, [cacheKey, commit, gate, noun]);

  useEffect(() => {
    void reload();
  }, [reload, online]);

  const mutate = useCallback(
    async <R,>(fn: () => Promise<R>, apply: (current: T[], result: R) => T[]) => {
      setError(undefined);
      try {
        const result = await fn();
        commit(apply(latest.current ?? [], result));
        return result;
      } catch (e) {
        reportIfUnauthorized(e);
        setError(describeError(e, noun));
        return undefined;
      }
    },
    [commit, noun],
  );

  const reloadNow = useCallback(() => void reload(), [reload]);
  const latestItems = useCallback(() => latest.current ?? [], []);
  const replace = useCallback(
    (next: T[]) => {
      gate.bump();
      latest.current = next;
      setItems(next);
      void localDb()
        .then((db) => db.setValue(cacheKey, next))
        .catch(() => {});
    },
    [cacheKey, gate],
  );
  return { items, loading, stale, error, reload: reloadNow, mutate, latest: latestItems, replace, setError };
}
