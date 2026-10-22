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
  update: (patch: PreferencesPatch) => Promise<void>;
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

  const update = useCallback(
    async (patch: PreferencesPatch) => {
      setError(undefined);
      commit(applyPreferencesPatch(latest.current, patch));
      const db = await localDb().catch(() => undefined);
      const queued = db ? (await db.outbox().catch(() => [])).length > 0 : false;
      if (navigator.onLine && !queued) {
        try {
          commit(await api.updatePreferences(patch));
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

  return { prefs, error, update };
}
