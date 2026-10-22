import { describe, expect, it } from "vitest";
import { breadcrumbs, filesRoute, joinFolder, parentFolder } from "./files";

describe("file paths", () => {
  it("joins and splits folder paths", () => {
    expect(joinFolder("", "docs")).toBe("docs");
    expect(joinFolder("docs", " /2026/ ")).toBe("docs/2026");
    expect(parentFolder("docs/2026")).toBe("docs");
    expect(parentFolder("docs")).toBe("");
  });

  it("builds breadcrumbs and routes", () => {
    expect(breadcrumbs("docs/2026")).toEqual([
      { name: "files", path: "" },
      { name: "docs", path: "docs" },
      { name: "2026", path: "docs/2026" },
    ]);
    expect(filesRoute("")).toBe("#/files");
    expect(filesRoute("a b/c")).toBe("#/files?folder=a%20b%2Fc");
  });
});
