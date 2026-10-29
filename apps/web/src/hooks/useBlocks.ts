import { useCallback, useEffect } from "react";
import { api } from "../lib/api";
import type { BlockPatch, NewBlock, TimeBlock } from "../lib/blocks";
import { applyBlockLocally, clientId } from "../lib/sync";
import { runOptimistic, upsert } from "./offline";
import { useCachedList } from "./useCachedList";
import { SYNCED_EVENT } from "./useSync";

export interface BlocksState {
  blocks?: TimeBlock[];
  loading: boolean;
  /** Showing the copy saved on this device. Changes still work and sync later. */
  stale: boolean;
  error?: string;
  reload: () => void;
  /** `id` re-creates a deleted block (undo). */
  create: (b: NewBlock, id?: string) => Promise<TimeBlock | undefined>;
  update: (id: string, patch: BlockPatch) => Promise<TimeBlock | undefined>;
  remove: (id: string) => Promise<boolean>;
  /** Drop a deleted task's sessions locally (the server deletes them with it). */
  forgetTask: (taskId: string) => void;
}

const fetchAll = () => api.blocks();

/** Every time block, cached for offline use; edits apply at once (D-047). */
export function useBlocks(online: boolean): BlocksState {
  const list = useCachedList<TimeBlock>("blocks:last", "session", fetchAll, online);
  const { reload } = list;

  useEffect(() => {
    window.addEventListener(SYNCED_EVENT, reload);
    return () => window.removeEventListener(SYNCED_EVENT, reload);
  }, [reload]);

  const create = useCallback(
    (b: NewBlock, id = clientId()) => {
      const block = { ...b, id };
      return runOptimistic({
        list,
        noun: "session",
        op: { kind: "block.create", block },
        server: () => api.createBlock(block),
        fromServer: upsert,
        applyLocal: applyBlockLocally,
        local: (cur) => cur.find((x) => x.id === id)!,
      });
    },
    [list],
  );

  const update = useCallback(
    (id: string, patch: BlockPatch) =>
      runOptimistic({
        list,
        noun: "session",
        op: { kind: "block.update", id, patch },
        server: () => api.updateBlock(id, patch),
        fromServer: upsert,
        applyLocal: applyBlockLocally,
        local: (cur) => cur.find((x) => x.id === id)!,
      }),
    [list],
  );

  const remove = useCallback(
    async (id: string) =>
      (await runOptimistic({
        list,
        noun: "session",
        op: { kind: "block.delete", id },
        server: async () => {
          await api.deleteBlock(id);
          return true;
        },
        fromServer: (cur) => cur.filter((x) => x.id !== id),
        applyLocal: applyBlockLocally,
        local: () => true,
      })) ?? false,
    [list],
  );

  const forgetTask = useCallback((taskId: string) => list.replace(list.latest().filter((b) => b.task_id !== taskId)), [list]);

  return { blocks: list.items, loading: list.loading, stale: list.stale, error: list.error, reload: list.reload, create, update, remove, forgetTask };
}
