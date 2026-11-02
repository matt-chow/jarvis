import { useCallback, useEffect } from "react";
import { ApiError, api } from "../lib/api";
import { sortLists, type NewReminder, type Reminder, type ReminderList, type ReminderPatch } from "../lib/reminders";
import { applyReminderListLocally, applyReminderLocally, clientId, type Op } from "../lib/sync";
import { runOptimistic, runOrQueue, upsert } from "./offline";
import { useCachedList, type CachedList } from "./useCachedList";
import { SYNCED_EVENT } from "./useSync";

export interface RemindersState {
  reminders?: Reminder[];
  lists?: ReminderList[];
  loading: boolean;
  /** Showing the copy saved on this device. Changes still work and sync later. */
  stale: boolean;
  error?: string;
  reload: () => void;
  create: (r: NewReminder) => Promise<Reminder | undefined>;
  update: (id: string, patch: ReminderPatch) => Promise<Reminder | undefined>;
  /** Soft delete (recoverable), or `purge` one already deleted. */
  remove: (id: string, purge?: boolean) => Promise<boolean>;
  restore: (id: string) => Promise<boolean>;
  /** Complete the current occurrence (a series moves on to the next). */
  complete: (r: Reminder) => Promise<boolean>;
  uncomplete: (r: Reminder) => Promise<boolean>;
  snooze: (r: Reminder, until: number) => Promise<boolean>;
  createList: (name: string, color?: string) => Promise<ReminderList | undefined>;
  updateList: (id: string, patch: { name?: string; color?: string }) => Promise<ReminderList | undefined>;
  removeList: (id: string) => Promise<boolean>;
}

const fetchReminders = () => api.reminders();
const fetchLists = async () => sortLists(await api.reminderLists());
const applyLocal = (cur: Reminder[], op: Op) => applyReminderLocally(cur, op);
const applyListLocal = (cur: ReminderList[], op: Op) => applyReminderListLocally(cur, op);
const byId = (cur: Reminder[], id: string) => cur.find((x) => x.id === id)!;

/**
 * A 409 from the reminder routes carries the current record: show it, and
 * say why the change didn't apply (D-049).
 */
function refused(list: CachedList<Reminder>, e: unknown): string | undefined {
  if (!(e instanceof ApiError) || e.status !== 409) return undefined;
  const body = e.body as { reminder?: Reminder; fields?: string[]; message?: string } | undefined;
  if (body?.reminder) list.replace(upsert(list.latest(), body.reminder));
  if (e.code === "conflict") return `Changed on another device meanwhile (${(body?.fields ?? []).join(", ")}). Showing that version; make your change again if you still want it.`;
  if (e.code === "stale") return `That occurrence has moved on (${body?.message ?? "stale"}). Showing the current one.`;
  return undefined;
}

