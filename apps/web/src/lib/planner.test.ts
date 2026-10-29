import { describe, expect, it } from "vitest";
import { newBlock, type NewBlock } from "./blocks";
import { nthStart, occurrenceStarts, repeatLabel, type CalendarEvent } from "./calendar";
import {
  ALL_FILTERS,
  allocationLabel,
  allocations,
  applyFilters,
  atRisk,
  buildItems,
  conflicts,
  conflictText,
  DEFAULT_HOURS,
  daySummary,
  freeGaps,
  layoutDay,
  suggestSlots,
  unscheduled,
  weekDates,
  type CalItem,
} from "./planner";
import type { Task } from "./tasks";

const ZONE = "America/New_York";

const task = (id: string, over: Partial<Task> = {}): Task => ({
  id,
  parent_id: null,
  project_id: null,
  title: `Task ${id}`,
  notes: "",
  status: "todo",
  priority: "none",
  due_date: null,
  estimate_minutes: null,
  created_at: 0,
  updated_at: 0,
  completed_at: null,
  ...over,
});

const event = (id: string, over: Partial<CalendarEvent> = {}): CalendarEvent => ({
  id,
  title: `Event ${id}`,
  start_date: "2026-11-02",
  end_date: "2026-11-02",
  start_time: "10:00",
  end_time: "11:00",
  location: "",
  notes: "",
  repeat: "none",
  repeat_until: null,
  created_at: 0,
  updated_at: 0,
  ...over,
});

let n = 0;
const block = (b: Partial<NewBlock> & { status?: "planned" | "done" | "skipped"; actual?: number } = {}) => ({
  ...newBlock(`b${++n}`, { date: "2026-11-02", start_time: "09:00", end_time: "10:00", ...b }),
  ...(b.status ? { status: b.status } : {}),
  ...(b.actual !== undefined ? { actual_minutes: b.actual } : {}),
});

const items = (over: { events?: CalendarEvent[]; blocks?: ReturnType<typeof block>[]; tasks?: Task[] }, from = "2026-11-02", to = "2026-11-08") =>
  buildItems({ events: over.events ?? [], blocks: over.blocks ?? [], tasks: over.tasks ?? [], from, to, zone: ZONE });

describe("calendar repeats (mirrors directory-core)", () => {
  it("expands intervals, counts, and skipped dates", () => {
    const fortnight = event("f", { start_date: "2026-10-05", end_date: "2026-10-05", repeat: "weekly", repeat_interval: 2 });
    expect(occurrenceStarts(fortnight, "2026-10-01", "2026-11-15")).toEqual(["2026-10-05", "2026-10-19", "2026-11-02"]);
    const quarterly = event("q", { start_date: "2026-01-31", end_date: "2026-01-31", repeat: "monthly", repeat_interval: 3 });
    expect(occurrenceStarts(quarterly, "2026-01-01", "2026-12-31")).toEqual(["2026-01-31", "2026-07-31", "2026-10-31"]);
    const five = event("d", { start_date: "2026-10-26", end_date: "2026-10-26", repeat: "daily", repeat_until: "2026-10-30", skip_dates: ["2026-10-28"] });
    expect(nthStart(five, 5)).toBe("2026-10-30");
    expect(occurrenceStarts(five, "2026-10-01", "2026-11-30")).toEqual(["2026-10-26", "2026-10-27", "2026-10-29", "2026-10-30"]);
    expect(nthStart(event("l", { start_date: "2024-02-29", repeat: "yearly" }), 2)).toBe("2028-02-29");
    expect(repeatLabel(fortnight)).toBe("Every 2 weeks");
    expect(repeatLabel({ repeat: "monthly" })).toBe("Every month");
  });
});

