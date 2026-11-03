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
import { newBlock, patchBlock, type BlockPatch, type NewBlock, type TimeBlock } from "./blocks";
import type { Entity as BudgetEntity, Imported, Inputs as BudgetInputs, Records as BudgetRecords, Template as BudgetTemplate, Transaction as BudgetTransaction, TransactionInput } from "./budget";
import { sortCategories, withTaskCategories, type Category, type CategoryItem, type CategoryPatch, type NewCategory } from "./categories";
import { addDays, dayNumber, nthStart, type CalendarEvent, type EventPatch, type NewEvent, type SplitRequest, type SplitResult } from "./calendar";
import type { OutboxEntry } from "./db";
import { withCheck, type Habit, type HabitPatch, type HabitView, type NewHabit } from "./habits";
import type { InboxItem, Triaged, TriageTarget } from "./inbox";
import type { EntityRef, Linked } from "./links";
import type { JournalEntry, PutJournal } from "./journal";
import { applyProgress, type LibraryItem, type Progress, type ProgressPatch } from "./library";
import type { Note, NotePatch } from "./notes";
import type { Preferences, PreferencesPatch } from "./preferences";
import type { NewProject, Project, ProjectPatch } from "./projects";
import {
  completeLocally,
  newReminderFrom,
  patchReminder,
  sortLists,
  uncompleteLocally,
  withoutCategory,
  withoutList,
  type NewReminder,
  type Reminder,
  type ReminderList,
  type ReminderPatch,
} from "./reminders";
import type { NewTask, Task, TaskPatch } from "./tasks";

export type Op =
  | { kind: "task.create"; task: NewTask & { id: string } }
  | { kind: "task.update"; id: string; patch: TaskPatch }
  | { kind: "task.delete"; id: string }
  /** Replaces the task's whole set (a PUT), so replays are safe (D-049). */
  | { kind: "task.categories"; task_id: string; category_ids: string[] }
  | { kind: "category.create"; category: NewCategory & { id: string } }
  | { kind: "category.update"; id: string; patch: CategoryPatch }
  | { kind: "category.delete"; id: string }
  | { kind: "reminder.create"; reminder: NewReminder & { id: string } }
  /** `patch.base_version`: the version edited (field-level merge on the server). */
  | { kind: "reminder.update"; id: string; patch: ReminderPatch }
  /** Soft delete, or `purge` one already deleted. Neither repeats on replay. */
  | { kind: "reminder.delete"; id: string; purge?: boolean }
  | { kind: "reminder.restore"; id: string }
  /** Names the occurrence completed, so a replay after the series moved on still means that one. */
  | { kind: "reminder.complete"; id: string; occurrence?: string }
  | { kind: "reminder.uncomplete"; id: string; occurrence?: string }
  /** An absolute instant, fixed when queued. */
  | { kind: "reminder.snooze"; id: string; until: number; occurrence?: string }
  | { kind: "reminderList.create"; list: { id: string; name: string; color?: string } }
  | { kind: "reminderList.update"; id: string; patch: { name?: string; color?: string } }
  | { kind: "reminderList.delete"; id: string }
  | { kind: "inbox.capture"; item: { id: string; text: string } }
  | { kind: "inbox.delete"; id: string }
  /** Filed into another module on replay; the journal date is fixed when queued (D-036). */
  | { kind: "inbox.triage"; id: string; to: TriageTarget; date?: string }
  | { kind: "project.create"; project: NewProject & { id: string } }
  | { kind: "project.update"; id: string; patch: ProjectPatch }
  | { kind: "project.delete"; id: string }
  | { kind: "note.create"; note: { id: string; title?: string; body?: string } }
  /** `base`: the server version edited; undefined for a note created offline. */
  | { kind: "note.save"; id: string; fields: NoteFields; base?: number }
  | { kind: "note.delete"; id: string }
  /** `base` 0 = the day had no entry (create only). */
  | { kind: "journal.save"; date: string; fields: JournalFields; base?: number }
  | { kind: "journal.delete"; date: string }
  | { kind: "habit.create"; habit: NewHabit & { id: string } }
  | { kind: "habit.update"; id: string; patch: HabitPatch }
  | { kind: "habit.delete"; id: string }
  /** Idempotent: sets the day to `done`. */
  | { kind: "habit.check"; id: string; date: string; done: boolean }
  | { kind: "event.create"; event: NewEvent & { id: string } }
  | { kind: "event.update"; id: string; patch: EventPatch }
  | { kind: "event.delete"; id: string }
  /** Atomic on the server; the replacement's client id makes a replay a 409 (D-047). */
  | { kind: "event.split"; id: string; split: SplitRequest }
  | { kind: "block.create"; block: NewBlock & { id: string } }
  | { kind: "block.update"; id: string; patch: BlockPatch }
  | { kind: "block.delete"; id: string }
  /** Last write wins, so replays are safe. */
  | { kind: "prefs.update"; patch: PreferencesPatch }
  /** Idempotent per pair on the server (D-037). */
  | { kind: "link.create"; from: EntityRef; to: EntityRef }
  | { kind: "link.delete"; link_id: string }
  /** Reading progress (a PUT, so replays are safe). */
  | { kind: "library.progress"; file_id: string; patch: ProgressPatch }
  /** Budget records (D-050): creates carry a client id; updates replace every field (a PUT). */
  | { kind: "budget.create"; entity: BudgetEntity; id: string; input: BudgetInputs[BudgetEntity] }
  | { kind: "budget.update"; entity: BudgetEntity; id: string; input: BudgetInputs[BudgetEntity] }
  | { kind: "budget.delete"; entity: BudgetEntity; id: string }
  /** The transaction's client id makes a replay a 409. */
  | { kind: "budget.post"; template_id: string; txn_id: string }
  | { kind: "budget.currency"; currency: string }
  /** Ids already stored are skipped, so a replay is safe. */
  | { kind: "budget.import"; transactions: (TransactionInput & { id: string })[] };

