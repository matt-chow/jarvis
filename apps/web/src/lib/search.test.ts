import { describe, expect, it } from "vitest";
import { hitRoute, type SearchHit } from "./search";

const hit = (over: Partial<SearchHit>): SearchHit => ({ kind: "note", id: "n1", title: "", snippet: "", title_match: false, updated_at: 0, ...over });

describe("hitRoute", () => {
  it("opens each kind where it lives", () => {
    expect(hitRoute(hit({}))).toBe("#/notes?note=n1");
    expect(hitRoute(hit({ kind: "event", id: "e1", date: "2026-10-23" }))).toBe("#/calendar?date=2026-10-23");
    expect(hitRoute(hit({ kind: "habit", id: "h1" }))).toBe("#/habits");
    expect(hitRoute(hit({ kind: "journal", id: "2026-10-21" }))).toBe("#/journal?date=2026-10-21");
  });
});
