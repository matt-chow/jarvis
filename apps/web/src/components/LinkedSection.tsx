import { useCallback, useEffect, useState } from "react";
import { runOrQueue } from "../hooks/offline";
import { useCachedList } from "../hooks/useCachedList";
import { useOnline } from "../hooks/useOnline";
import { SYNCED_EVENT } from "../hooks/useSync";
import { api, isNetworkError } from "../lib/api";
import { localDb } from "../lib/db";
import { KIND_GLYPH, entityRoute, filterCandidates, isLocalLink, localLinkId, type Candidate, type EntityRef, type Linked } from "../lib/links";
import { loadCachedData } from "../lib/searchCache";
import type { Op } from "../lib/sync";
import { localDate } from "../lib/tasks";
import { inputClass } from "./ui";

/** Every task, note, and project title, for the link picker (offline: the copies on this device). */
async function loadCandidates(): Promise<Candidate[]> {
  let tasks, notes, projects;
  try {
    [tasks, notes, projects] = await Promise.all([api.tasks("all"), api.notes(), api.projects(true)]);
  } catch (e) {
    if (!isNetworkError(e)) throw e;
    const c = await loadCachedData(await localDb());
    [tasks, notes, projects] = [c.tasks ?? [], c.notes ?? [], c.projects ?? []];
  }
  return [
    ...tasks.map((t) => ({ kind: "task" as const, id: t.id, title: t.title })),
    ...notes.map((n) => ({ kind: "note" as const, id: n.id, title: n.title })),
    ...projects.map((p) => ({ kind: "project" as const, id: p.id, title: p.name })),
  ];
}

const samePair = (op: Op, from: EntityRef, to: EntityRef) =>
  op.kind === "link.create" && op.from.kind === from.kind && op.from.id === from.id && op.to.kind === to.kind && op.to.id === to.id;

/**
 * "Linked" list with a cross-module picker, shown inside an item's editor.
 * Works offline (D-037): the list is cached, and links made or removed are queued.
 */
export function LinkedSection({ entity, disabled }: { entity: EntityRef; disabled?: boolean }) {
  const online = useOnline();
  const { kind, id } = entity;
  const fetchLinks = useCallback(() => api.links({ kind, id }), [kind, id]);
  const list = useCachedList<Linked>(`links:${kind}:${id}`, "link", fetchLinks, online);
  const links = list.items;
  const error = list.error;
  const [picking, setPicking] = useState(false);
  const [query, setQuery] = useState("");
  const [candidates, setCandidates] = useState<Candidate[]>();
  const { reload } = list;

  useEffect(() => {
    window.addEventListener(SYNCED_EVENT, reload);
    return () => window.removeEventListener(SYNCED_EVENT, reload);
  }, [reload]);

  const open = () => {
    setPicking(true);
    setQuery("");
    if (!candidates) loadCandidates().then(setCandidates, (e: unknown) => list.setError(String(e)));
  };

  const add = async (to: Candidate) => {
    const target: EntityRef = { kind: to.kind, id: to.id };
    const placeholder: Linked = { link_id: localLinkId(entity, target), kind: to.kind, id: to.id, title: to.title, created_at: Date.now() };
    const done = await runOrQueue<Linked, Linked>({
      list,
      noun: "link",
      op: { kind: "link.create", from: entity, to: target },
      server: () => api.link(entity, target),
      fromServer: (cur, l) => [...cur.filter((x) => x.link_id !== l.link_id), l],
      applyLocal: (cur) => [...cur.filter((x) => x.link_id !== placeholder.link_id), placeholder],
      local: () => placeholder,
    });
    if (done) setPicking(false);
  };

  const remove = async (l: Linked) => {
    if (isLocalLink(l.link_id)) {
      // Made offline and not synced yet: cancel the queued create instead.
      const target: EntityRef = { kind: l.kind, id: l.id };
      await (await localDb()).dropWhere((op) => samePair(op as Op, entity, target));
      list.replace(list.latest().filter((x) => x.link_id !== l.link_id));
      return;
    }
    await runOrQueue<Linked, true>({
      list,
      noun: "link",
      op: { kind: "link.delete", link_id: l.link_id },
      server: async () => {
        await api.unlink(l.link_id);
        return true;
      },
      fromServer: (cur) => cur.filter((x) => x.link_id !== l.link_id),
      applyLocal: (cur) => cur.filter((x) => x.link_id !== l.link_id),
      local: () => true,
    });
  };

  // Journal days are offered when the query is a date (or "today").
  const q = query.trim().toLowerCase();
  const day = q === "today" ? localDate() : /^\d{4}-\d{2}-\d{2}$/.test(q) ? q : undefined;
  const matches = filterCandidates(
    [...(day ? [{ kind: "journal" as const, id: day, title: day }] : []), ...(candidates ?? [])],
    day ? "" : query,
    entity,
    links ?? [],
  );

  return (
    <div>
      <div className="mb-1 flex items-center justify-between">
        <span className="label">linked</span>
        {!picking && (
          <button type="button" onClick={open} disabled={disabled} className="font-mono text-[11px] text-accent hover:underline disabled:opacity-50">
            + link
          </button>
        )}
      </div>
      {links && links.length === 0 && !picking && <p className="text-sm text-muted">Nothing linked yet.</p>}
      {links && links.length > 0 && (
        <ul className="mb-2 flex flex-col gap-0.5">
          {links.map((l) => (
            <li key={l.link_id} className="flex items-center gap-2 text-sm">
              <span aria-hidden className="w-4 text-center font-mono text-muted">
                {KIND_GLYPH[l.kind]}
              </span>
              <a href={entityRoute(l)} className="min-w-0 flex-1 truncate hover:text-accent">
                {l.title}
              </a>
              <button
                type="button"
                disabled={disabled}
                onClick={() => void remove(l)}
                aria-label={`Unlink “${l.title}”`}
                className="font-mono text-xs text-muted hover:text-bad disabled:opacity-50"
              >
                ×
              </button>
            </li>
          ))}
        </ul>
      )}
      {picking && (
        <div className="rounded-sm border border-line bg-sunken/40 p-2">
          <div className="flex gap-2">
            <input
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.key === "Escape" && setPicking(false)}
              placeholder="Search tasks, notes, projects — or a date / today"
              aria-label="Link to"
              className={`${inputClass} min-w-0 flex-1`}
            />
            <button type="button" onClick={() => setPicking(false)} className="font-mono text-[11px] text-muted hover:text-ink">
              cancel
            </button>
          </div>
          {!candidates && !day ? (
            <p className="mt-2 text-sm text-muted">Loading…</p>
          ) : matches.length === 0 ? (
            <p className="mt-2 text-sm text-muted">No matches.</p>
          ) : (
            <ul className="mt-2 flex flex-col">
              {matches.map((c) => (
                <li key={`${c.kind}:${c.id}`}>
                  <button
                    type="button"
                    onClick={() => void add(c)}
                    className="flex w-full items-center gap-2 rounded-sm px-1 py-1 text-left text-sm hover:bg-sunken"
                  >
                    <span aria-hidden className="w-4 text-center font-mono text-muted">
                      {KIND_GLYPH[c.kind]}
                    </span>
                    <span className="truncate">{c.title}</span>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
      {error && (
        <p role="alert" className="mt-1 text-sm text-bad">
          {error}
        </p>
      )}
    </div>
  );
}
