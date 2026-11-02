import { describe, expect, it } from "vitest";
import {
  afterCompletion,
  alertLabel,
  completeLocally,
  defaultAlerts,
  dueAt,
  isOverdue,
  listNameProblem,
  matches,
  newReminderFrom,
  nextAfter,
  patchReminder,
  projectReminders,
  ruleSummary,
  selectReminders,
  snoozeUntil,
  tomorrowMorning,
  upcoming,
  zonedMs,
  type NewReminder,
  type Reminder,
  type Rule,
} from "./reminders";

const TZ = "America/Toronto";
const rem = (id: string, n: Partial<NewReminder> = {}, over: Partial<Reminder> = {}): Reminder => ({
  ...newReminderFrom(id, { title: id, timezone: TZ, ...n }, 0),
  ...over,
});
const rule = (r: Partial<Rule>): Rule => ({ freq: "daily", interval: 1, weekdays: [], until: null, ...r });

describe("zones (DST, like the server)", () => {
  it("maps Toronto wall times to UTC across both DST changes", () => {
    expect(zonedMs("2026-01-15", 9 * 60, TZ)).toBe(Date.UTC(2026, 0, 15, 14, 0)); // EST
    expect(zonedMs("2026-07-15", 9 * 60, TZ)).toBe(Date.UTC(2026, 6, 15, 13, 0)); // EDT
    // The day after each change.
    expect(zonedMs("2026-03-09", 8 * 60, TZ)).toBe(Date.UTC(2026, 2, 9, 12, 0));
    expect(zonedMs("2026-11-02", 8 * 60, TZ)).toBe(Date.UTC(2026, 10, 2, 13, 0));
    // A skipped wall time moves on by the gap; a repeated one is the first.
    expect(zonedMs("2026-03-08", 2 * 60 + 30, TZ)).toBe(Date.UTC(2026, 2, 8, 7, 30));
    expect(zonedMs("2026-11-01", 90, TZ)).toBe(Date.UTC(2026, 10, 1, 5, 30));
    expect(zonedMs("2026-01-15", 0, "Nowhere/Zone")).toBeNull();
  });

  it("date-only reminders are due at local midnight", () => {
    expect(dueAt("2026-11-02", null, TZ)).toBe(Date.UTC(2026, 10, 2, 5, 0));
    expect(dueAt(null, null, TZ)).toBeNull();
  });

  it("snoozes to an absolute instant", () => {
    const now = Date.UTC(2026, 10, 2, 3, 30); // 22:30 on Nov 1 in Toronto
    expect(snoozeUntil("10m", TZ, now)).toBe(now + 600_000);
    expect(snoozeUntil("1h", TZ, now)).toBe(now + 3_600_000);
    expect(tomorrowMorning(TZ, now)).toBe(Date.UTC(2026, 10, 2, 14, 0));
    expect(snoozeUntil("tomorrow", "Asia/Tokyo", now)).toBe(Date.UTC(2026, 10, 3, 0, 0));
  });
});

