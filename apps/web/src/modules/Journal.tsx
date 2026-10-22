import { useCallback, useEffect, useState } from "react";
import type { ModuleProps } from "../App";
import { ConflictBanner } from "../components/ConflictBanner";
import { LinkedSection } from "../components/LinkedSection";
import { Button, Panel, inputClass } from "../components/ui";
import { SAVE_STATUS_TEXT, useAutosaveDoc } from "../hooks/useAutosaveDoc";
import { useCachedList, type CachedList } from "../hooks/useCachedList";
import { SYNCED_EVENT } from "../hooks/useSync";
import { useHashParam } from "../hooks/useHashParam";
import { reportIfUnauthorized } from "../hooks/useSession";
import { api } from "../lib/api";
import {
  MOODS,
  longDate,
  moodMeter,
  monthLabel,
  monthOf,
  monthRange,
  shiftDate,
  shiftMonth,
  type JournalEntry,
  type JournalSummary,
} from "../lib/journal";
import { localDate } from "../lib/tasks";

// Provisional screen: awaiting design integration (see docs/MODULES.md › Design status).

const small = "min-h-7 px-2 text-[10px]";

function dayLabel(date: string): string {
  const [y, m, d] = date.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString([], { weekday: "short", day: "numeric" });
}

export function Journal({ online }: ModuleProps) {
  const today = localDate();
  const [date, setDate] = useState(today);
  const [month, setMonth] = useState(monthOf(today));
  // Bumped after a delete so the editor reloads the (now empty) day.
  const [generation, setGeneration] = useState(0);
  const { from, to } = monthRange(month);
  const fetchMonth = useCallback(() => api.journal(from, to), [from, to]);
  const list = useCachedList<JournalSummary>(`journal:${month}`, "entry", fetchMonth, online);
  // Entries are editable offline: edits stay on this device and sync later (D-018).
  const offline = list.stale || !online;
  const { reload } = list;
  useEffect(() => {
    window.addEventListener(SYNCED_EVENT, reload);
    return () => window.removeEventListener(SYNCED_EVENT, reload);
  }, [reload]);

  const go = (d: string) => {
    setDate(d);
    setMonth(monthOf(d));
  };

  // Deep link (#/journal?date=YYYY-MM-DD).
  const deepDate = useHashParam("date");
  useEffect(() => {
    if (deepDate && /^\d{4}-\d{2}-\d{2}$/.test(deepDate)) {
      setDate(deepDate);
      setMonth(monthOf(deepDate));
    }
  }, [deepDate]);

  return (
    <div className="grid items-start gap-5 md:grid-cols-[17rem_minmax(0,1fr)]">
      <Panel
        title="journal"
        actions={
          <Button className={small} onClick={() => go(today)} disabled={date === today}>
            today
          </Button>
        }
      >
        <div className="mb-2 flex items-center justify-between">
          <button type="button" aria-label="Previous month" onClick={() => setMonth(shiftMonth(month, -1))} className="px-2 font-mono text-muted hover:text-ink">
            ‹
          </button>
          <span className="label">{monthLabel(month)}</span>
          <button type="button" aria-label="Next month" onClick={() => setMonth(shiftMonth(month, 1))} className="px-2 font-mono text-muted hover:text-ink">
            ›
          </button>
        </div>
        {list.loading && !list.items ? (
          <p className="text-sm text-muted">Loading…</p>
        ) : !list.items ? (
          <div className="text-sm">
            <p className="text-muted">The journal couldn't be loaded.</p>
            <Button className={`${small} mt-2`} onClick={list.reload}>
              retry
            </Button>
          </div>
        ) : list.items.length === 0 ? (
          <p className="text-sm text-muted">No entries this month.</p>
        ) : (
          <ul className="-mx-2 flex max-h-80 flex-col gap-0.5 overflow-y-auto md:max-h-[60vh]">
            {list.items.map((e) => (
              <li key={e.date}>
                <button
                  type="button"
                  onClick={() => setDate(e.date)}
                  aria-current={e.date === date}
                  className={`w-full rounded-sm px-2 py-1.5 text-left ${e.date === date ? "bg-sunken" : "hover:bg-sunken/60"}`}
                >
                  <span className="flex items-center justify-between font-mono text-xs">
                    <span>{dayLabel(e.date)}</span>
                    {e.mood && (
                      <span className="text-[10px] tracking-tight text-accent" title={`Mood: ${MOODS[e.mood - 1].label}`}>
                        {moodMeter(e.mood)}
                      </span>
                    )}
                  </span>
                  <span className="block truncate text-sm text-muted">{e.excerpt || "—"}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
        {offline && <p className="mt-3 text-sm text-muted">Offline: entries are saved on this device and sync when you reconnect.</p>}
      </Panel>

      <DayEditor
        key={`${date}:${generation}`}
        date={date}
        month={month}
        today={today}
        list={list}
        offline={offline}
        onGo={go}
        onDeleted={() => setGeneration((g) => g + 1)}
      />
    </div>
  );
}

function DayEditor(props: {
  date: string;
  /** Month shown in the list; only entries in it are mirrored there. */
  month: string;
  today: string;
  list: CachedList<JournalSummary>;
  /** Deleting and links need a connection. */
  offline: boolean;
  onGo: (date: string) => void;
  onDeleted: () => void;
}) {
  const { date, list } = props;
  const editor = useAutosaveDoc<JournalEntry, { body: string; mood: number | null }>({
    cacheKey: `journal:${date}`,
    noun: "entry",
    load: () => api.journalEntry(date),
    fieldsOf: (e) => ({ body: e?.body ?? "", mood: e?.mood ?? null }),
    withFields: (e, f) => ({ ...(e ?? { date, created_at: Date.now(), updated_at: 0 }), ...f }),
    save: (f, expected) => api.putJournal(date, { ...f, expected_updated_at: expected }),
    toOp: (fields, base) => ({ kind: "journal.save", date, fields, base }),
    // A day with no entry is saved create-only, so it can't overwrite one written elsewhere.
    newVersion: 0,
    startEmptyOffline: true,
    conflictOf: (e) => (e.body as { entry?: JournalEntry }).entry,
    onSaved: (saved) => {
      if (monthOf(saved.date) !== props.month) return;
      const summary: JournalSummary = {
        date: saved.date,
        excerpt: saved.body.split(/\s+/).join(" ").trim().slice(0, 160),
        mood: saved.mood,
        updated_at: saved.updated_at,
      };
      void list.mutate(
        async () => summary,
        (cur) => [summary, ...cur.filter((e) => e.date !== saved.date)].sort((a, b) => (a.date < b.date ? 1 : -1)),
      );
    },
  });
  const [confirmDelete, setConfirmDelete] = useState(false);
  const online = !props.offline && !editor.fromCache;
  const exists = Boolean(editor.doc);

  const remove = async () => {
    editor.discard();
    try {
      await api.deleteJournal(date);
      void list.mutate(async () => true, (cur) => cur.filter((e) => e.date !== date));
      props.onDeleted();
    } catch (e) {
      reportIfUnauthorized(e);
    }
  };

  return (
    <Panel
      title="entry"
      actions={
        <span className="flex items-center gap-1">
          <span role="status" className={`mr-2 font-mono text-[11px] ${editor.status === "error" || editor.status === "offline" ? "text-bad" : "text-muted"}`}>
            {editor.doc === undefined ? "" : exists || editor.status !== "saved" ? SAVE_STATUS_TEXT[editor.status] : "new"}
          </span>
          <Button className={small} aria-label="Previous day" onClick={() => props.onGo(shiftDate(date, -1))}>
            ‹
          </Button>
          <Button className={small} aria-label="Next day" onClick={() => props.onGo(shiftDate(date, 1))} disabled={date >= props.today}>
            ›
          </Button>
        </span>
      }
    >
      <h2 className="mb-3 text-lg font-semibold">
        {longDate(date)}
        {date === props.today && <span className="label ml-2">today</span>}
      </h2>
      {editor.loadError ? (
        <p role="alert" className="text-sm text-bad">
          {editor.loadError}
        </p>
      ) : editor.doc === undefined ? (
        <p className="text-sm text-muted">Loading…</p>
      ) : (
        <>
          {editor.conflict && <ConflictBanner at={editor.conflict.updated_at} onTheirs={editor.useTheirs} onMine={editor.keepMine} />}
          <div role="radiogroup" aria-label="Mood" className="mb-3 flex items-center gap-1">
            <span className="label mr-2">mood</span>
            {MOODS.map((m) => (
              <button
                key={m.value}
                type="button"
                role="radio"
                aria-checked={editor.fields.mood === m.value}
                aria-label={m.label}
                title={m.label}
                onClick={() => editor.edit({ mood: editor.fields.mood === m.value ? null : m.value })}
                className={`grid size-8 place-items-center rounded-sm border font-mono text-sm ${
                  editor.fields.mood === m.value ? "border-accent bg-accent text-accent-ink" : "border-line text-muted hover:border-accent"
                } disabled:opacity-50`}
              >
                {m.value}
              </button>
            ))}
          </div>
          <textarea
            value={editor.fields.body}
            onChange={(e) => editor.edit({ body: e.target.value })}
            maxLength={50000}
            aria-label="Journal entry"
            placeholder="How was your day?"
            className={`${inputClass} min-h-[45vh] w-full resize-y py-3 leading-relaxed`}
          />
          {exists && (
            <div className="mt-3">
              <LinkedSection entity={{ kind: "journal", id: date }} disabled={!online} />
            </div>
          )}
          {exists && (
            <div className="mt-3 flex justify-end">
              {confirmDelete ? (
                <span className="flex gap-1.5">
                  <Button className={small} onClick={() => setConfirmDelete(false)}>
                    keep
                  </Button>
                  <Button variant="danger" className={small} onClick={() => void remove()}>
                    confirm delete
                  </Button>
                </span>
              ) : (
                <Button variant="danger" className={small} disabled={!online} onClick={() => setConfirmDelete(true)}>
                  delete entry
                </Button>
              )}
            </div>
          )}
        </>
      )}
    </Panel>
  );
}
