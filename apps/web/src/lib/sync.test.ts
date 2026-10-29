import { IDBFactory } from "fake-indexeddb";
import { describe, expect, it } from "vitest";
import { ApiError } from "./api";
import { LocalDb } from "./db";
import { isLocalLink, localLinkId } from "./links";
import { applyBlockLocally, applyEventLocally, applyHabitLocally, applyInboxLocally, applyLocally, applyProjectLocally, clientId, replay, type Op, type SyncApi } from "./sync";
import type { Note } from "./notes";
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

const note = (id: string, updated_at: number): Note => ({ id, title: "", body: "", pinned: false, created_at: 0, updated_at });

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
    createNote: async (n) => {
      await run(`note:${n.id}`);
      return note(n.id, 1);
    },
    // Saves answer with server version 50 (notes) or 60 (journal).
    updateNote: async (id, patch) => {
      await run(`note-save:${id}@${patch.expected_updated_at}${patch.pinned === undefined ? "" : ` pinned=${patch.pinned}`}`);
      return note(id, 50);
    },
    deleteNote: async (id) => void (await run(`note-delete:${id}`)),
    link: async (from, to) => ({ ...(await run(`link:${from.kind}:${from.id}>${to.kind}:${to.id}`)), link_id: "l1", kind: to.kind, id: to.id, title: "" }),
    unlink: async (id) => void (await run(`unlink:${id}`)),
    triage: async (id, to, date) => ({ kind: "note", note: { ...(await run(`triage:${id}:${to}${date ? `:${date}` : ""}`)), id: "n", title: "", body: "", pinned: false } }),
    deleteJournal: async (date) => void (await run(`journal-delete:${date}`)),
    createHabit: async (h) => ({ ...(await run(`habit:${h.id}`)), name: h.name, days: [], archived: false }),
    updateHabit: async (id) => ({ ...(await run(`habit-update:${id}`)), name: "", days: [], archived: false }),
    deleteHabit: async (id) => void (await run(`habit-delete:${id}`)),
    checkHabit: async (id, date, done) => void (await run(`habit-check:${id}:${date}:${done}`)),
    createEvent: async (e) => {
      await run(`event:${e.id}`);
      return applyEventLocally([], { kind: "event.create", event: e })[0];
    },
    updateEvent: async (id) => {
      await run(`event-update:${id}`);
      return applyEventLocally([], { kind: "event.create", event: { id, title: "", start_date: "2026-01-01" } })[0];
    },
    deleteEvent: async (id) => void (await run(`event-delete:${id}`)),
    splitEvent: async (id, split) => {
      await run(`event-split:${id}:${split.scope}:${split.date}${split.replacement ? `+${split.replacement.id}` : ""}`);
      const series = applyEventLocally([], { kind: "event.create", event: { id, title: "", start_date: "2026-01-01" } })[0];
      return { series, replacement: null };
    },
    createBlock: async (b) => {
      await run(`block:${b.id}`);
      return applyBlockLocally([], { kind: "block.create", block: b })[0];
    },
    updateBlock: async (id) => {
      await run(`block-update:${id}`);
      return applyBlockLocally([], { kind: "block.create", block: { id, title: "x", date: "2026-01-01", start_time: "09:00", end_time: "10:00" } })[0];
    },
    deleteBlock: async (id) => void (await run(`block-delete:${id}`)),
    setProgress: async (fileId) => {
      await run(`progress:${fileId}`);
      return { file_id: fileId, status: "reading", current_page: 1, total_pages: 2, position: null, percent: 50, started_at: 0, finished_at: null, updated_at: 0 };
    },
    updatePreferences: async (patch) => {
      await run(`prefs:${JSON.stringify(patch)}`);
      return {};
    },
    putJournal: async (date, e) => {
      await run(`journal:${date}@${e.expected_updated_at}`);
      return { date, body: "", mood: null, created_at: 0, updated_at: 60 };
    },
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

