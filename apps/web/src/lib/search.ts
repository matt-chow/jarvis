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
