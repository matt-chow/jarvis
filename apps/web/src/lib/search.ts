/** Search result types (mirrors directory-core `search`). */
import { KIND_GLYPH, entityRoute, type EntityKind } from "./links";

export type SearchKind = EntityKind | "event" | "habit";

export interface SearchHit {
  kind: SearchKind;
  id: string;
  title: string;
  snippet: string;
  title_match: boolean;
  updated_at: number;
  /** Events: the start date (opens that day on the calendar). */
  date?: string;
}

export const SEARCH_GLYPH: Record<SearchKind, string> = { ...KIND_GLYPH, event: "▣", habit: "✓" };

/** Mirrors directory-core `search` limits. */
export const MIN_QUERY = 2;
export const MAX_HITS = 50;
const SNIPPET_RADIUS = 60;

/** Searchable text cached on this device (what the server would match on). */
export interface LocalDoc {
  kind: SearchKind;
  id: string;
  title: string;
  /** Matched in addition to the title (journal: body only). */
  body: string;
  updated_at: number;
  date?: string;
  /** False for journal days: the server matches their body only. */
  titleSearchable?: boolean;
}

const lowerChar = (c: string) => c.toLowerCase()[0] ?? c;

/** Case-insensitive char index of `needle` in `hay` (per character, like core). */
function findCi(hay: string[], needle: string[]): number {
  if (needle.length === 0 || needle.length > hay.length) return -1;
  const n = needle.map(lowerChar);
  for (let i = 0; i + n.length <= hay.length; i++) {
    let ok = true;
    for (let j = 0; j < n.length && ok; j++) ok = lowerChar(hay[i + j]) === n[j];
    if (ok) return i;
  }
  return -1;
}

/** Text around the first match, on one line, with ellipses where cut (core `snippet`). */
export function snippet(text: string, query: string): string {
  const flat = Array.from(text.split(/\s+/).filter(Boolean).join(" "));
  const q = Array.from(query);
  const i = findCi(flat, q);
  const start = i === -1 ? 0 : Math.max(0, i - SNIPPET_RADIUS);
  const end = i === -1 ? Math.min(flat.length, SNIPPET_RADIUS * 2) : Math.min(flat.length, i + q.length + SNIPPET_RADIUS);
  return `${start > 0 ? "…" : ""}${flat.slice(start, end).join("")}${end < flat.length ? "…" : ""}`;
}

/** Search documents cached on this device, ranked like the server: title matches, then newest. */
export function searchLocal(docs: LocalDoc[], query: string): SearchHit[] {
  const q = query.trim();
  if (Array.from(q).length < MIN_QUERY) return [];
  const needle = Array.from(q);
  const hits: SearchHit[] = [];
  for (const d of docs) {
    const titleMatch = findCi(Array.from(d.title), needle) !== -1;
    const bodyMatch = findCi(Array.from(d.body), needle) !== -1;
    if (!(bodyMatch || (titleMatch && d.titleSearchable !== false))) continue;
    hits.push({
      kind: d.kind,
      id: d.id,
      title: d.title,
      snippet: snippet(d.body, q),
      title_match: titleMatch,
      updated_at: d.updated_at,
      ...(d.date ? { date: d.date } : {}),
    });
  }
  hits.sort((a, b) => Number(b.title_match) - Number(a.title_match) || b.updated_at - a.updated_at);
  return hits.slice(0, MAX_HITS);
}

/** Where a hit opens. */
export function hitRoute(h: SearchHit): string {
  switch (h.kind) {
    case "event":
      return h.date ? `#/calendar?date=${h.date}` : "#/calendar";
    case "habit":
      return "#/habits";
    default:
      return entityRoute({ kind: h.kind, id: h.id });
  }
}