export interface NoteFields {
  title: string;
  body: string;
  /** Saved with the text so pinning works offline (D-034); absent in older queued saves. */
  pinned?: boolean;
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

/**
 * A queued reminder change the server couldn't apply as it was, kept until
 * the owner chooses (D-049 M7). `fields`: this device's values for fields
 * another device also changed (the rest of the edit was applied).
 * `stale`: an occurrence action for an occurrence the reminder has moved
 * past; it can be applied to the current occurrence instead.
 */
export type ReminderConflict =
  | { kind: "fields"; id: string; fields: string[]; mine: ReminderPatch; theirs: Reminder; at: number }
  | { kind: "stale"; id: string; op: Extract<Op, { kind: "reminder.complete" | "reminder.uncomplete" | "reminder.snooze" }>; message: string; theirs: Reminder; at: number };

/** IndexedDB key prefix for unresolved reminder conflicts. */
export const REMINDER_CONFLICT = "conflict:reminder:";
/** One field conflict per reminder; one stale conflict per action and occurrence. */
export const reminderConflictKey = (c: ReminderConflict) =>
  c.kind === "fields" ? `${REMINDER_CONFLICT}${c.id}` : `${REMINDER_CONFLICT}${c.id}:${c.op.kind}:${c.op.occurrence ?? ""}`;

/** Keep both: a newer field conflict for the same reminder adds to the older one. */
export function mergeReminderConflict(old: ReminderConflict | undefined, next: ReminderConflict): ReminderConflict {
  if (old?.kind !== "fields" || next.kind !== "fields") return next;
  return { ...next, fields: [...new Set([...old.fields, ...next.fields])], mine: { ...old.mine, ...next.mine } };
}

export interface Kv {
  getValue<T>(key: string): Promise<T | undefined>;
  setValue<T>(key: string, value: T): Promise<void>;
}

/** Store reminder conflicts on this device until the owner resolves them. */
export async function keepReminderConflicts(kv: Kv, conflicts: ReminderConflict[]): Promise<void> {
  for (const c of conflicts) {
    const key = reminderConflictKey(c);
    await kv.setValue(key, mergeReminderConflict(await kv.getValue<ReminderConflict>(key), c));
  }
}

export interface Outbox {
  outbox(): Promise<OutboxEntry[]>;
  settle(sent: OutboxEntry, rebase?: (op: unknown) => unknown): Promise<void>;
}

export interface SyncApi {
  createTask(t: NewTask & { id: string }): Promise<Task>;
  updateTask(id: string, patch: TaskPatch): Promise<Task>;
  deleteTask(id: string): Promise<void>;
  setTaskCategories(taskId: string, ids: string[]): Promise<unknown>;
  createCategory(c: NewCategory & { id: string }): Promise<Category>;
  updateCategory(id: string, patch: CategoryPatch): Promise<Category>;
  deleteCategory(id: string): Promise<void>;
  reminder(id: string): Promise<Reminder>;
  createReminder(r: NewReminder & { id: string }): Promise<Reminder>;
  updateReminder(id: string, patch: ReminderPatch): Promise<Reminder>;
  deleteReminder(id: string): Promise<void>;
  restoreReminder(id: string): Promise<Reminder>;
  completeReminder(id: string, occurrence?: string): Promise<Reminder>;
  uncompleteReminder(id: string, occurrence?: string): Promise<Reminder>;
  snoozeReminder(id: string, until: number, occurrence?: string): Promise<Reminder>;
  createReminderList(l: { id: string; name: string; color?: string }): Promise<ReminderList>;
  updateReminderList(id: string, patch: { name?: string; color?: string }): Promise<ReminderList>;
  deleteReminderList(id: string): Promise<void>;
  capture(text: string, id?: string): Promise<InboxItem>;
  deleteInboxItem(id: string): Promise<void>;
  triage(id: string, to: TriageTarget, date?: string): Promise<Triaged>;
  createProject(p: NewProject & { id: string }): Promise<Project>;
  updateProject(id: string, patch: ProjectPatch): Promise<Project>;
  deleteProject(id: string): Promise<void>;
  createNote(n: { id: string; title?: string; body?: string }): Promise<Note>;
  updateNote(id: string, patch: NotePatch): Promise<Note>;
  deleteNote(id: string): Promise<void>;
  putJournal(date: string, entry: PutJournal): Promise<JournalEntry>;
  deleteJournal(date: string): Promise<void>;
  createHabit(h: NewHabit & { id: string }): Promise<Habit>;
  updateHabit(id: string, patch: HabitPatch): Promise<Habit>;
  deleteHabit(id: string): Promise<void>;
  checkHabit(id: string, date: string, done: boolean): Promise<void>;
  createEvent(e: NewEvent & { id: string }): Promise<CalendarEvent>;
  updateEvent(id: string, patch: EventPatch): Promise<CalendarEvent>;
  deleteEvent(id: string): Promise<void>;
  splitEvent(id: string, split: SplitRequest): Promise<SplitResult>;
  createBlock(b: NewBlock & { id: string }): Promise<TimeBlock>;
  updateBlock(id: string, patch: BlockPatch): Promise<TimeBlock>;
  deleteBlock(id: string): Promise<void>;
  updatePreferences(patch: PreferencesPatch): Promise<Preferences>;
  setProgress(fileId: string, patch: ProgressPatch): Promise<Progress>;
  link(from: EntityRef, to: EntityRef): Promise<Linked>;
  unlink(linkId: string): Promise<void>;
  createBudget<E extends BudgetEntity>(entity: E, id: string, input: BudgetInputs[E]): Promise<BudgetRecords[E]>;
  replaceBudget<E extends BudgetEntity>(entity: E, id: string, input: BudgetInputs[E]): Promise<BudgetRecords[E]>;
  deleteBudget(entity: BudgetEntity, id: string): Promise<void>;
  postTemplate(templateId: string, txnId: string): Promise<{ transaction: BudgetTransaction; template: BudgetTemplate }>;
  setBudgetCurrency(currency: string): Promise<{ currency: string }>;
  importBudget(transactions: (TransactionInput & { id: string })[]): Promise<Imported>;
}

export interface SyncResult {
  applied: number;
  /** Changes the server rejected; they were dropped from the outbox. */
  rejected: { op: Op; reason: string }[];
  /** Saves that conflicted with a newer server copy; dropped from the outbox. */
  conflicts: Conflict[];
  /** Reminder changes kept for the owner to resolve; dropped from the outbox. */
  reminderConflicts: ReminderConflict[];
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

/**
 * Applies one op; returns the new server version for document saves.
 * `seen`: reminder versions this replay produced, so later queued edits of
 * the same reminder build on them instead of conflicting with themselves.
 */
async function apply(api: SyncApi, op: Op, seen: Map<string, number>): Promise<number | void> {
  const saw = (r: Reminder) => void seen.set(r.id, r.version);
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
      case "task.categories":
        await api.setTaskCategories(op.task_id, op.category_ids);
        return;
      case "category.create":
        await api.createCategory(op.category);
        return;
      case "category.update":
        await api.updateCategory(op.id, op.patch);
        return;
      case "category.delete":
        await api.deleteCategory(op.id);
        return;
      case "reminder.create":
        saw(await api.createReminder(op.reminder));
        return;
      case "reminder.update":
        saw(await api.updateReminder(op.id, { ...op.patch, base_version: seen.get(op.id) ?? op.patch.base_version }));
        return;
      case "reminder.delete": {
        // The server's DELETE soft-deletes, then purges: check first so a replay never purges by accident.
        const current = await api.reminder(op.id);
        if ((current.deleted_at !== null) === !!op.purge) await api.deleteReminder(op.id);
        return;
      }
      case "reminder.restore":
        saw(await api.restoreReminder(op.id));
        return;
      case "reminder.complete":
        saw(await api.completeReminder(op.id, op.occurrence));
        return;
      case "reminder.uncomplete":
        saw(await api.uncompleteReminder(op.id, op.occurrence));
        return;
      case "reminder.snooze":
        saw(await api.snoozeReminder(op.id, op.until, op.occurrence));
        return;
      case "reminderList.create":
        await api.createReminderList(op.list);
        return;
      case "reminderList.update":
        await api.updateReminderList(op.id, op.patch);
        return;
      case "reminderList.delete":
        await api.deleteReminderList(op.id);
        return;
      case "inbox.capture":
        await api.capture(op.item.text, op.item.id);
        return;
      case "inbox.delete":
        await api.deleteInboxItem(op.id);
        return;
      case "inbox.triage":
        await api.triage(op.id, op.to, op.date);
        return;
      case "link.create":
        await api.link(op.from, op.to);
        return;
      case "link.delete":
        await api.unlink(op.link_id);
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
      case "journal.delete":
        await api.deleteJournal(op.date);
        return;
      case "habit.create":
        await api.createHabit(op.habit);
        return;
      case "habit.update":
        await api.updateHabit(op.id, op.patch);
        return;
      case "habit.delete":
        await api.deleteHabit(op.id);
        return;
      case "habit.check":
        await api.checkHabit(op.id, op.date, op.done);
        return;
      case "event.create":
        await api.createEvent(op.event);
        return;
      case "event.update":
        await api.updateEvent(op.id, op.patch);
        return;
      case "event.delete":
        await api.deleteEvent(op.id);
        return;
      case "event.split":
        await api.splitEvent(op.id, op.split);
        return;
      case "block.create":
        await api.createBlock(op.block);
        return;
      case "block.update":
        await api.updateBlock(op.id, op.patch);
        return;
      case "block.delete":
        await api.deleteBlock(op.id);
        return;
      case "prefs.update":
        await api.updatePreferences(op.patch);
        return;
      case "library.progress":
        await api.setProgress(op.file_id, op.patch);
        return;
      case "budget.create":
        await api.createBudget(op.entity, op.id, op.input);
        return;
      case "budget.update":
        await api.replaceBudget(op.entity, op.id, op.input);
        return;
      case "budget.delete":
        await api.deleteBudget(op.entity, op.id);
        return;
      case "budget.post":
        await api.postTemplate(op.template_id, op.txn_id);
        return;
      case "budget.currency":
        await api.setBudgetCurrency(op.currency);
        return;
      case "budget.import":
        await api.importBudget(op.transactions);
        return;
    }
  } catch (e) {
    // Already applied by an earlier, interrupted replay.
    // A split whose replacement exists was applied (the series edit is in the same transaction).
    const isCreate = op.kind.endsWith(".create") || op.kind === "inbox.capture" || op.kind === "event.split" || op.kind === "budget.post";
    // A check-in for a habit deleted meanwhile has nothing left to do.
    // A check-in or reading progress for something deleted meanwhile has nothing left to do.
    // A 404 means it is already done (deleted, or filed from another device).
    // A reminder deleted meanwhile has nothing left to complete, snooze, or restore.
    const isDelete =
      op.kind.endsWith(".delete") ||
      ["habit.check", "task.categories", "library.progress", "inbox.triage", "event.split", "reminder.complete", "reminder.uncomplete", "reminder.snooze", "reminder.restore"].includes(op.kind);
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
    case "task.categories":
      return "change a task's categories";
    case "category.create":
      return `create category “${op.category.name}”`;
    case "category.update":
      return "update a category";
    case "category.delete":
      return "delete a category";
    case "reminder.create":
      return `create reminder “${op.reminder.title}”`;
    case "reminder.update":
      return "update a reminder";
    case "reminder.delete":
      return op.purge ? "delete a reminder for good" : "delete a reminder";
    case "reminder.restore":
      return "restore a reminder";
    case "reminder.complete":
      return "complete a reminder";
    case "reminder.uncomplete":
      return "reopen a reminder";
    case "reminder.snooze":
      return "snooze a reminder";
    case "reminderList.create":
      return `create list “${op.list.name}”`;
    case "reminderList.update":
      return "update a list";
    case "reminderList.delete":
      return "delete a list";
    case "inbox.capture":
      return `capture “${op.item.text.slice(0, 40)}”`;
    case "inbox.delete":
      return "discard an inbox item";
    case "inbox.triage":
      return `file an inbox item as a ${op.to === "journal" ? "journal entry" : op.to}`;
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
    case "journal.delete":
      return `delete the journal for ${op.date}`;
    case "habit.create":
      return `create habit “${op.habit.name}”`;
    case "habit.update":
      return "update a habit";
    case "habit.delete":
      return "delete a habit";
    case "habit.check":
      return `check in a habit for ${op.date}`;
    case "event.create":
      return `create event “${op.event.title}”`;
    case "event.update":
      return "update an event";
    case "event.delete":
      return "delete an event";
    case "event.split":
      return op.split.scope === "one" ? `change the ${op.split.date} occurrence of an event` : `change an event from ${op.split.date} on`;
    case "block.create":
      return `schedule a session on ${op.block.date}`;
    case "block.update":
      return "update a session";
    case "block.delete":
      return "delete a session";
    case "prefs.update":
      return "save your preferences";
    case "library.progress":
      return "save your reading progress";
    case "link.create":
      return `link a ${op.from.kind} to a ${op.to.kind}`;
    case "link.delete":
      return "remove a link";
    case "budget.create":
      return `add a budget ${BUDGET_NOUN[op.entity]}`;
    case "budget.update":
      return `change a budget ${BUDGET_NOUN[op.entity]}`;
    case "budget.delete":
      return `delete a budget ${BUDGET_NOUN[op.entity]}`;
    case "budget.post":
      return "record a recurring item";
    case "budget.currency":
      return `set the budget currency to ${op.currency}`;
    case "budget.import":
      return `import ${op.transactions.length} transaction${op.transactions.length === 1 ? "" : "s"}`;
  }
}

