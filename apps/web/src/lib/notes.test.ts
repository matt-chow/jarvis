import { describe, expect, it } from "vitest";
import { sortSummaries, summarize, type Note } from "./notes";

const note = (over: Partial<Note>): Note => ({
  id: "n1",
  title: "",
  body: "",
  pinned: false,
  created_at: 1,
  updated_at: 1,
  ...over,
});

describe("summarize", () => {
  it("derives the title from the first line, stripping Markdown headings", () => {
    expect(summarize(note({ body: "# Groceries\n\nmilk\neggs" }))).toMatchObject({ title: "Groceries", excerpt: "milk eggs" });
    expect(summarize(note({ title: "Ideas", body: "one\ntwo" }))).toMatchObject({ title: "Ideas", excerpt: "one two" });
    expect(summarize(note({}))).toMatchObject({ title: "Untitled", excerpt: "" });
  });

  it("orders pinned first, then newest", () => {
    const list = [
      summarize(note({ id: "a", updated_at: 5 })),
      summarize(note({ id: "b", updated_at: 1, pinned: true })),
      summarize(note({ id: "c", updated_at: 9 })),
    ];
    expect(sortSummaries(list).map((n) => n.id)).toEqual(["b", "c", "a"]);
  });
});
