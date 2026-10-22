/** Search result types. */
import type { EntityKind } from "./links";

export interface SearchHit {
  kind: EntityKind;
  id: string;
  title: string;
  snippet: string;
  title_match: boolean;
  updated_at: number;
}
