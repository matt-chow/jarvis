import { useCallback, useEffect } from "react";
import { api } from "../lib/api";
import { applyLocally, clientId } from "../lib/sync";
import type { NewTask, Task, TaskPatch } from "../lib/tasks";
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

/** Server-backed task list that keeps working offline (DECISIONS D-017). */
export function useTasks(online: boolean): TasksState {
  const list = useCachedList<Task>("tasks:last", "task", fetchAll, online);
  const { reload } = list;

  // Server data changed by a replay: refresh from the source of truth.
  useEffect(() => {
    window.addEventListener(SYNCED_EVENT, reload);
    return () => window.removeEventListener(SYNCED_EVENT, reload);
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
    (id: string, patch: TaskPatch) =>
      runOrQueue({
        list,
        noun: "task",
        op: { kind: "task.update", id, patch },
        server: () => api.updateTask(id, patch),
        fromServer: (cur, u) => cur.map((t) => (t.id === id ? u : t)),
        applyLocal: applyLocally,
        local: (cur) => cur.find((t) => t.id === id)!,
      }),
    [list],
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