describe("recurrence (mirrors core)", () => {
  it("clamps month ends and leap days", () => {
    const monthly = rule({ freq: "monthly" });
    expect(nextAfter(monthly, "2026-01-31", "2026-01-31")).toBe("2026-02-28");
    expect(nextAfter(monthly, "2026-01-31", "2026-02-28")).toBe("2026-03-31");
    expect(nextAfter(rule({ freq: "yearly" }), "2024-02-29", "2024-02-29")).toBe("2025-02-28");
    expect(nextAfter(rule({ freq: "yearly", interval: 4 }), "2024-02-29", "2024-02-29")).toBe("2028-02-29");
    expect(matches(monthly, "2026-01-31", "2026-04-30")).toBe(true);
    expect(matches(monthly, "2026-01-31", "2026-04-29")).toBe(false);
  });

  it("walks weekdays and intervals, and stops at until", () => {
    const monWed = rule({ freq: "weekly", weekdays: [0, 2] });
    expect(nextAfter(monWed, "2026-11-02", "2026-11-02")).toBe("2026-11-04");
    expect(nextAfter(monWed, "2026-11-02", "2026-11-04")).toBe("2026-11-09");
    const fortnight = rule({ freq: "weekly", interval: 2, weekdays: [0] });
    expect(nextAfter(fortnight, "2026-11-02", "2026-11-02")).toBe("2026-11-16");
    expect(nextAfter(rule({ interval: 3 }), "2026-11-01", "2026-11-02")).toBe("2026-11-04");
    expect(nextAfter(rule({ until: "2026-11-03" }), "2026-11-01", "2026-11-03")).toBeNull();
    // Before the anchor, the anchor itself is next.
    expect(nextAfter(rule({}), "2026-11-10", "2026-11-01")).toBe("2026-11-10");
  });

  it("counts completion advance from the completion day", () => {
    expect(afterCompletion(rule({ interval: 3 }), "2026-11-05")).toBe("2026-11-08");
    expect(afterCompletion(rule({ freq: "monthly" }), "2026-01-31")).toBe("2026-02-28");
    expect(afterCompletion(rule({ freq: "weekly", weekdays: [0] }), "2026-11-04")).toBe("2026-11-09");
  });
});

describe("local edits", () => {
  it("completes a one-off, and moves a series on", () => {
    const once = rem("a", { due_date: "2026-11-02" });
    expect(completeLocally(once, 5).completed_at).toBe(5);
    const daily = rem("b", { due_date: "2026-11-02", due_time: "08:00", recurrence: rule({}) }, { snoozed_until: 9 });
    const next = completeLocally(daily, 5, "2026-11-02");
    expect(next).toMatchObject({ due_date: "2026-11-03", completed_at: null, snoozed_until: null, due_at: Date.UTC(2026, 10, 3, 13, 0) });
    // Completion advance counts from today when late, and re-anchors.
    const late = rem("c", { due_date: "2026-11-02", recurrence: rule({ interval: 2 }), advance: "completion" });
    expect(completeLocally(late, 5, "2026-11-10")).toMatchObject({ due_date: "2026-11-12", recurrence: { anchor: "2026-11-12" } });
    // The last occurrence closes the series.
    const last = rem("d", { due_date: "2026-11-02", recurrence: rule({ until: "2026-11-02" }) });
    expect(completeLocally(last, 5, "2026-11-02").completed_at).toBe(5);
  });

  it("patches like the server: no time or repeat without a date; a new time clears the snooze", () => {
    const r = rem("a", { due_date: "2026-11-02", due_time: "08:00", recurrence: rule({}), task_id: "t", complete_with_task: true }, { snoozed_until: 9, version: 4 });
    const moved = patchReminder(r, { due_time: "10:00", base_version: 4 }, 2);
    expect(moved).toMatchObject({ due_time: "10:00", snoozed_until: null, due_at: Date.UTC(2026, 10, 2, 15, 0), updated_at: 2 });
    expect(moved).not.toHaveProperty("base_version");
    expect(patchReminder(r, { due_date: null }, 2)).toMatchObject({ due_time: null, recurrence: null, due_at: null });
    expect(patchReminder(r, { due_date: "2026-11-09" }, 2).recurrence?.anchor).toBe("2026-11-09");
    expect(patchReminder(r, { task_id: null }, 2).complete_with_task).toBe(false);
    expect(patchReminder(r, { title: "  x " }, 2).title).toBe("x");
  });

  it("gives default alerts only to dated reminders", () => {
    expect(defaultAlerts({ due_date: null })).toEqual([]);
    expect(rem("a").alerts).toEqual([]);
    expect(rem("b", { due_date: "2026-11-02", due_time: "08:00" }).alerts).toEqual([{ kind: "before", minutes: 0 }]);
    expect(alertLabel({ kind: "at", time: "18:00", days_before: 1 })).toBe("At 18:00, 1 day before");
    expect(alertLabel({ kind: "before", minutes: 2880 })).toBe("2 days before");
  });
});

