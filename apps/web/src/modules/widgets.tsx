/** Dashboard widgets backed by module hooks (each keeps its own offline cache). */
import { useMemo } from "react";
import { SeekBar, Transport, usePlayer, usePlayerTime } from "../components/Player";
import { SnoozeMenu } from "../components/SnoozeMenu";
import { Turntable } from "../components/Turntable";
import { Panel } from "../components/ui";
import { useBudget } from "../hooks/useBudget";
import { useEvents } from "../hooks/useEvents";
import { useHabits } from "../hooks/useHabits";
import { useNotes } from "../hooks/useNotes";
import { useReminders } from "../hooks/useReminders";
import { useTasks } from "../hooks/useTasks";
import { budgetLines, formatMoney, monthOf, monthRange, totals } from "../lib/budget";
import { occurrences, timeLabel } from "../lib/calendar";
import { formatDate, formatTime } from "../lib/format";
import { weekday } from "../lib/habits";
import { trackTitle } from "../lib/music";
import { dueLabel, isOverdue, upcoming } from "../lib/reminders";
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

export function RemindersWidget({ online, className }: { online: boolean; className?: string }) {
  const state = useReminders(online);
  const now = Date.now();
  const today = localDate();
  const { overdue, soon } = useMemo(() => upcoming(state.reminders ?? [], now, today), [state.reminders, now, today]);
  const shown = [...overdue, ...soon].slice(0, 6);

  return (
    <Panel
      title={`reminders${state.reminders && overdue.length ? ` · ${overdue.length} overdue` : ""}`}
      className={className}
      actions={
        <a href="#/reminders" className={link}>
          reminders →
        </a>
      }
    >
      {!state.reminders ? (
        <p className="text-sm text-muted">{state.loading ? "Loading…" : "Reminders couldn't be loaded."}</p>
      ) : shown.length === 0 ? (
        <p className="text-sm text-muted">Nothing overdue or due this week.</p>
      ) : (
        <ul aria-label="Upcoming reminders" className="flex flex-col gap-1.5">
          {shown.map((r) => {
            const late = isOverdue(r, now, today);
            return (
              <li key={r.id} className="flex min-h-9 items-center gap-3 text-[15px]">
                <button
                  type="button"
                  aria-label={`Complete “${r.title}”`}
                  onClick={() => void state.complete(r)}
                  className="grid size-[18px] shrink-0 place-items-center rounded-full border-[1.5px] border-muted transition hover:border-accent hover:bg-accent/10"
                />
                <a href={`#/reminders?reminder=${r.id}`} className="min-w-0 truncate hover:underline">
                  {r.flagged && <span className="mr-1 text-warn">⚑</span>}
                  {r.title}
                </a>
                <span className={`ml-auto shrink-0 font-mono text-[11px] ${late ? "text-bad" : "text-muted"}`}>
                  {late ? `overdue · ${dueLabel(r, today)}` : dueLabel(r, today)}
                </span>
                <SnoozeMenu reminder={r} disabled={false} onSnooze={(until) => void state.snooze(r, until)} />
              </li>
            );
          })}
        </ul>
      )}
      {state.error && <p className="mt-2 text-sm text-bad">{state.error}</p>}
    </Panel>
  );
}

/** This month at a glance; the Budget screen has the detail. */
export function BudgetWidget({ online, className }: { online: boolean; className?: string }) {
  const month = monthOf(localDate());
  const state = useBudget(online);
  const data = state.data;
  const t = useMemo(() => (data ? totals(data.transactions, monthRange(month)) : undefined), [data, month]);
  const over = useMemo(() => (data ? budgetLines(data, month).filter((l) => l.left < 0) : []), [data, month]);
  const money = (n: number, sign = false) => formatMoney(n, data?.currency ?? "CAD", { sign });

  return (
    <Panel
      title="budget · this month"
      className={className}
      actions={
        <a href="#/budget" className={link}>
          budget →
        </a>
      }
    >
      {!data || !t ? (
        <p className="text-sm text-muted">{state.loading ? "Loading…" : "The budget couldn't be loaded."}</p>
      ) : data.accounts.length === 0 ? (
        <p className="text-sm text-muted">
          No accounts yet.{" "}
          <a href="#/budget" className="text-accent underline-offset-4 hover:underline">
            Set up your budget
          </a>
          .
        </p>
      ) : (
        <>
          <dl className="grid grid-cols-3 gap-3">
            {(
              [
                ["Spent", money(t.spending), ""],
                ["Income", money(t.income), ""],
                ["Net", money(t.net, true), t.net < 0 ? "text-bad" : "text-ok"],
              ] as const
            ).map(([label, value, tone]) => (
              <div key={label} className="min-w-0">
                <dt className="text-xs text-muted">{label}</dt>
                <dd className={`truncate font-mono text-[15px] ${tone}`}>{value}</dd>
              </div>
            ))}
          </dl>
          <p className="mt-3 text-sm text-muted">
            {t.count === 0 ? "Nothing recorded this month yet." : over.length > 0 ? `Over budget in ${over.map((l) => l.category.name).join(", ")}.` : `${t.count} ${t.count === 1 ? "entry" : "entries"} this month.`}
          </p>
          <a href="#/budget?add=expense" className={`${link} mt-2 inline-block`}>
            add an expense →
          </a>
        </>
      )}
      {state.stale && <p className="mt-2 text-xs text-muted">Showing the copy saved on this device.</p>}
    </Panel>
  );
}

/** The mini record player: tied to the same audio as the full turntable. */
export function MusicWidget({ className }: { className?: string }) {
  const player = usePlayer();
  const time = usePlayerTime();
  const current = player.current;
  const title = current ? trackTitle(current.name) : "";
  const progress = Number.isFinite(time.length) && time.length > 0 ? time.at / time.length : 0;

  return (
    <Panel
      title="music"
      className={className}
      actions={
        <a href="#/music" className={link}>
          turntable →
        </a>
      }
    >
      <div className="flex items-center gap-4">
        <div className="shrink-0">
          <Turntable playing={player.playing} progress={progress} label={title || "idle"} size={112} />
        </div>
        <div className="flex min-w-0 flex-1 flex-col gap-2">
          {current ? (
            <>
              <p className="truncate text-[15px]">{title}</p>
              <Transport player={player} />
              <SeekBar player={player} compact />
            </>
          ) : (
            <p className="text-sm text-muted">
              Nothing playing.{" "}
              <a href="#/music" className="text-accent underline-offset-4 hover:underline">
                Choose a record
              </a>
              .
            </p>
          )}
        </div>
      </div>
    </Panel>
  );
}
