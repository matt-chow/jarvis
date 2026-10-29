import { describe, expect, it } from "vitest";
import { greeting } from "./format";

describe("greeting", () => {
  it("follows the local hour", () => {
    expect(greeting(3)).toBe("Good evening");
    expect(greeting(5)).toBe("Good morning");
    expect(greeting(11)).toBe("Good morning");
    expect(greeting(12)).toBe("Good afternoon");
    expect(greeting(17)).toBe("Good afternoon");
    expect(greeting(18)).toBe("Good evening");
    expect(greeting(23)).toBe("Good evening");
  });
});
