/**
 * Module registry. Each Directory capability is a module with a stable id;
 * the shell renders navigation from this list. Future clients (Windows, iOS)
 * should share these ids.
 */

export type ModuleId = "home" | "ask" | "search" | "inbox" | "tasks" | "projects" | "calendar" | "habits" | "notes" | "journal" | "library" | "music" | "files" | "jarvis" | "settings" | "modules";

export interface ModuleMeta {
  id: ModuleId;
  label: string;
  /** Monospace glyph used as the nav icon. */
  glyph: string;
  summary: string;
  stage: "ready" | "preview" | "planned";
  /** Works without a network connection. */
  offline: boolean;
  /** Sidebar section (Paper: main, Knowledge, Media; System at the bottom). */
  group: ModuleGroup;
}

export type ModuleGroup = "main" | "knowledge" | "media" | "system";

export const MODULES: readonly ModuleMeta[] = [
  { id: "home", label: "Home", glyph: "~", summary: "System status and overview.", stage: "ready", offline: true, group: "main" },
  { id: "ask", label: "Ask", glyph: "◎", summary: "Talk or type to JARVIS. Changes wait for your confirm.", stage: "preview", offline: false, group: "main" },
  { id: "search", label: "Search", glyph: "⌕", summary: "Find anything across tasks, notes, projects, and journal.", stage: "preview", offline: false, group: "main" },
  { id: "inbox", label: "Inbox", glyph: "↓", summary: "Capture first, sort later: triage into tasks, notes, or the journal.", stage: "preview", offline: true, group: "main" },
  { id: "tasks", label: "Tasks", glyph: "☐", summary: "Tasks and subtasks with due dates and priorities.", stage: "preview", offline: true, group: "main" },
  { id: "projects", label: "Projects", glyph: "▦", summary: "Group tasks and track progress.", stage: "preview", offline: true, group: "main" },
  { id: "calendar", label: "Calendar", glyph: "▣", summary: "Events by month and day, with tasks that are due.", stage: "preview", offline: true, group: "main" },
  { id: "habits", label: "Habits", glyph: "✓", summary: "Daily routines on chosen days, with streaks.", stage: "preview", offline: true, group: "main" },
  { id: "notes", label: "Notes", glyph: "¶", summary: "Notes and wiki pages, with autosave and backlinks.", stage: "preview", offline: true, group: "knowledge" },
  { id: "journal", label: "Journal", glyph: "≡", summary: "One entry per day, with mood.", stage: "preview", offline: true, group: "main" },
  { id: "library", label: "Library", glyph: "◫", summary: "PDFs and ePubs with reading progress.", stage: "preview", offline: false, group: "knowledge" },
  { id: "music", label: "Music", glyph: "♫", summary: "Your audio files, playlists, and offline listening.", stage: "preview", offline: true, group: "media" },
  { id: "files", label: "Files", glyph: "▤", summary: "Private cloud storage.", stage: "preview", offline: false, group: "knowledge" },
  { id: "jarvis", label: "JARVIS", glyph: "◉", summary: "Command console: preview and approval, presets, automations, history.", stage: "preview", offline: false, group: "system" },
  { id: "modules", label: "Modules", glyph: "⊞", summary: "Every module in one place.", stage: "ready", offline: true, group: "system" },
  { id: "settings", label: "Settings", glyph: "§", summary: "Passkeys, device tokens, and sign-out.", stage: "ready", offline: false, group: "system" },
];

export const DEFAULT_MODULE: ModuleId = "home";

/** Phone tab bar (Paper Mobile): Ask sits in the middle as the JARVIS button. */
export const TABS: readonly ModuleId[] = ["home", "tasks", "ask", "projects", "modules"];

/** Sidebar groups, in order. Settings lives in the account footer; Modules is the phone launcher. */
export const SIDEBAR_GROUPS: readonly { group: ModuleGroup; label?: string }[] = [
  { group: "main" },
  { group: "knowledge", label: "Knowledge" },
  { group: "media", label: "Media" },
  { group: "system", label: "System" },
];

export const sidebarModules = (group: ModuleGroup) => MODULES.filter((m) => m.group === group && m.id !== "settings" && m.id !== "modules");

/** Resolve a location hash like `#/notes` to a module id. */
export function moduleFromHash(hash: string): ModuleId {
  const id = hash.replace(/^#\/?/, "").split(/[/?]/)[0];
  return MODULES.find((m) => m.id === id)?.id ?? DEFAULT_MODULE;
}

/** Read a query parameter from a hash route like `#/tasks?project=abc`. */
export function hashParam(hash: string, name: string): string | undefined {
  const q = hash.split("?")[1];
  return q ? (new URLSearchParams(q).get(name) ?? undefined) : undefined;
}
