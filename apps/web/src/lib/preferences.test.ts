import { describe, expect, it } from "vitest";
import { applyPreferencesPatch, DEFAULT_WIDGETS, moveWidget, widgetsOf } from "./preferences";

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
