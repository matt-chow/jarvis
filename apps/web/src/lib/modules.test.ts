import { describe, expect, it } from "vitest";
import { DEFAULT_MODULE, MODULES, moduleFromHash } from "./modules";

describe("module registry", () => {
  it("has unique ids", () => {
    const ids = MODULES.map((m) => m.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("resolves hashes", () => {
    expect(moduleFromHash("#/notes")).toBe("notes");
    expect(moduleFromHash("#files")).toBe("files");
    expect(moduleFromHash("#/jarvis/sub?x=1")).toBe("jarvis");
    expect(moduleFromHash("")).toBe(DEFAULT_MODULE);
    expect(moduleFromHash("#/unknown")).toBe(DEFAULT_MODULE);
  });
});

describe("hash params", () => {
  it("reads query parameters from hash routes", async () => {
    const { hashParam } = await import("./modules");
    expect(hashParam("#/tasks?project=abc", "project")).toBe("abc");
    expect(hashParam("#/tasks", "project")).toBeUndefined();
    expect(hashParam("#/tasks?x=1", "project")).toBeUndefined();
  });
});
