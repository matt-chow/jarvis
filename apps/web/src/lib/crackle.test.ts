import { describe, expect, it } from "vitest";
import { MAX_CRACKLE_GAIN, crackleGain, crackleSamples } from "./crackle";

describe("crackle", () => {
  it("maps the level to a quiet, clamped gain", () => {
    expect(crackleGain(0)).toBe(0);
    expect(crackleGain(1)).toBe(MAX_CRACKLE_GAIN);
    expect(crackleGain(2)).toBe(MAX_CRACKLE_GAIN);
    expect(crackleGain(-1)).toBe(0);
    expect(crackleGain(0.5)).toBeLessThan(MAX_CRACKLE_GAIN / 2);
  });

  it("generates a bounded loop of noise with pops", () => {
    let seed = 7;
    const random = () => ((seed = (seed * 16807) % 2147483647) - 1) / 2147483646;
    const s = crackleSamples(8000, 2, random);
    expect(s.length).toBe(16000);
    const peak = s.reduce((m, x) => Math.max(m, Math.abs(x)), 0);
    expect(peak).toBeGreaterThan(0.2); // pops stand out from the hiss
    expect(peak).toBeLessThan(2);
    expect(s.every(Number.isFinite)).toBe(true);
  });
});