describe("inbox triage offline (D-036)", () => {
  it("files a capture made offline after the capture replays; a 404 means already filed", async () => {
    const db = await outboxWith([
      { kind: "inbox.capture", item: { id: "a1", text: "call the bank" } },
      { kind: "inbox.triage", id: "a1", to: "task" },
      { kind: "inbox.triage", id: "b2", to: "journal", date: "2026-10-22" },
    ]);
    const { api, calls } = fakeApi((op) => (op.startsWith("triage:b2") ? new ApiError(404, "not_found") : undefined));
    const r = await replay(db, api);
    expect(calls).toEqual(["capture:a1", "triage:a1:task", "triage:b2:journal:2026-10-22"]);
    expect(r).toMatchObject({ applied: 3, rejected: [], pending: 0 });
  });

  it("removes the item from the local inbox", () => {
    const items = [{ id: "a1", text: "x", created_at: 1 }, { id: "b2", text: "y", created_at: 2 }];
    expect(applyInboxLocally(items, { kind: "inbox.triage", id: "a1", to: "note" }).map((i) => i.id)).toEqual(["b2"]);
  });
});

describe("links offline (D-037)", () => {
  const note = { kind: "note" as const, id: "n1" };
  const task = { kind: "task" as const, id: "t1" };

  it("replays link and unlink; an unlink of a link already gone is done", async () => {
    const db = await outboxWith([
      { kind: "link.create", from: note, to: task },
      { kind: "link.delete", link_id: "old" },
    ]);
    const { api, calls } = fakeApi((op) => (op === "unlink:old" ? new ApiError(404, "not_found") : undefined));
    const r = await replay(db, api);
    expect(calls).toEqual(["link:note:n1>task:t1", "unlink:old"]);
    expect(r).toMatchObject({ applied: 2, rejected: [], pending: 0 });
  });

  it("removing a link made offline cancels its queued create", async () => {
    const db = await outboxWith([
      { kind: "link.create", from: note, to: task },
      { kind: "link.create", from: note, to: { kind: "project", id: "p1" } },
    ]);
    const dropped = await db.dropWhere((op) => (op as Op).kind === "link.create" && (op as { to: { id: string } }).to.id === "t1");
    expect(dropped).toBe(1);
    expect((await db.outbox()).map((e) => (e.op as { to: { id: string } }).to.id)).toEqual(["p1"]);
    expect(localLinkId(note, task)).toBe("local:note:n1|task:t1");
    expect(isLocalLink(localLinkId(note, task)) && !isLocalLink("abc")).toBe(true);
  });
});

