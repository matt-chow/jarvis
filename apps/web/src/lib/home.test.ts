import { describe, expect, it } from "vitest";
import type { CalendarEvent, Occurrence } from "./calendar";
import { glance, glanceParts } from "./home";
import type { Task } from "./tasks";

const TODAY = "2026-11-03";
const occ = (title: string, start_time: string | null, end_time: string | null): Occurrence => ({
  event: { title, start_time, end_time } as CalendarEvent,
  start_date: TODAY,
  end_date: TODAY,
});
const task = (due_date: string | null, status = "todo") => ({ due_date, status }) as Task;

describe("home at a glance", () => {
  it("finds the next event and counts what needs attention", () => {
    const events = [occ("Holiday", null, null), occ("Standup", "09:00", "09:15"), occ("Lunch", "12:00", "13:00"), occ("Review", "15:00", null)];
    const tasks = [task("2026-11-01"), task(TODAY), task(TODAY, "done"), task(null), task("2026-11-09")];
    expect(glance(events, tasks, 2, TODAY, "10:00")).toEqual({ next: { title: "Lunch", start: "12:00", now: false }, overdue: 3, dueToday: 1 });
    expect(glance(events, tasks, 0, TODAY, "12:30").next).toEqual({ title: "Lunch", start: "12:00", now: true });
    expect(glance(events, [], 0, TODAY, "15:30").next).toBeUndefined();
  });

  it("says something even on a quiet day", () => {
    expect(glanceParts({ overdue: 0, dueToday: 0 })).toEqual(["Nothing else scheduled or due today."]);
    expect(glanceParts({ next: { title: "Lunch", start: "12:00", now: false }, overdue: 1, dueToday: 2 })).toEqual(["Next: Lunch at 12:00", "2 due today", "1 overdue"]);
    expect(glanceParts({ next: { title: "Lunch", start: "12:00", now: true }, overdue: 0, dueToday: 0 })).toEqual(["Now: Lunch"]);
  });
});
