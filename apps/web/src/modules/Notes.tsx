import { useCallback, useEffect, useMemo, useState } from "react";
import type { ModuleProps } from "../App";
import { ConflictBanner } from "../components/ConflictBanner";
import { LinkedSection } from "../components/LinkedSection";
import { Button, Panel, inputClass } from "../components/ui";
import { SAVE_STATUS_TEXT, useAutosaveDoc } from "../hooks/useAutosaveDoc";
import { useCachedList } from "../hooks/useCachedList";
import { useHashParam } from "../hooks/useHashParam";
import { useNotes, type NotesState } from "../hooks/useNotes";
import { reportIfUnauthorized } from "../hooks/useSession";
import { api } from "../lib/api";
import { formatDate, formatTime } from "../lib/format";
import type { Note, NoteSummary } from "../lib/notes";
import { resolveLinks } from "../lib/wiki";

// Provisional screen: awaiting design integration (see docs/MODULES.md › Design status).

const small = "min-h-7 px-2 text-[10px]";

export function Notes({ online }: ModuleProps) {
  const state = useNotes(online);
  const [selected, setSelected] = useState<string>();
  // Notes are editable offline: edits stay on this device and sync later (D-018).
  const offline = state.stale || !online;

  const deepNote = useHashParam("note");
  useEffect(() => {
    if (deepNote) setSelected(deepNote);
  }, [deepNote]);

  useEffect(() => {
    if (!selected && state.notes?.length) setSelected(state.notes[0].id);
    if (selected && state.notes && !state.notes.some((n) => n.id === selected)) setSelected(state.notes[0]?.id);
  }, [selected, state.notes]);

  const create = async (title?: string) => {
    const n = await state.create(title);
    if (n) setSelected(n.id);
  };

  return (
    <div className="grid items-start gap-5 md:grid-cols-[17rem_minmax(0,1fr)]">
      <Panel
        title={`notes${state.notes ? ` · ${state.notes.length}` : ""}`}
        actions={
          <Button onClick={() => void create()} variant="primary" disabled={!state.notes} className={small}>
            + new
          </Button>
        }
      >
        {state.migrated ? (
          <p role="status" className="mb-3 text-sm text-muted">
            Moved {state.migrated} note{state.migrated === 1 ? "" : "s"} from this device into your account.
          </p>
        ) : null}
        {offline && state.notes && (
          <p className="mb-3 text-sm text-muted">Offline: changes are saved on this device and sync when you reconnect. Pinning and links need a connection.</p>
        )}
        {state.error && !offline && (
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
        <NoteEditor
          key={selected}
          id={selected}
          state={state}
          offline={offline}
          online={online}
          onOpen={setSelected}
          onCreate={(title) => void create(title)}
        />
      ) : (
        <Panel title="editor">
          <p className="text-sm text-muted">Select or create a note.</p>
        </Panel>
      )}
    </div>
  );
}

