/**
 * Offline outbox replay (DECISIONS D-017). Changes made offline are queued in
 * IndexedDB and replayed in order when the server is reachable again.
 *
 * Every operation is safe to replay: creates carry a client-generated id
 * (a repeat answers 409 `exists`), updates send only changed fields, and
 * deletes treat 404 as already done.
 */
import { ApiError, isNetworkError } from "./api";
import type { NewTask, Task, TaskPatch } from "./tasks";

export type Op =
  | { kind: "task.create"; task: NewTask & { id: string } }
  | { kind: "task.update"; id: string; patch: TaskPatch }
  | { kind: "task.delete"; id: string };

export interface Outbox {
  outbox(): Promise<{ seq?: number; op: unknown }[]>;
  dequeue(seq: number): Promise<void>;
}

export interface SyncApi {
  createTask(t: NewTask & { id: string }): Promise<Task>;
  updateTask(id: string, patch: TaskPatch): Promise<Task>;
  deleteTask(id: string): Promise<void>;
}

export interface SyncResult {
  applied: number;
  /** Changes the server rejected; they were dropped from the outbox. */
  rejected: { op: Op; reason: string }[];
  /** Replay stopped early (offline, server error, or signed out). */
  stopped?: "offline" | "server" | "unauthorized";
  pending: number;
}

/** A random 32-hex id, the format the server accepts for client creates. */
export function clientId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

async function apply(api: SyncApi, op: Op): Promise<void> {
  try {
    switch (op.kind) {
      case "task.create":
        await api.createTask(op.task);
        return;
      case "task.update":
        await api.updateTask(op.id, op.patch);
        return;
      case "task.delete":
        await api.deleteTask(op.id);
        return;
    }
  } catch (e) {
    // Already applied by an earlier, interrupted replay.
    if (e instanceof ApiError && op.kind === "task.create" && e.code === "exists") return;
    if (e instanceof ApiError && op.kind === "task.delete" && e.status === 404) return;
    throw e;
  }
}

function describeOp(op: Op): string {
  switch (op.kind) {
    case "task.create":
      return `create “${op.task.title}”`;
    case "task.update":
      return "update a task";
    case "task.delete":
      return "delete a task";
  }
}

/** Replay queued changes in order. Never throws. */
export async function replay(outbox: Outbox, api: SyncApi): Promise<SyncResult> {
  const entries = await outbox.outbox();
  const result: SyncResult = { applied: 0, rejected: [], pending: entries.length };
  for (const entry of entries) {
    const op = entry.op as Op;
    try {
      await apply(api, op);
      result.applied++;
    } catch (e) {
      if (isNetworkError(e)) result.stopped = "offline";
      else if (e instanceof ApiError && e.status === 401) result.stopped = "unauthorized";
      else if (e instanceof ApiError && e.status >= 500) result.stopped = "server";
      if (result.stopped) return result;
      // A 4xx the server will never accept: drop it, but tell the user.
      const reason = e instanceof ApiError ? (e.detail ?? e.code) : String(e);
      result.rejected.push({ op, reason: `Couldn't ${describeOp(op)}: ${reason}` });
    }
    if (entry.seq !== undefined) await outbox.dequeue(entry.seq);
    result.pending--;
  }
  return result;
}

/** Apply an op to a local task list (optimistic offline edits). */
export function applyLocally(tasks: Task[], op: Op, now = Date.now()): Task[] {
  switch (op.kind) {
    case "task.create": {
      const t = op.task;
      const task: Task = {
        id: t.id,
        parent_id: t.parent_id ?? null,
        project_id: t.project_id ?? null,
        title: t.title.trim(),
        notes: t.notes ?? "",
        status: "todo",
        priority: t.priority ?? "none",
        due_date: t.due_date ?? null,
        created_at: now,
        updated_at: now,
        completed_at: null,
      };
      return [...tasks, task];
    }
    case "task.update":
      return tasks.map((t) => {
        if (t.id !== op.id) return t;
        const next = { ...t, ...op.patch, updated_at: now };
        if (op.patch.status && op.patch.status !== t.status) {
          const open = op.patch.status === "todo" || op.patch.status === "in_progress";
          next.completed_at = open ? null : now;
        }
        return next;
      });
    case "task.delete":
      return tasks.filter((t) => t.id !== op.id && t.parent_id !== op.id);
  }
}
