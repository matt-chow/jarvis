/** Dashboard widgets backed by module hooks (each keeps its own offline cache). */
import { useMemo } from "react";
import { Panel } from "../components/ui";
import { useEvents } from "../hooks/useEvents";
import { useHabits } from "../hooks/useHabits";
import { useNotes } from "../hooks/useNotes";
import { useTasks } from "../hooks/useTasks";
import { occurrences, timeLabel } from "../lib/calendar";
import { formatDate, formatTime } from "../lib/format";
import { weekday } from "../lib/habits";
import { isOpen, localDate } from "../lib/tasks";

// Paper "Desktop · Home" cards: round task checks, square habit marks, mono times.

const link = "font-mono text-[11px] text-accent underline-offset-4 hover:underline";

export function TodayWidget({ online, className }: { online: boolean; className?: string }) {
  const today = localDate();
  const { events } = useEvents(online);
  const tasks = useTasks(online);
  const occ = useMemo(() => occurrences(events ?? [], today, today), [events, today]);
  const due = useMemo(
    () =>
      (tasks.tasks ?? [])
        .filter((t) => isOpen(t) && t.due_date && t.due_date <= today)
        .sort((a, b) => (a.due_date ?? "").localeCompare(b.due_date ?? "")),
    [tasks.tasks, today],
  );
  const loading = events === undefined && tasks.tasks === undefined;

  return (
    <Panel
      title="today"
      className={className}
      actions={
        <a href={`#/calendar?date=${today}`} className={link}>
          calendar →
        </a>
      }
    >
      {loading ? (
        <p className="text-sm text-muted">Loading…</p>
      ) : occ.length === 0 && due.length === 0 ? (
        <p className="text-sm text-muted">Nothing scheduled or due today.</p>
      ) : (
        <ul className="flex flex-col gap-1.5">
          {occ.map((o) => (
            <li key={`${o.event.id}${o.start_date}`} className="flex min-h-9 items-center gap-3 text-[15px]">
              <span className="w-24 shrink-0 font-mono text-[11px] text-muted">{timeLabel(o.event)}</span>
              <span className="truncate">{o.event.title}</span>
            </li>
          ))}
          {due.map((t) => (
            <li key={t.id} className="flex min-h-9 items-center gap-3 text-[15px]">
              <button
                type="button"
                aria-label={`Complete “${t.title}”`}
                onClick={() => void tasks.update(t.id, { status: "done" })}
                className="grid size-[18px] shrink-0 place-items-center rounded-full border-[1.5px] border-muted transition hover:border-accent hover:bg-accent/10"
              />
              <a href={`#/tasks?task=${t.id}`} className="min-w-0 truncate hover:underline">
                {t.title}
              </a>
              <span className={`ml-auto shrink-0 font-mono text-[11px] ${t.due_date! < today ? "text-bad" : "text-muted"}`}>
                {t.due_date! < today ? `overdue · ${formatDate(Date.parse(`${t.due_date}T00:00:00`))}` : "due today"}
              </span>
            </li>
          ))}
        </ul>
      )}
    </Panel>
  );
}

export function HabitsWidget({ online, className }: { online: boolean; className?: string }) {
  const today = localDate();
  const state = useHabits(online);
  const todays = (state.habits ?? []).filter((h) => !h.archived && h.days.includes(weekday(today)));
  const left = todays.filter((h) => !h.checkins.includes(today)).length;

  return (
    <Panel
      title={`habits${state.habits ? ` · ${todays.length - left}/${todays.length}` : ""}`}
      className={className}
      actions={
        <a href="#/habits" className={link}>
          habits →
        </a>
      }
    >
      {!state.habits ? (
        <p className="text-sm text-muted">{state.loading ? "Loading…" : "Habits couldn't be loaded."}</p>
      ) : todays.length === 0 ? (
        <p className="text-sm text-muted">No habits scheduled today.</p>
      ) : (
        <ul className="flex flex-col gap-1.5">
          {todays.map((h) => {
            const done = h.checkins.includes(today);
            return (
              <li key={h.id}>
                <button
                  type="button"
                  aria-pressed={done}
                  onClick={() => void state.check(h.id, today, !done)}
                  className="flex min-h-9 w-full items-center gap-3 rounded-sm px-1 text-left text-[15px] hover:bg-sunken/60"
                >
                  <span
                    aria-hidden
                    className={`grid size-[22px] shrink-0 place-items-center rounded-[6px] border font-mono text-[11px] transition ${done ? "border-accent bg-accent text-accent-ink" : "border-muted/50 bg-panel"}`}
                  >
                    {done ? "✓" : ""}
                  </span>
                  <span className={`truncate ${done ? "text-muted line-through" : ""}`}>{h.name}</span>
                  {h.streak > 1 && <span className="ml-auto font-mono text-[11px] text-muted">{h.streak} days</span>}
                </button>
              </li>
            );
          })}
        </ul>
      )}
      {state.habits && todays.length > 0 && left === 0 && <p className="mt-3 text-sm text-muted">All done for today.</p>}
    </Panel>
  );
}

export function NotesWidget({ online, className }: { online: boolean; className?: string }) {
  const state = useNotes(online, false);
  const recent = useMemo(() => [...(state.notes ?? [])].sort((a, b) => b.updated_at - a.updated_at).slice(0, 5), [state.notes]);
  return (
    <Panel
      title="recent notes"
      className={className}
      actions={
        <a href="#/notes" className={link}>
          notes →
        </a>
      }
    >
      {!state.notes ? (
        <p className="text-sm text-muted">{state.loading ? "Loading…" : "Notes couldn't be loaded."}</p>
      ) : recent.length === 0 ? (
        <p className="text-sm text-muted">No notes yet.</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {recent.map((n) => (
            <li key={n.id}>
              <a href={`#/notes?note=${n.id}`} className="flex min-h-9 items-center gap-3 rounded-sm px-1 text-[15px] hover:bg-sunken/60">
                <span className="min-w-0 truncate">{n.title}</span>
                <span className="ml-auto shrink-0 font-mono text-[11px] text-muted">
                  {new Date(n.updated_at).toDateString() === new Date().toDateString() ? formatTime(n.updated_at) : formatDate(n.updated_at)}
                </span>
              </a>
            </li>
          ))}
        </ul>
      )}
    </Panel>
  );
}
