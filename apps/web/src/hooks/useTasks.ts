import { useCallback, useEffect } from "react";
import { api } from "../lib/api";
import { applyLocally, clientId } from "../lib/sync";
import { isOpen, repeats, type NewTask, type Task, type TaskPatch } from "../lib/tasks";
import { runOrQueue } from "./offline";
import { useCachedList } from "./useCachedList";
import { SYNCED_EVENT } from "./useSync";

export interface TasksState {
  tasks?: Task[];
  loading: boolean;
  /** Showing the copy saved on this device (server unreachable). Edits still work and sync later. */
  stale: boolean;
  error?: string;
  reload: () => void;
  create: (t: NewTask) => Promise<Task | undefined>;
  update: (id: string, patch: TaskPatch) => Promise<Task | undefined>;
  remove: (id: string) => Promise<boolean>;
}

const fetchAll = () => api.tasks("all");

/** Dispatched after a task is added outside a task list (quick capture). */
export const TASKS_CHANGED_EVENT = "directory:tasks-changed";

/** Server-backed task list that keeps working offline (DECISIONS D-017). */
export function useTasks(online: boolean): TasksState {
  const list = useCachedList<Task>("tasks:last", "task", fetchAll, online);
  const { reload } = list;

  // Server data changed by a replay, or a task was added elsewhere (quick
  // capture): refresh. Offline, this reads the device copy the change went to.
  useEffect(() => {
    window.addEventListener(SYNCED_EVENT, reload);
    window.addEventListener(TASKS_CHANGED_EVENT, reload);
    return () => {
      window.removeEventListener(SYNCED_EVENT, reload);
      window.removeEventListener(TASKS_CHANGED_EVENT, reload);
    };
  }, [reload]);

  const create = useCallback(
    (t: NewTask) => {
      const task = { ...t, id: clientId() };
      return runOrQueue({
        list,
        noun: "task",
        op: { kind: "task.create", task },
        server: () => api.createTask(task),
        fromServer: (cur, created) => [...cur, created],
        applyLocal: applyLocally,
        local: (cur) => cur.find((x) => x.id === task.id)!,
      });
    },
    [list],
  );

  const update = useCallback(
    async (id: string, patch: TaskPatch) => {
      const before = list.latest().find((t) => t.id === id);
      const updated = await runOrQueue({
        list,
        noun: "task",
        op: { kind: "task.update", id, patch },
        server: () => api.updateTask(id, patch),
        fromServer: (cur, u) => cur.map((t) => (t.id === id ? u : t)),
        applyLocal: applyLocally,
        local: (cur) => cur.find((t) => t.id === id)!,
      });
      // Closing a repeating task created its next occurrence on the server (D-047).
      if (updated && before && repeats(before) && isOpen(before) && !isOpen(updated)) reload();
      return updated;
    },
    [list, reload],
  );

  const remove = useCallback(
    async (id: string) =>
      (await runOrQueue({
        list,
        noun: "task",
        op: { kind: "task.delete", id },
        server: async () => {
          await api.deleteTask(id);
          return true;
        },
        fromServer: (cur) => cur.filter((t) => t.id !== id && t.parent_id !== id),
        applyLocal: applyLocally,
        local: () => true,
      })) ?? false,
    [list],
  );

  return { tasks: list.items, loading: list.loading, stale: list.stale, error: list.error, reload: list.reload, create, update, remove };
}
