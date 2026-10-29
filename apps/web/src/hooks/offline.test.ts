import { IDBFactory } from "fake-indexeddb";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { newBlock, type TimeBlock } from "../lib/blocks";
import { applyBlockLocally, type Op } from "../lib/sync";
import type { CachedList } from "./useCachedList";

/** A list like `useCachedList` returns, without React. */
function fakeList(initial: TimeBlock[]) {
  let items = initial;
  const errors: (string | undefined)[] = [];
  let reloads = 0;
  const list = {
    items,
    loading: false,
    stale: false,
    reload: () => void reloads++,
    mutate: async () => undefined,
    latest: () => items,
    replace: (next: TimeBlock[]) => void (items = next),
    setError: (m: string | undefined) => void errors.push(m),
  } as unknown as CachedList<TimeBlock>;
  return { list, errors, items: () => items, reloads: () => reloads };
}

const block = newBlock("b1", { task_id: "t1", date: "2026-11-02", start_time: "09:00", end_time: "10:00" }, 1);
const move: Op = { kind: "block.update", id: "b1", patch: { start_time: "11:00", end_time: "12:00" } };

describe("runOptimistic (D-047): edits apply at once and roll back if refused", () => {
  beforeEach(() => {
    vi.resetModules();
    vi.stubGlobal("indexedDB", new IDBFactory());
    vi.stubGlobal("navigator", { onLine: true });
    vi.stubGlobal("window", new EventTarget());
  });
  afterEach(() => vi.unstubAllGlobals());

  const run = async (server: () => Promise<TimeBlock>, f: ReturnType<typeof fakeList>) => {
    const { runOptimistic, upsert } = await import("./offline");
    let seenBeforeServer: TimeBlock | undefined;
    const r = await runOptimistic({
      list: f.list,
      noun: "session",
      op: move,
      server: () => {
        seenBeforeServer = f.items()[0];
        return server();
      },
      fromServer: upsert,
      applyLocal: applyBlockLocally,
      local: (cur) => cur[0],
    });
    return { r, seenBeforeServer };
  };

  it("shows the change before the server answers, then keeps the server's copy", async () => {
    const f = fakeList([block]);
    const saved = { ...block, start_time: "11:00", end_time: "12:00", updated_at: 99 };
    const { r, seenBeforeServer } = await run(async () => saved, f);
    expect(seenBeforeServer?.start_time).toBe("11:00");
    expect(r).toEqual(saved);
    expect(f.items()).toEqual([saved]);
  });

  it("restores the previous list and explains when the server refuses", async () => {
    const f = fakeList([block]);
    const { r } = await run(async () => {
      const { ApiError } = await import("../lib/api"); // the copy offline.ts sees after resetModules
      throw new ApiError(400, "invalid_input", "end_time: must be after start_time");
    }, f);
    expect(r).toBeUndefined();
    expect(f.items()).toEqual([block]);
    expect(f.errors.at(-1)).toBe("Check end_time — must be after start_time.");
  });

  it("reloads instead of restoring when something else changed meanwhile", async () => {
    const f = fakeList([block]);
    const other = newBlock("b2", { title: "Focus", date: "2026-11-02", start_time: "13:00", end_time: "14:00" }, 2);
    await run(async () => {
      f.list.replace([...f.items(), other]); // another edit landed first
      const { ApiError } = await import("../lib/api");
      throw new ApiError(404, "not_found");
    }, f);
    expect(f.reloads()).toBe(1);
  });

  it("queues the change and keeps it applied when the network is down", async () => {
    const f = fakeList([block]);
    const { r } = await run(async () => {
      throw new TypeError("Failed to fetch");
    }, f);
    expect(r?.start_time).toBe("11:00");
    expect(f.items()[0].start_time).toBe("11:00");
    const { localDb } = await import("../lib/db");
    expect((await (await localDb()).outbox()).map((e) => e.op)).toEqual([move]);
  });
});