const BUDGET_NOUN: Record<BudgetEntity, string> = { accounts: "account", categories: "category", transactions: "transaction", templates: "recurring item" };

function conflictOf(op: Op, e: unknown): Conflict | undefined {
  if (!(e instanceof ApiError) || e.code !== "conflict") return undefined;
  const body = e.body as { note?: Note; entry?: JournalEntry } | undefined;
  if (op.kind === "note.save" && body?.note) return { doc: noteDoc(op.id), mine: op.fields, theirs: body.note };
  if (op.kind === "journal.save" && body?.entry) return { doc: journalDoc(op.date), mine: op.fields, theirs: body.entry };
  return undefined;
}

/**
 * A reminder edit that clashed with another device's: apply the fields that
 * didn't clash, and keep this device's values for the rest. A stale
 * occurrence action is kept as is. Anything else isn't a reminder conflict.
 */
export async function reminderConflictOf(api: Pick<SyncApi, "updateReminder">, op: Op, e: unknown, seen: Map<string, number>): Promise<ReminderConflict | undefined> {
  if (!(e instanceof ApiError) || e.status !== 409) return undefined;
  const body = e.body as { fields?: string[]; reminder?: Reminder; message?: string } | undefined;
  const theirs = body?.reminder;
  if (!theirs) return undefined;
  const at = Date.now();
  if (e.code === "stale" && (op.kind === "reminder.complete" || op.kind === "reminder.uncomplete" || op.kind === "reminder.snooze")) {
    return { kind: "stale", id: op.id, op, message: body.message ?? e.detail ?? "the reminder has moved on", theirs, at };
  }
  if (e.code !== "conflict" || op.kind !== "reminder.update" || !body.fields?.length) return undefined;
  const clash = new Set(body.fields);
  const mine: Record<string, unknown> = {};
  const rest: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(op.patch)) {
    if (k === "base_version") continue;
    (clash.has(k) ? mine : rest)[k] = v;
  }
  let current = theirs;
  if (Object.keys(rest).length) {
    try {
      current = await api.updateReminder(op.id, { ...(rest as ReminderPatch), base_version: theirs.version });
      seen.set(current.id, current.version);
    } catch (again) {
      // Changed again meanwhile: keep the whole edit for the owner instead.
      if (!(again instanceof ApiError) || again.status !== 409) throw again;
      Object.assign(mine, rest);
    }
  }
  return { kind: "fields", id: op.id, fields: Object.keys(mine), mine: mine as ReminderPatch, theirs: current, at };
}

