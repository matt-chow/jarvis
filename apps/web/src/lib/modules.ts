/**
 * Module registry. Each Directory capability is a module with a stable id;
 * the shell renders navigation from this list. Future clients (Windows, iOS)
 * should share these ids.
 */

export type ModuleId = "home" | "notes" | "files" | "jarvis" | "settings";

export interface ModuleMeta {
  id: ModuleId;
  label: string;
  /** Monospace glyph used as the nav icon. */
  glyph: string;
  summary: string;
  stage: "ready" | "preview" | "planned";
  /** Works without a network connection. */
  offline: boolean;
}

export const MODULES: readonly ModuleMeta[] = [
  { id: "home", label: "Home", glyph: "~", summary: "System status and overview.", stage: "ready", offline: true },
  { id: "notes", label: "Scratchpad", glyph: "¶", summary: "Quick notes stored on this device.", stage: "preview", offline: true },
  { id: "files", label: "Files", glyph: "▤", summary: "Private cloud storage.", stage: "preview", offline: false },
  { id: "jarvis", label: "JARVIS", glyph: "◉", summary: "Voice and AI assistant.", stage: "planned", offline: false },
  { id: "settings", label: "Settings", glyph: "§", summary: "Passkeys, device tokens, and sign-out.", stage: "ready", offline: false },
];

export const DEFAULT_MODULE: ModuleId = "home";

/** Resolve a location hash like `#/notes` to a module id. */
export function moduleFromHash(hash: string): ModuleId {
  const id = hash.replace(/^#\/?/, "").split(/[/?]/)[0];
  return MODULES.find((m) => m.id === id)?.id ?? DEFAULT_MODULE;
}