describe("document saves (D-018)", () => {
  const save = (id: string, body: string, base?: number): Op => ({ kind: "note.save", id, fields: { title: "", body }, base });

  it("coalesces edits into one queued save that keeps the original base version", async () => {
    const db = await outboxWith([]);
    for (const body of ["a", "ab", "abc"]) {
      await db.enqueueMerged("note:n1", (queued) => ({ ...save("n1", body, 7), base: queued ? (queued as { base?: number }).base : 7 }));
    }
    const entries = await db.outbox();
    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({ doc: "note:n1", rev: 2, op: { fields: { body: "abc" }, base: 7 } });
    expect(await db.pendingFor("note:n1")).toHaveLength(1);
  });

  it("sends the base version, reports new versions, and turns conflicts into records", async () => {
    const theirs = note("n2", 99);
    const db = await outboxWith([
      save("n1", "mine", 7),
      { kind: "journal.save", date: "2026-10-22", fields: { body: "offline day", mood: 3 }, base: 0 },
      save("n2", "stale edit", 8),
    ]);
    const { api, calls } = fakeApi((op) => (op.startsWith("note-save:n2") ? new ApiError(409, "conflict", undefined, { error: "conflict", note: theirs }) : undefined));
    const r = await replay(db, api);
    expect(calls).toEqual(["note-save:n1@7", "journal:2026-10-22@0", "note-save:n2@8"]);
    expect(r).toMatchObject({ applied: 2, rejected: [], pending: 0, versions: { "note:n1": 50, "journal:2026-10-22": 60 } });
    expect(r.conflicts).toEqual([{ doc: "note:n2", mine: { title: "", body: "stale edit" }, theirs }]);
    expect(await db.outbox()).toHaveLength(0);
  });

  it("replays a pin made offline with the text, as one versioned save (D-034)", async () => {
    const db = await outboxWith([]);
    await db.enqueueMerged("note:n1", () => save("n1", "draft", 7));
    await db.enqueueMerged("note:n1", (q) => ({ kind: "note.save", id: "n1", fields: { title: "", body: "draft", pinned: true }, base: (q as { base?: number }).base }));
    const { api, calls } = fakeApi(() => undefined);
    await replay(db, api);
    expect(calls).toEqual(["note-save:n1@7 pinned=true"]);
  });

  it("deletes a journal day offline without a later rewrite replaying before the delete (D-035)", async () => {
    const day = (body: string, base?: number): Op => ({ kind: "journal.save", date: "2026-10-20", fields: { body, mood: null }, base });
    const db = await outboxWith([]);
    await db.enqueueMerged("journal:2026-10-20", () => day("first draft", 5));
    // Delete: the queued save is dropped, then the delete is queued.
    await db.dropMerged("journal:2026-10-20");
    await db.enqueue({ kind: "journal.delete", date: "2026-10-20" } satisfies Op, "journal:2026-10-20");
    // Writing the day again starts a new create-only save after the delete.
    await db.enqueueMerged("journal:2026-10-20", (q) => ({ ...day("fresh start", 0), base: q ? (q as { base?: number }).base : 0 }));
    const { api, calls } = fakeApi((op) => (op === "journal-delete:2026-10-20" ? new ApiError(404, "not_found") : undefined));
    const r = await replay(db, api);
    expect(calls).toEqual(["journal-delete:2026-10-20", "journal:2026-10-20@0"]);
    expect(r).toMatchObject({ applied: 2, rejected: [], pending: 0 });
  });

  it("keeps an edit made while its save was in flight, rebased on the version just written", async () => {
    const db = await outboxWith([]);
    await db.enqueueMerged("note:n1", () => save("n1", "first", 7));
    const { api } = fakeApi((op) => {
      // The user keeps typing while the request is out.
      if (op.startsWith("note-save")) void db.enqueueMerged("note:n1", (q) => ({ ...save("n1", "second"), base: (q as { base?: number }).base }));
      return undefined;
    });
    await replay(db, api);
    const left = await db.outbox();
    expect(left).toHaveLength(1);
    expect(left[0].op).toMatchObject({ fields: { body: "second" }, base: 50 });
  });
});

describe("habit ops", () => {
  it("replays idempotently and applies locally with streaks", async () => {
    const db = await outboxWith([
      { kind: "habit.create", habit: { id: "h1", name: "Stretch" } },
      { kind: "habit.check", id: "h1", date: "2026-10-22", done: true },
      { kind: "habit.check", id: "gone", date: "2026-10-22", done: true },
    ]);
    const { api, calls } = fakeApi((op) => (op === "habit:h1" ? new ApiError(409, "exists") : op.startsWith("habit-check:gone") ? new ApiError(404, "not_found") : undefined));
    expect(await replay(db, api)).toMatchObject({ applied: 3, rejected: [] });
    expect(calls).toEqual(["habit:h1", "habit-check:h1:2026-10-22:true", "habit-check:gone:2026-10-22:true"]);

    const today = "2026-10-22";
    let list = applyHabitLocally([], { kind: "habit.create", habit: { id: "h1", name: " Stretch ", days: [4, 0, 0] } }, today, 5);
    expect(list[0]).toMatchObject({ name: "Stretch", days: [0, 4], checkins: [], streak: 0 });
    list = applyHabitLocally(list, { kind: "habit.check", id: "h1", date: today, done: true }, today);
    expect(list[0]).toMatchObject({ checkins: [today], streak: 1 });
    expect(applyHabitLocally(list, { kind: "habit.delete", id: "h1" }, today)).toEqual([]);
  });
});

