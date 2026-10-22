import { useCallback, useEffect } from "react";
import { api, isNetworkError } from "../lib/api";
import { localDb } from "../lib/db";
import { applyLocally, clientId, type Op } from "../lib/sync";
import type { NewTask, Task, TaskPatch } from "../lib/tasks";
import { describeError, useCachedList } from "./useCachedList";
import { reportIfUnauthorized } from "./useSession";
import { OUTBOX_EVENT, SYNCED_EVENT } from "./useSync";

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

/**
 * Server-backed task list that keeps working offline: edits made without a
 * connection apply locally and queue in the outbox (DECISIONS D-017).
 */
export function useTasks(online: boolean): TasksState {
  const list = useCachedList<Task>("tasks:last", "task", fetchAll, online);
  const { latest, replace, setError, reload } = list;

  // Server data changed by a replay: refresh from the source of truth.
  useEffect(() => {
    window.addEventListener(SYNCED_EVENT, reload);
    return () => window.removeEventListener(SYNCED_EVENT, reload);
  }, [reload]);

  /** Try the server; queue instead when offline or when earlier changes are still queued (keeps order). */
  const run = useCallback(
    async <R,>(op: Op, server: () => Promise<R>, fromServer: (cur: Task[], r: R) => Task[], local: (cur: Task[]) => R) => {
      setError(undefined);
      const db = await localDb();
      const queued = (await db.outbox()).length > 0;
      if (navigator.onLine && !queued) {
        try {
          const r = await server();
          replace(fromServer(latest(), r));
          return r;
        } catch (e) {
          if (!isNetworkError(e)) {
            reportIfUnauthorized(e);
            setError(describeError(e, "task"));
            return undefined;
          }
        }
      }
      await db.enqueue(op);
      const next = applyLocally(latest(), op);
      replace(next);
      window.dispatchEvent(new Event(OUTBOX_EVENT));
      return local(next);
    },
    [latest, replace, setError],
  );

  const create = useCallback(
    (t: NewTask) => {
      const task = { ...t, id: clientId() };
      return run(
        { kind: "task.create", task },
        () => api.createTask(task),
        (cur, created) => [...cur, created],
        (cur) => cur.find((x) => x.id === task.id)!,
      );
    },
    [run],
  );

  const update = useCallback(
    (id: string, patch: TaskPatch) =>
      run(
        { kind: "task.update", id, patch },
        () => api.updateTask(id, patch),
        (cur, u) => cur.map((t) => (t.id === id ? u : t)),
        (cur) => cur.find((t) => t.id === id)!,
      ),
    [run],
  );

  const remove = useCallback(
    async (id: string) =>
      (await run(
        { kind: "task.delete", id },
        async () => {
          await api.deleteTask(id);
          return true;
        },
        (cur) => cur.filter((t) => t.id !== id && t.parent_id !== id),
        () => true,
      )) ?? false,
    [run],
  );

  return { tasks: list.items, loading: list.loading, stale: list.stale, error: list.error, reload: list.reload, create, update, remove };
}
