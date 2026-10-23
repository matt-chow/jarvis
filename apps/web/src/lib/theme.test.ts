import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

// WCAG 2.2 AA: text needs 4.5:1 against what it sits on. Every text colour
// token is checked on the surfaces it's used on, in both themes.
const css = readFileSync(new URL("../index.css", import.meta.url), "utf8");

function tokens(block: string): Record<string, string> {
  return Object.fromEntries([...block.matchAll(/--([a-z-]+):\s*(#[0-9a-fA-F]{6})/g)].map((m) => [m[1], m[2]]));
}

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255).map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

// The light theme is the first block with hex values; the dark theme is the media-query block.
const light = tokens(css.slice(css.indexOf("--panel: #") - 200, css.indexOf("prefers-color-scheme")));
const dark = tokens(css.slice(css.indexOf("prefers-color-scheme")));

describe.each([
  ["light", light],
  ["dark", dark],
])("%s theme contrast", (_name, t) => {
  it.each(["ink", "muted", "accent", "ok", "warn", "bad"])("%s text is readable on panels", (fg) => {
    expect(t[fg], `--${fg}`).toBeDefined();
    expect(contrast(t[fg], t.panel)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(t[fg], t.sunken)).toBeGreaterThanOrEqual(4.5);
  });

  it("button text on the accent is readable", () => {
    expect(contrast(t["accent-ink"], t.accent)).toBeGreaterThanOrEqual(4.5);
  });
});
