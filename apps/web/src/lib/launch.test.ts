import { describe, expect, it } from "vitest";
import { defaultChoice, launchHash, parseChoice, type LaunchEnv } from "./launch";

const env = (over: Partial<LaunchEnv> = {}): LaunchEnv => ({
  hash: "",
  standalone: true,
  ios: true,
  navigationType: "navigate",
  handled: false,
  saved: null,
  ...over,
});

describe("Home Screen launch route", () => {
  it("opens Ask by default in the iPhone Home Screen app", () => {
    for (const hash of ["", "#", "#/", "#/home"]) expect(launchHash(env({ hash }))).toBe("#/ask");
  });

  it("never redirects a browser tab", () => {
    expect(launchHash(env({ standalone: false, ios: false }))).toBeUndefined();
    expect(launchHash(env({ standalone: false, ios: false, saved: "ask" }))).toBeUndefined();
  });

  it("keeps a route the user chose", () => {
    for (const hash of ["#/tasks", "#/settings", "#/notes?note=abc", "#/ask"]) expect(launchHash(env({ hash }))).toBeUndefined();
  });

  it("decides once per app session, and not on reload or back/forward", () => {
    expect(launchHash(env({ handled: true }))).toBeUndefined();
    expect(launchHash(env({ navigationType: "reload" }))).toBeUndefined();
    expect(launchHash(env({ navigationType: "back_forward" }))).toBeUndefined();
    expect(launchHash(env({ navigationType: undefined }))).toBe("#/ask");
  });

  it("follows the device's choice", () => {
    expect(launchHash(env({ saved: "home" }))).toBeUndefined();
    expect(launchHash(env({ saved: "ask", ios: false }))).toBe("#/ask");
    expect(launchHash(env({ saved: "bogus" }))).toBe("#/ask");
  });

  it("defaults to Ask on iOS and Home for other installed apps", () => {
    expect(defaultChoice(true)).toBe("ask");
    expect(defaultChoice(false)).toBe("home");
    expect(launchHash(env({ ios: false }))).toBeUndefined();
    expect(parseChoice("ask")).toBe("ask");
    expect(parseChoice(null)).toBeUndefined();
  });
});
