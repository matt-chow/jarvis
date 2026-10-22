import { describe, expect, it } from "vitest";
import { monthGrid, occurrenceStarts, occurrences, timeLabel, type CalendarEvent, type Repeat } from "./calendar";

const ev = (start: string, end: string, repeat: Repeat = "none", until: string | null = null, over: Partial<CalendarEvent> = {}): CalendarEvent => ({
  id: "e",
  title: "T",
  start_date: start,
  end_date: end,
  start_time: null,
  end_time: null,
  location: "",
  notes: "",
  repeat,
  repeat_until: until,
  created_at: 0,
  updated_at: 0,
  ...over,
});

describe("calendar expansion (same cases as the server)", () => {
  it("expands repeats within a range", () => {
    expect(occurrenceStarts(ev("2026-10-22", "2026-10-22"), "2026-10-01", "2026-10-31")).toEqual(["2026-10-22"]);
    expect(occurrenceStarts(ev("2026-10-22", "2026-10-22"), "2026-11-01", "2026-11-30")).toEqual([]);
    expect(occurrenceStarts(ev("2026-09-29", "2026-10-02"), "2026-10-01", "2026-10-31")).toEqual(["2026-09-29"]);
    expect(occurrenceStarts(ev("2026-10-01", "2026-10-01", "weekly", "2026-10-22"), "2026-10-10", "2026-10-31")).toEqual(["2026-10-15", "2026-10-22"]);
    expect(occurrenceStarts(ev("2026-10-30", "2026-10-30", "daily"), "2026-10-01", "2026-11-02")).toEqual(["2026-10-30", "2026-10-31", "2026-11-01", "2026-11-02"]);
    expect(occurrenceStarts(ev("2026-01-31", "2026-01-31", "monthly"), "2026-02-01", "2026-05-31")).toEqual(["2026-03-31", "2026-05-31"]);
    expect(occurrenceStarts(ev("2024-02-29", "2024-02-29", "yearly"), "2025-01-01", "2027-12-31")).toEqual([]);
    expect(occurrenceStarts(ev("2024-02-29", "2024-02-29", "yearly"), "2028-01-01", "2028-12-31")).toEqual(["2028-02-29"]);
    expect(occurrenceStarts(ev("1990-06-15", "1990-06-15", "yearly"), "2026-01-01", "2026-12-31")).toEqual(["2026-06-15"]);
  });

  it("sorts all-day first, then by time", () => {
    const a = ev("2026-10-22", "2026-10-22", "none", null, { id: "a", start_time: "09:00" });
    const b = ev("2026-10-22", "2026-10-22", "none", null, { id: "b" });
    const c = ev("2026-10-22", "2026-10-22", "none", null, { id: "c", start_time: "08:30" });
    expect(occurrences([a, b, c], "2026-10-22", "2026-10-22").map((o) => o.event.id)).toEqual(["b", "c", "a"]);
  });
});

describe("month grid", () => {
  it("covers the month in Monday-first weeks", () => {
    const oct = monthGrid("2026-10"); // Oct 1st 2026 is a Thursday
    expect(oct[0][0]).toBe("2026-09-28");
    expect(oct[0][3]).toBe("2026-10-01");
    expect(oct.at(-1)!.at(-1)).toBe("2026-11-01");
    expect(oct).toHaveLength(5);
    expect(monthGrid("2026-02")).toHaveLength(5); // Feb 1st 2026 is a Sunday: 6 lead days + 28
    expect(monthGrid("2027-02")[0][0]).toBe("2027-02-01"); // a Monday: 4 exact weeks
    expect(monthGrid("2027-02")).toHaveLength(4);
  });

  it("labels times", () => {
    expect(timeLabel({ start_time: null, end_time: null })).toBe("All day");
    expect(timeLabel({ start_time: "09:30", end_time: "10:15" })).toBe("09:30–10:15");
  });
});
