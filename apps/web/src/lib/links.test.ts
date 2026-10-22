import { describe, expect, it } from "vitest";
import { entityRoute, filterCandidates, type Candidate } from "./links";

describe("links", () => {
  it("routes each kind to its module with a selection param", () => {
    expect(entityRoute({ kind: "task", id: "t1" })).toBe("#/tasks?task=t1");
    expect(entityRoute({ kind: "journal", id: "2026-10-21" })).toBe("#/journal?date=2026-10-21");
  });

  it("filters candidates by title, excluding self and existing links", () => {
    const all: Candidate[] = [
      { kind: "task", id: "t1", title: "Draft talk" },
      { kind: "note", id: "n1", title: "Talk outline" },
      { kind: "note", id: "n2", title: "Groceries" },
      { kind: "project", id: "p1", title: "Conference talk" },
    ];
    const linked = [{ link_id: "l", kind: "project" as const, id: "p1", title: "Conference talk", created_at: 0 }];
    const got = filterCandidates(all, "TALK", { kind: "task", id: "t1" }, linked).map((c) => c.id);
    expect(got).toEqual(["n1"]);
    expect(filterCandidates(all, "", { kind: "task", id: "t1" }, [], 2)).toHaveLength(2);
  });
});
