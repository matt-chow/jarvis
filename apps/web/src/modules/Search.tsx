import { useEffect, useState, type ReactNode } from "react";
import type { ModuleProps } from "../App";
import { Panel, inputClass } from "../components/ui";
import { describeError } from "../hooks/useCachedList";
import { useHashParam } from "../hooks/useHashParam";
import { reportIfUnauthorized } from "../hooks/useSession";
import { api, isNetworkError } from "../lib/api";
import { localDb } from "../lib/db";
import { MIN_QUERY, SEARCH_GLYPH, hitRoute, searchLocal, type SearchHit } from "../lib/search";
import { loadCachedData, toLocalDocs } from "../lib/searchCache";

// Provisional screen: awaiting design integration (see docs/MODULES.md › Design status).

const DEBOUNCE_MS = 250;

/** Offline: match what this device has cached (D-031). */
async function searchDevice(q: string): Promise<SearchHit[]> {
  return searchLocal(toLocalDocs(await loadCachedData(await localDb())), q);
}

/** Wrap case-insensitive occurrences of `query` in <mark>. */
export function highlight(text: string, query: string): ReactNode[] {
  const q = query.trim();
  if (!q) return [text];
  const lower = text.toLowerCase();
  const needle = q.toLowerCase();
  const out: ReactNode[] = [];
  let at = 0;
  for (let i = lower.indexOf(needle); i !== -1; i = lower.indexOf(needle, i + needle.length)) {
    if (i > at) out.push(text.slice(at, i));
    out.push(
      <mark key={i} className="rounded-[2px] bg-accent/30 text-ink">
        {text.slice(i, i + needle.length)}
      </mark>,
    );
    at = i + needle.length;
  }
  if (at < text.length) out.push(text.slice(at));
  return out;
}

export function Search({ online }: ModuleProps) {
  const initial = useHashParam("q") ?? "";
  const [query, setQuery] = useState(initial);
  const [hits, setHits] = useState<SearchHit[]>();
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string>();
  /** Results came from this device, not the server. */
  const [local, setLocal] = useState(false);

  useEffect(() => {
    const q = query.trim();
    if (q.length < MIN_QUERY) {
      setHits(undefined);
      setError(undefined);
      return;
    }
    let cancelled = false;
    const timer = window.setTimeout(() => {
      setLoading(true);
      const device = () => searchDevice(q).then((h) => !cancelled && (setHits(h), setLocal(true), setError(undefined)));
      (online
        ? api.search(q).then(
            (h) => !cancelled && (setHits(h), setLocal(false), setError(undefined)),
            (e: unknown) => {
              reportIfUnauthorized(e);
              if (isNetworkError(e)) return device();
              if (!cancelled) setError(describeError(e, "result"));
            },
          )
        : device()
      )
        .catch((e: unknown) => !cancelled && setError(`Couldn't search this device: ${String(e)}`))
        .finally(() => !cancelled && setLoading(false));
      // Keep the query in the URL so results can be linked and survive reloads.
      history.replaceState(null, "", `#/search?q=${encodeURIComponent(q)}`);
    }, DEBOUNCE_MS);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [query, online]);

  return (
    <div className="mx-auto max-w-3xl">
      <Panel title={`search${hits ? ` · ${hits.length}` : ""}`}>
        <input
          autoFocus
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          maxLength={100}
          aria-label="Search"
          placeholder="Search tasks, notes, projects, and journal…"
          className={`${inputClass} w-full`}
        />
        {hits && local && (
          <p className="mt-3 rounded-sm border border-line bg-sunken/60 px-3 py-2 text-sm text-muted">
            Offline: showing matches from what this device has saved. Results may be incomplete.
          </p>
        )}
        {error && (
          <p role="alert" className="mt-3 text-sm text-bad">
            {error}
          </p>
        )}
        {query.trim().length > 0 && query.trim().length < 2 && <p className="mt-3 text-sm text-muted">Type at least 2 characters.</p>}
        {loading && !hits && <p className="mt-3 text-sm text-muted">Searching…</p>}
        {hits && hits.length === 0 && <p className="mt-3 text-sm text-muted">No matches for “{query.trim()}”.</p>}
        {hits && hits.length > 0 && (
          <ul className="mt-3 divide-y divide-line">
            {hits.map((h) => (
              <li key={`${h.kind}:${h.id}`}>
                <a href={hitRoute(h)} className="flex gap-3 py-2.5 hover:bg-sunken/40">
                  <span aria-hidden className="w-4 shrink-0 text-center font-mono text-muted">
                    {SEARCH_GLYPH[h.kind]}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm">{highlight(h.title, query)}</span>
                    {h.snippet && h.snippet !== h.title && (
                      <span className="block truncate text-xs text-muted">{highlight(h.snippet, query)}</span>
                    )}
                  </span>
                  <span className="label shrink-0">{h.kind}</span>
                </a>
              </li>
            ))}
          </ul>
        )}
      </Panel>
      <p className="mt-2 text-center font-mono text-[11px] text-muted">Tip: press “/” anywhere to search.</p>
    </div>
  );
}
