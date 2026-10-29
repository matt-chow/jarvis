import { describe, expect, it } from "vitest";
import { convert, durationLabel, elapsed, fromMinutes, instant, snap, toMinutes, wallClock } from "./time";

describe("wall-clock time", () => {
  it("converts HH:MM and snaps", () => {
    expect(toMinutes("09:30")).toBe(570);
    expect(toMinutes("24:00")).toBe(1440);
    expect(fromMinutes(570)).toBe("09:30");
    expect(fromMinutes(1440)).toBe("24:00");
    expect(fromMinutes(-5)).toBe("00:00");
    expect(snap(52)).toBe(45);
    expect(snap(53)).toBe(60);
    expect(durationLabel(90)).toBe("1 h 30 min");
    expect(durationLabel(45)).toBe("45 min");
  });

  it("converts between zones", () => {
    // 10:00 in Toronto (EDT, -4) is 15:00 in London (BST, +1) in early October.
    expect(convert("2026-10-05", 600, "America/Toronto", "Europe/London")).toEqual({ date: "2026-10-05", minutes: 900 });
    // Late evening in New York is the next morning in Tokyo.
    expect(convert("2026-11-02", 22 * 60, "America/New_York", "Asia/Tokyo")).toEqual({ date: "2026-11-03", minutes: 12 * 60 });
    expect(convert("2026-11-02", 600, "UTC", "UTC")).toEqual({ date: "2026-11-02", minutes: 600 });
    expect(wallClock(Date.UTC(2026, 0, 1, 12, 0), "UTC")).toEqual({ date: "2026-01-01", minutes: 720 });
  });

  it("measures real minutes across DST changes", () => {
    // New York springs forward on 2026-03-08 and falls back on 2026-11-01.
    expect(elapsed("2026-03-08", 0, 240, "America/New_York")).toBe(180);
    expect(elapsed("2026-11-01", 0, 180, "America/New_York")).toBe(240);
    expect(elapsed("2026-11-02", 0, 180, "America/New_York")).toBe(180);
    // London moves on 2026-03-29 and 2026-10-25.
    expect(elapsed("2026-03-29", 0, 1440, "Europe/London")).toBe(23 * 60);
    expect(elapsed("2026-10-25", 0, 1440, "Europe/London")).toBe(25 * 60);
  });

  it("resolves skipped times forward and repeated ones to the first", () => {
    // 02:30 doesn't exist on 2026-03-08 in New York: it becomes 03:30 EDT (07:30 UTC).
    expect(instant("2026-03-08", 150, "America/New_York")).toBe(Date.UTC(2026, 2, 8, 7, 30));
    // 01:30 happens twice on 2026-11-01: the first is EDT (05:30 UTC).
    expect(instant("2026-11-01", 90, "America/New_York")).toBe(Date.UTC(2026, 10, 1, 5, 30));
    // 24:00 is the next midnight.
    expect(instant("2026-11-02", 1440, "UTC")).toBe(Date.UTC(2026, 10, 3));
  });
});
