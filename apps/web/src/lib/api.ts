/**
 * Typed client for the Directory Worker API (`/api/v1`).
 * The browser authenticates with the HttpOnly session cookie (same origin);
 * native clients can pass a device token instead.
 */
import type { JournalEntry, JournalSummary, PutJournal } from "./journal";
import type { Note, NotePatch, NoteSummary } from "./notes";
import type { NewProject, Project, ProjectPatch } from "./projects";
import type { NewTask, Task, TaskFilter, TaskPatch } from "./tasks";
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

export interface ObjectInfo {
  key: string;
  size: number;
  content_type: string | null;
  uploaded_at: string | null;
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

/** Encode each path segment so keys like `a/b c.txt` survive the URL. */
export function objectPath(key: string): string {
  return `/api/v1/storage/objects/${key.split("/").map(encodeURIComponent).join("/")}`;
}

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
  createNote = (n: { title?: string; body?: string; pinned?: boolean }) => this.call<Note>("POST", "/api/v1/notes", n);
  updateNote = (id: string, patch: NotePatch) => this.call<Note>("PATCH", `/api/v1/notes/${encodeURIComponent(id)}`, patch);
  deleteNote = (id: string) => this.call<void>("DELETE", `/api/v1/notes/${encodeURIComponent(id)}`);

  // ---- Projects ----
  projects = async (includeArchived = false) =>
    (await this.call<{ projects: Project[] }>("GET", `/api/v1/projects${includeArchived ? "?include=archived" : ""}`)).projects;
  createProject = (p: NewProject) => this.call<Project>("POST", "/api/v1/projects", p);
  updateProject = (id: string, patch: ProjectPatch) =>
    this.call<Project>("PATCH", `/api/v1/projects/${encodeURIComponent(id)}`, patch);
  deleteProject = (id: string) => this.call<void>("DELETE", `/api/v1/projects/${encodeURIComponent(id)}`);

  // ---- Tasks ----
  tasks = async (filter: TaskFilter = "all") =>
    (await this.call<{ tasks: Task[] }>("GET", `/api/v1/tasks?status=${filter}`)).tasks;
  createTask = (t: NewTask) => this.call<Task>("POST", "/api/v1/tasks", t);
  updateTask = (id: string, patch: TaskPatch) => this.call<Task>("PATCH", `/api/v1/tasks/${encodeURIComponent(id)}`, patch);
  deleteTask = (id: string) => this.call<void>("DELETE", `/api/v1/tasks/${encodeURIComponent(id)}`);

  // ---- Files ----
  async listObjects(prefix?: string): Promise<ObjectInfo[]> {
    const q = prefix ? `?prefix=${encodeURIComponent(prefix)}` : "";
    return (await this.call<{ objects: ObjectInfo[] }>("GET", `/api/v1/storage/objects${q}`)).objects;
  }

  async upload(key: string, body: Blob): Promise<ObjectInfo> {
    const res = await this.send("PUT", objectPath(key), {
      body,
      headers: { "Content-Type": body.type || "application/octet-stream" },
    });
    if (!res.ok) return fail(res);
    return (await res.json()) as ObjectInfo;
  }

  async download(key: string): Promise<Blob> {
    const res = await this.send("GET", objectPath(key));
    if (!res.ok) return fail(res);
    return res.blob();
  }

  removeObject = (key: string) => this.call<void>("DELETE", objectPath(key));
}

/** Shared client for the web app. */
export const api = new Api();

/** Turn a file name into a valid storage key (see core `ObjectKey`). */
export function toObjectKey(fileName: string, folder = "uploads"): string {
  const safe = fileName
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "") // drop accents: é → e
    .replace(/[^A-Za-z0-9._-]+/g, "-")
    .replace(/^[-.]+|-+$/g, "")
    .slice(0, 200);
  return `${folder}/${safe || "file"}`;
}

/** True when the request never reached the server (offline, DNS, CORS…). */
export function isNetworkError(e: unknown): boolean {
  return e instanceof TypeError;
}
