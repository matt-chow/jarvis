import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { localDb } from "../lib/db";
import { sortSummaries, summarize, type Note, type NoteSummary } from "../lib/notes";
import { clientId, noteDoc, type Op } from "../lib/sync";
import { runOrQueue } from "./offline";
import { useCachedList } from "./useCachedList";
import { SYNCED_EVENT } from "./useSync";

const MIGRATED_KEY = "notes:migrated";

export interface NotesState {
  notes?: NoteSummary[];
  loading: boolean;
  stale: boolean;
  error?: string;
  /** Set after legacy device-only notes were moved into the account. */
  migrated?: number;
  reload: () => void;
  /** A new note, optionally titled (e.g. from an unresolved wiki link). */
  create: (title?: string) => Promise<Note | undefined>;
  /** Reflect a saved note in the list without refetching. */
  saved: (n: Note) => void;
  remove: (id: string) => Promise<boolean>;
}

const fetchAll = () => api.notes();

/** A local summary list after an offline note op. */
function applyNoteLocally(list: NoteSummary[], op: Op, now = Date.now()): NoteSummary[] {
  switch (op.kind) {
    case "note.create":
      return sortSummaries([summarize(blankNote(op.note.id, now, op.note.title)), ...list]);
    case "note.delete":
      return list.filter((s) => s.id !== op.id);
    default:
      return list;
  }
}

const blankNote = (id: string, now: number, title = ""): Note => ({ id, title, body: "", pinned: false, created_at: now, updated_at: now });

/** Server notes list (summaries); create, edit, and delete work offline (D-017, D-018). */
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

  useEffect(() => {
    window.addEventListener(SYNCED_EVENT, reload);
    return () => window.removeEventListener(SYNCED_EVENT, reload);
  }, [reload]);

  const create = useCallback(async (title?: string) => {
    const id = clientId();
    const local = blankNote(id, Date.now(), title);
    // The editor opens a note created offline from this copy.
    await localDb()
      .then((db) => db.setValue(noteDoc(id), local))
      .catch(() => {});
    return runOrQueue({
      list,
      noun: "note",
      op: { kind: "note.create", note: { id, title } },
      doc: noteDoc(id),
      server: () => api.createNote({ id, title }),
      fromServer: (cur, n) => sortSummaries([summarize(n), ...cur]),
      applyLocal: applyNoteLocally,
      local: () => local,
    });
  }, [list]);

  const saved = useCallback(
    (n: Note) => void mutate(async () => n, (cur) => sortSummaries(cur.map((s) => (s.id === n.id ? summarize(n) : s)))),
    [mutate],
  );

  const remove = useCallback(
    async (id: string) =>
      (await runOrQueue({
        list,
        noun: "note",
        op: { kind: "note.delete", id },
        doc: noteDoc(id),
        server: async () => {
          await api.deleteNote(id);
          return true;
        },
        fromServer: (cur) => cur.filter((s) => s.id !== id),
        applyLocal: applyNoteLocally,
        local: () => true,
      })) ?? false,
    [list],
  );

  return { notes: items, loading: list.loading, stale, error: list.error, migrated, reload: list.reload, create, saved, remove };
}