describe("buildItems", () => {
  it("merges events, sessions, and deadlines, sorted all-day first", () => {
    const t = task("t1", { due_date: "2026-11-02", project_id: "p1", estimate_minutes: 120 });
    const list = items({
      events: [event("e1"), event("a", { start_time: null, end_time: null, end_date: "2026-11-03" })],
      blocks: [block({ task_id: "t1" })],
      tasks: [t],
    });
    const day = list.filter((i) => i.date === "2026-11-02");
    expect(day.map((i) => `${i.kind}:${i.start}`)).toEqual(["event:null", "deadline:null", "block:540", "event:600"]);
    const session = day.find((i) => i.kind === "block")!;
    expect(session).toMatchObject({ title: "Task t1", taskId: "t1", projectId: "p1", fixed: false });
    expect(day.find((i) => i.kind === "event" && i.start === 600)!.fixed).toBe(true);
    // The multi-day all-day event shows on both days.
    expect(list.filter((i) => i.id === "a").map((i) => i.date)).toEqual(["2026-11-02", "2026-11-03"]);
  });

  it("converts entries from another zone, across midnight too", () => {
    // 10:00 London (GMT in November) is 05:00 in New York.
    const london = event("l", { timezone: "Europe/London" });
    expect(items({ events: [london] })[0]).toMatchObject({ date: "2026-11-02", start: 300, end: 360, zone: "Europe/London" });
    // 01:00–02:00 Tokyo on the 3rd is 11:00–12:00 New York on the 2nd.
    const tokyo = block({ title: "Call", date: "2026-11-03", start_time: "01:00", end_time: "02:00", timezone: "Asia/Tokyo" });
    expect(items({ blocks: [tokyo] })[0]).toMatchObject({ date: "2026-11-02", start: 660, end: 720 });
  });

  it("splits timed multi-day events and late sessions per day", () => {
    const overnight = event("o", { start_time: "22:00", end_time: "02:00", end_date: "2026-11-03" });
    expect(items({ events: [overnight] }).map((i) => [i.date, i.start, i.end, i.fromPrev, i.toNext])).toEqual([
      ["2026-11-02", 1320, 1440, false, true],
      ["2026-11-03", 0, 120, true, false],
    ]);
    expect(items({ blocks: [block({ title: "x", start_time: "23:00", end_time: "24:00" })] })[0]).toMatchObject({ start: 1380, end: 1440 });
  });

  it("expands repeating events only in range, without skipped dates", () => {
    const weekly = event("w", { start_date: "2026-10-05", end_date: "2026-10-05", repeat: "weekly", skip_dates: ["2026-11-09"] });
    const list = items({ events: [weekly] }, "2026-11-01", "2026-11-30");
    expect(list.map((i) => i.occurrence)).toEqual(["2026-11-02", "2026-11-16", "2026-11-23", "2026-11-30"]);
    expect(new Set(list.map((i) => i.key)).size).toBe(4);
  });
});

describe("filters", () => {
  it("separates events, commitments, flexible work, deadlines, completed, and projects", () => {
    const list = items({
      events: [event("fixed"), event("flex", { fixed: false })],
      blocks: [block({ task_id: "t1" }), block({ title: "Focus", fixed: true }), block({ task_id: "t2", status: "done" })],
      tasks: [task("t1", { due_date: "2026-11-03", project_id: "p" }), task("t2", { due_date: "2026-11-04", status: "done" })],
    });
    const kinds = (f: Partial<typeof ALL_FILTERS>) => applyFilters(list, { ...ALL_FILTERS, ...f }).map((i) => `${i.kind}${i.fixed ? "!" : ""}${i.done ? "✓" : ""}`).sort();
    expect(kinds({ events: false })).not.toContain("event!");
    expect(kinds({ fixed: false })).toEqual(["block", "block✓", "deadline!", "deadline!✓", "event"]);
    expect(kinds({ flexible: false })).toEqual(["block!", "deadline!", "deadline!✓", "event!"]);
    expect(kinds({ deadlines: false, completed: false })).toEqual(["block", "block!", "event", "event!"]);
    expect(kinds({ project: "p" })).toEqual(["block", "deadline!", "event", "event!"]);
  });
});

describe("layoutDay", () => {
  it("puts overlapping items side by side and reuses free columns", () => {
    const list = items({
      blocks: [
        block({ title: "A", start_time: "09:00", end_time: "11:00" }),
        block({ title: "B", start_time: "09:30", end_time: "10:00" }),
        block({ title: "C", start_time: "10:00", end_time: "10:30" }),
        block({ title: "D", start_time: "12:00", end_time: "13:00" }),
      ],
    });
    const placed = Object.fromEntries(layoutDay(list).map((p) => [p.item.title, [p.col, p.cols]]));
    expect(placed).toEqual({ A: [0, 2], B: [1, 2], C: [1, 2], D: [0, 1] });
  });
});

