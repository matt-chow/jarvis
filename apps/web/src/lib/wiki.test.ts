import { describe, expect, it } from "vitest";
import type { NoteSummary } from "./notes";
import { resolveLinks, wikiTargets } from "./wiki";

const summary = (id: string, title: string): NoteSummary => ({ id, title, excerpt: "", pinned: false, created_at: 0, updated_at: 0 });

describe("wiki links", () => {
  it("parses targets and aliases like the server", () => {
    expect(wikiTargets("See [[Garden plan]] and [[ garden PLAN |the plan]].\n[[Tomatoes|red]] [[]] [[open\nnext]]")).toEqual(["Garden plan", "Tomatoes"]);
  });

  it("resolves case-insensitively and leaves missing targets unresolved", () => {
    const notes = [summary("a", "Garden plan"), summary("b", "Me")];
    const links = resolveLinks("[[garden PLAN]] [[Missing]] [[me]]", notes, "b");
    expect(links.map((l) => [l.target, l.note?.id])).toEqual([
      ["garden PLAN", "a"],
      ["Missing", undefined],
      ["me", undefined],
    ]);
  });
});
