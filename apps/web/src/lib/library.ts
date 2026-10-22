/** Library (PDF/ePub) types and helpers. */
import type { FileEntry } from "./files";

export type ReadingStatus = "want" | "reading" | "finished";

export interface Progress {
  file_id: string;
  status: ReadingStatus;
  current_page: number | null;
  total_pages: number | null;
  position: string | null;
  percent: number;
  started_at: number | null;
  finished_at: number | null;
  updated_at: number;
}

export interface LibraryItem {
  file: FileEntry;
  progress: Progress | null;
}

export interface ProgressPatch {
  status?: ReadingStatus;
  current_page?: number;
  total_pages?: number;
  position?: string;
}

export const STATUS_LABEL: Record<ReadingStatus | "unread", string> = {
  want: "Want to read",
  reading: "Reading",
  finished: "Finished",
  unread: "Unread",
};

export function isPdf(f: FileEntry): boolean {
  return f.content_type === "application/pdf" || f.name.toLowerCase().endsWith(".pdf");
}

/** Title without the extension: "Dune.pdf" → "Dune". */
export function documentTitle(f: FileEntry): string {
  return f.name.replace(/\.(pdf|epub)$/i, "");
}
