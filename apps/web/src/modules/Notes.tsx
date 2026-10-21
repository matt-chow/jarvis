import { useEffect, useRef, useState } from "react";
import { Button, Panel } from "../components/ui";
import { localDb, type Note } from "../lib/db";
import { formatTime } from "../lib/format";

const SAVE_DELAY_MS = 400;

function firstLine(body: string) {
  return body.split("\n").find((l) => l.trim())?.trim().slice(0, 60) || "Untitled";
}

/** Offline-first scratchpad: notes live only in IndexedDB on this device. */
export function Notes() {
  const [notes, setNotes] = useState<Note[]>([]);
  const [selected, setSelected] = useState<string>();
  const [error, setError] = useState<string>();
  const saveTimer = useRef<number | undefined>(undefined);

  useEffect(() => {
    localDb()
      .then((db) => db.listNotes())
      .then((list) => {
        setNotes(list);
        setSelected((s) => s ?? list[0]?.id);
      })
      .catch((e: unknown) => setError(`On-device storage unavailable: ${String(e)}`));
    return () => window.clearTimeout(saveTimer.current);
  }, []);

  const current = notes.find((n) => n.id === selected);

  function persist(note: Note) {
    window.clearTimeout(saveTimer.current);
    saveTimer.current = window.setTimeout(() => {
      localDb()
        .then((db) => db.putNote(note))
        .catch((e: unknown) => setError(String(e)));
    }, SAVE_DELAY_MS);
  }

  function create() {
    const now = Date.now();
    const note: Note = { id: crypto.randomUUID(), body: "", createdAt: now, updatedAt: now };
    setNotes((ns) => [note, ...ns]);
    setSelected(note.id);
    void localDb().then((db) => db.putNote(note));
  }

  function edit(body: string) {
    if (!current) return;
    const next = { ...current, body, updatedAt: Date.now() };
    setNotes((ns) => [next, ...ns.filter((n) => n.id !== next.id)]);
    persist(next);
  }

  async function remove(id: string) {
    await (await localDb()).deleteNote(id);
    const rest = notes.filter((n) => n.id !== id);
    setNotes(rest);
    setSelected(rest[0]?.id);
  }

  return (
    <div className="grid gap-5 md:grid-cols-[16rem_1fr]">
      <Panel
        title={`notes · ${notes.length}`}
        actions={
          <Button onClick={create} variant="primary" className="min-h-7 px-2 text-[10px]">
            + new
          </Button>
        }
      >
        {notes.length === 0 ? (
          <p className="text-sm text-muted">No notes yet. They are saved on this device and work offline.</p>
        ) : (
          <ul className="-mx-2 flex max-h-80 flex-col gap-0.5 overflow-y-auto md:max-h-[60vh]">
            {notes.map((n) => (
              <li key={n.id}>
                <button
                  type="button"
                  onClick={() => setSelected(n.id)}
                  className={`w-full rounded-sm px-2 py-1.5 text-left ${n.id === selected ? "bg-sunken" : "hover:bg-sunken/60"}`}
                >
                  <span className="block truncate text-sm">{firstLine(n.body)}</span>
                  <span className="font-mono text-[11px] text-muted">{formatTime(n.updatedAt)}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </Panel>

      <Panel
        title="editor · stored on this device"
        actions={
          current && (
            <Button variant="danger" onClick={() => void remove(current.id)} className="min-h-7 px-2 text-[10px]">
              delete
            </Button>
          )
        }
      >
        {error && <p className="mb-3 text-sm text-bad">{error}</p>}
        {current ? (
          <textarea
            aria-label="Note"
            value={current.body}
            onChange={(e) => edit(e.target.value)}
            placeholder="Start typing…"
            className="min-h-[50vh] w-full resize-y rounded-sm border border-line bg-sunken/40 p-3 font-mono text-sm leading-relaxed outline-none focus:border-accent"
          />
        ) : (
          <p className="text-sm text-muted">Select or create a note.</p>
        )}
      </Panel>
    </div>
  );
}