function NoteEditor(props: {
  id: string;
  state: NotesState;
  offline: boolean;
  /** Browser connectivity, for the backlinks list. */
  online: boolean;
  onOpen: (id: string) => void;
  onCreate: (title: string) => void;
}) {
  const { id, state, offline } = props;
  const { saved } = state;
  const editor = useAutosaveDoc<Note, { title: string; body: string }>({
    cacheKey: `note:${id}`,
    noun: "note",
    load: () => api.note(id),
    fieldsOf: (n) => ({ title: n?.title ?? "", body: n?.body ?? "" }),
    withFields: (n, f) => ({ ...(n ?? { id, pinned: false, created_at: Date.now(), updated_at: Date.now() }), ...f }),
    save: (f, expected) => api.updateNote(id, { ...f, expected_updated_at: expected }),
    toOp: (fields, base) => ({ kind: "note.save", id, fields, base }),
    conflictOf: (e) => (e.body as { note?: Note }).note,
    onSaved: saved,
  });
  const [confirmDelete, setConfirmDelete] = useState(false);
  const note = editor.doc;

  if (editor.loadError) {
    return (
      <Panel title="editor">
        <p role="alert" className="text-sm text-bad">
          {editor.loadError}
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

  // Pinning and links are online-only.
  const online = !offline && !editor.fromCache;
  const togglePin = () =>
    void api.updateNote(id, { pinned: !note.pinned }).then((n) => {
      editor.adopt({ ...n, title: editor.fields.title, body: editor.fields.body });
      saved(n);
    }, reportIfUnauthorized);

  return (
    <Panel
      title="editor"
      actions={
        <span className="flex items-center gap-2">
          <span
            role="status"
            className={`font-mono text-[11px] ${editor.status === "error" || editor.status === "offline" ? "text-bad" : "text-muted"}`}
          >
            {SAVE_STATUS_TEXT[editor.status]}
          </span>
          <Button className={small} disabled={!online} aria-pressed={note.pinned} onClick={togglePin}>
            {note.pinned ? "unpin" : "pin"}
          </Button>
        </span>
      }
    >
      {editor.conflict && (
        <ConflictBanner at={editor.conflict.updated_at} onTheirs={editor.useTheirs} onMine={editor.keepMine} />
      )}
      <input
        value={editor.fields.title}
        onChange={(e) => editor.edit({ title: e.target.value })}
        maxLength={200}
        placeholder="Title (optional: first line is used)"
        aria-label="Note title"
        className={`${inputClass} mb-3 w-full font-medium`}
      />
      <textarea
        value={editor.fields.body}
        onChange={(e) => editor.edit({ body: e.target.value })}
        maxLength={100000}
        aria-label="Note body"
        placeholder="Start typing…"
        className={`${inputClass} min-h-[50vh] w-full resize-y py-3 font-mono leading-relaxed`}
      />
      <WikiSection id={id} body={editor.fields.body} notes={state.notes ?? []} online={props.online} onOpen={props.onOpen} onCreate={props.onCreate} />
      <div className="mt-3">
        <LinkedSection entity={{ kind: "note", id }} disabled={!online} />
      </div>
      <div className="mt-3 flex items-center justify-between">
        <span className="font-mono text-[11px] text-muted">created {formatDate(note.created_at)}</span>
        {confirmDelete ? (
          <span className="flex gap-1.5">
            <Button className={small} onClick={() => setConfirmDelete(false)}>
              keep
            </Button>
            <Button
              variant="danger"
              className={small}
              onClick={() => {
                editor.discard();
                void state.remove(id);
              }}
            >
              confirm delete
            </Button>
          </span>
        ) : (
          <Button variant="danger" className={small} onClick={() => setConfirmDelete(true)}>
            delete
          </Button>
        )}
      </div>
    </Panel>
  );
}

/** `[[wiki links]]` in this note, and the notes that link here (DECISIONS D-019). */
function WikiSection(props: {
  id: string;
  body: string;
  notes: NoteSummary[];
  online: boolean;
  onOpen: (id: string) => void;
  onCreate: (title: string) => void;
}) {
  const { id } = props;
  const links = useMemo(() => resolveLinks(props.body, props.notes, id), [props.body, props.notes, id]);
  const fetchBacklinks = useCallback(() => api.backlinks(id), [id]);
  const backlinks = useCachedList<NoteSummary>(`backlinks:${id}`, "backlink", fetchBacklinks, props.online);
  const link = "font-mono text-xs text-accent hover:underline";

  return (
    <div className="mt-3 border-t border-line pt-3">
      <p className="label mb-1">wiki links</p>
      {links.length === 0 ? (
        <p className="text-xs text-muted">Link another note by writing [[its title]].</p>
      ) : (
        <ul className="flex flex-wrap gap-x-3 gap-y-1">
          {links.map((l) => (
            <li key={l.target.toLowerCase()}>
              {l.note ? (
                <button type="button" className={link} onClick={() => props.onOpen(l.note!.id)}>
                  {l.target}
                </button>
              ) : (
                <button
                  type="button"
                  className="font-mono text-xs text-muted hover:text-accent"
                  aria-label={`Create note “${l.target}”`}
                  onClick={() => props.onCreate(l.target)}
                >
                  {l.target} <span className="text-accent">+ create</span>
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
      <p className="label mt-3 mb-1">backlinks</p>
      {!backlinks.items ? (
        <p className="text-xs text-muted">{backlinks.loading ? "Loading…" : "Backlinks need a connection."}</p>
      ) : backlinks.items.length === 0 ? (
        <p className="text-xs text-muted">No notes link here yet.</p>
      ) : (
        <ul className="flex flex-wrap gap-x-3 gap-y-1">
          {backlinks.items.map((n) => (
            <li key={n.id}>
              <button type="button" className={link} aria-label={`Open backlink “${n.title}”`} onClick={() => props.onOpen(n.id)}>
                ← {n.title}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