describe("project reminders", () => {
  it("are open reminders linked to the project or one of its tasks, by due date", () => {
    const all = [
      rem("late", { project_id: "p", due_date: "2026-11-09" }),
      rem("viaTask", { task_id: "t1", due_date: "2026-11-02" }),
      rem("undated", { project_id: "p" }),
      rem("done", { project_id: "p" }, { completed_at: 1 }),
      rem("deleted", { project_id: "p" }, { deleted_at: 1 }),
      rem("other", { project_id: "q", task_id: "t9" }),
    ];
    expect(projectReminders(all, "p", new Set(["t1"])).map((r) => r.id)).toEqual(["viaTask", "late", "undated"]);
  });
});

describe("views", () => {
  const today = "2026-11-02";
  const now = Date.UTC(2026, 10, 2, 17, 0); // 12:00 in Toronto
  const all = [
    rem("late", { due_date: "2026-11-01", priority: "high" }),
    rem("morning", { due_date: today, due_time: "09:00", list_id: "l1" }),
    rem("evening", { due_date: today, due_time: "20:00", category_ids: ["c1"], flagged: true }),
    rem("later", { due_date: "2026-11-05", notes: "call the dentist" }),
    rem("someday", {}),
    rem("done", { due_date: "2026-11-01" }, { completed_at: 1 }),
    rem("gone", { title: "dentist old" }, { deleted_at: 1 }),
  ];
  const ids = (q: Partial<Parameters<typeof selectReminders>[1]>) =>
    selectReminders(all, { view: "all", list: "", category: "", search: "", sort: "due", ...q }, today).map((r) => r.id);

  it("splits smart views, lists, and categories", () => {
    expect(ids({ view: "today" })).toEqual(["late", "morning", "evening"]);
    expect(ids({ view: "scheduled" })).toEqual(["late", "morning", "evening", "later"]);
    expect(ids({})).toEqual(["late", "morning", "evening", "later", "someday"]);
    expect(ids({ view: "flagged" })).toEqual(["evening"]);
    expect(ids({ view: "completed" })).toEqual(["done"]);
    expect(ids({ view: "deleted" })).toEqual(["gone"]);
    expect(ids({ list: "l1" })).toEqual(["morning"]);
    expect(ids({ list: "none" })).toHaveLength(4);
    expect(ids({ category: "c1" })).toEqual(["evening"]);
    expect(ids({ sort: "title" })[0]).toBe("evening");
  });

  it("searches completed reminders too, but not deleted ones", () => {
    expect(ids({ view: "today", search: "DENTIST" })).toEqual(["later"]);
    expect(ids({ search: "done" })).toEqual(["done"]);
    expect(ids({ view: "deleted", search: "dentist" })).toEqual(["gone"]);
  });

  it("is overdue past its time or its date, until resolved", () => {
    const [late, morning, evening] = all;
    expect(isOverdue(late, now, today)).toBe(true);
    expect(isOverdue(morning, now, today)).toBe(true);
    expect(isOverdue(evening, now, today)).toBe(false);
    expect(isOverdue(all[5], now, today)).toBe(false);
    const u = upcoming(all, now, today, 2);
    expect(u.overdue.map((r) => r.id)).toEqual(["late", "morning"]);
    expect(u.soon.map((r) => r.id)).toEqual(["evening"]);
  });

  it("names rules and checks list names", () => {
    expect(ruleSummary(rule({ freq: "weekly", weekdays: [2, 0] }))).toBe("Weekly on Mon, Wed");
    expect(ruleSummary(rule({ interval: 3, until: "2026-12-01" }))).toBe("Every 3 days until 2026-12-01");
    const lists = [{ id: "l1", name: "Errands", color: "#000", created_at: 0, updated_at: 0 }];
    expect(listNameProblem(lists, " errands ")).toBe("A list with that name exists.");
    expect(listNameProblem(lists, "Errands", "l1")).toBeUndefined();
    expect(listNameProblem(lists, " ")).toBe("Give the list a name.");
  });
});
