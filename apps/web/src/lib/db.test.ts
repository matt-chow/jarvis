import { IDBFactory } from "fake-indexeddb";
import { describe, expect, it } from "vitest";
import { LocalDb, type Note } from "./db";

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
