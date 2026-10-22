/**
 * Wiki links between notes (DECISIONS D-019): `[[Title]]` or `[[Title|label]]`
 * in a note body points to the note with that title (case-insensitive).
 * Mirrors `wiki_links` in directory-core, which computes backlinks.
 */
import type { NoteSummary } from "./notes";

export const MAX_TITLE = 200;

export interface WikiLink {
  target: string;
  /** The matching note, if one exists. */
  note?: NoteSummary;
}

/** Link targets in a body, first occurrence kept. Links can't span lines. */
export function wikiTargets(body: string): string[] {
  const out: string[] = [];
  for (const line of body.split("\n")) {
    for (const m of line.matchAll(/\[\[(.*?)\]\]/g)) {
      const target = m[1].split("|")[0].trim();
      if (target && [...target].length <= MAX_TITLE && !out.some((t) => t.toLowerCase() === target.toLowerCase())) out.push(target);
    }
  }
  return out;
}

/** Resolve link targets against the notes list (excluding the linking note itself). */
export function resolveLinks(body: string, notes: NoteSummary[], selfId?: string): WikiLink[] {
  return wikiTargets(body).map((target) => ({
    target,
    note: notes.find((n) => n.id !== selfId && n.title.toLowerCase() === target.toLowerCase()),
  }));
}
