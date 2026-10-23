import { describe, expect, it } from "vitest";
import { applyProgress } from "./library";

describe("applyProgress (same rules as the server)", () => {
  it("derives percent and status", () => {
    const first = applyProgress("f", null, { current_page: 5, total_pages: 20 }, 1);
    expect(first).toMatchObject({ status: "reading", percent: 25, started_at: 1, finished_at: null });
    const done = applyProgress("f", first, { current_page: 20 }, 2);
    expect(done).toMatchObject({ status: "finished", percent: 100, finished_at: 2, started_at: 1 });
    const want = applyProgress("f", done, { status: "want" }, 3);
    expect(want).toMatchObject({ status: "want", finished_at: null });
    expect(applyProgress("f", null, { status: "finished", total_pages: 10 }, 4)).toMatchObject({ current_page: 10, percent: 100 });
  });
});
