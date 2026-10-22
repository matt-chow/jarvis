import { useCallback } from "react";
import { api } from "../lib/api";
import type { NewTask, Task, TaskPatch } from "../lib/tasks";
import { useCachedList } from "./useCachedList";

export interface TasksState {
  tasks?: Task[];
  loading: boolean;
  /** Showing the last list saved on this device; edits are disabled. */
  stale: boolean;
  error?: string;
  reload: () => void;
  create: (t: NewTask) => Promise<Task | undefined>;
  update: (id: string, patch: TaskPatch) => Promise<Task | undefined>;
  remove: (id: string) => Promise<boolean>;
}

const fetchAll = () => api.tasks("all");

/** Server-backed task list with a read-only offline copy in IndexedDB. */
export function useTasks(online: boolean): TasksState {
  const list = useCachedList<Task>("tasks:last", "task", fetchAll, online);
  const { mutate } = list;

  const create = useCallback((t: NewTask) => mutate(() => api.createTask(t), (cur, created) => [...cur, created]), [mutate]);

  const update = useCallback(
    (id: string, patch: TaskPatch) => mutate(() => api.updateTask(id, patch), (cur, u) => cur.map((t) => (t.id === id ? u : t))),
    [mutate],
  );

  const remove = useCallback(
    async (id: string) =>
      (await mutate(
        async () => {
          await api.deleteTask(id);
          return true;
        },
        (cur) => cur.filter((t) => t.id !== id && t.parent_id !== id),
      )) ?? false,
    [mutate],
  );

  return { tasks: list.items, loading: list.loading, stale: list.stale, error: list.error, reload: list.reload, create, update, remove };
}
