import { describe, expect, it } from "vitest";
import { createGate } from "./gate";

describe("createGate (D-039)", () => {
  it("lets the latest load commit", () => {
    const g = createGate();
    const t = g.begin();
    expect(g.current(t)).toBe(true);
  });

  it("drops a load that a local edit overtook", () => {
    const g = createGate();
    const t = g.begin(); // reload starts…
    g.bump(); // …an offline edit lands…
    expect(g.current(t)).toBe(false); // …so the late response is ignored
  });

  it("drops an older load that resolves after a newer one", () => {
    const g = createGate();
    const older = g.begin();
    const newer = g.begin();
    expect(g.current(newer)).toBe(true);
    expect(g.current(older)).toBe(false);
  });
});
