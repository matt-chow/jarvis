/** Collects the copies cached on this device into searchable documents (offline search). */
import type { CalendarEvent } from "./calendar";
import type { LocalDb } from "./db";
import type { HabitView } from "./habits";
import type { JournalEntry, JournalSummary } from "./journal";
import { summarize, type Note, type NoteSummary } from "./notes";
import type { Project } from "./projects";
import type { LocalDoc } from "./search";
import type { Task } from "./tasks";

export interface CachedData {
  tasks?: Task[];
  projects?: Project[];
  notes?: NoteSummary[];
  /** Opened notes (`note:<id>`): full bodies. */
  noteDocs?: Note[];
  events?: CalendarEvent[];
  habits?: HabitView[];
  /** Journal days (`journal:<date>`): full bodies. */
  journalDays?: JournalEntry[];
  /** Journal month lists (`journal:<YYYY-MM>`): excerpts. */
  journalMonths?: JournalSummary[];
}

/** One document per entity; the fullest cached copy wins. */
export function toLocalDocs(c: CachedData): LocalDoc[] {
  const docs: LocalDoc[] = [];
  for (const t of c.tasks ?? []) docs.push({ kind: "task", id: t.id, title: t.title, body: t.notes, updated_at: t.updated_at });
  for (const p of c.projects ?? []) docs.push({ kind: "project", id: p.id, title: p.name, body: p.description, updated_at: p.updated_at });

  const notes = new Map<string, LocalDoc>();
  for (const n of c.notes ?? []) notes.set(n.id, { kind: "note", id: n.id, title: n.title, body: n.excerpt, updated_at: n.updated_at });
  for (const n of c.noteDocs ?? []) {
    const listed = notes.get(n.id);
    // A summary newer than the opened copy is the better title; the body is still the fullest text we have.
    const title = listed && listed.updated_at > n.updated_at ? listed.title : summarize(n).title;
    notes.set(n.id, { kind: "note", id: n.id, title, body: n.body, updated_at: Math.max(n.updated_at, listed?.updated_at ?? 0) });
  }
  docs.push(...notes.values());

  for (const e of c.events ?? [])
    docs.push({ kind: "event", id: e.id, title: e.title, body: `${e.location} ${e.notes}`.trim(), updated_at: e.updated_at, date: e.start_date });
  for (const h of c.habits ?? []) docs.push({ kind: "habit", id: h.id, title: h.name, body: "", updated_at: h.updated_at });

  const days = new Map<string, LocalDoc>();
  const day = (date: string, body: string, updated_at: number): LocalDoc => ({ kind: "journal", id: date, title: date, body, updated_at, titleSearchable: false });
  for (const s of c.journalMonths ?? []) days.set(s.date, day(s.date, s.excerpt, s.updated_at));
  for (const e of c.journalDays ?? []) days.set(e.date, day(e.date, e.body, e.updated_at));
  docs.push(...days.values());
  return docs;
}

const JOURNAL_DAY = /^journal:\d{4}-\d{2}-\d{2}$/;
const JOURNAL_MONTH = /^journal:\d{4}-\d{2}$/;

/** Read every cached list and document from IndexedDB. */
export async function loadCachedData(db: LocalDb): Promise<CachedData> {
  const [tasks, projects, notes, events, habits, noteEntries, journalEntries] = await Promise.all([
    db.getValue<Task[]>("tasks:last"),
    db.getValue<Project[]>("projects:last"),
    db.getValue<NoteSummary[]>("notes:last"),
    db.getValue<CalendarEvent[]>("events:last"),
    db.getValue<HabitView[]>("habits:last"),
    db.entriesWithPrefix<Note | null>("note:"),
    db.entriesWithPrefix<unknown>("journal:"),
  ]);
  const journalDays: JournalEntry[] = [];
  const journalMonths: JournalSummary[] = [];
  for (const [key, value] of journalEntries) {
    if (JOURNAL_DAY.test(key) && value && typeof value === "object" && "body" in value) journalDays.push(value as JournalEntry);
    else if (JOURNAL_MONTH.test(key) && Array.isArray(value)) journalMonths.push(...(value as JournalSummary[]));
  }
  return {
    tasks,
    projects,
    notes,
    noteDocs: noteEntries.map(([, n]) => n).filter((n): n is Note => !!n && typeof n.body === "string"),
    events,
    habits,
    journalDays,
    journalMonths,
  };
}
