/** Time blocks: scheduled work sessions (mirrors directory-core `blocks`, D-047). */

export type BlockStatus = "planned" | "done" | "skipped";

export interface TimeBlock {
  id: string;
  /** The task this session works on; null = a standalone focus block. */
  task_id: string | null;
  /** Required without a task; an optional label with one. */
  title: string;
  date: string;
  /** `HH:MM` */
  start_time: string;
  /** `HH:MM`, after the start; `24:00` = midnight. */
  end_time: string;
  timezone: string | null;
  /** Flexible by default. */
  fixed: boolean;
  status: BlockStatus;
  actual_minutes: number | null;
  notes: string;
  created_at: number;
  updated_at: number;
}

export interface NewBlock {
  task_id?: string | null;
  title?: string;
  date: string;
  start_time: string;
  end_time: string;
  timezone?: string | null;
  fixed?: boolean;
  notes?: string;
}

export type BlockPatch = Partial<Omit<TimeBlock, "id" | "created_at" | "updated_at">>;

export const BLOCK_STATUS_LABELS: Record<BlockStatus, string> = {
  planned: "Planned",
  done: "Done",
  skipped: "Skipped",
};

/** Apply a patch the way the server does (for optimistic and offline edits). */
export function patchBlock(b: TimeBlock, patch: BlockPatch, now = Date.now()): TimeBlock {
  return { ...b, ...patch, title: (patch.title ?? b.title).trim(), updated_at: now };
}

export function newBlock(id: string, b: NewBlock, now = Date.now()): TimeBlock {
  return {
    id,
    task_id: b.task_id ?? null,
    title: (b.title ?? "").trim(),
    date: b.date,
    start_time: b.start_time,
    end_time: b.end_time,
    timezone: b.timezone ?? null,
    fixed: b.fixed ?? false,
    status: "planned",
    actual_minutes: null,
    notes: b.notes ?? "",
    created_at: now,
    updated_at: now,
  };
}
