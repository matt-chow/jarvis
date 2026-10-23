import { IDBFactory } from "fake-indexeddb";
import { describe, expect, it } from "vitest";
import { LocalDb } from "./db";
import { MAX_HITS, hitRoute, searchLocal, snippet, type LocalDoc, type SearchHit } from "./search";
import { loadCachedData, toLocalDocs } from "./searchCache";

const hit = (over: Partial<SearchHit>): SearchHit => ({ kind: "note", id: "n1", title: "", snippet: "", title_match: false, updated_at: 0, ...over });
const doc = (over: Partial<LocalDoc>): LocalDoc => ({ kind: "task", id: "t1", title: "", body: "", updated_at: 0, ...over });

describe("hitRoute", () => {
  it("opens each kind where it lives", () => {
    expect(hitRoute(hit({}))).toBe("#/notes?note=n1");
    expect(hitRoute(hit({ kind: "event", id: "e1", date: "2026-10-23" }))).toBe("#/calendar?date=2026-10-23");
    expect(hitRoute(hit({ kind: "habit", id: "h1" }))).toBe("#/habits");
    expect(hitRoute(hit({ kind: "journal", id: "2026-10-21" }))).toBe("#/journal?date=2026-10-21");
  });
});

describe("snippet (mirrors core)", () => {
  it("cuts around the match, flattens whitespace, and is unicode-safe", () => {
    const long = `${"a ".repeat(100)}needle ${"b ".repeat(100)}`;
    const s = snippet(long, "NEEDLE");
    expect(s.startsWith("…") && s.endsWith("…") && s.includes("needle")).toBe(true);
    expect(Array.from(s).length).toBeLessThanOrEqual(2 * 60 + 8);
    expect(snippet("Ünïcödé text here", "TEXT")).toBe("Ünïcödé text here");
    expect(snippet("line one\n\nline two", "two")).toBe("line one line two");
  });
});

describe("searchLocal", () => {
  it("ranks title matches first, then the most recent", () => {
    const hits = searchLocal(
      [
        doc({ id: "old-title", title: "Dentist", updated_at: 1 }),
        doc({ id: "new-body", title: "Errands", body: "buy dentist floss", updated_at: 9 }),
        doc({ id: "new-title", title: "Call the DENTIST", updated_at: 5 }),
        doc({ id: "miss", title: "Groceries", updated_at: 10 }),
      ],
      "dentist",
    );
    expect(hits.map((h) => h.id)).toEqual(["new-title", "old-title", "new-body"]);
    expect(hits[2]).toMatchObject({ title_match: false, snippet: "buy dentist floss" });
  });

  it("matches journal days on their body only, and enforces the limits", () => {
    const day = doc({ kind: "journal", id: "2026-10-21", title: "2026-10-21", body: "quiet day", titleSearchable: false });
    expect(searchLocal([day], "2026-10")).toEqual([]);
    expect(searchLocal([day], "quiet")).toHaveLength(1);
    expect(searchLocal([day], "q")).toEqual([]);
    const many = Array.from({ length: 80 }, (_, i) => doc({ id: `t${i}`, title: "match" }));
    expect(searchLocal(many, "match")).toHaveLength(MAX_HITS);
  });
});

describe("offline search over the device cache", () => {
  it("reads lists and opened documents, preferring the fullest copy", async () => {
    const db = await LocalDb.open(new IDBFactory(), "search-test");
    const base = { created_at: 0, updated_at: 5 };
    await db.setValue("tasks:last", [{ id: "t1", parent_id: null, project_id: null, title: "Renew passport", notes: "", status: "todo", priority: "normal", due_date: null, completed_at: null, ...base }]);
    await db.setValue("notes:last", [{ id: "n1", title: "Trip", excerpt: "packing list", pinned: false, ...base }]);
    await db.setValue("note:n1", { id: "n1", title: "", body: "Trip\npacking list\npassport photocopy", pinned: false, ...base });
    await db.setValue("events:last", [{ id: "e1", title: "Flight", start_date: "2026-11-02", end_date: "2026-11-02", start_time: null, end_time: null, location: "Passport control", notes: "", repeat: "none", repeat_until: null, ...base }]);
    await db.setValue("journal:2026-10", [{ date: "2026-10-20", excerpt: "booked flights", mood: null, updated_at: 3 }]);
    await db.setValue("journal:2026-10-21", { date: "2026-10-21", body: "Found my old passport", mood: 4, ...base });
    await db.setValue("conflict:note:n1", { body: "passport conflict copy" });

    const hits = searchLocal(toLocalDocs(await loadCachedData(db)), "passport");
    expect(hits.map((h) => `${h.kind}:${h.id}`).sort()).toEqual(["event:e1", "journal:2026-10-21", "note:n1", "task:t1"]);
    expect(hits[0]).toMatchObject({ kind: "task", title_match: true });
    expect(hits.find((h) => h.kind === "event")?.date).toBe("2026-11-02");
    expect(hits.find((h) => h.kind === "note")?.title).toBe("Trip");
    expect(searchLocal(toLocalDocs(await loadCachedData(db)), "booked")).toHaveLength(1);
  });

  it("is empty when nothing is cached", async () => {
    const db = await LocalDb.open(new IDBFactory(), "empty-search");
    expect(searchLocal(toLocalDocs(await loadCachedData(db)), "anything")).toEqual([]);
  });
});
