/** Note types. Bodies are plain text / Markdown. */

export interface Note {
  id: string;
  /** Empty = derived from the first line of the body. */
  title: string;
  body: string;
  pinned: boolean;
  created_at: number;
  /** Also the version for optimistic concurrency. */
  updated_at: number;
}

export interface NoteSummary {
  id: string;
  title: string;
  excerpt: string;
  pinned: boolean;
  created_at: number;
  updated_at: number;
}

export interface NotePatch {
  title?: string;
  body?: string;
  pinned?: boolean;
  /** Reject the save (409) if the note changed since this version. */
  expected_updated_at?: number;
}

export function summarize(n: Note): NoteSummary {
  const lines = n.body.split("\n").map((l) => l.trim()).filter(Boolean);
  const derived = !n.title;
  const title = (derived ? (lines.shift() ?? "").replace(/^#+\s*/, "") : n.title) || "Untitled";
  return {
    id: n.id,
    title,
    excerpt: lines.join(" ").slice(0, 160),
    pinned: n.pinned,
    created_at: n.created_at,
    updated_at: n.updated_at,
  };
}

/** Pinned first, then most recently updated (matches the server order). */
export function sortSummaries(list: NoteSummary[]): NoteSummary[] {
  return [...list].sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.updated_at - a.updated_at);
}
