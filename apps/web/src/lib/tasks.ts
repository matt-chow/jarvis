/** Task types and pure presentation helpers (no I/O). */
import type { Repeat } from "./calendar";

export type TaskStatus = "todo" | "in_progress" | "done" | "cancelled";
export type TaskPriority = "none" | "low" | "medium" | "high";

export interface Task {
  id: string;
  parent_id: string | null;
  /** Top-level tasks only; subtasks follow their parent. */
  project_id: string | null;
  title: string;
  notes: string;
  status: TaskStatus;
  priority: TaskPriority;
  due_date: string | null;
  /** Expected minutes of work (planner, D-041). Missing in caches from before it existed. */
  estimate_minutes?: number | null;
  /** Completing a repeating task creates the next one (D-047). Missing in older caches. */
  repeat?: Repeat;
  repeat_until?: string | null;
  created_at: number;
  updated_at: number;
  completed_at: number | null;
}

export interface NewTask {
  /** Client-chosen id (used for offline creates). */
  id?: string;
  title: string;
  notes?: string;
  priority?: TaskPriority;
  due_date?: string | null;
  parent_id?: string;
  project_id?: string;
  estimate_minutes?: number | null;
  repeat?: Repeat;
  repeat_until?: string | null;
}

/** Fields omitted are unchanged; `due_date: null` clears. */
export type TaskPatch = Partial<Pick<Task, "title" | "notes" | "status" | "priority" | "due_date" | "project_id" | "estimate_minutes" | "repeat" | "repeat_until">>;

/** Closing this task makes the server create the next occurrence. */
export const repeats = (t: Pick<Task, "repeat" | "due_date" | "parent_id">) => !!t.due_date && !t.parent_id && (t.repeat ?? "none") !== "none";

export type TaskFilter = "open" | "closed" | "all";

/** The server lists every open task but only this many recently closed ones (DECISIONS D-022). */
export const RECENT_CLOSED_SHOWN = 500;

export const STATUS_LABELS: Record<TaskStatus, string> = {
  todo: "To do",
  in_progress: "In progress",
  done: "Done",
  cancelled: "Cancelled",
};

export const PRIORITY_LABELS: Record<TaskPriority, string> = {
  none: "None",
  low: "Low",
  medium: "Medium",
  high: "High",
};

const PRIORITY_RANK: Record<TaskPriority, number> = { high: 0, medium: 1, low: 2, none: 3 };

export function isOpen(t: Pick<Task, "status">): boolean {
  return t.status === "todo" || t.status === "in_progress";
}

/** Local calendar date as YYYY-MM-DD. */
export function localDate(d = new Date()): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

function addDays(date: string, days: number): string {
  const [y, m, d] = date.split("-").map(Number);
  return localDate(new Date(y, m - 1, d + days));
}

export type DueTone = "overdue" | "today" | "soon" | "later";

export function dueInfo(due: string | null, today: string): { label: string; tone: DueTone } | undefined {
  if (!due) return undefined;
  if (due < today) return { label: `Overdue · ${shortDate(due)}`, tone: "overdue" };
  if (due === today) return { label: "Today", tone: "today" };
  if (due === addDays(today, 1)) return { label: "Tomorrow", tone: "soon" };
  return { label: shortDate(due), tone: due <= addDays(today, 7) ? "soon" : "later" };
}

function shortDate(date: string): string {
  const [y, m, d] = date.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString([], { month: "short", day: "numeric" });
}

/** Open first; then due date (none last), priority, and creation order. */
export function compareTasks(a: Task, b: Task): number {
  if (isOpen(a) !== isOpen(b)) return isOpen(a) ? -1 : 1;
  if (!isOpen(a)) return (b.completed_at ?? 0) - (a.completed_at ?? 0); // recently closed first
  if (a.due_date !== b.due_date) {
    if (!a.due_date) return 1;
    if (!b.due_date) return -1;
    return a.due_date < b.due_date ? -1 : 1;
  }
  if (a.priority !== b.priority) return PRIORITY_RANK[a.priority] - PRIORITY_RANK[b.priority];
  return a.created_at - b.created_at;
}

export interface TaskNode {
  task: Task;
  subtasks: Task[];
}

/** Top-level tasks matching `filter`, each with all of its subtasks (sorted). */
export function taskTree(tasks: Task[], filter: TaskFilter): TaskNode[] {
  const children = new Map<string, Task[]>();
  for (const t of tasks) {
    if (t.parent_id) children.set(t.parent_id, [...(children.get(t.parent_id) ?? []), t]);
  }
  return tasks
    .filter((t) => !t.parent_id)
    .filter((t) => filter === "all" || (filter === "open" ? isOpen(t) : !isOpen(t)))
    .sort(compareTasks)
    .map((task) => ({ task, subtasks: (children.get(task.id) ?? []).sort(compareTasks) }));
}

/** One reason a task scored the way it did (planner, D-041). */
export interface PlanFactor {
  points: number;
  reason: string;
}

export interface PlanPick {
  task_id: string;
  title: string;
  score: number;
  factors: PlanFactor[];
  /** The task's estimate, or 30 when it has none (`estimated: false`). */
  minutes: number;
  estimated: boolean;
  due_date: string | null;
}

/** `GET /api/v1/tasks/plan`: a proposal only; nothing is saved. */
export interface PlanProposal {
  date: string;
  minutes: number | null;
  picks: PlanPick[];
  total_minutes: number;
  /** Open tasks waiting on their own open subtasks. */
  blocked: number;
  /** Ranked tasks left out because they don't fit the time. */
  too_long: number;
}

/** "45 min", "1 h", "1 h 30 min" (matches the server's wording). */
export function formatMinutes(minutes: number): string {
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  if (h === 0) return `${m} min`;
  return m === 0 ? `${h} h` : `${h} h ${m} min`;
}

/** Parse an estimate input: whole minutes 1–1440, empty clears; otherwise undefined (invalid). */
export function parseEstimate(raw: string): number | null | undefined {
  const s = raw.trim();
  if (s === "") return null;
  if (!/^\d+$/.test(s)) return undefined;
  const n = Number(s);
  return n >= 1 && n <= 1440 ? n : undefined;
}

/**
 * The id the server gives a repeating task's next occurrence (mirrors
 * directory-core `tasks::successor_id`), so undoing a completion can remove it.
 */
export function successorId(id: string): string {
  const mask = (1n << 128n) - 1n;
  const n = /^[0-9a-f]{32}$/.test(id) ? BigInt(`0x${id}`) : 0n;
  const next = (n * 0x9e3779b97f4a7c15f39cc0605cedc835n + 0x2545f4914f6cdd1dn) & mask;
  return next.toString(16).padStart(32, "0");
}