describe("event ops", () => {
  it("replays idempotently and applies locally like the server", async () => {
    const db = await outboxWith([
      { kind: "event.create", event: { id: "e1", title: "Trip", start_date: "2026-11-01", end_date: "2026-11-03" } },
      { kind: "event.delete", id: "gone" },
    ]);
    const { api, calls } = fakeApi((op) => (op === "event:e1" ? new ApiError(409, "exists") : op === "event-delete:gone" ? new ApiError(404, "not_found") : undefined));
    expect(await replay(db, api)).toMatchObject({ applied: 2, rejected: [] });
    expect(calls).toEqual(["event:e1", "event-delete:gone"]);

    let list = applyEventLocally([], { kind: "event.create", event: { id: "e1", title: " Trip ", start_date: "2026-11-01", end_date: "2026-11-03", end_time: "10:00" } }, 1);
    expect(list[0]).toMatchObject({ title: "Trip", end_time: null, repeat: "none" });
    list = applyEventLocally(list, { kind: "event.update", id: "e1", patch: { start_date: "2026-11-10" } }, 2);
    expect(list[0]).toMatchObject({ start_date: "2026-11-10", end_date: "2026-11-12" });
    expect(applyEventLocally(list, { kind: "event.delete", id: "e1" })).toEqual([]);
  });
});

describe("planner ops (D-047)", () => {
  it("replays block and split ops idempotently", async () => {
    const db = await outboxWith([
      { kind: "block.create", block: { id: "b1", task_id: "t1", date: "2026-11-02", start_time: "09:00", end_time: "10:00" } },
      { kind: "block.update", id: "b1", patch: { status: "done", actual_minutes: 50 } },
      { kind: "event.split", id: "s1", split: { date: "2026-11-09", scope: "one", replacement: { id: "r1", title: "Class", start_date: "2026-11-10" } } },
      { kind: "event.split", id: "gone", split: { date: "2026-11-09", scope: "one" } },
      { kind: "block.delete", id: "b0" },
    ]);
    const { api, calls } = fakeApi((op) =>
      op === "block:b1" || op.startsWith("event-split:s1")
        ? new ApiError(409, "exists")
        : op.startsWith("event-split:gone") || op === "block-delete:b0"
          ? new ApiError(404, "not_found")
          : undefined,
    );
    expect(await replay(db, api)).toMatchObject({ applied: 5, rejected: [] });
    expect(calls).toEqual(["block:b1", "block-update:b1", "event-split:s1:one:2026-11-09+r1", "event-split:gone:one:2026-11-09", "block-delete:b0"]);
  });

  it("applies blocks locally; deleting a task drops its sessions", () => {
    let blocks = applyBlockLocally([], { kind: "block.create", block: { id: "b1", task_id: "t1", title: " ", date: "2026-11-02", start_time: "09:00", end_time: "10:00" } }, 1);
    blocks = applyBlockLocally(blocks, { kind: "block.create", block: { id: "b1", task_id: "t1", date: "2026-11-02", start_time: "09:00", end_time: "10:00" } }, 1);
    expect(blocks).toHaveLength(1);
    expect(blocks[0]).toMatchObject({ title: "", fixed: false, status: "planned", actual_minutes: null });
    blocks = applyBlockLocally(blocks, { kind: "block.update", id: "b1", patch: { start_time: "10:00", end_time: "11:30" } }, 2);
    expect(blocks[0]).toMatchObject({ start_time: "10:00", end_time: "11:30", updated_at: 2 });
    expect(applyBlockLocally(blocks, { kind: "task.delete", id: "t1" })).toEqual([]);
  });

  it("splits a series locally like the server", () => {
    let list = applyEventLocally([], { kind: "event.create", event: { id: "s", title: "Class", start_date: "2026-11-02", repeat: "weekly", repeat_count: 6 } }, 1);
    expect(list[0]).toMatchObject({ repeat_until: "2026-12-07", fixed: true, skip_dates: [] });
    list = applyEventLocally(list, { kind: "event.split", id: "s", split: { date: "2026-11-09", scope: "one", replacement: { id: "r", title: "Class", start_date: "2026-11-10" } } }, 2);
    expect(list.map((e) => e.id)).toEqual(["s", "r"]);
    expect(list[0].skip_dates).toEqual(["2026-11-09"]);
    list = applyEventLocally(list, { kind: "event.split", id: "s", split: { date: "2026-11-30", scope: "following" } }, 3);
    expect(list[0]).toMatchObject({ repeat_until: "2026-11-29", repeat_count: null });
    // A one-off has no repeat fields.
    list = applyEventLocally(list, { kind: "event.update", id: "s", patch: { repeat: "none" } }, 4);
    expect(list[0]).toMatchObject({ repeat_until: null, skip_dates: [], repeat_interval: 1 });
  });
});
