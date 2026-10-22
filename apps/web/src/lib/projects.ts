/** Project types and pure helpers. */
import { isOpen, type Task } from "./tasks";

export type ProjectStatus = "active" | "archived";

export interface Project {
  id: string;
  name: string;
  description: string;
  status: ProjectStatus;
  created_at: number;
  updated_at: number;
}

export interface NewProject {
  name: string;
  description?: string;
}

export type ProjectPatch = Partial<Pick<Project, "name" | "description" | "status">>;

export interface Progress {
  done: number;
  /** Excludes cancelled tasks. */
  total: number;
  open: number;
}

/** Completion across a project's top-level tasks. */
export function projectProgress(tasks: Task[], projectId: string): Progress {
  const mine = tasks.filter((t) => t.project_id === projectId && !t.parent_id && t.status !== "cancelled");
  const done = mine.filter((t) => t.status === "done").length;
  return { done, total: mine.length, open: mine.filter(isOpen).length };
}

export function percent(p: Progress): number {
  return p.total === 0 ? 0 : Math.round((p.done / p.total) * 100);
}
