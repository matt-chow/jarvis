import { useCallback, useEffect, useState } from "react";
import { fetchHealth, type HealthReport } from "../lib/api";
import { localDb } from "../lib/db";

const CACHE_KEY = "health:last";

export interface HealthState {
  report?: HealthReport;
  checkedAt?: number;
  /** Report came from IndexedDB because the API was unreachable. */
  stale: boolean;
  loading: boolean;
  error?: string;
  refresh: () => void;
}

interface Cached {
  report: HealthReport;
  checkedAt: number;
}

/** Live health with a last-known-good copy kept on device for offline use. */
export function useHealth(online: boolean): HealthState {
  const [state, setState] = useState<Omit<HealthState, "refresh">>({ stale: false, loading: true });

  const refresh = useCallback(async () => {
    setState((s) => ({ ...s, loading: true }));
    try {
      const report = await fetchHealth();
      const checkedAt = Date.now();
      setState({ report, checkedAt, stale: false, loading: false });
      await (await localDb()).setValue<Cached>(CACHE_KEY, { report, checkedAt });
    } catch (e) {
      const cached = await localDb()
        .then((db) => db.getValue<Cached>(CACHE_KEY))
        .catch(() => undefined);
      setState({
        report: cached?.report,
        checkedAt: cached?.checkedAt,
        stale: Boolean(cached),
        loading: false,
        error: e instanceof Error ? e.message : String(e),
      });
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh, online]);

  return { ...state, refresh: () => void refresh() };
}
