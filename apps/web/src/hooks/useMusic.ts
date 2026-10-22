import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { localDb } from "../lib/db";
import type { MusicLibrary, Playlist } from "../lib/music";
import { describeError } from "./useCachedList";
import { reportIfUnauthorized } from "./useSession";

const CACHE_KEY = "music:last";

export interface MusicState {
  library?: MusicLibrary;
  /** Showing the copy saved on this device (server unreachable). */
  stale: boolean;
  loading: boolean;
  error?: string;
  reload: () => void;
  /** Track ids saved on this device for offline playback, with sizes. */
  saved: Map<string, number>;
  save: (id: string) => Promise<void>;
  unsave: (id: string) => Promise<void>;
  /** Playlist changes need a connection. */
  createPlaylist: (name: string) => Promise<Playlist | undefined>;
  updatePlaylist: (id: string, patch: { name?: string; tracks?: string[] }) => Promise<boolean>;
  deletePlaylist: (id: string) => Promise<boolean>;
}

export function useMusic(online: boolean): MusicState {
  const [library, setLibrary] = useState<MusicLibrary>();
  const [stale, setStale] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string>();
  const [saved, setSaved] = useState<Map<string, number>>(new Map());
  const latest = useRef<MusicLibrary>(undefined);

  const commit = useCallback((lib: MusicLibrary) => {
    latest.current = lib;
    setLibrary(lib);
    void localDb()
      .then((db) => db.setValue(CACHE_KEY, lib))
      .catch(() => {});
  }, []);

  const refreshSaved = useCallback(() => {
    void localDb()
      .then((db) => db.mediaSizes())
      .then(setSaved)
      .catch(() => {});
  }, []);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      commit(await api.music());
      setStale(false);
      setError(undefined);
    } catch (e) {
      reportIfUnauthorized(e);
      const cached = await localDb()
        .then((db) => db.getValue<MusicLibrary>(CACHE_KEY))
        .catch(() => undefined);
      if (cached) {
        latest.current = cached;
        setLibrary(cached);
        setStale(true);
      } else setError(describeError(e, "music library"));
    } finally {
      setLoading(false);
    }
  }, [commit]);

  useEffect(() => {
    void reload();
    refreshSaved();
  }, [reload, refreshSaved, online]);

  const guard = useCallback(async <T,>(fn: () => Promise<T>): Promise<T | undefined> => {
    setError(undefined);
    try {
      return await fn();
    } catch (e) {
      reportIfUnauthorized(e);
      setError(describeError(e, "playlist"));
      return undefined;
    }
  }, []);

  const withPlaylists = useCallback(
    (f: (ps: Playlist[]) => Playlist[]) => {
      const cur = latest.current;
      if (cur) commit({ ...cur, playlists: f(cur.playlists) });
    },
    [commit],
  );

  return {
    library,
    stale,
    loading,
    error,
    reload: () => void reload(),
    saved,
    save: async (id) => {
      setError(undefined);
      try {
        const res = await fetch(api.fileContentUrl(id), { credentials: "same-origin" });
        if (!res.ok) throw new Error(`download failed (${res.status})`);
        const blob = await res.blob();
        const db = await localDb();
        await db.putMedia({ id, blob, size: blob.size, saved_at: Date.now() });
      } catch (e) {
        setError(e instanceof Error && /quota/i.test(e.name) ? "Not enough space on this device." : `Couldn't save for offline: ${String(e)}`);
      }
      refreshSaved();
    },
    unsave: async (id) => {
      await localDb()
        .then((db) => db.deleteMedia(id))
        .catch(() => {});
      refreshSaved();
    },
    createPlaylist: async (name) => {
      const p = await guard(() => api.createPlaylist(name));
      if (p) withPlaylists((ps) => [...ps, p].sort((a, b) => a.name.localeCompare(b.name)));
      return p;
    },
    updatePlaylist: async (id, patch) => {
      const p = await guard(() => api.updatePlaylist(id, patch));
      if (p) withPlaylists((ps) => ps.map((x) => (x.id === id ? p : x)));
      return p !== undefined;
    },
    deletePlaylist: async (id) => {
      const ok = await guard(async () => {
        await api.deletePlaylist(id);
        return true;
      });
      if (ok) withPlaylists((ps) => ps.filter((x) => x.id !== id));
      return ok ?? false;
    },
  };
}
