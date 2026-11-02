import { describe, expect, it } from "vitest";
import { byName, fromListing, groupByCategory, matchesCategory, nameProblem, NO_CATEGORY, taskCategories, withTaskCategories, type Category } from "./categories";

const cat = (id: string, name: string): Category => ({ id, name, color: "#3b3fa8", icon: null, created_at: 0, updated_at: 0 });

describe("categories", () => {
  const listing = {
    categories: [cat("b", "work"), cat("a", "School")],
    tasks: [
      { task_id: "t1", category_id: "a" },
      { task_id: "t1", category_id: "b" },
      { task_id: "t2", category_id: "b" },
    ],
  };

  it("builds the cache from a listing, sorted by name", () => {
    const items = fromListing(listing);
    expect(items.map((c) => [c.id, c.task_ids])).toEqual([
      ["a", ["t1"]],
      ["b", ["t1", "t2"]],
    ]);
    expect(taskCategories(items, "t2").map((c) => c.id)).toEqual(["b"]);
    expect(taskCategories(undefined, "t2")).toEqual([]);
  });

  it("replaces a task's set", () => {
    const items = withTaskCategories(fromListing(listing), "t1", ["b"]);
    expect(items.map((c) => c.task_ids)).toEqual([[], ["t1", "t2"]]);
    expect(withTaskCategories(items, "t3", ["a"])[0].task_ids).toEqual(["t3"]);
  });

  it("checks names like the server", () => {
    const items = listing.categories;
    expect(byName(items, "  SCHOOL ")?.id).toBe("a");
    expect(nameProblem(items, " ")).toBe("Enter a name.");
    expect(nameProblem(items, "x".repeat(41))).toMatch(/40/);
    expect(nameProblem(items, "school")).toMatch(/exists/);
    expect(nameProblem(items, "school", "a")).toBeUndefined();
    expect(nameProblem(items, "Home")).toBeUndefined();
  });

  it("filters and groups", () => {
    expect(matchesCategory("", [])).toBe(true);
    expect(matchesCategory(NO_CATEGORY, [])).toBe(true);
    expect(matchesCategory(NO_CATEGORY, ["a"])).toBe(false);
    expect(matchesCategory("a", ["b", "a"])).toBe(true);
    const items = fromListing(listing);
    const tasks = ["t1", "t2", "t3"];
    const groups = groupByCategory(tasks, items, (t) => taskCategories(items, t).map((c) => c.id));
    expect(groups.map((g) => [g.category?.name, g.items])).toEqual([
      ["School", ["t1"]],
      ["work", ["t1", "t2"]],
      [undefined, ["t3"]],
    ]);
  });
});
