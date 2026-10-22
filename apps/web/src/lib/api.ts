/**
 * Typed client for the Directory Worker API (`/api/v1`).
 * The browser authenticates with the HttpOnly session cookie (same origin);
 * native clients can pass a device token instead.
 */
import type { NewRule, Rule, RulePatch } from "./automations";
import type { CalendarEvent, EventPatch, NewEvent } from "./calendar";
import type { FileEntry, Listing, Usage } from "./files";
import { HISTORY_DAYS, type Habit, type HabitPatch, type HabitView, type NewHabit } from "./habits";
import type { InboxItem, Triaged, TriageTarget } from "./inbox";
import type { Plan, Preset } from "./jarvis";
import type { JournalEntry, JournalSummary, PutJournal } from "./journal";
import type { LibraryItem, Progress, ProgressPatch } from "./library";
import type { EntityRef, Linked } from "./links";
import type { MusicLibrary, Playlist } from "./music";
import type { Note, NotePatch, NoteSummary } from "./notes";
import type { Preferences, PreferencesPatch } from "./preferences";
import type { NewProject, Project, ProjectPatch } from "./projects";
import type { SearchHit } from "./search";
import { localDate, type NewTask, type Task, type TaskFilter, type TaskPatch } from "./tasks";
import type { AssertionJson, CreationOptionsJson, RegistrationJson, RequestOptionsJson } from "./webauthn";

export interface HealthCheck {
  name: string;
  status: "ok" | "degraded";
  detail?: string;
}

export interface HealthReport {
  status: "ok" | "degraded";
  service: string;
  version: string;
  checks: HealthCheck[];
}

export interface SessionUser {
  id: string;
  displayName: string;
}

export interface SessionState {
  authenticated: boolean;
  setupRequired: boolean;
  user?: SessionUser;
}

export interface PasskeyInfo {
  id: string;
  name: string;
  created_at: number;
  last_used_at: number | null;
}

export interface DeviceToken {
  id: string;
  name: string;
  created_at: number;
  last_used_at: number | null;
}

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    readonly detail?: string,
    /** Parsed JSON error body (e.g. the current note on a 409 conflict). */
    readonly body?: unknown,
  ) {
    super(`${status} ${code}`);
  }
}

type Fetch = typeof fetch;

/** Empty = same origin. Native clients can point this at the deployed Worker. */
const BASE = (import.meta.env?.VITE_API_BASE as string | undefined) ?? "";

async function fail(res: Response): Promise<never> {
  let code = "request_failed";
  let detail: string | undefined;
  let body: unknown;
  try {
    body = await res.json();
    const b = body as { error?: string; message?: string };
    code = b.error ?? code;
    detail = b.message;
  } catch {
    /* non-JSON body */
  }
  throw new ApiError(res.status, code, detail, body);
}

export class Api {
  constructor(
    private readonly fetchImpl: Fetch = (...args) => fetch(...args),
    private readonly base = BASE,
    private readonly bearer?: string,
  ) {}

  private send(method: string, path: string, init: { body?: BodyInit; json?: unknown; headers?: HeadersInit } = {}) {
    const headers = new Headers(init.headers);
    if (this.bearer) headers.set("Authorization", `Bearer ${this.bearer}`);
    let body = init.body;
    if (init.json !== undefined) {
      headers.set("Content-Type", "application/json");
      body = JSON.stringify(init.json);
    }
    return this.fetchImpl(`${this.base}${path}`, { method, headers, body, credentials: "same-origin" });
  }

  private async call<T>(method: string, path: string, json?: unknown): Promise<T> {
    const res = await this.send(method, path, { json });
    if (!res.ok) return fail(res);
    return res.status === 204 ? (undefined as T) : ((await res.json()) as T);
  }

  async health(): Promise<HealthReport> {
    const res = await this.send("GET", "/api/v1/health", { headers: { Accept: "application/json" } });
    // 503 still carries a report describing which check is degraded.
    if (res.ok || res.status === 503) {
      const body = (await res.json()) as Partial<HealthReport>;
      if (body.service === "directory" && Array.isArray(body.checks)) return body as HealthReport;
    }
    return fail(res);
  }

