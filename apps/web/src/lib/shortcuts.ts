/**
 * Keyboard shortcuts (D-050): one list drives the handler and the help panel,
 * so what the panel shows is what works. Shortcuts never fire while typing in
 * a field, and none replaces a browser default (no Ctrl+N, Ctrl+T, Ctrl+W).
 */

import type { ModuleId } from "./modules";

export const isMac = () => typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform);

export interface Shortcut {
  id: string;
  /** Keys as typed, e.g. ["g", "t"] for a sequence or ["Mod", "j"] for a chord. */
  keys: readonly string[];
  /** A sequence (press one key, then the next) rather than a chord. */
  sequence?: boolean;
  label: string;
  group: "Navigate" | "Capture" | "Panels" | "Help";
  /** Where it goes, for navigation shortcuts. */
  to?: ModuleId;
}

const go = (key: string, to: ModuleId, label: string): Shortcut => ({ id: `go-${to}`, keys: ["g", key], sequence: true, label, group: "Navigate", to });

export const SHORTCUTS: readonly Shortcut[] = [
  { id: "search", keys: ["/"], label: "Search everything", group: "Navigate", to: "search" },
  go("h", "home", "Go to Home"),
  go("t", "tasks", "Go to Tasks"),
  go("c", "calendar", "Go to Calendar"),
  go("r", "reminders", "Go to Reminders"),
  go("p", "projects", "Go to Projects"),
  go("b", "habits", "Go to Habits"),
  go("n", "notes", "Go to Notes"),
  go("j", "journal", "Go to Journal"),
  go("u", "music", "Go to Music"),
  go("s", "settings", "Go to Settings"),
  { id: "task", keys: ["Alt", "n"], label: "Quick add a task", group: "Capture" },
  { id: "ask", keys: ["Mod", "j"], label: "Open Ask JARVIS", group: "Capture", to: "ask" },
  { id: "panel", keys: ["Mod", "."], label: "Show or hide the JARVIS side panel", group: "Panels" },
  { id: "help", keys: ["?"], label: "Show keyboard shortcuts", group: "Help" },
];

/** Platform key names: ⌘ ⌥ on Apple keyboards, Ctrl Alt elsewhere. */
export function keyLabel(key: string, mac = isMac()): string {
  if (key === "Mod") return mac ? "⌘" : "Ctrl";
  if (key === "Alt") return mac ? "⌥" : "Alt";
  if (key === "Shift") return mac ? "⇧" : "Shift";
  return key.length === 1 ? key.toUpperCase() : key;
}

/** `aria-keyshortcuts` value for a chord (sequences have none). */
export function ariaKeys(s: Shortcut, mac = isMac()): string | undefined {
  if (s.sequence) return undefined;
  return s.keys.map((k) => (k === "Mod" ? (mac ? "Meta" : "Control") : k.length === 1 ? k.toUpperCase() : k)).join("+");
}

export const shortcutById = (id: string) => SHORTCUTS.find((s) => s.id === id)!;

/** Typing in a field (or a content-editable): shortcuts stay out of the way. */
export function isTyping(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el || typeof el.tagName !== "string") return false;
  if (el.isContentEditable) return true;
  if (el.tagName === "TEXTAREA" || el.tagName === "SELECT") return true;
  if (el.tagName === "INPUT") {
    const type = (el as HTMLInputElement).type;
    return !["checkbox", "radio", "button", "submit", "range", "color", "file", "reset"].includes(type);
  }
  return false;
}

/** Filter the list for the help panel's search box. */
export function searchShortcuts(query: string, list: readonly Shortcut[] = SHORTCUTS, mac = isMac()): Shortcut[] {
  const q = query.trim().toLowerCase();
  if (!q) return [...list];
  return list.filter(
    (s) =>
      s.label.toLowerCase().includes(q) ||
      s.group.toLowerCase().includes(q) ||
      s.keys
        .map((k) => keyLabel(k, mac).toLowerCase())
        .join(" ")
        .includes(q),
  );
}

export interface KeyLike {
  key: string;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  code?: string;
}

/**
 * Match a key event to a shortcut. `pending` is the first key of a sequence
 * pressed just before ("g"), or null. Returns the shortcut, "pending" when the
 * key starts a sequence, or null.
 */
export function matchShortcut(e: KeyLike, pending: string | null, mac = isMac()): Shortcut | "pending" | null {
  const mod = mac ? e.metaKey : e.ctrlKey;
  const otherMod = mac ? e.ctrlKey : e.metaKey;
  if (otherMod) return null;
  const key = e.key.toLowerCase();
  if (pending) {
    if (mod || e.altKey) return null;
    return SHORTCUTS.find((s) => s.sequence && s.keys[0] === pending && s.keys[1] === key) ?? null;
  }
  if (mod && !e.altKey && !e.shiftKey) {
    return SHORTCUTS.find((s) => s.keys.length === 2 && s.keys[0] === "Mod" && s.keys[1] === key) ?? null;
  }
  if (e.altKey && !mod) {
    // Option changes e.key on a Mac keyboard; the physical key is in e.code.
    const physical = e.code?.startsWith("Key") ? e.code.slice(3).toLowerCase() : key;
    return SHORTCUTS.find((s) => s.keys[0] === "Alt" && s.keys[1] === physical) ?? null;
  }
  if (mod) return null;
  if (key === "g" && !e.shiftKey) return "pending";
  return SHORTCUTS.find((s) => !s.sequence && s.keys.length === 1 && s.keys[0] === e.key) ?? null;
}
