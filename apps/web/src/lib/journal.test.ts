import { describe, expect, it } from "vitest";
import { monthOf, monthRange, shiftDate, shiftMonth } from "./journal";

describe("journal dates", () => {
  it("shifts days across month and year boundaries", () => {
    expect(shiftDate("2026-10-31", 1)).toBe("2026-11-01");
    expect(shiftDate("2026-01-01", -1)).toBe("2025-12-31");
    expect(shiftDate("2024-02-28", 1)).toBe("2024-02-29");
  });

  it("computes month ranges and shifts", () => {
    expect(monthRange("2026-02")).toEqual({ from: "2026-02-01", to: "2026-02-28" });
    expect(monthRange("2024-02")).toEqual({ from: "2024-02-01", to: "2024-02-29" });
    expect(monthRange("2026-12").to).toBe("2026-12-31");
    expect(shiftMonth("2026-12", 1)).toBe("2027-01");
    expect(shiftMonth("2026-01", -1)).toBe("2025-12");
    expect(monthOf("2026-10-21")).toBe("2026-10");
  });
});