  // ---- Identity ----
  session = () => this.call<SessionState>("GET", "/api/v1/auth/session");
  setupOptions = (setupToken: string, displayName: string) =>
    this.call<CreationOptionsJson>("POST", "/api/v1/auth/setup/options", { setupToken, displayName });
  setupVerify = (reg: RegistrationJson) => this.call<SessionUser>("POST", "/api/v1/auth/setup/verify", reg);
  loginOptions = () => this.call<RequestOptionsJson>("POST", "/api/v1/auth/login/options", {});
  loginVerify = (a: AssertionJson) => this.call<SessionUser>("POST", "/api/v1/auth/login/verify", a);
  logout = () => this.call<void>("POST", "/api/v1/auth/logout");
  passkeys = async () => (await this.call<{ passkeys: PasskeyInfo[] }>("GET", "/api/v1/auth/passkeys")).passkeys;
  addPasskeyOptions = () => this.call<CreationOptionsJson>("POST", "/api/v1/auth/passkeys/options", {});
  addPasskeyVerify = (reg: RegistrationJson) => this.call<PasskeyInfo>("POST", "/api/v1/auth/passkeys/verify", reg);
  deletePasskey = (id: string) => this.call<void>("DELETE", `/api/v1/auth/passkeys/${encodeURIComponent(id)}`);
  tokens = async () => (await this.call<{ tokens: DeviceToken[] }>("GET", "/api/v1/auth/tokens")).tokens;
  createToken = (name: string) => this.call<{ token: DeviceToken; secret: string }>("POST", "/api/v1/auth/tokens", { name });
  deleteToken = (id: string) => this.call<void>("DELETE", `/api/v1/auth/tokens/${encodeURIComponent(id)}`);

  // ---- Links ----
  links = async (e: EntityRef) =>
    (await this.call<{ links: Linked[] }>("GET", `/api/v1/links?kind=${e.kind}&id=${encodeURIComponent(e.id)}`)).links;
  link = (from: EntityRef, to: EntityRef) => this.call<Linked>("POST", "/api/v1/links", { from, to });
  unlink = (linkId: string) => this.call<void>("DELETE", `/api/v1/links/${encodeURIComponent(linkId)}`);

  // ---- Inbox ----
  inbox = async () => (await this.call<{ items: InboxItem[] }>("GET", "/api/v1/inbox")).items;
  /** `id`: client-chosen id for offline captures (replays answer 409). */
  capture = (text: string, id?: string) => this.call<InboxItem>("POST", "/api/v1/inbox", { text, id });
  deleteInboxItem = (id: string) => this.call<void>("DELETE", `/api/v1/inbox/${encodeURIComponent(id)}`);
  /** `date` (local YYYY-MM-DD) is required when triaging to the journal. */
  triage = (id: string, to: TriageTarget, date?: string) =>
    this.call<Triaged>("POST", `/api/v1/inbox/${encodeURIComponent(id)}/triage`, { to, date });

  // ---- Journal ----
  journal = async (from: string, to: string) =>
    (await this.call<{ entries: JournalSummary[] }>("GET", `/api/v1/journal?from=${from}&to=${to}`)).entries;
  /** `null` when there is no entry for that date. */
  journalEntry = async (date: string): Promise<JournalEntry | null> => {
    try {
      return await this.call<JournalEntry>("GET", `/api/v1/journal/${date}`);
    } catch (e) {
      if (e instanceof ApiError && e.status === 404) return null;
      throw e;
    }
  };
  putJournal = (date: string, entry: PutJournal) => this.call<JournalEntry>("PUT", `/api/v1/journal/${date}`, entry);
  deleteJournal = (date: string) => this.call<void>("DELETE", `/api/v1/journal/${date}`);

