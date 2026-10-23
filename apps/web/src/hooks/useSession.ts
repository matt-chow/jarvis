import { useCallback, useEffect, useState } from "react";
import { api, ApiError, type SessionUser } from "../lib/api";
import { localDb, deleteLocalData } from "../lib/db";

const CACHE_KEY = "session:last";

export type SessionStatus =
  | { kind: "loading" }
  | { kind: "signed-out"; setupRequired: boolean }
  /** `verified: false` = server unreachable; using the last session seen on this device. */
  | { kind: "signed-in"; user: SessionUser; verified: boolean }
  | { kind: "unavailable"; message: string };

export interface SessionApi {
  status: SessionStatus;
  refresh: () => Promise<void>;
  signedIn: (user: SessionUser) => void;
  /** `forget`: also delete everything saved in this browser (D-028). */
  signOut: (opts?: { forget?: boolean }) => Promise<void>;
}

interface Cached {
  user: SessionUser;
  at: number;
}

async function cache(user: SessionUser | undefined) {
  const db = await localDb();
  await db.setValue<Cached | null>(CACHE_KEY, user ? { user, at: Date.now() } : null);
}

/** Dispatched by modules when the API answers 401, so the shell re-checks the session. */
export const UNAUTHORIZED_EVENT = "directory:unauthorized";

export function reportIfUnauthorized(e: unknown) {
  if (e instanceof ApiError && e.status === 401) window.dispatchEvent(new Event(UNAUTHORIZED_EVENT));
}

export function useSession(online: boolean): SessionApi {
  const [status, setStatus] = useState<SessionStatus>({ kind: "loading" });

  const refresh = useCallback(async () => {
    try {
      const s = await api.session();
      if (s.authenticated && s.user) {
        setStatus({ kind: "signed-in", user: s.user, verified: true });
        await cache(s.user).catch(() => {});
      } else {
        setStatus({ kind: "signed-out", setupRequired: s.setupRequired });
        await cache(undefined).catch(() => {});
      }
    } catch (e) {
      // Unreachable or server error: fall back to the last session seen here.
      const cached = await localDb()
        .then((db) => db.getValue<Cached | null>(CACHE_KEY))
        .catch(() => undefined);
      if (cached) setStatus({ kind: "signed-in", user: cached.user, verified: false });
      else setStatus({ kind: "unavailable", message: e instanceof Error ? e.message : String(e) });
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh, online]);

  useEffect(() => {
    const onUnauthorized = () => void refresh();
    window.addEventListener(UNAUTHORIZED_EVENT, onUnauthorized);
    return () => window.removeEventListener(UNAUTHORIZED_EVENT, onUnauthorized);
  }, [refresh]);

  const signedIn = useCallback((user: SessionUser) => {
    setStatus({ kind: "signed-in", user, verified: true });
    void cache(user).catch(() => {});
  }, []);

  const signOut = useCallback(async (opts?: { forget?: boolean }) => {
    try {
      await api.logout();
    } finally {
      await cache(undefined).catch(() => {});
      if (opts?.forget) await deleteLocalData().catch(() => {});
      setStatus({ kind: "signed-out", setupRequired: false });
    }
  }, []);

  return { status, refresh, signedIn, signOut };
}
