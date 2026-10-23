import { describe, expect, it } from "vitest";
import { DEDUPE_MS, MAX_PER_MINUTE, Throttle, describeThrown } from "./errors";

describe("error reporting", () => {
  it("dedupes repeats and caps the rate", () => {
    const t = new Throttle();
    expect(t.allow("a", 0)).toBe(true);
    expect(t.allow("a", 1_000)).toBe(false);
    expect(t.allow("a", DEDUPE_MS + 1)).toBe(true);
    const many = new Throttle();
    const allowed = Array.from({ length: 10 }, (_, i) => many.allow(`k${i}`, i)).filter(Boolean).length;
    expect(allowed).toBe(MAX_PER_MINUTE);
    expect(many.allow("later", 61_000)).toBe(true);
  });

  it("describes anything thrown", () => {
    expect(describeThrown(new TypeError("boom")).message).toBe("TypeError: boom");
    expect(describeThrown("plain").message).toBe("plain");
    expect(describeThrown({ code: 1 }).message).toBe('{"code":1}');
  });
});