/** Reminders and their lists (D-049); everything works offline (D-017). */
export function useReminders(online: boolean): RemindersState {
  const list = useCachedList<Reminder>("reminders:last", "reminder", fetchReminders, online);
  const lists = useCachedList<ReminderList>("reminder-lists:last", "list", fetchLists, online);
  const { reload } = list;
  const reloadLists = lists.reload;

  useEffect(() => {
    const both = () => {
      reload();
      reloadLists();
    };
    window.addEventListener(SYNCED_EVENT, both);
    return () => window.removeEventListener(SYNCED_EVENT, both);
  }, [reload, reloadLists]);

  /** Run a reminder mutation; a 409 shows the server's copy and a reason. */
  const run = useCallback(
    async <R>(optimistic: boolean, op: Op, server: () => Promise<R>, fromServer: (cur: Reminder[], r: R) => Reminder[], local: (next: Reminder[]) => R) => {
      let why: string | undefined;
      const opts = {
        list,
        noun: "reminder",
        op,
        server: async () => {
          try {
            return await server();
          } catch (e) {
            why = refused(list, e);
            throw e;
          }
        },
        fromServer,
        applyLocal,
        local,
      };
      const r = await (optimistic ? runOptimistic(opts) : runOrQueue(opts));
      if (why) list.setError(why);
      return r;
    },
    [list],
  );

  const saved = (cur: Reminder[], r: Reminder) => upsert(cur, r);

  const create = useCallback(
    (n: NewReminder) => {
      const reminder = { ...n, id: clientId() };
      return run(true, { kind: "reminder.create", reminder }, () => api.createReminder(reminder), saved, (cur) => byId(cur, reminder.id));
    },
    [run],
  );

  const update = useCallback(
    (id: string, patch: ReminderPatch) => {
      // The version this edit was based on, for the server's field-level merge.
      const base = list.latest().find((x) => x.id === id)?.version;
      const p = { ...patch, base_version: base || undefined };
      return run(true, { kind: "reminder.update", id, patch: p }, () => api.updateReminder(id, p), saved, (cur) => byId(cur, id));
    },
    [run, list],
  );

  const remove = useCallback(
    async (id: string, purge = false) =>
      (await run(
        true,
        { kind: "reminder.delete", id, purge },
        async () => {
          await api.deleteReminder(id);
          return true;
        },
        (cur) => applyLocal(cur, { kind: "reminder.delete", id, purge }),
        () => true,
      )) ?? false,
    [run],
  );

  const restore = useCallback(
    async (id: string) => (await run(true, { kind: "reminder.restore", id }, () => api.restoreReminder(id), saved, (cur) => byId(cur, id))) !== undefined,
    [run],
  );

  const complete = useCallback(
    async (r: Reminder) => {
      const occurrence = r.due_date ?? undefined;
      return (await run(true, { kind: "reminder.complete", id: r.id, occurrence }, () => api.completeReminder(r.id, occurrence), saved, (cur) => byId(cur, r.id))) !== undefined;
    },
    [run],
  );

  const uncomplete = useCallback(
    async (r: Reminder) => {
      // A one-off's own date; a series' latest completion (the server's default).
      const occurrence = r.recurrence ? undefined : (r.due_date ?? undefined);
      return (await run(true, { kind: "reminder.uncomplete", id: r.id, occurrence }, () => api.uncompleteReminder(r.id, occurrence), saved, (cur) => byId(cur, r.id))) !== undefined;
    },
    [run],
  );

  const snooze = useCallback(
    async (r: Reminder, until: number) => {
      const occurrence = r.due_date ?? undefined;
      return (await run(true, { kind: "reminder.snooze", id: r.id, until, occurrence }, () => api.snoozeReminder(r.id, until, occurrence), saved, (cur) => byId(cur, r.id))) !== undefined;
    },
    [run],
  );

  const createList = useCallback(
    (name: string, color?: string) => {
      const l = { id: clientId(), name: name.trim(), color };
      return runOrQueue({
        list: lists,
        noun: "list",
        op: { kind: "reminderList.create", list: l },
        server: () => api.createReminderList(l),
        fromServer: (cur, created) => sortLists(upsert(cur, created)),
        applyLocal: applyListLocal,
        local: (cur) => cur.find((x) => x.id === l.id)!,
      });
    },
    [lists],
  );

  const updateList = useCallback(
    (id: string, patch: { name?: string; color?: string }) =>
      runOrQueue({
        list: lists,
        noun: "list",
        op: { kind: "reminderList.update", id, patch },
        server: () => api.updateReminderList(id, patch),
        fromServer: (cur, u) => sortLists(upsert(cur, u)),
        applyLocal: applyListLocal,
        local: (cur) => cur.find((x) => x.id === id)!,
      }),
    [lists],
  );

  const removeList = useCallback(
    async (id: string) => {
      const op: Op = { kind: "reminderList.delete", id };
      const ok =
        (await runOrQueue({
          list: lists,
          noun: "list",
          op,
          server: async () => {
            await api.deleteReminderList(id);
            return true;
          },
          fromServer: (cur) => cur.filter((x) => x.id !== id),
          applyLocal: applyListLocal,
          local: () => true,
        })) ?? false;
      // Its reminders stay, without a list (as on the server).
      if (ok) list.replace(applyLocal(list.latest(), op));
      return ok;
    },
    [lists, list],
  );

  return {
    reminders: list.items,
    lists: lists.items,
    loading: list.loading,
    stale: list.stale,
    error: list.error ?? lists.error,
    reload: () => {
      list.reload();
      lists.reload();
    },
    create,
    update,
    remove,
    restore,
    complete,
    uncomplete,
    snooze,
    createList,
    updateList,
    removeList,
  };
}
