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

/**
 * Apply a progress patch locally, by the server's rules (directory-core
 * `library`), so offline updates show the same percent and status.
 */
export function applyProgress(fileId: string, current: Progress | null, patch: ProgressPatch, now = Date.now()): Progress {
  const pr: Progress = current
    ? { ...current }
    : { file_id: fileId, status: "want", current_page: null, total_pages: null, position: null, percent: 0, started_at: null, finished_at: null, updated_at: now };
  if (patch.total_pages !== undefined) pr.total_pages = patch.total_pages;
  if (patch.current_page !== undefined) pr.current_page = patch.current_page;
  if (patch.position !== undefined) pr.position = patch.position.trim() || null;
  if (pr.current_page !== null && pr.total_pages) pr.percent = Math.floor((pr.current_page * 100) / pr.total_pages);
  if (patch.status) pr.status = patch.status;
  else if (pr.percent >= 100) pr.status = "finished";
  else if ((pr.current_page ?? 0) > 0 && pr.status === "want") pr.status = "reading";
  if (pr.status === "finished") {
    pr.percent = 100;
    if (pr.total_pages !== null) pr.current_page = pr.total_pages;
    pr.finished_at ??= now;
  } else pr.finished_at = null;
  if (pr.status !== "want") pr.started_at ??= now;
  pr.updated_at = now;
  return pr;
}

/** Title without the extension: "Dune.pdf" → "Dune". */
export function documentTitle(f: FileEntry): string {
  return f.name.replace(/\.(pdf|epub)$/i, "");
}