  // ---- Notes ----
  notes = async () => (await this.call<{ notes: NoteSummary[] }>("GET", "/api/v1/notes")).notes;
  note = (id: string) => this.call<Note>("GET", `/api/v1/notes/${encodeURIComponent(id)}`);
  /** Notes that wiki-link to this one. */
  backlinks = async (id: string) =>
    (await this.call<{ notes: NoteSummary[] }>("GET", `/api/v1/notes/${encodeURIComponent(id)}/backlinks`)).notes;
  createNote = (n: { id?: string; title?: string; body?: string; pinned?: boolean }) => this.call<Note>("POST", "/api/v1/notes", n);
  updateNote = (id: string, patch: NotePatch) => this.call<Note>("PATCH", `/api/v1/notes/${encodeURIComponent(id)}`, patch);
  deleteNote = (id: string) => this.call<void>("DELETE", `/api/v1/notes/${encodeURIComponent(id)}`);

  // ---- Music ----
  music = () => this.call<MusicLibrary>("GET", "/api/v1/music");
  createPlaylist = (name: string, tracks: string[] = []) => this.call<Playlist>("POST", "/api/v1/music/playlists", { name, tracks });
  updatePlaylist = (id: string, patch: { name?: string; tracks?: string[] }) =>
    this.call<Playlist>("PATCH", `/api/v1/music/playlists/${encodeURIComponent(id)}`, patch);
  deletePlaylist = (id: string) => this.call<void>("DELETE", `/api/v1/music/playlists/${encodeURIComponent(id)}`);

  // ---- Automations ----
  automations = async () => (await this.call<{ automations: Rule[] }>("GET", "/api/v1/automations")).automations;
  createAutomation = (r: NewRule) => this.call<Rule>("POST", "/api/v1/automations", r);
  updateAutomation = (id: string, patch: RulePatch) => this.call<Rule>("PATCH", `/api/v1/automations/${encodeURIComponent(id)}`, patch);
  deleteAutomation = (id: string) => this.call<void>("DELETE", `/api/v1/automations/${encodeURIComponent(id)}`);

  // ---- Preferences ----
  preferences = () => this.call<Preferences>("GET", "/api/v1/preferences");
  updatePreferences = (patch: PreferencesPatch) => this.call<Preferences>("PATCH", "/api/v1/preferences", patch);

  // ---- Calendar ----
  /** Every event, repeats not expanded (see lib/calendar `occurrences`). */
  events = async () => (await this.call<{ events: CalendarEvent[] }>("GET", "/api/v1/events")).events;
  createEvent = (e: NewEvent & { id?: string }) => this.call<CalendarEvent>("POST", "/api/v1/events", e);
  updateEvent = (id: string, patch: EventPatch) => this.call<CalendarEvent>("PATCH", `/api/v1/events/${encodeURIComponent(id)}`, patch);
  deleteEvent = (id: string) => this.call<void>("DELETE", `/api/v1/events/${encodeURIComponent(id)}`);

  // ---- Habits ----
  habits = async (today: string, days = HISTORY_DAYS) =>
    (await this.call<{ habits: HabitView[] }>("GET", `/api/v1/habits?today=${today}&days=${days}`)).habits;
  createHabit = (h: NewHabit & { id?: string }) => this.call<Habit>("POST", "/api/v1/habits", h);
  updateHabit = (id: string, patch: HabitPatch) => this.call<Habit>("PATCH", `/api/v1/habits/${encodeURIComponent(id)}`, patch);
  deleteHabit = (id: string) => this.call<void>("DELETE", `/api/v1/habits/${encodeURIComponent(id)}`);
  /** Idempotent: mark `date` done or not. */
  checkHabit = (id: string, date: string, done: boolean) =>
    this.call<void>(done ? "PUT" : "DELETE", `/api/v1/habits/${encodeURIComponent(id)}/checkins/${date}`);

  // ---- Projects ----
  projects = async (includeArchived = false) =>
    (await this.call<{ projects: Project[] }>("GET", `/api/v1/projects${includeArchived ? "?include=archived" : ""}`)).projects;
  createProject = (p: NewProject & { id?: string }) => this.call<Project>("POST", "/api/v1/projects", p);
  updateProject = (id: string, patch: ProjectPatch) =>
    this.call<Project>("PATCH", `/api/v1/projects/${encodeURIComponent(id)}`, patch);
  deleteProject = (id: string) => this.call<void>("DELETE", `/api/v1/projects/${encodeURIComponent(id)}`);

