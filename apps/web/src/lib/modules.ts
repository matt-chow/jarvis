/**
 * Module registry. Each Directory capability is a module with a stable id;
 * the shell renders navigation from this list. Future clients (Windows, iOS)
 * should share these ids.
 */

import type { IconName } from "../components/icons";

export type ModuleId = "home" | "ask" | "search" | "inbox" | "tasks" | "reminders" | "projects" | "calendar" | "habits" | "notes" | "journal" | "library" | "budget" | "music" | "files" | "jarvis" | "settings" | "modules";

export interface ModuleMeta {
  id: ModuleId;
  label: string;
  /** Nav icon from the authored set (components/icons.tsx, D-050). */
  icon: IconName;
  summary: string;
  stage: "ready" | "preview" | "planned";
  /** Works without a network connection. */
  offline: boolean;
  /** Sidebar section; System sits at the bottom. */
  group: ModuleGroup;
}

export type ModuleGroup = "main" | "plan" | "knowledge" | "life" | "system";

export const MODULES: readonly ModuleMeta[] = [
  { id: "home", label: "Home", icon: "home", summary: "Today at a glance: schedule, tasks, reminders, habits.", stage: "ready", offline: true, group: "main" },
  { id: "ask", label: "Ask", icon: "ask", summary: "Talk or type to JARVIS. Changes wait for your confirm.", stage: "preview", offline: false, group: "main" },
  { id: "search", label: "Search", icon: "search", summary: "Find anything across tasks, notes, projects, and journal.", stage: "preview", offline: false, group: "main" },
  { id: "inbox", label: "Inbox", icon: "inbox", summary: "Capture first, sort later: triage into tasks, notes, or the journal.", stage: "preview", offline: true, group: "main" },
  { id: "tasks", label: "Tasks", icon: "tasks", summary: "Tasks and subtasks with due dates and priorities.", stage: "preview", offline: true, group: "plan" },
  { id: "calendar", label: "Calendar", icon: "calendar", summary: "Month, week, and day, with scheduled work and due dates.", stage: "preview", offline: true, group: "plan" },
  { id: "reminders", label: "Reminders", icon: "reminders", summary: "Reminders with alerts, repeats, lists, and snooze.", stage: "preview", offline: true, group: "plan" },
  { id: "projects", label: "Projects", icon: "projects", summary: "Group tasks and track progress.", stage: "preview", offline: true, group: "plan" },
  { id: "habits", label: "Habits", icon: "habits", summary: "Routines on chosen days, with streaks and a year grid.", stage: "preview", offline: true, group: "plan" },
  { id: "notes", label: "Notes", icon: "notes", summary: "Notes and wiki pages, with autosave and backlinks.", stage: "preview", offline: true, group: "knowledge" },
  { id: "journal", label: "Journal", icon: "journal", summary: "One entry per day, with mood.", stage: "preview", offline: true, group: "knowledge" },
  { id: "library", label: "Library", icon: "library", summary: "PDFs and ePubs with reading progress.", stage: "preview", offline: false, group: "knowledge" },
  { id: "files", label: "Files", icon: "files", summary: "Private cloud storage.", stage: "preview", offline: false, group: "knowledge" },
  { id: "budget", label: "Budget", icon: "budget", summary: "Accounts, spending, income, and monthly budgets.", stage: "preview", offline: true, group: "life" },
  { id: "music", label: "Music", icon: "music", summary: "A turntable for your own audio files and playlists.", stage: "preview", offline: true, group: "life" },
  { id: "jarvis", label: "JARVIS", icon: "jarvis", summary: "Command console: preview and approval, presets, automations, history.", stage: "preview", offline: false, group: "system" },
  { id: "modules", label: "Modules", icon: "modules", summary: "Every module in one place.", stage: "ready", offline: true, group: "system" },
  { id: "settings", label: "Settings", icon: "settings", summary: "Appearance, sounds, passkeys, device tokens, and sign-out.", stage: "ready", offline: false, group: "system" },
];

export const DEFAULT_MODULE: ModuleId = "home";

/** Phone tab bar: JARVIS capture sits in the middle as the raised button. */
export const TABS: readonly ModuleId[] = ["home", "tasks", "ask", "calendar", "modules"];

/** Sidebar groups, in order. Settings lives in the account footer; Modules is the phone launcher. */
export const SIDEBAR_GROUPS: readonly { group: ModuleGroup; label?: string }[] = [
  { group: "main" },
  { group: "plan", label: "Plan" },
  { group: "knowledge", label: "Write and read" },
  { group: "life", label: "Money and music" },
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
