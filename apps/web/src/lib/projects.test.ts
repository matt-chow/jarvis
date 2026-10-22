import { describe, expect, it } from "vitest";
import { percent, projectProgress } from "./projects";
import type { Task } from "./tasks";

const t = (over: Partial<Task>): Task => ({
  id: Math.random().toString(36).slice(2),
  parent_id: null,
  project_id: "p1",
  title: "t",
  notes: "",
  status: "todo",
  priority: "none",
  due_date: null,
  created_at: 0,
  updated_at: 0,
  completed_at: null,
  ...over,
});

describe("projectProgress", () => {
  it("counts top-level tasks of the project, excluding cancelled", () => {
    const tasks = [
      t({ status: "done" }),
      t({ status: "in_progress" }),
      t({ status: "cancelled" }),
      t({ status: "done", parent_id: "x" }), // subtask: ignored
      t({ status: "done", project_id: "p2" }), // other project
    ];
    const p = projectProgress(tasks, "p1");
    expect(p).toEqual({ done: 1, total: 2, open: 1 });
    expect(percent(p)).toBe(50);
    expect(percent(projectProgress([], "p1"))).toBe(0);
  });
});
