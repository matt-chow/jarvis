/**
 * Restore from a data export (DECISIONS D-025): split the document into
 * small batches, in the order the server needs (later kinds refer to earlier
 * ones). The server validates every record and never overwrites.
 */

export const FORMAT = "directory-export";
export const FORMAT_VERSION = 1;
export const MAX_BATCH = 100;
/** Keep each request well under the server's 4 MiB body limit. */
const MAX_BATCH_BYTES = 1024 * 1024;

export type ImportKind = "projects" | "tasks" | "notes" | "journal" | "inbox" | "habits" | "events" | "playlists" | "links" | "preferences";

/** Server order (directory-core `backup::KINDS`). */
export const KINDS: ImportKind[] = ["projects", "tasks", "notes", "journal", "inbox", "habits", "events", "playlists", "links", "preferences"];

export const KIND_LABELS: Record<ImportKind, string> = {
  projects: "Projects",
  tasks: "Tasks",
  notes: "Notes",
  journal: "Journal days",
  inbox: "Inbox items",
  habits: "Habits",
  events: "Events",
  playlists: "Playlists",
  links: "Links",
  preferences: "Dashboard layout",
};

export interface Batch {
  kind: ImportKind;
  records: unknown[];
}

export interface ImportReport {
  inserted: number;
  skipped: number;
  rejected: { id: string; reason: string }[];
}

export interface ExportPlan {
  batches: Batch[];
  /** Records per kind. */
  counts: Record<ImportKind, number>;
  /** Files listed in the export: their contents aren't in it. */
  files: number;
}

type Doc = Record<string, unknown>;
const list = (doc: Doc, key: string): unknown[] => (Array.isArray(doc[key]) ? (doc[key] as unknown[]) : []);

/** Validate the header and plan the batches; throws a readable Error if it isn't an export. */
export function planImport(doc: unknown): ExportPlan {
  if (!doc || typeof doc !== "object" || (doc as Doc).format !== FORMAT) throw new Error("This isn't a Directory export file.");
  const d = doc as Doc;
  if (d.format_version !== FORMAT_VERSION) throw new Error(`This export uses format version ${String(d.format_version)}; this app reads version ${FORMAT_VERSION}.`);

  // Subtasks after their parents.
  const tasks = [...list(d, "tasks")].sort((a, b) => Number(Boolean((a as Doc).parent_id)) - Number(Boolean((b as Doc).parent_id)));
  const byKind: Record<ImportKind, unknown[]> = {
    projects: list(d, "projects"),
    tasks,
    notes: list(d, "notes"),
    journal: list(d, "journal"),
    inbox: list(d, "inbox"),
    habits: list(d, "habits"),
    events: list(d, "events"),
    playlists: list(d, "playlists"),
    links: list(d, "links"),
    preferences: d.preferences && typeof d.preferences === "object" ? [d.preferences] : [],
  };

  const batches: Batch[] = [];
  for (const kind of KINDS) {
    let current: unknown[] = [];
    let bytes = 0;
    for (const r of byKind[kind]) {
      const size = JSON.stringify(r).length;
      if (current.length > 0 && (current.length >= MAX_BATCH || bytes + size > MAX_BATCH_BYTES)) {
        batches.push({ kind, records: current });
        current = [];
        bytes = 0;
      }
      current.push(r);
      bytes += size;
    }
    if (current.length) batches.push({ kind, records: current });
  }
  const counts = Object.fromEntries(KINDS.map((k) => [k, byKind[k].length])) as Record<ImportKind, number>;
  return { batches, counts, files: list(d, "files").length };
}
