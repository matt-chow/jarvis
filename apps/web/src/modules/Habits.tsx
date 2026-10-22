import { useMemo, useState, type FormEvent } from "react";
import type { ModuleProps } from "../App";
import { Button, Panel, inputClass } from "../components/ui";
import { useHabits, type HabitsState } from "../hooks/useHabits";
import { EVERY_DAY, WEEKDAY_LETTERS, WEEKDAY_NAMES, describeDays, lastDays, weekday, type HabitView } from "../lib/habits";
import { longDate } from "../lib/journal";
import { localDate } from "../lib/tasks";

// Provisional screen: awaiting design integration (see docs/MODULES.md › Design status).

const small = "min-h-7 px-2 text-[10px]";
/** Days shown in each habit's check-in row. */
const SHOWN_DAYS = 7;

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
              retry
            </Button>
          </div>
        ) : active.length === 0 ? (
          <p className="mt-4 text-sm text-muted">No habits yet. Add one, pick its days, and check it off each day.</p>
        ) : (
          <ul className="mt-4 divide-y divide-line">
            {active.map((h) => (
              <HabitRow key={h.id} habit={h} days={days} today={today} state={state} />
            ))}
          </ul>
        )}
        {archived.length > 0 && (
          <div className="mt-4 border-t border-line pt-3">
            <button type="button" onClick={() => setShowArchived((v) => !v)} className="label hover:text-ink" aria-expanded={showArchived}>
              {showArchived ? "▾" : "▸"} archived · {archived.length}
            </button>
            {showArchived && (
              <ul className="mt-2 flex flex-col gap-1">
                {archived.map((h) => (
                  <li key={h.id} className="flex items-center justify-between gap-2 text-sm">
                    <span className="truncate text-muted">{h.name}</span>
                    <Button className={small} onClick={() => void state.update(h.id, { archived: false })}>
                      restore
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
        add
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
                onClick={() => void state.check(habit.id, d, !done)}
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
      {editing && <HabitEditor habit={habit} state={state} onDone={() => setEditing(false)} />}
    </li>
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
        <span className="label">name</span>
        <input value={name} onChange={(e) => setName(e.target.value)} maxLength={120} className={inputClass} />
      </label>
      <DayPicker value={days} onChange={setDays} />
      <div className="flex flex-wrap items-center gap-1.5 pt-1">
        <Button type="submit" variant="primary" className={small} disabled={!changed || !name.trim() || days.length === 0}>
          save
        </Button>
        <Button className={small} onClick={() => void state.update(habit.id, { archived: true })}>
          archive
        </Button>
        <span className="ml-auto flex items-center gap-1.5">
          {confirmDelete ? (
            <>
              <span className="text-xs text-muted">Deletes its history too.</span>
              <Button className={small} onClick={() => setConfirmDelete(false)}>
                keep
              </Button>
              <Button variant="danger" className={small} onClick={() => void state.remove(habit.id)}>
                confirm delete
              </Button>
            </>
          ) : (
            <Button variant="danger" className={small} onClick={() => setConfirmDelete(true)}>
              delete
            </Button>
          )}
        </span>
      </div>
    </form>
  );
}
