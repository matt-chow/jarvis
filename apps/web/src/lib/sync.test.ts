import { IDBFactory } from "fake-indexeddb";
import { describe, expect, it } from "vitest";
import { ApiError } from "./api";
import { LocalDb } from "./db";
import { applyInboxLocally, applyLocally, applyProjectLocally, clientId, replay, type Op, type SyncApi } from "./sync";
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
    capture: async (_text, id) => ({ id: (await run(`capture:${id}`)).id, text: "", created_at: 0 }),
    deleteInboxItem: async (id) => void (await run(`discard:${id}`)),
    createProject: async (p) => ({ ...(await run(`project:${p.id}`)), name: p.name, description: "", status: "active" }),
    updateProject: async (id) => ({ ...(await run(`project-update:${id}`)), name: "", description: "", status: "active" }),
    deleteProject: async (id) => void (await run(`project-delete:${id}`)),
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

describe("inbox ops", () => {
  it("replays captures idempotently and applies locally", async () => {
    const db = await outboxWith([{ kind: "inbox.capture", item: { id: "i1", text: "thought" } }, { kind: "inbox.delete", id: "i0" }]);
    const { api, calls } = fakeApi((op) => (op === "capture:i1" ? new ApiError(409, "exists") : op === "discard:i0" ? new ApiError(404, "not_found") : undefined));
    expect(await replay(db, api)).toMatchObject({ applied: 2, rejected: [] });
    expect(calls).toEqual(["capture:i1", "discard:i0"]);
    const items = applyInboxLocally([], { kind: "inbox.capture", item: { id: "i1", text: " thought " } }, 3);
    expect(items).toEqual([{ id: "i1", text: "thought", created_at: 3 }]);
    expect(applyInboxLocally(items, { kind: "inbox.delete", id: "i1" })).toEqual([]);
  });
});

describe("project ops", () => {
  it("replays creates idempotently and applies locally", async () => {
    const db = await outboxWith([
      { kind: "project.create", project: { id: "p1", name: "Garden" } },
      { kind: "project.update", id: "p1", patch: { status: "archived" } },
      { kind: "project.delete", id: "p0" },
    ]);
    const { api, calls } = fakeApi((op) => (op === "project:p1" ? new ApiError(409, "exists") : op === "project-delete:p0" ? new ApiError(404, "not_found") : undefined));
    expect(await replay(db, api)).toMatchObject({ applied: 3, rejected: [] });
    expect(calls).toEqual(["project:p1", "project-update:p1", "project-delete:p0"]);

    let list = applyProjectLocally([], { kind: "project.create", project: { id: "p1", name: " Garden " } }, 4);
    expect(list).toEqual([{ id: "p1", name: "Garden", description: "", status: "active", created_at: 4, updated_at: 4 }]);
    list = applyProjectLocally(list, { kind: "project.update", id: "p1", patch: { status: "archived" } }, 6);
    expect(list[0]).toMatchObject({ status: "archived", updated_at: 6 });
    expect(applyProjectLocally(list, { kind: "project.delete", id: "p1" })).toEqual([]);
  });
});
