import { describe, expect, it } from "vitest";
import { MAX_BATCH, planImport } from "./backup";

const doc = (extra: Record<string, unknown> = {}) => ({ format: "directory-export", format_version: 1, ...extra });

describe("planImport", () => {
  it("rejects files that aren't exports", () => {
    expect(() => planImport({})).toThrow("isn't a Directory export");
    expect(() => planImport(doc({ format_version: 2 }))).toThrow("format version 2");
  });

  it("orders kinds, puts subtasks after parents, and splits batches", () => {
    const tasks = [
      { id: "sub", parent_id: "top" },
      { id: "top", parent_id: null },
    ];
    const notes = Array.from({ length: MAX_BATCH + 5 }, (_, i) => ({ id: `n${i}` }));
    const plan = planImport(doc({ links: [{ id: "l" }], tasks, notes, projects: [{ id: "p" }], preferences: { dashboard: { widgets: ["today"] } }, files: [{}, {}] }));
    expect(plan.batches.map((b) => `${b.kind}:${b.records.length}`)).toEqual(["projects:1", "tasks:2", "notes:100", "notes:5", "links:1", "preferences:1"]);
    expect((plan.batches[1].records as { id: string }[]).map((t) => t.id)).toEqual(["top", "sub"]);
    expect(plan.counts.notes).toBe(105);
    expect(plan.files).toBe(2);
  });

  it("keeps big records in smaller batches", () => {
    const body = "x".repeat(400_000);
    const plan = planImport(doc({ notes: [{ id: "a", body }, { id: "b", body }, { id: "c", body }] }));
    expect(plan.batches.map((b) => b.records.length)).toEqual([2, 1]);
  });
});
