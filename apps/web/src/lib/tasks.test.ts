import { describe, expect, it } from "vitest";
import { compareTasks, dueInfo, localDate, taskTree, type Task } from "./tasks";

let seq = 0;
function task(over: Partial<Task>): Task {
  seq++;
  return {
    id: `t${seq}`,
    parent_id: null,
    project_id: null,
    title: `Task ${seq}`,
    notes: "",
    status: "todo",
    priority: "none",
    due_date: null,
    created_at: seq,
    updated_at: seq,
    completed_at: null,
    ...over,
  };
}

describe("dueInfo", () => {
  const today = "2026-10-21";
  it("labels relative to today", () => {
    expect(dueInfo(null, today)).toBeUndefined();
    expect(dueInfo("2026-10-20", today)?.tone).toBe("overdue");
    expect(dueInfo("2026-10-21", today)).toEqual({ label: "Today", tone: "today" });
    expect(dueInfo("2026-10-22", today)).toEqual({ label: "Tomorrow", tone: "soon" });
    expect(dueInfo("2026-10-27", today)?.tone).toBe("soon");
    expect(dueInfo("2026-11-30", today)?.tone).toBe("later");
  });

  it("handles month and year boundaries", () => {
    expect(dueInfo("2027-01-01", "2026-12-31")?.label).toBe("Tomorrow");
    expect(localDate(new Date(2026, 0, 5))).toBe("2026-01-05");
  });
});

describe("ordering and tree", () => {
  it("sorts open by due date, then priority, then age; closed after", () => {
    const later = task({ due_date: "2026-12-01" });
    const soonLow = task({ due_date: "2026-11-01", priority: "low" });
    const soonHigh = task({ due_date: "2026-11-01", priority: "high" });
    const undated = task({ priority: "high" });
    const done = task({ status: "done", completed_at: 5 });
    const sorted = [done, undated, later, soonLow, soonHigh].sort(compareTasks).map((t) => t.id);
    expect(sorted).toEqual([soonHigh.id, soonLow.id, later.id, undated.id, done.id]);
  });

  it("groups subtasks under parents and filters top level only", () => {
    const parent = task({});
    const sub1 = task({ parent_id: parent.id, status: "done", completed_at: 1 });
    const sub2 = task({ parent_id: parent.id });
    const closed = task({ status: "cancelled", completed_at: 2 });
    const all = [sub1, parent, closed, sub2];

    const open = taskTree(all, "open");
    expect(open.map((n) => n.task.id)).toEqual([parent.id]);
    expect(open[0].subtasks.map((t) => t.id)).toEqual([sub2.id, sub1.id]);
    expect(taskTree(all, "closed").map((n) => n.task.id)).toEqual([closed.id]);
    expect(taskTree(all, "all")).toHaveLength(2);
  });
});
