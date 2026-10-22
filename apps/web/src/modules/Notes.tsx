import { useCallback, useEffect, useRef, useState } from "react";
import type { ModuleProps } from "../App";
import { Button, Panel, inputClass } from "../components/ui";
import { describeError } from "../hooks/useCachedList";
import { useNotes, type NotesState } from "../hooks/useNotes";
import { reportIfUnauthorized } from "../hooks/useSession";
import { api, ApiError, isNetworkError } from "../lib/api";
import { localDb } from "../lib/db";
import { formatDate, formatTime } from "../lib/format";
import type { Note } from "../lib/notes";

// Provisional screen: awaiting design integration (see docs/MODULES.md › Design status).

const SAVE_DELAY_MS = 800;
const small = "min-h-7 px-2 text-[10px]";
const cacheKey = (id: string) => `note:${id}`;

export function Notes({ online }: ModuleProps) {
  const state = useNotes(online);
  const [selected, setSelected] = useState<string>();
  const readOnly = state.stale || !online;

  useEffect(() => {
    if (!selected && state.notes?.length) setSelected(state.notes[0].id);
    if (selected && state.notes && !state.notes.some((n) => n.id === selected)) setSelected(state.notes[0]?.id);
  }, [selected, state.notes]);

  const create = async () => {
    const n = await state.create();
    if (n) setSelected(n.id);
  };

  return (
    <div className="grid items-start gap-5 md:grid-cols-[17rem_minmax(0,1fr)]">
      <Panel
        title={`notes${state.notes ? ` · ${state.notes.length}` : ""}`}
        actions={
          <Button onClick={() => void create()} variant="primary" disabled={readOnly} className={small}>
            + new
          </Button>
        }
      >
        {state.migrated ? (
          <p role="status" className="mb-3 text-sm text-muted">
            Moved {state.migrated} note{state.migrated === 1 ? "" : "s"} from this device into your account.
          </p>
        ) : null}
        {readOnly && state.notes && <p className="mb-3 text-sm text-muted">Offline: notes are read-only.</p>}
        {state.error && !readOnly && (
          <p role="alert" className="mb-3 text-sm text-bad">
            {state.error}
          </p>
        )}
        {state.loading && !state.notes ? (
          <p className="text-sm text-muted">Loading notes…</p>
        ) : !state.notes ? (
          <div className="text-sm">
            <p className="text-muted">Notes couldn't be loaded.</p>
            <Button className={`${small} mt-2`} onClick={state.reload}>
              retry
            </Button>
          </div>
        ) : state.notes.length === 0 ? (
          <p className="text-sm text-muted">No notes yet. Create one to get started.</p>
        ) : (
          <ul className="-mx-2 flex max-h-80 flex-col gap-0.5 overflow-y-auto md:max-h-[65vh]">
            {state.notes.map((n) => (
              <li key={n.id}>
                <button
                  type="button"
                  onClick={() => setSelected(n.id)}
                  aria-current={n.id === selected}
                  className={`w-full rounded-sm px-2 py-1.5 text-left ${n.id === selected ? "bg-sunken" : "hover:bg-sunken/60"}`}
                >
                  <span className="flex items-center gap-1.5 text-sm">
                    {n.pinned && (
                      <span aria-label="Pinned" className="text-accent">
                        ▲
                      </span>
                    )}
                    <span className="truncate">{n.title}</span>
                  </span>
                  {n.excerpt && <span className="block truncate text-xs text-muted">{n.excerpt}</span>}
                  <span className="font-mono text-[11px] text-muted">
                    {new Date(n.updated_at).toDateString() === new Date().toDateString() ? formatTime(n.updated_at) : formatDate(n.updated_at)}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </Panel>

      {selected ? (
        <NoteEditor key={selected} id={selected} state={state} readOnly={readOnly} />
      ) : (
        <Panel title="editor">
          <p className="text-sm text-muted">Select or create a note.</p>
        </Panel>
      )}
    </div>
  );
}

type SaveStatus = "saved" | "dirty" | "saving" | "offline" | "error";

const STATUS_TEXT: Record<SaveStatus, string> = {
  saved: "saved",
  dirty: "unsaved changes",
  saving: "saving…",
  offline: "offline · not saved",
  error: "save failed",
};

function NoteEditor({ id, state, readOnly }: { id: string; state: NotesState; readOnly: boolean }) {
  const [note, setNote] = useState<Note>();
  const [loadError, setLoadError] = useState<string>();
  const [fromCache, setFromCache] = useState(false);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [status, setStatus] = useState<SaveStatus>("saved");
  const [conflict, setConflict] = useState<Note>();
  const [confirmDelete, setConfirmDelete] = useState(false);
  // The version we last read or wrote; sent as expected_updated_at.
  const version = useRef(0);
  const draft = useRef({ title: "", body: "" });
  const timer = useRef<number | undefined>(undefined);
  const { saved } = state;

  const adopt = useCallback((n: Note) => {
    version.current = n.updated_at;
    draft.current = { title: n.title, body: n.body };
    setNote(n);
    setTitle(n.title);
    setBody(n.body);
    setStatus("saved");
    void localDb()
      .then((db) => db.setValue(cacheKey(n.id), n))
      .catch(() => {});
  }, []);

  useEffect(() => {
    let cancelled = false;
    api
      .note(id)
      .then((n) => !cancelled && adopt(n))
      .catch(async (e: unknown) => {
        reportIfUnauthorized(e);
        const cached = await localDb()
          .then((db) => db.getValue<Note>(cacheKey(id)))
          .catch(() => undefined);
        if (cancelled) return;
        if (cached) {
          adopt(cached);
          setFromCache(true);
        } else setLoadError(describeError(e, "note"));
      });
    return () => {
      cancelled = true;
    };
  }, [id, adopt]);

  const save = useCallback(
    async (force = false) => {
      window.clearTimeout(timer.current);
      timer.current = undefined;
      const { title: t, body: b } = draft.current;
      setStatus("saving");
      try {
        const n = await api.updateNote(id, { title: t, body: b, expected_updated_at: force ? undefined : version.current });
        version.current = n.updated_at;
        setNote(n);
        saved(n);
        void localDb()
          .then((db) => db.setValue(cacheKey(n.id), n))
          .catch(() => {});
        // Typing may have continued while the request was in flight.
        setStatus(draft.current.title === t && draft.current.body === b ? "saved" : "dirty");
        setConflict(undefined);
      } catch (e) {
        reportIfUnauthorized(e);
        if (e instanceof ApiError && e.code === "conflict") {
          setConflict((e.body as { note: Note }).note);
          setStatus("dirty");
        } else setStatus(isNetworkError(e) ? "offline" : "error");
      }
    },
    [id, saved],
  );

  const edit = (next: { title?: string; body?: string }) => {
    draft.current = { ...draft.current, ...next };
    if (next.title !== undefined) setTitle(next.title);
    if (next.body !== undefined) setBody(next.body);
    setStatus("dirty");
    window.clearTimeout(timer.current);
    if (!conflict) timer.current = window.setTimeout(() => void save(), SAVE_DELAY_MS);
  };

  // Flush pending edits when switching notes or leaving the module.
  useEffect(
    () => () => {
      if (timer.current !== undefined) {
        window.clearTimeout(timer.current);
        void save();
      }
    },
    [save],
  );

  if (loadError) {
    return (
      <Panel title="editor">
        <p role="alert" className="text-sm text-bad">
          {loadError}
        </p>
      </Panel>
    );
  }
  if (!note) {
    return (
      <Panel title="editor">
        <p className="text-sm text-muted">Loading…</p>
      </Panel>
    );
  }

  const locked = readOnly || fromCache;
  return (
    <Panel
      title="editor"
      actions={
        <span className="flex items-center gap-2">
          <span role="status" className={`font-mono text-[11px] ${status === "error" || status === "offline" ? "text-bad" : "text-muted"}`}>
            {locked ? "read-only" : STATUS_TEXT[status]}
          </span>
          <Button
            className={small}
            disabled={locked}
            aria-pressed={note.pinned}
            onClick={() =>
              void api.updateNote(id, { pinned: !note.pinned }).then((n) => {
                version.current = n.updated_at;
                setNote(n);
                saved(n);
              }, reportIfUnauthorized)
            }
          >
            {note.pinned ? "unpin" : "pin"}
          </Button>
        </span>
      }
    >
      {conflict && (
        <div role="alert" className="mb-3 rounded-sm border border-warn bg-sunken/60 p-3 text-sm">
          <p>This note was changed on another device ({formatTime(conflict.updated_at)}).</p>
          <div className="mt-2 flex flex-wrap gap-2">
            <Button className={small} onClick={() => (adopt(conflict), setConflict(undefined))}>
              use theirs
            </Button>
            <Button className={small} variant="primary" onClick={() => void save(true)}>
              keep mine
            </Button>
          </div>
        </div>
      )}
      <input
        value={title}
        onChange={(e) => edit({ title: e.target.value })}
        disabled={locked}
        maxLength={200}
        placeholder="Title (optional: first line is used)"
        aria-label="Note title"
        className={`${inputClass} mb-3 w-full font-medium`}
      />
      <textarea
        value={body}
        onChange={(e) => edit({ body: e.target.value })}
        disabled={locked}
        maxLength={100000}
        aria-label="Note body"
        placeholder="Start typing…"
        className={`${inputClass} min-h-[50vh] w-full resize-y py-3 font-mono leading-relaxed`}
      />
      <div className="mt-3 flex items-center justify-between">
        <span className="font-mono text-[11px] text-muted">created {formatDate(note.created_at)}</span>
        {confirmDelete ? (
          <span className="flex gap-1.5">
            <Button className={small} onClick={() => setConfirmDelete(false)}>
              keep
            </Button>
            <Button variant="danger" className={small} onClick={() => void state.remove(id)}>
              confirm delete
            </Button>
          </span>
        ) : (
          <Button variant="danger" className={small} disabled={locked} onClick={() => setConfirmDelete(true)}>
            delete
          </Button>
        )}
      </div>
    </Panel>
  );
}
