/**
 * Offline outbox replay (DECISIONS D-017). Changes made offline are queued in
 * IndexedDB and replayed in order when the server is reachable again.
 *
 * Every operation is safe to replay: creates carry a client-generated id
 * (a repeat answers 409 `exists`), updates send only changed fields, and
 * deletes treat 404 as already done. Document saves (notes, journal) carry the
 * version they were based on; if the server copy changed meanwhile, the save
 * becomes a conflict for the owner to resolve (DECISIONS D-018).
 */
import { ApiError, isNetworkError } from "./api";
import type { OutboxEntry } from "./db";
import type { InboxItem } from "./inbox";
import type { JournalEntry, PutJournal } from "./journal";
import type { Note, NotePatch } from "./notes";
import type { NewProject, Project, ProjectPatch } from "./projects";
import type { NewTask, Task, TaskPatch } from "./tasks";

export type Op =
  | { kind: "task.create"; task: NewTask & { id: string } }
  | { kind: "task.update"; id: string; patch: TaskPatch }
  | { kind: "task.delete"; id: string }
  | { kind: "inbox.capture"; item: { id: string; text: string } }
  | { kind: "inbox.delete"; id: string }
  | { kind: "project.create"; project: NewProject & { id: string } }
  | { kind: "project.update"; id: string; patch: ProjectPatch }
  | { kind: "project.delete"; id: string }
  | { kind: "note.create"; note: { id: string; title?: string; body?: string } }
  /** `base`: the server version edited; undefined for a note created offline. */
  | { kind: "note.save"; id: string; fields: NoteFields; base?: number }
  | { kind: "note.delete"; id: string }
  /** `base` 0 = the day had no entry (create only). */
  | { kind: "journal.save"; date: string; fields: JournalFields; base?: number };

export interface NoteFields {
  title: string;
  body: string;
}

export interface JournalFields {
  body: string;
  mood: number | null;
}

/** A queued save the server refused because the document changed elsewhere. */
export interface Conflict {
  /** `note:<id>` or `journal:<date>`. */
  doc: string;
  mine: NoteFields | JournalFields;
  theirs: Note | JournalEntry;
}

/** The outbox entry key for a document. */
export const noteDoc = (id: string) => `note:${id}`;
export const journalDoc = (date: string) => `journal:${date}`;
/** IndexedDB key where a document's unresolved sync conflict is kept. */
export const conflictKey = (doc: string) => `conflict:${doc}`;

export interface Outbox {
  outbox(): Promise<OutboxEntry[]>;
  settle(sent: OutboxEntry, rebase?: (op: unknown) => unknown): Promise<void>;
}

export interface SyncApi {
  createTask(t: NewTask & { id: string }): Promise<Task>;
  updateTask(id: string, patch: TaskPatch): Promise<Task>;
  deleteTask(id: string): Promise<void>;
  capture(text: string, id?: string): Promise<InboxItem>;
  deleteInboxItem(id: string): Promise<void>;
  createProject(p: NewProject & { id: string }): Promise<Project>;
  updateProject(id: string, patch: ProjectPatch): Promise<Project>;
  deleteProject(id: string): Promise<void>;
  createNote(n: { id: string; title?: string; body?: string }): Promise<Note>;
  updateNote(id: string, patch: NotePatch): Promise<Note>;
  deleteNote(id: string): Promise<void>;
  putJournal(date: string, entry: PutJournal): Promise<JournalEntry>;
}

export interface SyncResult {
  applied: number;
  /** Changes the server rejected; they were dropped from the outbox. */
  rejected: { op: Op; reason: string }[];
  /** Saves that conflicted with a newer server copy; dropped from the outbox. */
  conflicts: Conflict[];
  /** New server versions of documents saved by this replay, by document key. */
  versions: Record<string, number>;
  /** Replay stopped early (offline, server error, or signed out). */
  stopped?: "offline" | "server" | "unauthorized";
  pending: number;
}

/** A random 32-hex id, the format the server accepts for client creates. */
export function clientId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

/** Applies one op; returns the new server version for document saves. */
async function apply(api: SyncApi, op: Op): Promise<number | void> {
  try {
    switch (op.kind) {
      case "task.create":
        await api.createTask(op.task);
        return;
      case "task.update":
        await api.updateTask(op.id, op.patch);
        return;
      case "task.delete":
        await api.deleteTask(op.id);
        return;
      case "inbox.capture":
        await api.capture(op.item.text, op.item.id);
        return;
      case "inbox.delete":
        await api.deleteInboxItem(op.id);
        return;
      case "project.create":
        await api.createProject(op.project);
        return;
      case "project.update":
        await api.updateProject(op.id, op.patch);
        return;
      case "project.delete":
        await api.deleteProject(op.id);
        return;
      case "note.create":
        await api.createNote(op.note);
        return;
      case "note.save":
        return (await api.updateNote(op.id, { ...op.fields, expected_updated_at: op.base })).updated_at;
      case "note.delete":
        await api.deleteNote(op.id);
        return;
      case "journal.save":
        return (await api.putJournal(op.date, { ...op.fields, expected_updated_at: op.base })).updated_at;
    }
  } catch (e) {
    // Already applied by an earlier, interrupted replay.
    const isCreate = op.kind.endsWith(".create") || op.kind === "inbox.capture";
    const isDelete = op.kind.endsWith(".delete");
    if (e instanceof ApiError && isCreate && e.code === "exists") return;
    if (e instanceof ApiError && isDelete && e.status === 404) return;
    throw e;
  }
}

