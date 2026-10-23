import { IDBFactory } from "fake-indexeddb";
import { describe, expect, it } from "vitest";
import { LocalDb, deleteLocalData, type Note } from "./db";

const note = (id: string, updatedAt: number): Note => ({ id, body: `note ${id}`, createdAt: 0, updatedAt });

describe("LocalDb", () => {
  it("stores, orders, and deletes notes", async () => {
    const db = await LocalDb.open(new IDBFactory(), "test");
    await db.putNote(note("a", 1));
    await db.putNote(note("b", 3));
    await db.putNote(note("c", 2));
    expect((await db.listNotes()).map((n) => n.id)).toEqual(["b", "c", "a"]);

    await db.putNote({ ...note("a", 9), body: "edited" });
    const [first] = await db.listNotes();
    expect(first).toMatchObject({ id: "a", body: "edited" });

    await db.deleteNote("a");
    expect(await db.listNotes()).toHaveLength(2);
    db.close();
  });

  it("persists across connections", async () => {
    const factory = new IDBFactory();
    const first = await LocalDb.open(factory, "persist");
    await first.setValue("health:last", { status: "ok" });
    first.close();

    const second = await LocalDb.open(factory, "persist");
    expect(await second.getValue("health:last")).toEqual({ status: "ok" });
    expect(await second.getValue("missing")).toBeUndefined();
    second.close();
  });

  it("completes a database something else created empty", async () => {
    const factory = new IDBFactory();
    await new Promise<void>((res) => {
      const r = factory.open("empty", 1); // no stores
      r.onsuccess = () => {
        r.result.close();
        res();
      };
    });
    const db = await LocalDb.open(factory, "empty");
    await db.setValue("session:last", { ok: true });
    expect(await db.getValue("session:last")).toEqual({ ok: true });
    expect(await db.outbox()).toEqual([]);
    db.close();
  });

  it("forgets everything on request", async () => {
    const factory = new IDBFactory();
    const db = await LocalDb.open(factory, "forget");
    await db.setValue("tasks:last", [1]);
    await db.enqueue({ kind: "x" });
    db.close();
    await deleteLocalData(factory, "forget");
    const fresh = await LocalDb.open(factory, "forget");
    expect(await fresh.getValue("tasks:last")).toBeUndefined();
    expect(await fresh.outbox()).toHaveLength(0);
    fresh.close();
  });

  it("keeps tracks saved for offline playback", async () => {
    const db = await LocalDb.open(new IDBFactory(), "media");
    await db.putMedia({ id: "t1", blob: new Blob(["abc"]), size: 3, saved_at: 1 });
    expect(await db.mediaSizes()).toEqual(new Map([["t1", 3]]));
    expect((await db.getMedia("t1"))?.size).toBe(3);
    await db.deleteMedia("t1");
    expect(await db.getMedia("t1")).toBeUndefined();
    db.close();
  });
});
