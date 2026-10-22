import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { localDb } from "../lib/db";
import { sortSummaries, summarize, type Note, type NoteSummary } from "../lib/notes";
import { useCachedList } from "./useCachedList";

const MIGRATED_KEY = "notes:migrated";

export interface NotesState {
  notes?: NoteSummary[];
  loading: boolean;
  stale: boolean;
  error?: string;
  /** Set after legacy device-only notes were moved into the account. */
  migrated?: number;
  reload: () => void;
  create: () => Promise<Note | undefined>;
  /** Reflect a saved note in the list without refetching. */
  saved: (n: Note) => void;
  remove: (id: string) => Promise<boolean>;
}

const fetchAll = () => api.notes();

/** Server notes list (summaries) with a read-only offline copy. */
export function useNotes(online: boolean): NotesState {
  const list = useCachedList<NoteSummary>("notes:last", "note", fetchAll, online);
  const { mutate, items, stale, reload } = list;
  const [migrated, setMigrated] = useState<number>();
  const migrating = useRef(false);

  // One-time move of Sprint 00 Scratchpad notes (IndexedDB only) into the account.
  useEffect(() => {
    if (!items || stale || migrating.current) return;
    migrating.current = true;
    void (async () => {
      const db = await localDb();
      if (await db.getValue<boolean>(MIGRATED_KEY)) return;
      const legacy = await db.listNotes();
      let moved = 0;
      for (const n of legacy.reverse()) {
        if (n.body.trim()) await api.createNote({ body: n.body });
        await db.deleteNote(n.id);
        moved++;
      }
      await db.setValue(MIGRATED_KEY, true);
      if (moved > 0) {
        setMigrated(moved);
        reload();
      }
    })()
      .catch(() => {
        /* retried on next load */
      })
      .finally(() => {
        migrating.current = false;
      });
  }, [items, stale, reload]);

  const create = useCallback(
    () => mutate(() => api.createNote({}), (cur, n) => sortSummaries([summarize(n), ...cur])),
    [mutate],
  );

  const saved = useCallback(
    (n: Note) => void mutate(async () => n, (cur) => sortSummaries(cur.map((s) => (s.id === n.id ? summarize(n) : s)))),
    [mutate],
  );

  const remove = useCallback(
    async (id: string) =>
      (await mutate(
        async () => {
          await api.deleteNote(id);
          return true;
        },
        (cur) => cur.filter((s) => s.id !== id),
      )) ?? false,
    [mutate],
  );

  return { notes: items, loading: list.loading, stale, error: list.error, migrated, reload: list.reload, create, saved, remove };
}