/** Why the server refused a queued change, in words. */
function rejection(e: ApiError): string {
  const fields = (e.body as { fields?: string[] } | undefined)?.fields;
  // A reminder edited on two devices: the other edit of the same fields was kept (D-049).
  if (e.code === "conflict" && fields?.length) return `${fields.join(", ")} also changed on another device; that version was kept`;
  return e.detail ?? e.code;
}

/** Replay queued changes in order. Never throws. */
export async function replay(outbox: Outbox, api: SyncApi): Promise<SyncResult> {
  const entries = await outbox.outbox();
  const result: SyncResult = { applied: 0, rejected: [], conflicts: [], reminderConflicts: [], versions: {}, pending: entries.length };
  const seen = new Map<string, number>();
  for (const entry of entries) {
    const op = entry.op as Op;
    try {
      const version = await apply(api, op, seen);
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
      const kept = await reminderConflictOf(api, op, e, seen);
      if (kept) {
        result.reminderConflicts.push(kept);
        await outbox.settle(entry);
        result.pending--;
        continue;
      }
      if (isNetworkError(e)) result.stopped = "offline";
      else if (e instanceof ApiError && e.status === 401) result.stopped = "unauthorized";
      else if (e instanceof ApiError && e.status >= 500) result.stopped = "server";
      if (result.stopped) return result;
      // A 4xx the server will never accept: drop it, but tell the user.
      const reason = e instanceof ApiError ? rejection(e) : String(e);
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
        estimate_minutes: t.estimate_minutes ?? null,
        repeat: t.repeat ?? "none",
        repeat_until: t.repeat_until ?? null,
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

/** Apply a category op (or a task's category set) to the local category list. */
export function applyCategoryLocally(items: CategoryItem[], op: Op, now = Date.now()): CategoryItem[] {
  switch (op.kind) {
    case "category.create": {
      const c = op.category;
      const item: CategoryItem = {
        id: c.id,
        name: c.name.trim(),
        color: c.color ?? "#3b3fa8",
        icon: c.icon?.trim() || null,
        created_at: now,
        updated_at: now,
        task_ids: [],
      };
      return sortCategories([...items, item]);
    }
    case "category.update":
      return sortCategories(
        items.map((c) =>
          c.id === op.id
            ? {
                ...c,
                name: op.patch.name?.trim() ?? c.name,
                color: op.patch.color ?? c.color,
                icon: op.patch.icon === undefined ? c.icon : op.patch.icon?.trim() || null,
                updated_at: now,
              }
            : c,
        ),
      );
    case "category.delete":
      return items.filter((c) => c.id !== op.id);
    case "task.categories":
      return withTaskCategories(items, op.task_id, op.category_ids);
    case "task.delete":
      return items.map((c) => (c.task_ids.includes(op.id) ? { ...c, task_ids: c.task_ids.filter((t) => t !== op.id) } : c));
    default:
      return items;
  }
}

/** Apply a reminder op (or one that touches reminders) to the local reminder list. */
export function applyReminderLocally(items: Reminder[], op: Op, now = Date.now()): Reminder[] {
  const each = (id: string, f: (r: Reminder) => Reminder) => items.map((r) => (r.id === id ? f(r) : r));
  switch (op.kind) {
    case "reminder.create":
      return items.some((r) => r.id === op.reminder.id) ? items : [...items, newReminderFrom(op.reminder.id, op.reminder, now)];
    case "reminder.update":
      return each(op.id, (r) => patchReminder(r, op.patch, now));
    case "reminder.delete":
      return op.purge ? items.filter((r) => r.id !== op.id) : each(op.id, (r) => ({ ...r, deleted_at: r.deleted_at ?? now, updated_at: now }));
    case "reminder.restore":
      return each(op.id, (r) => ({ ...r, deleted_at: null, updated_at: now }));
    case "reminder.complete":
      return each(op.id, (r) => (op.occurrence && r.due_date && op.occurrence !== r.due_date ? r : completeLocally(r, now)));
    case "reminder.uncomplete":
      return each(op.id, (r) => uncompleteLocally(r, now));
    case "reminder.snooze":
      return each(op.id, (r) => ({ ...r, snoozed_until: op.until, updated_at: now }));
    case "reminderList.delete":
      return withoutList(items, op.id, now);
    case "category.delete":
      return withoutCategory(items, op.id);
    // The server unlinks reminders from a deleted task or project.
    case "task.delete":
      return items.map((r) => (r.task_id === op.id ? { ...r, task_id: null, complete_with_task: false } : r));
    case "project.delete":
      return items.map((r) => (r.project_id === op.id ? { ...r, project_id: null } : r));
    default:
      return items;
  }
}

/** Apply a reminder-list op to the local lists. */
export function applyReminderListLocally(lists: ReminderList[], op: Op, now = Date.now()): ReminderList[] {
  switch (op.kind) {
    case "reminderList.create": {
      const l = op.list;
      return lists.some((x) => x.id === l.id) ? lists : sortLists([...lists, { id: l.id, name: l.name.trim(), color: l.color ?? "#3b3fa8", created_at: now, updated_at: now }]);
    }
    case "reminderList.update":
      return sortLists(lists.map((l) => (l.id === op.id ? { ...l, name: op.patch.name?.trim() ?? l.name, color: op.patch.color ?? l.color, updated_at: now } : l)));
    case "reminderList.delete":
      return lists.filter((l) => l.id !== op.id);
    default:
      return lists;
  }
}

/** Apply an inbox op to a local inbox list. */
export function applyInboxLocally(items: InboxItem[], op: Op, now = Date.now()): InboxItem[] {
  switch (op.kind) {
    case "inbox.capture":
      return [...items, { id: op.item.id, text: op.item.text.trim(), created_at: now }];
    case "inbox.delete":
    case "inbox.triage":
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

/** Apply a habit op to a local habit list; `today` recomputes streaks. */
export function applyHabitLocally(habits: HabitView[], op: Op, today: string, now = Date.now()): HabitView[] {
  switch (op.kind) {
    case "habit.create": {
      const h = op.habit;
      const days = h.days ? [...new Set(h.days)].sort() : [0, 1, 2, 3, 4, 5, 6];
      return [...habits, { id: h.id, name: h.name.trim(), days, archived: false, created_at: now, updated_at: now, checkins: [], streak: 0 }];
    }
    case "habit.update":
      return habits.map((h) => (h.id === op.id ? withCheck({ ...h, ...op.patch, updated_at: now }, today, h.checkins.includes(today), today) : h));
    case "habit.delete":
      return habits.filter((h) => h.id !== op.id);
    case "habit.check":
      return habits.map((h) => (h.id === op.id ? withCheck(h, op.date, op.done, today) : h));
    default:
      return habits;
  }
}

/** Apply an event op to a local event list. */
/** The server's repeat rules: a count sets the last date; one-offs have no repeat fields. */
function normalizeRepeat(e: CalendarEvent): CalendarEvent {
  if (e.repeat === "none") return { ...e, repeat_until: null, repeat_count: null, repeat_interval: 1, skip_dates: [] };
  const next = { ...e, skip_dates: [...new Set(e.skip_dates ?? [])].sort() };
  if (next.repeat_count) next.repeat_until = nthStart(next, next.repeat_count) ?? null;
  return next;
}

function eventFrom(e: NewEvent & { id: string }, now: number): CalendarEvent {
  const start_time = e.start_time ?? null;
  return normalizeRepeat({
    id: e.id,
    title: e.title.trim(),
    start_date: e.start_date,
    end_date: e.end_date ?? e.start_date,
    start_time,
    end_time: start_time ? (e.end_time ?? null) : null,
    location: (e.location ?? "").trim(),
    notes: e.notes ?? "",
    repeat: e.repeat ?? "none",
    repeat_until: e.repeat_until ?? null,
    fixed: e.fixed ?? true,
    timezone: e.timezone ?? null,
    repeat_interval: e.repeat_interval ?? 1,
    repeat_count: e.repeat_count ?? null,
    skip_dates: [],
    created_at: now,
    updated_at: now,
  });
}

/** Apply an event op to a local event list. */
export function applyEventLocally(events: CalendarEvent[], op: Op, now = Date.now()): CalendarEvent[] {
  switch (op.kind) {
    case "event.create":
      return [...events, eventFrom(op.event, now)];
    case "event.update":
      return events.map((e) => {
        if (e.id !== op.id) return e;
        const next = { ...e, ...op.patch, updated_at: now };
        // Same rules as the server: moving the start keeps the length; all-day has no end time.
        if (op.patch.start_date && !op.patch.end_date) next.end_date = addDays(op.patch.start_date, dayNumber(e.end_date) - dayNumber(e.start_date));
        if (!next.start_time) next.end_time = null;
        if (op.patch.repeat_until !== undefined && op.patch.repeat_count === undefined) next.repeat_count = null;
        return normalizeRepeat(next);
      });
    case "event.split": {
      const { date, scope, replacement } = op.split;
      const updated = events.map((e) => {
        if (e.id !== op.id) return e;
        if (scope === "one") return normalizeRepeat({ ...e, skip_dates: [...(e.skip_dates ?? []), date], updated_at: now });
        const before = addDays(date, -1);
        return e.repeat_until && e.repeat_until <= before ? e : { ...e, repeat_until: before, repeat_count: null, updated_at: now };
      });
      return replacement && !events.some((e) => e.id === replacement.id) ? [...updated, eventFrom(replacement, now)] : updated;
    }
    case "event.delete":
      return events.filter((e) => e.id !== op.id);
    default:
      return events;
  }
}

/** Apply a time-block op to a local block list. */
export function applyBlockLocally(blocks: TimeBlock[], op: Op, now = Date.now()): TimeBlock[] {
  switch (op.kind) {
    case "block.create":
      return blocks.some((b) => b.id === op.block.id) ? blocks : [...blocks, newBlock(op.block.id, op.block, now)];
    case "block.update":
      return blocks.map((b) => (b.id === op.id ? patchBlock(b, op.patch, now) : b));
    case "block.delete":
      return blocks.filter((b) => b.id !== op.id);
    case "task.delete":
      // The server deletes a task's sessions with it.
      return blocks.filter((b) => b.task_id !== op.id);
    default:
      return blocks;
  }
}

/** Apply a reading-progress op to a local library list. */
export function applyLibraryLocally(items: LibraryItem[], op: Op, now = Date.now()): LibraryItem[] {
  if (op.kind !== "library.progress") return items;
  return items.map((i) => (i.file.id === op.file_id ? { ...i, progress: applyProgress(op.file_id, i.progress, op.patch, now) } : i));
}
