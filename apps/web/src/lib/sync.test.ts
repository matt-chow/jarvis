import { IDBFactory } from "fake-indexeddb";
import { describe, expect, it } from "vitest";
import { ApiError } from "./api";
import { LocalDb } from "./db";
import { applyLocally, clientId, replay, type Op, type SyncApi } from "./sync";
import type { Task } from "./tasks";

const task = (id: string, over: Partial<Task> = {}): Task => ({
  id,
  parent_id: null,
  project_id: null,
  title: "T",
  notes: "",
  status: "todo",
  priority: "none",
  due_date: null,
  created_at: 0,
  updated_at: 0,
  completed_at: null,
  ...over,
});

function fakeApi(fail: (op: string) => unknown = () => undefined) {
  const calls: string[] = [];
  const run = async (name: string) => {
    calls.push(name);
    const err = fail(name);
    if (err) throw err;
    return task("x");
  };
  const api: SyncApi = {
    createTask: (t) => run(`create:${t.id}`),
    updateTask: (id) => run(`update:${id}`),
    deleteTask: async (id) => void (await run(`delete:${id}`)),
  };
  return { api, calls };
}

async function outboxWith(ops: Op[]) {
  const db = await LocalDb.open(new IDBFactory(), `sync-${Math.random()}`);
  for (const op of ops) await db.enqueue(op);
  return db;
}

const create = (id: string): Op => ({ kind: "task.create", task: { id, title: `task ${id}` } });

describe("replay", () => {
  it("applies in order and empties the outbox", async () => {
    const db = await outboxWith([create("a"), { kind: "task.update", id: "a", patch: { status: "done" } }, { kind: "task.delete", id: "b" }]);
    const { api, calls } = fakeApi();
    const r = await replay(db, api);
    expect(calls).toEqual(["create:a", "update:a", "delete:b"]);
    expect(r).toMatchObject({ applied: 3, rejected: [], pending: 0 });
    expect(await db.outbox()).toHaveLength(0);
  });

  it("treats replayed creates and deletes as already applied", async () => {
    const db = await outboxWith([create("a"), { kind: "task.delete", id: "gone" }]);
    const { api } = fakeApi((op) => (op === "create:a" ? new ApiError(409, "exists") : op.startsWith("delete") ? new ApiError(404, "not_found") : undefined));
    expect(await replay(db, api)).toMatchObject({ applied: 2, rejected: [] });
  });

  it("stops (keeping the rest queued) when offline or the server fails", async () => {
    const db = await outboxWith([create("a"), create("b")]);
    const { api } = fakeApi((op) => (op === "create:b" ? new TypeError("Failed to fetch") : undefined));
    const r = await replay(db, api);
    expect(r).toMatchObject({ applied: 1, stopped: "offline", pending: 1 });
    expect(await db.outbox()).toHaveLength(1);

    const { api: down } = fakeApi(() => new ApiError(502, "backend_unavailable"));
    expect((await replay(db, down)).stopped).toBe("server");
    expect(await db.outbox()).toHaveLength(1);
  });

  it("drops changes the server rejects, and reports them", async () => {
    const db = await outboxWith([create("a"), create("b")]);
    const { api } = fakeApi((op) => (op === "create:a" ? new ApiError(400, "invalid_input", "title: too long") : undefined));
    const r = await replay(db, api);
    expect(r.applied).toBe(1);
    expect(r.rejected[0].reason).toBe("Couldn't create “task a”: title: too long");
    expect(await db.outbox()).toHaveLength(0);
  });
});

describe("applyLocally", () => {
  it("creates, completes, and deletes with subtasks", () => {
    let list = applyLocally([], create("p"), 5);
    list = applyLocally(list, { kind: "task.create", task: { id: "c", title: "child", parent_id: "p" } }, 5);
    list = applyLocally(list, { kind: "task.update", id: "p", patch: { status: "done" } }, 9);
    expect(list.find((t) => t.id === "p")).toMatchObject({ status: "done", completed_at: 9, updated_at: 9 });
    list = applyLocally(list, { kind: "task.update", id: "p", patch: { status: "todo" } }, 10);
    expect(list.find((t) => t.id === "p")?.completed_at).toBeNull();
    expect(applyLocally(list, { kind: "task.delete", id: "p" })).toHaveLength(0);
  });

  it("generates server-compatible ids", () => {
    expect(clientId()).toMatch(/^[0-9a-f]{32}$/);
  });
});
