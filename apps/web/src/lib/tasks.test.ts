import { describe, expect, it } from "vitest";
import { compareTasks, dueInfo, formatMinutes, localDate, parseEstimate, repeats, successorId, taskTree, type Task } from "./tasks";

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

describe("planner helpers (D-041)", () => {
  it("formats minutes like the server", () => {
    expect(formatMinutes(45)).toBe("45 min");
    expect(formatMinutes(60)).toBe("1 h");
    expect(formatMinutes(90)).toBe("1 h 30 min");
  });

  it("parses estimates: whole minutes 1–1440, empty clears", () => {
    expect(parseEstimate("")).toBeNull();
    expect(parseEstimate(" 30 ")).toBe(30);
    expect(parseEstimate("1440")).toBe(1440);
    expect(parseEstimate("0")).toBeUndefined();
    expect(parseEstimate("1441")).toBeUndefined();
    expect(parseEstimate("1.5")).toBeUndefined();
    expect(parseEstimate("soon")).toBeUndefined();
  });
});

describe("repeating tasks (D-047)", () => {
  it("predicts the server's id for the next occurrence", () => {
    // Same values as directory-core `tasks` tests.
    expect(successorId("ab".repeat(16))).toBe("9eefe192aaa3e3007be2dcf95963ff84");
    expect(successorId(`${"0".repeat(31)}1`)).toBe("9e3779b97f4a7c1618e2b4f1ac5aa552");
    expect(successorId(successorId("ab".repeat(16)))).not.toBe("ab".repeat(16));
  });

  it("repeats only top-level tasks with a due date", () => {
    expect(repeats({ repeat: "weekly", due_date: "2026-11-06", parent_id: null })).toBe(true);
    expect(repeats({ repeat: "weekly", due_date: null, parent_id: null })).toBe(false);
    expect(repeats({ repeat: undefined, due_date: "2026-11-06", parent_id: null })).toBe(false);
  });
});
