import { useMemo, useState, type FormEvent } from "react";
import type { ModuleProps } from "../App";
import { Icon } from "../components/icons";
import { Button, Panel, inputClass } from "../components/ui";
import { useHabits, type HabitsState } from "../hooks/useHabits";
import { EVERY_DAY, MILESTONES, WEEKDAY_LETTERS, WEEKDAY_NAMES, completion, dayState, describeDays, gridWeeks, lastDays, weekday, type DayState, type HabitView } from "../lib/habits";
import { longDate } from "../lib/journal";
import { playUiSound } from "../lib/sounds";
import { localDate } from "../lib/tasks";

// Habits (D-050): a week of check-ins to tap, and a contribution grid whose
// days read as done, missed, rest, or still open, so a schedule is respected
// (an unscheduled day is rest, never a miss). Celebrations stay quiet.

const small = "min-h-8 px-3 text-xs";
/** Days shown in each habit's check-in row. */
const SHOWN_DAYS = 7;
/** Weeks in each habit's grid (fits a phone). */
const GRID_WEEKS = 20;

const CELL: Record<DayState, string> = {
  done: "bg-accent",
  missed: "border border-bad/70",
  rest: "bg-sunken",
  today: "border border-dashed border-accent",
  future: "",
  before: "",
};
const STATE_WORD: Record<DayState, string> = { done: "done", missed: "missed", rest: "rest day", today: "today, not done yet", future: "", before: "" };

