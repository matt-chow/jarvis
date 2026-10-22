import { useEffect, useMemo, useState, type FormEvent } from "react";
import type { ModuleProps } from "../App";
import { Button, Panel, inputClass } from "../components/ui";
import { useEvents, type EventsState } from "../hooks/useEvents";
import { useHashParam } from "../hooks/useHashParam";
import { useTasks } from "../hooks/useTasks";
import { REPEAT_LABELS, covers, monthGrid, occurrences, timeLabel, type CalendarEvent, type Occurrence, type Repeat } from "../lib/calendar";
import { longDate, monthLabel, monthOf, shiftMonth } from "../lib/journal";
import { isOpen, localDate, type Task } from "../lib/tasks";

// Provisional screen: awaiting design integration (see docs/MODULES.md › Design status).

const small = "min-h-7 px-2 text-[10px]";
const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
/** Items listed in a month cell before "+N". */
const CELL_ITEMS = 3;

export function Calendar({ online }: ModuleProps) {
  const today = localDate();
  const state = useEvents(online);
  const { tasks } = useTasks(online);
  const [selected, setSelected] = useState(today);
  const [month, setMonth] = useState(monthOf(today));
  const offline = state.stale || !online;

  // Deep link (#/calendar?date=YYYY-MM-DD).
  const deepDate = useHashParam("date");
  useEffect(() => {
    if (deepDate && /^\d{4}-\d{2}-\d{2}$/.test(deepDate)) {
      setSelected(deepDate);
      setMonth(monthOf(deepDate));
    }
  }, [deepDate]);

  const grid = useMemo(() => monthGrid(month), [month]);
  const from = grid[0][0];
  const to = grid.at(-1)!.at(-1)!;
  const occ = useMemo(() => occurrences(state.events ?? [], from, to), [state.events, from, to]);
  const due = useMemo(() => (tasks ?? []).filter((t) => t.due_date && isOpen(t) && t.due_date >= from && t.due_date <= to), [tasks, from, to]);

  const go = (d: string) => {
    setSelected(d);
    setMonth(monthOf(d));
  };

  return (
    <div className="grid items-start gap-5 lg:grid-cols-[minmax(0,1fr)_22rem]">
      <Panel
        title="calendar"
        actions={
          <Button className={small} onClick={() => go(today)} disabled={selected === today && month === monthOf(today)}>
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
        {offline && state.events && (
          <p className="mb-3 rounded-sm border border-line bg-sunken/60 px-3 py-2 text-sm text-muted">
            Offline: changes are saved on this device and sync when you reconnect.
          </p>
        )}
        {state.error && !offline && (
          <p role="alert" className="mb-3 text-sm text-bad">
            {state.error}
          </p>
        )}
        {state.loading && !state.events ? (
          <p className="text-sm text-muted">Loading calendar…</p>
        ) : !state.events ? (
          <div className="text-sm">
            <p className="text-muted">The calendar couldn't be loaded.</p>
            <Button className={`${small} mt-2`} onClick={state.reload}>
              retry
            </Button>
          </div>
        ) : (
          <div aria-label={monthLabel(month)} className="grid grid-cols-7 gap-px overflow-hidden rounded-sm border border-line bg-line">
            {WEEKDAYS.map((d) => (
              <div key={d} aria-hidden="true" className="bg-panel px-1 py-1 text-center font-mono text-[10px] text-muted uppercase">
                {d}
              </div>
            ))}
            {grid.flat().map((d) => {
              const items = [
                ...occ.filter((o) => covers(o, d)).map((o) => ({ key: `e${o.event.id}${o.start_date}`, label: o.event.title, task: false })),
                ...due.filter((t) => t.due_date === d).map((t) => ({ key: `t${t.id}`, label: t.title, task: true })),
              ];
              const inMonth = monthOf(d) === month;
              return (
                <button
                  key={d}
                  type="button"
                  aria-pressed={d === selected}
                  aria-current={d === today ? "date" : undefined}
                  aria-label={`${longDate(d)}${items.length ? `, ${items.length} item${items.length === 1 ? "" : "s"}` : ""}`}
                  onClick={() => setSelected(d)}
                  className={`flex min-h-16 flex-col items-stretch gap-0.5 bg-panel p-1 text-left sm:min-h-20 ${d === selected ? "outline-2 -outline-offset-2 outline-accent" : "hover:bg-sunken/60"} ${inMonth ? "" : "opacity-50"}`}
                >
                  <span className={`font-mono text-[11px] ${d === today ? "font-bold text-accent" : "text-muted"}`}>{Number(d.slice(8))}</span>
                  {items.slice(0, CELL_ITEMS).map((it) => (
                    <span key={it.key} className={`truncate rounded-sm px-1 text-[10px] leading-4 ${it.task ? "border border-line text-muted" : "bg-accent/15 text-ink"}`}>
                      {it.task ? "☐ " : ""}
                      {it.label}
                    </span>
                  ))}
                  {items.length > CELL_ITEMS && <span className="px-1 font-mono text-[10px] text-muted">+{items.length - CELL_ITEMS}</span>}
                </button>
              );
            })}
          </div>
        )}
      </Panel>

      <DayPanel
        key={selected}
        date={selected}
        occurrences={occurrences(state.events ?? [], selected, selected)}
        tasks={(tasks ?? []).filter((t) => t.due_date === selected && isOpen(t))}
        state={state}
      />
    </div>
  );
}

function DayPanel(props: { date: string; occurrences: Occurrence[]; tasks: Task[]; state: EventsState }) {
  const { date, state } = props;
  const [editing, setEditing] = useState<CalendarEvent | "new">();

  return (
    <Panel
      title="day"
      actions={
        !editing && (
          <Button className={small} variant="primary" disabled={!state.events} onClick={() => setEditing("new")}>
            + event
          </Button>
        )
      }
    >
      <h2 className="mb-3 text-base font-semibold">{longDate(date)}</h2>
      {editing ? (
        <EventForm key={editing === "new" ? "new" : editing.id} date={date} event={editing === "new" ? undefined : editing} state={state} onDone={() => setEditing(undefined)} />
      ) : props.occurrences.length === 0 && props.tasks.length === 0 ? (
        <p className="text-sm text-muted">Nothing scheduled.</p>
      ) : (
        <ul className="flex flex-col gap-2">
          {props.occurrences.map((o) => (
            <li key={`${o.event.id}${o.start_date}`}>
              <button type="button" onClick={() => setEditing(o.event)} className="w-full rounded-sm px-2 py-1.5 text-left hover:bg-sunken/60" aria-label={`Edit “${o.event.title}”`}>
                <span className="block text-sm">{o.event.title}</span>
                <span className="font-mono text-[11px] text-muted">
                  {timeLabel(o.event)}
                  {o.start_date !== o.end_date && ` · ${o.start_date.slice(5)} → ${o.end_date.slice(5)}`}
                  {o.event.repeat !== "none" && ` · ${REPEAT_LABELS[o.event.repeat].toLowerCase()}`}
                  {o.event.location && ` · ${o.event.location}`}
                </span>
              </button>
            </li>
          ))}
          {props.tasks.map((t) => (
            <li key={t.id}>
              <a href={`#/tasks?task=${t.id}`} className="block rounded-sm px-2 py-1.5 hover:bg-sunken/60">
                <span className="block text-sm">☐ {t.title}</span>
                <span className="font-mono text-[11px] text-muted">task due</span>
              </a>
            </li>
          ))}
        </ul>
      )}
    </Panel>
  );
}

function EventForm({ date, event, state, onDone }: { date: string; event?: CalendarEvent; state: EventsState; onDone: () => void }) {
  const [title, setTitle] = useState(event?.title ?? "");
  const [start, setStart] = useState(event?.start_date ?? date);
  const [end, setEnd] = useState(event?.end_date ?? date);
  const [allDay, setAllDay] = useState(event ? !event.start_time : true);
  const [startTime, setStartTime] = useState(event?.start_time ?? "09:00");
  const [endTime, setEndTime] = useState(event?.end_time ?? "");
  const [location, setLocation] = useState(event?.location ?? "");
  const [notes, setNotes] = useState(event?.notes ?? "");
  const [repeat, setRepeat] = useState<Repeat>(event?.repeat ?? "none");
  const [until, setUntil] = useState(event?.repeat_until ?? "");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [busy, setBusy] = useState(false);

  const invalid = !title.trim() || end < start || (!allDay && !!endTime && start === end && endTime < startTime);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (invalid) return;
    const fields = {
      title,
      start_date: start,
      end_date: end,
      start_time: allDay ? null : startTime,
      end_time: allDay || !endTime ? null : endTime,
      location,
      notes,
      repeat,
      repeat_until: repeat === "none" || !until ? null : until,
    };
    setBusy(true);
    const ok = event ? await state.update(event.id, fields) : await state.create(fields);
    setBusy(false);
    if (ok) onDone();
  };

  return (
    <form onSubmit={(e) => void submit(e)} className="flex flex-col gap-2.5">
      <label className="flex flex-col gap-1">
        <span className="label">title</span>
        <input value={title} onChange={(e) => setTitle(e.target.value)} maxLength={200} required aria-label="Event title" className={inputClass} />
      </label>
      <div className="grid grid-cols-2 gap-2">
        <label className="flex flex-col gap-1">
          <span className="label">starts</span>
          <input
            type="date"
            value={start}
            onChange={(e) => {
              setStart(e.target.value);
              if (end < e.target.value) setEnd(e.target.value);
            }}
            required
            className={inputClass}
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="label">ends</span>
          <input type="date" value={end} min={start} onChange={(e) => setEnd(e.target.value)} required className={inputClass} />
        </label>
      </div>
      <label className="flex items-center gap-2 text-sm">
        <input type="checkbox" checked={allDay} onChange={(e) => setAllDay(e.target.checked)} />
        all day
      </label>
      {!allDay && (
        <div className="grid grid-cols-2 gap-2">
          <label className="flex flex-col gap-1">
            <span className="label">from</span>
            <input type="time" value={startTime} onChange={(e) => setStartTime(e.target.value)} required aria-label="Start time" className={inputClass} />
          </label>
          <label className="flex flex-col gap-1">
            <span className="label">to</span>
            <input type="time" value={endTime} onChange={(e) => setEndTime(e.target.value)} aria-label="End time" className={inputClass} />
          </label>
        </div>
      )}
      <div className="grid grid-cols-2 gap-2">
        <label className="flex flex-col gap-1">
          <span className="label">repeat</span>
          <select value={repeat} onChange={(e) => setRepeat(e.target.value as Repeat)} aria-label="Repeat" className={inputClass}>
            {(Object.keys(REPEAT_LABELS) as Repeat[]).map((r) => (
              <option key={r} value={r}>
                {REPEAT_LABELS[r]}
              </option>
            ))}
          </select>
        </label>
        {repeat !== "none" && (
          <label className="flex flex-col gap-1">
            <span className="label">until</span>
            <input type="date" value={until} min={start} onChange={(e) => setUntil(e.target.value)} aria-label="Repeat until" className={inputClass} />
          </label>
        )}
      </div>
      <label className="flex flex-col gap-1">
        <span className="label">location</span>
        <input value={location} onChange={(e) => setLocation(e.target.value)} maxLength={200} className={inputClass} />
      </label>
      <label className="flex flex-col gap-1">
        <span className="label">notes</span>
        <textarea value={notes} onChange={(e) => setNotes(e.target.value)} maxLength={5000} rows={3} className={`${inputClass} resize-y py-2`} />
      </label>
      {state.error && (
        <p role="alert" className="text-sm text-bad">
          {state.error}
        </p>
      )}
      <div className="flex flex-wrap items-center gap-1.5 pt-1">
        <Button type="submit" variant="primary" className={small} disabled={busy || invalid}>
          {event ? "save" : "add event"}
        </Button>
        <Button className={small} onClick={onDone}>
          cancel
        </Button>
        {event && (
          <span className="ml-auto flex items-center gap-1.5">
            {confirmDelete ? (
              <>
                {event.repeat !== "none" && <span className="text-xs text-muted">Deletes every occurrence.</span>}
                <Button className={small} onClick={() => setConfirmDelete(false)}>
                  keep
                </Button>
                <Button
                  variant="danger"
                  className={small}
                  onClick={() => {
                    void state.remove(event.id).then((ok) => ok && onDone());
                  }}
                >
                  confirm delete
                </Button>
              </>
            ) : (
              <Button variant="danger" className={small} onClick={() => setConfirmDelete(true)}>
                delete
              </Button>
            )}
          </span>
        )}
      </div>
    </form>
  );
}
