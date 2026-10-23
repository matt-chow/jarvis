import { useCallback, useEffect, useState } from "react";
import { api } from "../lib/api";
import { localDb } from "../lib/db";
import type { LibraryItem, Progress, ProgressPatch } from "../lib/library";
import { applyLibraryLocally } from "../lib/sync";
import { runOrQueue } from "./offline";
import { useCachedList, type CachedList } from "./useCachedList";
import { SYNCED_EVENT } from "./useSync";

export interface LibraryState {
  list: CachedList<LibraryItem>;
  /** Documents saved on this device (file id → bytes). */
  saved: Map<string, number>;
  /** Works offline: queued and synced later (D-030). */
  setProgress: (fileId: string, patch: ProgressPatch) => Promise<Progress | undefined>;
  save: (fileId: string) => Promise<string | undefined>;
  unsave: (fileId: string) => Promise<void>;
}

const fetchAll = () => api.library();

/** The document library, readable offline: cached list, saved documents, queued progress. */
export function useLibrary(online: boolean): LibraryState {
  const list = useCachedList<LibraryItem>("library:last", "document", fetchAll, online);
  const { reload } = list;
  const [saved, setSaved] = useState<Map<string, number>>(new Map());

  const refreshSaved = useCallback(() => {
    void localDb()
      .then((db) => db.mediaSizes())
      .then(setSaved)
      .catch(() => {});
  }, []);

  useEffect(() => {
    refreshSaved();
    window.addEventListener(SYNCED_EVENT, reload);
    return () => window.removeEventListener(SYNCED_EVENT, reload);
  }, [reload, refreshSaved]);

  const setProgress = useCallback(
    (fileId: string, patch: ProgressPatch) =>
      runOrQueue({
        list,
        noun: "document",
        op: { kind: "library.progress", file_id: fileId, patch },
        server: () => api.setProgress(fileId, patch),
        fromServer: (cur, pr) => cur.map((i) => (i.file.id === fileId ? { ...i, progress: pr } : i)),
        applyLocal: applyLibraryLocally,
        // applyLibraryLocally always sets it.
        local: (cur) => cur.find((i) => i.file.id === fileId)!.progress!,
      }),
    [list],
  );

  return {
    list,
    saved,
    setProgress,
    /** Returns an error message, if any. */
    save: async (fileId) => {
      try {
        const res = await fetch(api.fileContentUrl(fileId), { credentials: "same-origin" });
        if (!res.ok) throw new Error(`download failed (${res.status})`);
        const blob = await res.blob();
        await (await localDb()).putMedia({ id: fileId, blob, size: blob.size, saved_at: Date.now() });
        refreshSaved();
        return undefined;
      } catch (e) {
        return e instanceof Error && /quota/i.test(e.name) ? "Not enough space on this device." : `Couldn't save for offline: ${String(e)}`;
      }
    },
    unsave: async (fileId) => {
      await localDb()
        .then((db) => db.deleteMedia(fileId))
        .catch(() => {});
      refreshSaved();
    },
  };
}

/** A document's bytes: the copy saved on this device, else the server. */
export async function documentBytes(fileId: string): Promise<Blob> {
  const saved = await localDb()
    .then((db) => db.getMedia(fileId))
    .catch(() => undefined);
  if (saved) return saved.blob;
  const res = await fetch(api.fileContentUrl(fileId), { credentials: "same-origin" });
  if (!res.ok) throw new Error(`Couldn't load the document (${res.status}).`);
  return res.blob();
}
