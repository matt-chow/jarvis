import { useCallback, useEffect, useRef, useState } from "react";
import { api, isNetworkError } from "../lib/api";
import { localDb } from "../lib/db";
import { applyPreferencesPatch, type Preferences, type PreferencesPatch } from "../lib/preferences";
import { describeError } from "./useCachedList";
import { reportIfUnauthorized } from "./useSession";
import { OUTBOX_EVENT, SYNCED_EVENT } from "./useSync";

const CACHE_KEY = "preferences:last";

export interface PreferencesState {
  /** `undefined` while loading (and nothing is cached yet). */
  prefs?: Preferences;
  error?: string;
  /** Applies at once; saved to the account, or queued while offline. */
  update: (change: PreferencesPatch | ((current: Preferences) => PreferencesPatch)) => Promise<void>;
}

/** Preferences that follow the owner across devices, cached for offline use. */
export function usePreferences(online: boolean): PreferencesState {
  const [prefs, setPrefs] = useState<Preferences>();
  const [error, setError] = useState<string>();
  const latest = useRef<Preferences>({});

  const commit = useCallback((p: Preferences) => {
    latest.current = p;
    setPrefs(p);
    void localDb()
      .then((db) => db.setValue(CACHE_KEY, p))
      .catch(() => {});
  }, []);

  const load = useCallback(async () => {
    try {
      commit(await api.preferences());
    } catch (e) {
      reportIfUnauthorized(e);
      const cached = await localDb()
        .then((db) => db.getValue<Preferences>(CACHE_KEY))
        .catch(() => undefined);
      commit(cached ?? latest.current);
    }
  }, [commit]);

  useEffect(() => {
    void load();
  }, [load, online]);

  useEffect(() => {
    const onSynced = () => void load();
    window.addEventListener(SYNCED_EVENT, onSynced);
    return () => window.removeEventListener(SYNCED_EVENT, onSynced);
  }, [load]);

  // Changes go to the server one at a time, in order, so quick edits can't
  // overtake each other (last write wins on the server).
  const chain = useRef<Promise<void>>(Promise.resolve());
  const sent = useRef(0);

  const send = useCallback(
    async (patch: PreferencesPatch, n: number) => {
      const db = await localDb().catch(() => undefined);
      const queued = db ? (await db.outbox().catch(() => [])).length > 0 : false;
      if (navigator.onLine && !queued) {
        try {
          const saved = await api.updatePreferences(patch);
          // A later change is already showing; its own response will settle the state.
          if (n === sent.current) commit(saved);
          return;
        } catch (e) {
          if (!isNetworkError(e)) {
            reportIfUnauthorized(e);
            setError(describeError(e, "layout"));
            void load();
            return;
          }
        }
      }
      if (!db) return;
      await db.enqueue({ kind: "prefs.update", patch });
      window.dispatchEvent(new Event(OUTBOX_EVENT));
    },
    [commit, load],
  );

  const update = useCallback(
    (change: PreferencesPatch | ((current: Preferences) => PreferencesPatch)) => {
      setError(undefined);
      // Resolve against the latest state, not a possibly stale render.
      const patch = typeof change === "function" ? change(latest.current) : change;
      commit(applyPreferencesPatch(latest.current, patch));
      const n = ++sent.current;
      chain.current = chain.current.then(() => send(patch, n));
      return chain.current;
    },
    [commit, send],
  );

  return { prefs, error, update };
}