  // ---- Tasks ----
  tasks = async (filter: TaskFilter = "all") =>
    (await this.call<{ tasks: Task[] }>("GET", `/api/v1/tasks?status=${filter}`)).tasks;
  createTask = (t: NewTask) => this.call<Task>("POST", "/api/v1/tasks", t);
  /** `today` (local) dates any automations a completion fires. */
  updateTask = (id: string, patch: TaskPatch) =>
    this.call<Task>("PATCH", `/api/v1/tasks/${encodeURIComponent(id)}?today=${localDate()}`, patch);
  deleteTask = (id: string) => this.call<void>("DELETE", `/api/v1/tasks/${encodeURIComponent(id)}`);

  // ---- JARVIS ----
  /** Preview a command. Read-only plans come back already executed. */
  jarvisPlan = (input: string, today: string) => this.call<Plan>("POST", "/api/v1/jarvis/plan", { input, today });
  /** `today` (local) dates any automations the plan fires. */
  jarvisExecute = (id: string) => this.call<Plan>("POST", `/api/v1/jarvis/plans/${encodeURIComponent(id)}/execute?today=${localDate()}`);
  jarvisCancel = (id: string) => this.call<Plan>("POST", `/api/v1/jarvis/plans/${encodeURIComponent(id)}/cancel`);
  jarvisHistory = async () => (await this.call<{ plans: Plan[] }>("GET", "/api/v1/jarvis/history")).plans;
  presets = async () => (await this.call<{ presets: Preset[] }>("GET", "/api/v1/jarvis/presets")).presets;
  createPreset = (name: string, input: string) => this.call<Preset>("POST", "/api/v1/jarvis/presets", { name, input });
  deletePreset = (id: string) => this.call<void>("DELETE", `/api/v1/jarvis/presets/${encodeURIComponent(id)}`);

  // ---- Search ----
  search = async (q: string) =>
    (await this.call<{ hits: SearchHit[] }>("GET", `/api/v1/search?q=${encodeURIComponent(q)}`)).hits;

  // ---- Export ----
  async exportData(): Promise<Blob> {
    const res = await this.send("GET", "/api/v1/export");
    if (!res.ok) return fail(res);
    return res.blob();
  }

  // ---- Library ----
  library = async () => (await this.call<{ items: LibraryItem[] }>("GET", "/api/v1/library")).items;
  setProgress = (fileId: string, patch: ProgressPatch) =>
    this.call<Progress>("PUT", `/api/v1/library/${encodeURIComponent(fileId)}/progress`, patch);
  clearProgress = (fileId: string) => this.call<void>("DELETE", `/api/v1/library/${encodeURIComponent(fileId)}/progress`);

  // ---- Files ----
  files = (folder = "") => this.call<Listing>("GET", `/api/v1/files?folder=${encodeURIComponent(folder)}`);
  fileUsage = () => this.call<Usage>("GET", "/api/v1/files/usage");
  /** Streams the file as the request body (the browser sets Content-Length). */
  async uploadFile(folder: string, file: File): Promise<FileEntry> {
    const q = `folder=${encodeURIComponent(folder)}&name=${encodeURIComponent(file.name)}`;
    const res = await this.send("POST", `/api/v1/files?${q}`, {
      body: file,
      headers: { "Content-Type": file.type || "application/octet-stream" },
    });
    if (!res.ok) return fail(res);
    return (await res.json()) as FileEntry;
  }
  updateFile = (id: string, patch: { name?: string; folder?: string }) =>
    this.call<FileEntry>("PATCH", `/api/v1/files/${encodeURIComponent(id)}`, patch);
  deleteFile = (id: string) => this.call<void>("DELETE", `/api/v1/files/${encodeURIComponent(id)}`);
  /** Same-origin URL; the session cookie authorizes the download. */
  fileContentUrl = (id: string) => `${this.base}/api/v1/files/${encodeURIComponent(id)}/content`;
}

/** Shared client for the web app. */
export const api = new Api();

/** True when the request never reached the server (offline, DNS, CORS…). */
export function isNetworkError(e: unknown): boolean {
  return e instanceof TypeError;
}
