import { describe, expect, it } from "vitest";
import { describeDays, EVERY_DAY, lastDays, streak, weekday, withCheck, type HabitView } from "./habits";

const TODAY = "2026-10-22"; // a Thursday

describe("habits", () => {
  it("knows weekdays and recent days", () => {
    expect(weekday(TODAY)).toBe(3);
    expect(weekday("2026-10-25")).toBe(6);
    expect(lastDays(TODAY, 3)).toEqual(["2026-10-20", "2026-10-21", TODAY]);
    expect(lastDays("2026-03-01", 2)).toEqual(["2026-02-28", "2026-03-01"]);
  });

  it("computes streaks like the server", () => {
    const set = (...d: string[]) => new Set(d);
    expect(streak(EVERY_DAY, set("2026-10-21", "2026-10-20"), TODAY)).toBe(2);
    expect(streak(EVERY_DAY, set(TODAY, "2026-10-20"), TODAY)).toBe(1);
    const weekdays = [0, 1, 2, 3, 4];
    expect(streak(weekdays, set("2026-10-19", "2026-10-16", "2026-10-15"), "2026-10-19")).toBe(3);
    expect(streak(weekdays, set("2026-10-19", "2026-10-18", "2026-10-16"), "2026-10-19")).toBe(3);
  });

  it("toggles a check-in and recomputes the streak", () => {
    const h: HabitView = { id: "h", name: "Read", days: EVERY_DAY, archived: false, created_at: 0, updated_at: 0, checkins: ["2026-10-21"], streak: 1 };
    const on = withCheck(h, TODAY, true, TODAY);
    expect(on).toMatchObject({ checkins: ["2026-10-21", TODAY], streak: 2 });
    expect(withCheck(on, "2026-10-21", false, TODAY)).toMatchObject({ checkins: [TODAY], streak: 1 });
  });

  it("describes schedules", () => {
    expect(describeDays(EVERY_DAY)).toBe("Every day");
    expect(describeDays([0, 1, 2, 3, 4])).toBe("Weekdays");
    expect(describeDays([0, 2, 4])).toBe("Mon, Wed, Fri");
  });
});
