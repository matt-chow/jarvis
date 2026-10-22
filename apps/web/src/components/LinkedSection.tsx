import { useCallback, useEffect, useState } from "react";
import { describeError } from "../hooks/useCachedList";
import { reportIfUnauthorized } from "../hooks/useSession";
import { api } from "../lib/api";
import { KIND_GLYPH, entityRoute, filterCandidates, type Candidate, type EntityRef, type Linked } from "../lib/links";
import { localDate } from "../lib/tasks";
import { inputClass } from "./ui";

/** Every task, note, and project title, for the link picker. */
async function loadCandidates(): Promise<Candidate[]> {
  const [tasks, notes, projects] = await Promise.all([api.tasks("all"), api.notes(), api.projects(true)]);
  return [
    ...tasks.map((t) => ({ kind: "task" as const, id: t.id, title: t.title })),
    ...notes.map((n) => ({ kind: "note" as const, id: n.id, title: n.title })),
    ...projects.map((p) => ({ kind: "project" as const, id: p.id, title: p.name })),
  ];
}

/** "Linked" list with a cross-module picker, shown inside an item's editor. */
export function LinkedSection({ entity, disabled }: { entity: EntityRef; disabled?: boolean }) {
  const [links, setLinks] = useState<Linked[]>();
  const [error, setError] = useState<string>();
  const [picking, setPicking] = useState(false);
  const [query, setQuery] = useState("");
  const [candidates, setCandidates] = useState<Candidate[]>();

  const fail = useCallback((e: unknown) => {
    reportIfUnauthorized(e);
    setError(describeError(e, "link"));
  }, []);

  useEffect(() => {
    let cancelled = false;
    api.links(entity).then((l) => !cancelled && setLinks(l), (e: unknown) => !cancelled && fail(e));
    return () => {
      cancelled = true;
    };
  }, [entity.kind, entity.id, fail]);

  const open = () => {
    setPicking(true);
    setQuery("");
    if (!candidates) loadCandidates().then(setCandidates, fail);
  };

  const add = async (to: EntityRef) => {
    setError(undefined);
    try {
      const l = await api.link(entity, to);
      setLinks((cur) => [...(cur ?? []).filter((x) => x.link_id !== l.link_id), l]);
      setPicking(false);
    } catch (e) {
      fail(e);
    }
  };

  const remove = async (linkId: string) => {
    try {
      await api.unlink(linkId);
      setLinks((cur) => cur?.filter((l) => l.link_id !== linkId));
    } catch (e) {
      fail(e);
    }
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
                onClick={() => void remove(l.link_id)}
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