function describeOp(op: Op): string {
  switch (op.kind) {
    case "task.create":
      return `create “${op.task.title}”`;
    case "task.update":
      return "update a task";
    case "task.delete":
      return "delete a task";
    case "inbox.capture":
      return `capture “${op.item.text.slice(0, 40)}”`;
    case "inbox.delete":
      return "discard an inbox item";
    case "project.create":
      return `create project “${op.project.name}”`;
    case "project.update":
      return "update a project";
    case "project.delete":
      return "delete a project";
    case "note.create":
      return "create a note";
    case "note.save":
      return `save “${op.fields.title || op.fields.body.split("\n")[0].slice(0, 40) || "Untitled"}”`;
    case "note.delete":
      return "delete a note";
    case "journal.save":
      return `save the journal for ${op.date}`;
  }
}

function conflictOf(op: Op, e: unknown): Conflict | undefined {
  if (!(e instanceof ApiError) || e.code !== "conflict") return undefined;
  const body = e.body as { note?: Note; entry?: JournalEntry } | undefined;
  if (op.kind === "note.save" && body?.note) return { doc: noteDoc(op.id), mine: op.fields, theirs: body.note };
  if (op.kind === "journal.save" && body?.entry) return { doc: journalDoc(op.date), mine: op.fields, theirs: body.entry };
  return undefined;
}

/** Replay queued changes in order. Never throws. */
export async function replay(outbox: Outbox, api: SyncApi): Promise<SyncResult> {
  const entries = await outbox.outbox();
  const result: SyncResult = { applied: 0, rejected: [], conflicts: [], versions: {}, pending: entries.length };
  for (const entry of entries) {
    const op = entry.op as Op;
    try {
      const version = await apply(api, op);
      result.applied++;
      if (typeof version === "number") result.versions[op.kind === "note.save" ? noteDoc(op.id) : journalDoc((op as { date: string }).date)] = version;
      // Edited again while this save was in flight: keep the newer copy, based on what we just wrote.
      await outbox.settle(entry, (queued) => ({ ...(queued as Op), base: version }));
      result.pending--;
      continue;
    } catch (e) {
      const conflict = conflictOf(op, e);
      if (conflict) {
        result.conflicts.push(conflict);
        await outbox.settle(entry);
        result.pending--;
        continue;
      }
      if (isNetworkError(e)) result.stopped = "offline";
      else if (e instanceof ApiError && e.status === 401) result.stopped = "unauthorized";
      else if (e instanceof ApiError && e.status >= 500) result.stopped = "server";
      if (result.stopped) return result;
      // A 4xx the server will never accept: drop it, but tell the user.
      const reason = e instanceof ApiError ? (e.detail ?? e.code) : String(e);
      result.rejected.push({ op, reason: `Couldn't ${describeOp(op)}: ${reason}` });
    }
    await outbox.settle(entry);
    result.pending--;
  }
  return result;
}

/** Apply an op to a local task list (optimistic offline edits). */
export function applyLocally(tasks: Task[], op: Op, now = Date.now()): Task[] {
  switch (op.kind) {
    case "task.create": {
      const t = op.task;
      const task: Task = {
        id: t.id,
        parent_id: t.parent_id ?? null,
        project_id: t.project_id ?? null,
        title: t.title.trim(),
        notes: t.notes ?? "",
        status: "todo",
        priority: t.priority ?? "none",
        due_date: t.due_date ?? null,
        created_at: now,
        updated_at: now,
        completed_at: null,
      };
      return [...tasks, task];
    }
    case "task.update":
      return tasks.map((t) => {
        if (t.id !== op.id) return t;
        const next = { ...t, ...op.patch, updated_at: now };
        if (op.patch.status && op.patch.status !== t.status) {
          const open = op.patch.status === "todo" || op.patch.status === "in_progress";
          next.completed_at = open ? null : now;
        }
        return next;
      });
    case "task.delete":
      return tasks.filter((t) => t.id !== op.id && t.parent_id !== op.id);
    default:
      return tasks;
  }
}

/** Apply an inbox op to a local inbox list. */
export function applyInboxLocally(items: InboxItem[], op: Op, now = Date.now()): InboxItem[] {
  switch (op.kind) {
    case "inbox.capture":
      return [...items, { id: op.item.id, text: op.item.text.trim(), created_at: now }];
    case "inbox.delete":
      return items.filter((i) => i.id !== op.id);
    default:
      return items;
  }
}

/** Apply a project op to a local project list. */
export function applyProjectLocally(projects: Project[], op: Op, now = Date.now()): Project[] {
  switch (op.kind) {
    case "project.create": {
      const p = op.project;
      return [...projects, { id: p.id, name: p.name.trim(), description: p.description ?? "", status: "active", created_at: now, updated_at: now }];
    }
    case "project.update":
      return projects.map((p) => (p.id === op.id ? { ...p, ...op.patch, updated_at: now } : p));
    case "project.delete":
      return projects.filter((p) => p.id !== op.id);
    default:
      return projects;
  }
}
