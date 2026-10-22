import { useCallback, useEffect, useRef, useState } from "react";
import { api, ApiError, isNetworkError } from "../lib/api";
import { localDb } from "../lib/db";
import type { NewTask, Task, TaskPatch } from "../lib/tasks";
import { reportIfUnauthorized } from "./useSession";

const CACHE_KEY = "tasks:last";

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

function describe(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.code === "invalid_input" && e.detail) return `Check ${e.detail.replace(":", " —")}.`;
    if (e.status === 404) return "That task no longer exists.";
    return `Request failed (${e.code}).`;
  }
  if (isNetworkError(e)) return "Can't reach the server right now.";
  return String(e);
}

/** Server-backed task list with a read-only offline copy in IndexedDB. */
export function useTasks(online: boolean): TasksState {
  const [tasks, setTasks] = useState<Task[]>();
  const [loading, setLoading] = useState(true);
  const [stale, setStale] = useState(false);
  const [error, setError] = useState<string>();
  const latest = useRef<Task[] | undefined>(undefined);

  const commit = useCallback((next: Task[]) => {
    latest.current = next;
    setTasks(next);
    setStale(false);
    void localDb()
      .then((db) => db.setValue(CACHE_KEY, next))
      .catch(() => {});
  }, []);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      commit(await api.tasks("all"));
      setError(undefined);
    } catch (e) {
      reportIfUnauthorized(e);
      const cached = await localDb()
        .then((db) => db.getValue<Task[]>(CACHE_KEY))
        .catch(() => undefined);
      if (cached) {
        setTasks(cached);
        latest.current = cached;
        setStale(true);
      }
      setError(describe(e));
    } finally {
      setLoading(false);
    }
  }, [commit]);

  useEffect(() => {
    void reload();
  }, [reload, online]);

  const mutate = useCallback(async <T,>(fn: () => Promise<T>, apply: (current: Task[], result: T) => Task[]) => {
    setError(undefined);
    try {
      const result = await fn();
      commit(apply(latest.current ?? [], result));
      return result;
    } catch (e) {
      reportIfUnauthorized(e);
      setError(describe(e));
      return undefined;
    }
  }, [commit]);

  const create = useCallback(
    (t: NewTask) => mutate(() => api.createTask(t), (cur, created) => [...cur, created]),
    [mutate],
  );

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

  return { tasks, loading, stale, error, reload: () => void reload(), create, update, remove };
}