export function Habits({ online }: ModuleProps) {
  const state = useHabits(online);
  const [showArchived, setShowArchived] = useState(false);
  const today = localDate();
  const days = useMemo(() => lastDays(today, SHOWN_DAYS), [today]);
  // Everything works offline: changes apply here and sync later (D-017).
  const offline = state.stale || !online;
  const active = (state.habits ?? []).filter((h) => !h.archived);
  const archived = (state.habits ?? []).filter((h) => h.archived);
  const doneToday = active.filter((h) => h.checkins.includes(today)).length;
  const due = active.filter((h) => h.days.includes(weekday(today)));
  const allDone = due.length > 0 && due.every((h) => h.checkins.includes(today));

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-5">
      <Panel title={`habits${state.habits ? ` · ${doneToday}/${active.length} today` : ""}`}>
        {offline && state.habits && (
          <p className="mb-3 rounded-sm border border-line bg-sunken/60 px-3 py-2 text-sm text-muted">
            Offline: changes are saved on this device and sync when you reconnect.
          </p>
        )}
        <NewHabitForm disabled={!state.habits} onCreate={async (h) => (await state.create(h)) !== undefined} />
        {state.error && !offline && (
          <p role="alert" className="mt-3 text-sm text-bad">
            {state.error}
          </p>
        )}
        {state.loading && !state.habits ? (
          <p className="mt-4 text-sm text-muted">Loading habits…</p>
        ) : !state.habits ? (
          <div className="mt-4 text-sm">
            <p className="text-muted">Habits couldn't be loaded.</p>
            <Button className={`${small} mt-2`} onClick={state.reload}>
              Try again
            </Button>
          </div>
        ) : active.length === 0 ? (
          <p className="mt-4 text-sm text-muted">No habits yet. Add one, pick its days, and check it off each day.</p>
        ) : (
          <>
            <p role="status" className="mt-4 flex min-h-5 items-center gap-2 text-sm text-ok">
              {allDone && (
                <>
                  <Icon name="check" size={14} /> All done for today.
                </>
              )}
            </p>
            <ul className="mt-2 divide-y divide-line">
              {active.map((h) => (
                <HabitRow key={h.id} habit={h} days={days} today={today} state={state} />
              ))}
            </ul>
            <Legend />
          </>
        )}
        {archived.length > 0 && (
          <div className="mt-4 border-t border-line pt-3">
            <button type="button" onClick={() => setShowArchived((v) => !v)} className="label hover:text-ink" aria-expanded={showArchived}>
              {showArchived ? "▾" : "▸"} Archived · {archived.length}
            </button>
            {showArchived && (
              <ul className="mt-2 flex flex-col gap-1">
                {archived.map((h) => (
                  <li key={h.id} className="flex items-center justify-between gap-2 text-sm">
                    <span className="truncate text-muted">{h.name}</span>
                    <Button className={small} onClick={() => void state.update(h.id, { archived: false })}>
                      Restore
                    </Button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}
      </Panel>
    </div>
  );
}

function DayPicker({ value, onChange, disabled }: { value: number[]; onChange: (days: number[]) => void; disabled?: boolean }) {
  const toggle = (d: number) => onChange(value.includes(d) ? value.filter((x) => x !== d) : [...value, d].sort());
  return (
    <div role="group" aria-label="Days" className="flex gap-1">
      {WEEKDAY_LETTERS.map((letter, d) => (
        <button
          key={d}
          type="button"
          aria-pressed={value.includes(d)}
          aria-label={WEEKDAY_NAMES[d]}
          title={WEEKDAY_NAMES[d]}
          disabled={disabled}
          onClick={() => toggle(d)}
          className={`grid size-7 place-items-center rounded-sm border font-mono text-[11px] ${
            value.includes(d) ? "border-accent bg-accent text-accent-ink" : "border-line text-muted hover:border-accent"
          } disabled:opacity-50`}
        >
          {letter}
        </button>
      ))}
    </div>
  );
}

function NewHabitForm({ disabled, onCreate }: { disabled: boolean; onCreate: (h: { name: string; days: number[] }) => Promise<boolean> }) {
  const [name, setName] = useState("");
  const [days, setDays] = useState<number[]>(EVERY_DAY);
  const [busy, setBusy] = useState(false);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim() || days.length === 0) return;
    setBusy(true);
    if (await onCreate({ name, days })) {
      setName("");
      setDays(EVERY_DAY);
    }
    setBusy(false);
  };
  return (
    <form onSubmit={(e) => void submit(e)} className="flex flex-col gap-2 sm:flex-row sm:items-center">
      <input
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="New habit…"
        aria-label="New habit"
        maxLength={120}
        disabled={disabled || busy}
        className={`${inputClass} min-w-0 flex-1`}
      />
      <DayPicker value={days} onChange={setDays} disabled={disabled || busy} />
      <Button type="submit" variant="primary" disabled={disabled || busy || !name.trim() || days.length === 0}>
        Add
      </Button>
    </form>
  );
}

function HabitRow({ habit, days, today, state }: { habit: HabitView; days: string[]; today: string; state: HabitsState }) {
  const [editing, setEditing] = useState(false);
  const checked = new Set(habit.checkins);

  return (
    <li className="py-3">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <button type="button" className="mr-auto min-w-0 text-left" onClick={() => setEditing((v) => !v)} aria-expanded={editing}>
          <span className="block truncate text-sm">{habit.name}</span>
          <span className="font-mono text-[11px] text-muted">
            {describeDays(habit.days)} · {habit.streak > 0 ? `${habit.streak}-day streak` : "no streak yet"}
            {MILESTONES[habit.streak] && <span className="text-accent"> · {MILESTONES[habit.streak]}</span>}
          </span>
        </button>
        <div className="flex gap-1">
          {days.map((d) => {
            const done = checked.has(d);
            const scheduled = habit.days.includes(weekday(d));
            return (
              <button
                key={d}
                type="button"
                aria-pressed={done}
                aria-label={`${habit.name}, ${longDate(d)}${d === today ? " (today)" : ""}`}
                title={`${longDate(d)}${scheduled ? "" : " (not scheduled)"}`}
                onClick={() => {
                  if (!done) playUiSound("done");
                  void state.check(habit.id, d, !done);
                }}
                className={`grid size-8 place-items-center rounded-sm border font-mono text-[11px] ${
                  done
                    ? "border-accent bg-accent text-accent-ink"
                    : scheduled
                      ? "border-line hover:border-accent"
                      : "border-dashed border-line text-muted hover:border-accent"
                } ${d === today ? "ring-1 ring-accent ring-offset-1 ring-offset-panel" : ""}`}
              >
                {WEEKDAY_LETTERS[weekday(d)]}
              </button>
            );
          })}
        </div>
      </div>
      <HabitGrid habit={habit} today={today} />
      {editing && <HabitEditor habit={habit} state={state} onDone={() => setEditing(false)} />}
    </li>
  );
}

/** The last GRID_WEEKS weeks, Monday at the top. A picture; the buttons above are the controls. */
function HabitGrid({ habit, today }: { habit: HabitView; today: string }) {
  const weeks = useMemo(() => gridWeeks(today, GRID_WEEKS), [today]);
  const checked = useMemo(() => new Set(habit.checkins), [habit.checkins]);
  const { done, due } = completion(habit, weeks.flat(), checked, today);
  return (
    <div className="mt-3 flex flex-wrap items-end gap-x-4 gap-y-1">
      <div role="img" aria-label={`${habit.name}, last ${GRID_WEEKS} weeks: ${done} of ${due} scheduled days done`} className="flex gap-[3px]">
        {weeks.map((week) => (
          <div key={week[0]} className="flex flex-col gap-[3px]">
            {week.map((d) => {
              const st = dayState(habit, d, checked, today);
              return <span key={d} title={STATE_WORD[st] ? `${longDate(d)}: ${STATE_WORD[st]}` : undefined} className={`size-[11px] rounded-[2px] ${CELL[st]}`} />;
            })}
          </div>
        ))}
      </div>
      <span className="font-mono text-[11px] text-muted" aria-hidden>
        {due > 0 ? `${done} of ${due} days` : "starts today"}
      </span>
    </div>
  );
}

function Legend() {
  const items: [DayState, string][] = [
    ["done", "Done"],
    ["missed", "Missed"],
    ["rest", "Rest day"],
    ["today", "Today, open"],
  ];
  return (
    <ul aria-label="Grid key" className="mt-3 flex flex-wrap gap-x-4 gap-y-1 border-t border-line pt-3 text-xs text-muted">
      {items.map(([st, word]) => (
        <li key={st} className="flex items-center gap-1.5">
          <span aria-hidden className={`size-[11px] rounded-[2px] ${CELL[st]}`} />
          {word}
        </li>
      ))}
    </ul>
  );
}

function HabitEditor({ habit, state, onDone }: { habit: HabitView; state: HabitsState; onDone: () => void }) {
  const [name, setName] = useState(habit.name);
  const [days, setDays] = useState(habit.days);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const changed = name.trim() !== habit.name || days.join() !== habit.days.join();

  const save = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim() || days.length === 0) return;
    if (changed && (await state.update(habit.id, { name, days }))) onDone();
  };

  return (
    <form onSubmit={(e) => void save(e)} className="mt-3 flex flex-col gap-2 rounded-sm border border-line bg-sunken/40 p-3">
      <label className="flex flex-col gap-1">
        <span className="label">Name</span>
        <input value={name} onChange={(e) => setName(e.target.value)} maxLength={120} className={inputClass} />
      </label>
      <DayPicker value={days} onChange={setDays} />
      <div className="flex flex-wrap items-center gap-1.5 pt-1">
        <Button type="submit" variant="primary" className={small} disabled={!changed || !name.trim() || days.length === 0}>
          Save
        </Button>
        <Button className={small} onClick={() => void state.update(habit.id, { archived: true })}>
          Archive
        </Button>
        <span className="ml-auto flex items-center gap-1.5">
          {confirmDelete ? (
            <>
              <span className="text-xs text-muted">Deletes its history too.</span>
              <Button className={small} onClick={() => setConfirmDelete(false)}>
                Keep
              </Button>
              <Button variant="danger" className={small} onClick={() => void state.remove(habit.id)}>
                Delete it
              </Button>
            </>
          ) : (
            <Button variant="danger" className={small} onClick={() => setConfirmDelete(true)}>
              Delete
            </Button>
          )}
        </span>
      </div>
    </form>
  );
}
