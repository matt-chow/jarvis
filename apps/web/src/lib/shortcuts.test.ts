import { describe, expect, it } from "vitest";
import { parseDeviceSettings, DEVICE_DEFAULTS } from "./device";
import { SHORTCUTS, ariaKeys, isTyping, keyLabel, matchShortcut, searchShortcuts, shortcutById } from "./shortcuts";

const key = (k: string, mods: Partial<{ metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey: boolean; code: string }> = {}) => ({
  key: k,
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  ...mods,
});

describe("shortcuts", () => {
  it("have unique ids and unique key combinations", () => {
    expect(new Set(SHORTCUTS.map((s) => s.id)).size).toBe(SHORTCUTS.length);
    expect(new Set(SHORTCUTS.map((s) => s.keys.join("+"))).size).toBe(SHORTCUTS.length);
  });

  it("never claim a browser default chord", () => {
    const reserved = ["n", "t", "w", "r", "l", "f", "p", "s", "d", "h", "k", "q", "o"];
    for (const s of SHORTCUTS) if (s.keys[0] === "Mod") expect(reserved).not.toContain(s.keys[1]);
  });

  it("label keys for the platform", () => {
    expect(keyLabel("Mod", true)).toBe("⌘");
    expect(keyLabel("Mod", false)).toBe("Ctrl");
    expect(keyLabel("Alt", true)).toBe("⌥");
    expect(keyLabel("j", false)).toBe("J");
    expect(ariaKeys(shortcutById("ask"), true)).toBe("Meta+J");
    expect(ariaKeys(shortcutById("ask"), false)).toBe("Control+J");
    expect(ariaKeys(shortcutById("go-tasks"))).toBeUndefined();
  });

  it("match chords with the platform modifier only", () => {
    expect(matchShortcut(key("j", { metaKey: true }), null, true)).toMatchObject({ id: "ask" });
    expect(matchShortcut(key("j", { ctrlKey: true }), null, false)).toMatchObject({ id: "ask" });
    expect(matchShortcut(key("j", { ctrlKey: true }), null, true)).toBeNull();
    expect(matchShortcut(key("j", { metaKey: true, shiftKey: true }), null, true)).toBeNull();
  });

  it("match sequences: g then a letter", () => {
    expect(matchShortcut(key("g"), null)).toBe("pending");
    expect(matchShortcut(key("t"), "g")).toMatchObject({ to: "tasks" });
    expect(matchShortcut(key("z"), "g")).toBeNull();
    expect(matchShortcut(key("t"), null)).toBeNull();
  });

  it("read the physical key for Option chords on a Mac", () => {
    expect(matchShortcut(key("˜", { altKey: true, code: "KeyN" }), null, true)).toMatchObject({ id: "task" });
  });

  it("match single keys like / and ?", () => {
    expect(matchShortcut(key("/"), null)).toMatchObject({ id: "search" });
    expect(matchShortcut(key("?", { shiftKey: true }), null)).toMatchObject({ id: "help" });
  });

  it("search by label, group, or key", () => {
    expect(searchShortcuts("calendar").map((s) => s.id)).toEqual(["go-calendar"]);
    expect(searchShortcuts("capture").length).toBeGreaterThan(0);
    expect(searchShortcuts("").length).toBe(SHORTCUTS.length);
    expect(searchShortcuts("zzz")).toEqual([]);
  });

  it("stand aside while typing", () => {
    const el = (tagName: string, extra: Record<string, unknown> = {}) => ({ tagName, isContentEditable: false, ...extra }) as unknown as EventTarget;
    expect(isTyping(el("INPUT", { type: "text" }))).toBe(true);
    expect(isTyping(el("INPUT", { type: "checkbox" }))).toBe(false);
    expect(isTyping(el("TEXTAREA"))).toBe(true);
    expect(isTyping(el("DIV", { isContentEditable: true }))).toBe(true);
    expect(isTyping(el("BUTTON"))).toBe(false);
    expect(isTyping(null)).toBe(false);
  });
});

describe("device settings", () => {
  it("fall back to defaults field by field", () => {
    expect(parseDeviceSettings(null)).toEqual(DEVICE_DEFAULTS);
    expect(parseDeviceSettings("not json")).toEqual(DEVICE_DEFAULTS);
    expect(parseDeviceSettings("[1]")).toEqual(DEVICE_DEFAULTS);
    const v = parseDeviceSettings(JSON.stringify({ density: "compact", volume: 3, crackle: "yes", crackleLevel: 0.5, uiSounds: true }));
    expect(v).toMatchObject({ density: "compact", volume: 1, crackle: false, crackleLevel: 0.5, uiSounds: true });
  });

  it("keep sounds off unless switched on", () => {
    expect(DEVICE_DEFAULTS.uiSounds).toBe(false);
    expect(DEVICE_DEFAULTS.crackle).toBe(false);
  });
});
