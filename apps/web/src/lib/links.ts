/** Entity links: types and routes to each kind of item. */

export type EntityKind = "task" | "note" | "project" | "journal";

export interface EntityRef {
  kind: EntityKind;
  /** For journal days, the date `YYYY-MM-DD`. */
  id: string;
}

export interface Linked {
  link_id: string;
  kind: EntityKind;
  id: string;
  title: string;
  created_at: number;
}

/** Placeholder id for a link made offline, until the sync gives it a real one (D-037). */
export const localLinkId = (from: EntityRef, to: EntityRef) => `local:${from.kind}:${from.id}|${to.kind}:${to.id}`;
export const isLocalLink = (linkId: string) => linkId.startsWith("local:");

export const KIND_GLYPH: Record<EntityKind, string> = { task: "☐", note: "¶", project: "▦", journal: "≡" };

/** Hash route that opens an item (each module reads its selection param). */
export function entityRoute(e: EntityRef): string {
  switch (e.kind) {
    case "task":
      return `#/tasks?task=${e.id}`;
    case "note":
      return `#/notes?note=${e.id}`;
    case "project":
      return `#/projects?project=${e.id}`;
    case "journal":
      return `#/journal?date=${e.id}`;
  }
}

export interface Candidate extends EntityRef {
  title: string;
}

/** Case-insensitive title match, excluding the current item and existing links. */
export function filterCandidates(all: Candidate[], query: string, self: EntityRef, linked: Linked[], limit = 8): Candidate[] {
  const q = query.trim().toLowerCase();
  const taken = new Set(linked.map((l) => `${l.kind}:${l.id}`));
  return all
    .filter((c) => !(c.kind === self.kind && c.id === self.id) && !taken.has(`${c.kind}:${c.id}`))
    .filter((c) => !q || c.title.toLowerCase().includes(q))
    .slice(0, limit);
}
