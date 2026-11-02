import { useCallback, useEffect } from "react";
import { api } from "../lib/api";
import { fromListing, withTaskCategories, type CategoryItem, type CategoryPatch, type NewCategory } from "../lib/categories";
import { applyCategoryLocally, clientId, type Op } from "../lib/sync";
import { runOrQueue } from "./offline";
import { useCachedList } from "./useCachedList";
import { SYNCED_EVENT } from "./useSync";

export interface CategoriesState {
  categories?: CategoryItem[];
  loading: boolean;
  /** Showing the copy saved on this device. Changes still work and sync later. */
  stale: boolean;
  error?: string;
  reload: () => void;
  create: (c: NewCategory) => Promise<CategoryItem | undefined>;
  update: (id: string, patch: CategoryPatch) => Promise<CategoryItem | undefined>;
  remove: (id: string) => Promise<boolean>;
  /** Replace a task's categories (idempotent). */
  setTask: (taskId: string, ids: string[]) => Promise<boolean>;
}

const fetchAll = async () => fromListing(await api.categories());
const applyLocal = (cur: CategoryItem[], op: Op) => applyCategoryLocally(cur, op);

/** Shared task and reminder categories (D-049); everything works offline. */
export function useCategories(online: boolean): CategoriesState {
  const list = useCachedList<CategoryItem>("categories:last", "category", fetchAll, online);
  const { reload } = list;

  useEffect(() => {
    window.addEventListener(SYNCED_EVENT, reload);
    return () => window.removeEventListener(SYNCED_EVENT, reload);
  }, [reload]);

  const create = useCallback(
    (c: NewCategory) => {
      const category = { ...c, id: clientId() };
      return runOrQueue({
        list,
        noun: "category",
        op: { kind: "category.create", category },
        server: async (): Promise<CategoryItem> => ({ ...(await api.createCategory(category)), task_ids: [] }),
        fromServer: (cur, created) => applyLocal(cur.filter((x) => x.id !== created.id), { kind: "category.create", category: created }).map((x) => (x.id === created.id ? created : x)),
        applyLocal,
        local: (cur) => cur.find((x) => x.id === category.id)!,
      });
    },
    [list],
  );

  const update = useCallback(
    (id: string, patch: CategoryPatch) =>
      runOrQueue({
        list,
        noun: "category",
        op: { kind: "category.update", id, patch },
        server: async (): Promise<CategoryItem> => {
          const c = await api.updateCategory(id, patch);
          return { ...c, task_ids: list.latest().find((x) => x.id === id)?.task_ids ?? [] };
        },
        fromServer: (cur, u) => applyLocal(cur, { kind: "category.update", id, patch }).map((x) => (x.id === id ? u : x)),
        applyLocal,
        local: (cur) => cur.find((x) => x.id === id)!,
      }),
    [list],
  );

  const remove = useCallback(
    async (id: string) =>
      (await runOrQueue({
        list,
        noun: "category",
        op: { kind: "category.delete", id },
        server: async () => {
          await api.deleteCategory(id);
          return true;
        },
        fromServer: (cur) => cur.filter((x) => x.id !== id),
        applyLocal,
        local: () => true,
      })) ?? false,
    [list],
  );

  const setTask = useCallback(
    async (taskId: string, ids: string[]) =>
      (await runOrQueue({
        list,
        noun: "task",
        op: { kind: "task.categories", task_id: taskId, category_ids: ids },
        server: async () => (await api.setTaskCategories(taskId, ids)).category_ids,
        fromServer: (cur, saved) => withTaskCategories(cur, taskId, saved),
        applyLocal,
        local: () => ids,
      })) !== undefined,
    [list],
  );

  return { categories: list.items, loading: list.loading, stale: list.stale, error: list.error, reload: list.reload, create, update, remove, setTask };
}
