import { describe, expect, it } from "vitest";
import { applyPreferencesPatch, DEFAULT_WIDGETS, jarvisForm, moveWidget, parseJarvisForm, widgetsOf } from "./preferences";

describe("dashboard preferences", () => {
  it("defaults, patches, and resets", () => {
    expect(widgetsOf(undefined)).toEqual(DEFAULT_WIDGETS);
    const custom = applyPreferencesPatch({}, { dashboard: { widgets: ["today"] } });
    expect(widgetsOf(custom)).toEqual(["today"]);
    expect(applyPreferencesPatch(custom, {})).toEqual(custom);
    expect(widgetsOf(applyPreferencesPatch(custom, { dashboard: null }))).toEqual(DEFAULT_WIDGETS);
  });

  it("moves widgets within bounds", () => {
    expect(moveWidget(["capture", "today", "notes"], "today", -1)).toEqual(["today", "capture", "notes"]);
    expect(moveWidget(["capture", "today"], "capture", -1)).toEqual(["capture", "today"]);
    expect(moveWidget(["capture", "today"], "today", 1)).toEqual(["capture", "today"]);
  });
});

describe("jarvis preferences", () => {
  it("round-trips the form and treats blanks as defaults", () => {
    const j = { avoid_after: "21:00", break_minutes: 10, max_daily_minutes: 180, earliest_first: true };
    expect(parseJarvisForm(jarvisForm(j))).toEqual({ prefs: j });
    expect(parseJarvisForm(jarvisForm(undefined))).toEqual({ prefs: { break_minutes: 0, earliest_first: false } });
    expect(parseJarvisForm({ ...jarvisForm(undefined), break_minutes: "" })).toEqual({ prefs: { break_minutes: 0, earliest_first: false } });
  });

  it("rejects what the server rejects", () => {
    const base = jarvisForm(undefined);
    expect(parseJarvisForm({ ...base, break_minutes: "500" })).toEqual({ error: "Break must be 0 to 120 minutes." });
    expect(parseJarvisForm({ ...base, default_task_minutes: "5" })).toHaveProperty("error");
    expect(parseJarvisForm({ ...base, max_daily_minutes: "10" })).toHaveProperty("error");
    expect(parseJarvisForm({ ...base, max_daily_minutes: "1.5" })).toHaveProperty("error");
    expect(parseJarvisForm({ ...base, avoid_after: "25:00" })).toEqual({ error: "Use a time like 21:00." });
    expect(parseJarvisForm({ ...base, avoid_after: "24:00" })).toHaveProperty("prefs");
  });

  it("sets and removes them by patch", () => {
    const j = { break_minutes: 5, earliest_first: false };
    const set = applyPreferencesPatch({}, { jarvis: j });
    expect(set.jarvis).toEqual(j);
    expect(applyPreferencesPatch(set, {})).toEqual(set);
    expect(applyPreferencesPatch(set, { jarvis: null })).toEqual({});
  });
});