describe("allocation", () => {
  const t = task("t", { estimate_minutes: 180, due_date: "2026-11-04" });
  it("adds up many sessions per task; skipped ones don't count", () => {
    const blocks = [
      block({ task_id: "t", start_time: "09:00", end_time: "10:30", status: "done", actual: 75 }),
      block({ task_id: "t", date: "2026-11-03", start_time: "14:00", end_time: "15:00" }),
      block({ task_id: "t", date: "2026-11-05", start_time: "14:00", end_time: "15:00" }),
      block({ task_id: "t", date: "2026-11-03", start_time: "16:00", end_time: "17:00", status: "skipped" }),
    ];
    const a = allocations([t], blocks, ZONE).get("t")!;
    expect(a).toMatchObject({ estimate: 180, scheduled: 210, actual: 75, remaining: 0, beforeDue: 150, sessions: 3, state: "over" });
    expect(allocationLabel(a)).toBe("3 h 30 min scheduled · 30 min over");
    expect(atRisk(t, a)).toBe(true);
  });

  it("reports under-allocation, unestimated tasks, and the unscheduled inbox", () => {
    const blocks = [block({ task_id: "t", start_time: "09:00", end_time: "10:00" }), block({ task_id: "u" })];
    const tasks = [t, task("u"), task("v", { priority: "high" }), task("w", { status: "done" }), task("s", { parent_id: "t" })];
    const allocs = allocations(tasks, blocks, ZONE);
    expect(allocs.get("t")).toMatchObject({ state: "under", remaining: 120 });
    expect(allocationLabel(allocs.get("t")!)).toBe("1 h of 3 h scheduled");
    expect(allocs.get("u")!.state).toBe("unestimated");
    // `u` has a session and no estimate: scheduled enough. Closed tasks and subtasks stay out.
    expect(unscheduled(tasks, allocs).map((x) => x.id)).toEqual(["t", "v"]);
    expect(unscheduled(tasks, allocs, "priority").map((x) => x.id)).toEqual(["v", "t"]);
  });

  it("counts real minutes on a DST night", () => {
    const night = block({ task_id: "t", date: "2026-03-08", start_time: "01:00", end_time: "04:00" });
    expect(allocations([t], [night], ZONE).get("t")!.scheduled).toBe(120);
  });
});

describe("conflicts and suggestions", () => {
  const list: CalItem[] = items({
    events: [event("standup", { start_time: "09:00", end_time: "09:30" }), event("lunch", { start_time: "12:00", end_time: "13:00", fixed: false })],
    blocks: [block({ title: "Focus", start_time: "10:00", end_time: "11:00" }), block({ title: "Gone", start_time: "14:00", end_time: "15:00", status: "skipped" })],
  });

  it("finds overlaps, ignoring the item itself and skipped sessions", () => {
    const found = conflicts({ date: "2026-11-02", start: 540, end: 630 }, list);
    expect(found.map((i) => i.title)).toEqual(["Event standup", "Focus"]);
    expect(conflictText(found)).toBe("Overlaps “Event standup” (commitment), “Focus” (session).");
    const self = list.find((i) => i.title === "Focus")!;
    expect(conflicts({ date: "2026-11-02", start: 600, end: 660, ignore: (i) => i.key === self.key }, list)).toEqual([]);
    expect(conflicts({ date: "2026-11-02", start: 840, end: 900 }, list)).toEqual([]);
    expect(freeGaps(list, "2026-11-02", [540, 1020])).toEqual([[570, 600], [660, 720], [780, 1020]]);
  });

  it("suggests free working-hour slots before the deadline, never moving anything", () => {
    const now = { date: "2026-11-02", minutes: 8 * 60 };
    const slots = suggestSlots({ items: list, from: "2026-11-02", to: "2026-11-08", minutes: 60, hours: DEFAULT_HOURS, now, due: "2026-11-04" });
    // Monday 11:00 (the first hour-long gap), then Tuesday and Wednesday at 09:00; not after the due date.
    expect(slots.map((s) => [s.date, s.start, s.end, s.late])).toEqual([
      ["2026-11-02", 660, 720, false],
      ["2026-11-03", 540, 600, false],
      ["2026-11-04", 540, 600, false],
    ]);
    expect(slots[0].reason).toMatch(/2 days before it's due/);
    expect(slots[2].reason).toMatch(/on the due date/);
    // Weekends are off by default; later today starts at the next quarter hour.
    const later = suggestSlots({ items: [], from: "2026-11-07", to: "2026-11-09", minutes: 30, hours: DEFAULT_HOURS, now: { date: "2026-11-09", minutes: 9 * 60 + 7 } });
    expect(later.map((s) => [s.date, s.start])).toEqual([["2026-11-09", 555]]);
    // No room: the longest shorter gap, flagged.
    const tight = suggestSlots({ items: list, from: "2026-11-02", to: "2026-11-02", minutes: 600, hours: DEFAULT_HOURS, now });
    expect(tight[0]).toMatchObject({ short: true, start: 780, end: 1020 });
  });

  it("summarizes a day", () => {
    const tasks = [task("t", { due_date: "2026-11-02", estimate_minutes: 120 })];
    const day = items({ events: [event("standup", { start_time: "09:00", end_time: "09:30" })], blocks: [block({ task_id: "t", start_time: "10:00", end_time: "11:00" })], tasks });
    const s = daySummary("2026-11-02", day, allocations(tasks, [], ZONE), DEFAULT_HOURS);
    expect(s).toMatchObject({ commitments: 1, commitmentMinutes: 30, sessions: 1, plannedMinutes: 60, due: 1, remainingMinutes: 120, freeMinutes: 480 - 90 });
  });

  it("lays out Monday-first weeks", () => {
    expect(weekDates("2026-11-05")).toEqual(["2026-11-02", "2026-11-03", "2026-11-04", "2026-11-05", "2026-11-06", "2026-11-07", "2026-11-08"]);
  });
});
