/**
 * Light / Night theme (Paper artboards come in both). Per device, in
 * localStorage; "system" follows `prefers-color-scheme`. Applied before the
 * first render so there's no flash (see `main.tsx`).
 */

export type Theme = "system" | "light" | "night";
export const THEMES: readonly Theme[] = ["system", "light", "night"];
export const THEME_KEY = "theme";

/** Browser chrome colours (status bar, PWA title bar): Paper's paper and night ground. */
const CHROME: Record<Exclude<Theme, "system">, string> = { light: "#f5f3ee", night: "#12131f" };

export function parseTheme(value: string | null): Theme {
  return THEMES.find((t) => t === value) ?? "system";
}

export function readTheme(): Theme {
  try {
    return parseTheme(localStorage.getItem(THEME_KEY));
  } catch {
    return "system";
  }
}

export function applyTheme(theme: Theme = readTheme()) {
  const root = document.documentElement;
  if (theme === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", theme);
  // A forced theme overrides both media-scoped theme-color metas.
  for (const meta of document.querySelectorAll<HTMLMetaElement>('meta[name="theme-color"]')) {
    const scheme = meta.media.includes("dark") ? "night" : "light";
    meta.content = CHROME[theme === "system" ? scheme : theme];
  }
}

export function writeTheme(theme: Theme) {
  try {
    localStorage.setItem(THEME_KEY, theme);
  } catch {
    /* private mode: applies for this visit */
  }
  applyTheme(theme);
}
