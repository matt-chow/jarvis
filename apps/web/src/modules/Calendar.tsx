import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import type { ModuleProps } from "../App";
import { DayAgenda, EntryForm, ItemDetails, PlannerSettings, ScheduleTask, type EntryKind } from "../components/planner/Details";
import { longDay, shortDate } from "../components/planner/format";
import { MonthView } from "../components/planner/MonthView";
import { TimeGrid } from "../components/planner/TimeGrid";
import { Unscheduled } from "../components/planner/Unscheduled";
import { Button } from "../components/ui";
import { useHashParam } from "../hooks/useHashParam";
import { usePlanner, type Spot } from "../hooks/usePlanner";
import { addDays, monthGrid } from "../lib/calendar";
import { monthLabel, monthOf, shiftMonth } from "../lib/journal";
import { ALL_FILTERS, applyFilters, buildItems, daySummary, unscheduled, weekDates, type CalItem, type Filters, type InboxSort } from "../lib/planner";
import type { Task } from "../lib/tasks";
import { durationLabel, fromMinutes, wallClock } from "../lib/time";

// The unified planner (DECISIONS D-047): month, week, and day views over one
// set of items (events, work sessions, task deadlines), with direct creation,
// drag and keyboard rescheduling, an unscheduled-task inbox, suggestions,
// recurrence, filters, and undo. Provisional layout (D-013): no Paper
// design for these screens yet; built from the shared tokens and components.

type View = "month" | "week" | "day";

type Panel =
  | { kind: "inbox" }
  | { kind: "item"; key: string }
  | { kind: "edit"; key: string }
  | { kind: "create"; spot?: Spot; entry?: EntryKind }
  | { kind: "schedule"; taskId: string; movingKey?: string }
  | { kind: "day"; date: string }
  | { kind: "settings" };

const FILTER_LABELS: [keyof Omit<Filters, "project">, string][] = [
  ["events", "Events"],
  ["fixed", "Commitments"],
  ["flexible", "Flexible"],
  ["deadlines", "Deadlines"],
  ["completed", "Completed"],
];

/** Per-device settings, kept in localStorage (never required). */
function useLocal<T>(key: string, initial: T): [T, (v: T) => void] {
  const [value, setValue] = useState<T>(() => {
    try {
      const raw = localStorage.getItem(key);
      return raw ? { ...initial, ...JSON.parse(raw) } : initial;
    } catch {
      return initial;
    }
  });
  const set = useCallback(
    (v: T) => {
      setValue(v);
      try {
        localStorage.setItem(key, JSON.stringify(v));
      } catch {
        /* storage unavailable: keep it for this visit */
      }
    },
    [key],
  );
  return [value, set];
}

const isPhone = () => typeof matchMedia === "function" && matchMedia("(max-width: 767px)").matches;

function useNow(zone: string) {
  const [now, setNow] = useState(() => wallClock(Date.now(), zone));
  useEffect(() => {
    const t = setInterval(() => setNow(wallClock(Date.now(), zone)), 30_000);
    return () => clearInterval(t);
  }, [zone]);
  return now;
}

export function Calendar({ online }: ModuleProps) {
  const planner = usePlanner(online);
  const { events, blocks, tasks } = planner;
  const now = useNow(planner.zone);
  const today = now.date;

  const [prefs, setPrefs] = useLocal("planner:view", { view: (isPhone() ? "day" : "week") as View });
  const view = prefs.view;
  const [filters, setFilters] = useLocal<Filters>("planner:filters", ALL_FILTERS);
  const [visible, setVisible] = useLocal<{ range: [number, number] }>("planner:visible", { range: [0, 1440] });
  const [anchor, setAnchor] = useState(today);
  const [panel, setPanel] = useState<Panel | undefined>(isPhone() ? undefined : { kind: "inbox" });
  const [placing, setPlacing] = useState<Task>();
  const [sort, setSort] = useState<InboxSort>("due");
  const [inboxProject, setInboxProject] = useState("");
  const [showFilters, setShowFilters] = useState(false);

  // Deep links: #/calendar?date=YYYY-MM-DD&view=day
  const deepDate = useHashParam("date");
  const deepView = useHashParam("view");
  useEffect(() => {
    if (deepDate && /^\d{4}-\d{2}-\d{2}$/.test(deepDate)) setAnchor(deepDate);
    if (deepView === "month" || deepView === "week" || deepView === "day") setPrefs({ view: deepView });
  }, [deepDate, deepView]);

  const days = useMemo(() => (view === "day" ? [anchor] : weekDates(anchor)), [view, anchor]);
  const month = monthOf(anchor);
  const [from, to] = useMemo((): [string, string] => {
    if (view === "month") {
      const g = monthGrid(month);
      return [g[0][0], g.at(-1)!.at(-1)!];
    }
    return [days[0], days.at(-1)!];
  }, [view, month, days]);

  const sources = { events: events.events ?? [], blocks: blocks.blocks ?? [], tasks: tasks.tasks ?? [] };
  const allItems = useMemo(() => buildItems({ ...sources, from, to, zone: planner.zone }), [sources.events, sources.blocks, sources.tasks, from, to, planner.zone]);
  const shown = useMemo(() => applyFilters(allItems, filters), [allItems, filters]);
  // Suggestions look ahead from today, whatever is on screen.
  const ahead = useMemo(
    () => (panel?.kind === "schedule" ? buildItems({ ...sources, from: today, to: addDays(today, 62), zone: planner.zone }) : []),
    [panel?.kind, sources.events, sources.blocks, sources.tasks, today, planner.zone],
  );

  const projectNames = useMemo(() => new Map((planner.projects.projects ?? []).map((p) => [p.id, p.name])), [planner.projects.projects]);
  const inbox = useMemo(() => unscheduled(sources.tasks, planner.allocs, sort, projectNames), [sources.tasks, planner.allocs, sort, projectNames]);

  const findItem = (key: string) => allItems.find((i) => i.key === key);
  const selectedKey = panel?.kind === "item" || panel?.kind === "edit" ? panel.key : undefined;

  const go = (delta: number) => {
    if (view === "month") setAnchor(`${shiftMonth(month, delta)}-01`);
    else setAnchor(addDays(anchor, delta * (view === "week" ? 7 : 1)));
  };
  const setView = (v: View) => setPrefs({ view: v });
  const openDay = (d: string) => {
    setAnchor(d);
    setView("day");
  };
  const close = () => setPanel(isPhone() ? undefined : { kind: "inbox" });

  const place = async (t: Task, date: string, start: number) => {
    const a = planner.allocs.get(t.id);
    const len = Math.max(15, Math.min(a?.remaining || planner.hours.session_minutes, planner.hours.session_minutes));
    setPlacing(undefined);
    await planner.createBlock(
      { task_id: t.id, date, start_time: fromMinutes(start), end_time: fromMinutes(Math.min(1440, start + len)) },
      allItems,
      `a session for “${t.title}”`,
    );
  };

  const togglePlacing = (t: Task) => {
    if (placing?.id === t.id) setPlacing(undefined);
    else {
      setPlacing(t);
      if (view === "month") setView("week");
      if (isPhone()) setPanel(undefined);
    }
  };

  // Keyboard: Escape closes; Ctrl/⌘+Z undoes the last change.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLElement && e.target.closest("input, textarea, select, [contenteditable]");
      if (e.key === "Escape" && placing) setPlacing(undefined);
      else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z" && !e.shiftKey && !typing) {
        e.preventDefault();
        void planner.undo();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [placing, planner]);

  // Notices fade after a while.
  const { notice, dismiss } = planner;
  useEffect(() => {
    if (!notice) return;
    const t = setTimeout(dismiss, notice.tone === "ok" ? 7000 : 12_000);
    return () => clearTimeout(t);
  }, [notice, dismiss]);

  const loading = !events.events && !blocks.blocks && (events.loading || blocks.loading);
  const failed = !events.events && !blocks.blocks && !loading;
  const offline = !online || events.stale || blocks.stale;
  const error = events.error || blocks.error || tasks.error;
  const title = view === "month" ? monthLabel(month) : view === "week" ? `${shortDate(days[0])} – ${shortDate(days[6])} ${days[6].slice(0, 4)}` : longDay(anchor);
  const isCurrent = view === "month" ? month === monthOf(today) : days.includes(today);

  const panelContent = (): { title: string; body: ReactNode } | undefined => {
    if (!panel) return undefined;
    switch (panel.kind) {
      case "inbox":
        return {
          title: `unscheduled · ${inbox.length}`,
          body: (
            <Unscheduled
              tasks={inbox}
              allocs={planner.allocs}
              projects={(planner.projects.projects ?? []).filter((p) => p.status === "active")}
              today={today}
              sort={sort}
              project={inboxProject}
              placingId={placing?.id}
              onSort={setSort}
              onProject={setInboxProject}
              onPlace={togglePlacing}
              onPlan={(t) => setPanel({ kind: "schedule", taskId: t.id })}
            />
          ),
        };
      case "item": {
        const item = findItem(panel.key);
        if (!item) return undefined;
        return {
          title: "details",
          body: (
            <ItemDetails
              key={item.key}
              planner={planner}
              item={item}
              items={allItems}
              today={today}
              onEdit={() => setPanel({ kind: "edit", key: item.key })}
              onSchedule={(t, moving) => setPanel({ kind: "schedule", taskId: t.id, movingKey: moving?.key })}
              onClose={close}
            />
          ),
        };
      }
      case "edit": {
        const item = findItem(panel.key);
        if (!item) return undefined;
        return { title: "edit", body: <EntryForm key={item.key} planner={planner} items={allItems} item={item} onDone={() => setPanel({ kind: "item", key: item.key })} /> };
      }
      case "create":
        return {
          title: "new",
          body: <EntryForm key={JSON.stringify(panel.spot)} planner={planner} items={allItems} spot={panel.spot} kind={panel.entry} onDone={close} />,
        };
      case "schedule": {
        const task = sources.tasks.find((t) => t.id === panel.taskId);
        if (!task) return undefined;
        const moving = panel.movingKey ? findItem(panel.movingKey) : undefined;
        return {
          title: moving ? "reschedule" : "plan time",
          body: (
            <ScheduleTask
              key={task.id + (moving?.key ?? "")}
              planner={planner}
              task={task}
              items={ahead}
              today={today}
              now={now}
              moving={moving}
              placing={placing?.id === task.id}
              onPlace={() => togglePlacing(task)}
              onDone={close}
            />
          ),
        };
      }
      case "day":
        return {
          title: "day",
          body: (
            <DayAgenda
              date={panel.date}
              items={shown}
              onSelect={(i) => setPanel({ kind: "item", key: i.key })}
              onNew={() => setPanel({ kind: "create", spot: { date: panel.date, start: 9 * 60, end: 9 * 60 + planner.hours.session_minutes } })}
              onOpenDay={() => openDay(panel.date)}
            />
          ),
        };
      case "settings":
        return {
          title: "planner settings",
          body: (
            <PlannerSettings
              hours={planner.hours}
              visible={visible.range}
              onHours={(h) => {
                void planner.prefs.update({ planner: h });
                planner.show("Saved your working hours.");
              }}
              onVisible={(r) => setVisible({ range: r })}
            />
          ),
        };
    }
  };
  const content = panelContent();

  const summary = view === "day" ? daySummary(anchor, allItems, planner.allocs, planner.hours) : undefined;

  return (
    <div className="flex flex-col gap-3">
      {/* Toolbar */}
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex items-center gap-1">
          <Button className="min-h-9 px-3" onClick={() => setAnchor(today)} disabled={isCurrent && anchor === today}>
            today
          </Button>
          <button type="button" aria-label={`Previous ${view}`} onClick={() => go(-1)} className="grid size-9 place-items-center rounded-full font-mono text-muted hover:bg-sunken hover:text-ink">
            ‹
          </button>
          <button type="button" aria-label={`Next ${view}`} onClick={() => go(1)} className="grid size-9 place-items-center rounded-full font-mono text-muted hover:bg-sunken hover:text-ink">
            ›
          </button>
          <h2 aria-live="polite" className="ml-1 text-[15px] font-semibold">
            {title}
          </h2>
        </div>
        <div role="radiogroup" aria-label="Calendar view" className="ml-auto flex rounded-full border border-line p-0.5">
          {(["month", "week", "day"] as View[]).map((v) => (
            <button
              key={v}
              type="button"
              role="radio"
              aria-checked={view === v}
              onClick={() => setView(v)}
              className={`min-h-8 rounded-full px-3 text-xs font-medium capitalize ${view === v ? "bg-ink text-canvas" : "text-muted hover:text-ink"}`}
            >
              {v}
            </button>
          ))}
        </div>
        <div className="flex w-full flex-wrap gap-1.5 sm:w-auto">
          <Button
            variant="primary"
            className="min-h-9 px-3"
            onClick={() => setPanel({ kind: "create", spot: { date: view === "month" ? anchor : days.includes(today) ? today : days[0], start: Math.min(23 * 60, Math.ceil((now.minutes + 1) / 30) * 30), end: Math.min(1440, Math.ceil((now.minutes + 1) / 30) * 30 + 60) } })}
          >
            + new
          </Button>
          <Button className="min-h-9 px-3" aria-pressed={panel?.kind === "inbox"} onClick={() => setPanel(panel?.kind === "inbox" && isPhone() ? undefined : { kind: "inbox" })}>
            unscheduled · {inbox.length}
          </Button>
          <Button className="min-h-9 px-3" aria-expanded={showFilters} onClick={() => setShowFilters(!showFilters)}>
            filters{JSON.stringify(filters) !== JSON.stringify(ALL_FILTERS) ? " ●" : ""}
          </Button>
          <Button className="min-h-9 px-3" aria-label="Planner settings" onClick={() => setPanel({ kind: "settings" })}>
            ⚙
          </Button>
        </div>
      </div>

      {showFilters && (
        <div role="group" aria-label="Filters" className="flex flex-wrap items-center gap-1.5">
          {FILTER_LABELS.map(([k, label]) => (
            <button
              key={k}
              type="button"
              aria-pressed={filters[k]}
              onClick={() => setFilters({ ...filters, [k]: !filters[k] })}
              className={`min-h-8 rounded-full border px-3 text-xs ${filters[k] ? "border-ink bg-ink text-canvas" : "border-line text-muted line-through"}`}
            >
              {label}
            </button>
          ))}
          <select
            value={filters.project}
            onChange={(e) => setFilters({ ...filters, project: e.target.value })}
            aria-label="Show project"
            className="min-h-8 rounded-full border border-line bg-panel px-3 text-xs"
          >
            <option value="">All projects</option>
            {(planner.projects.projects ?? []).map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
          <span className="flex items-center gap-2 pl-1 text-[11px] text-muted">
            <span className="inline-block h-3 w-4 rounded-sm border-l-[3px] border-accent bg-accent/15" /> commitment
            <span className="inline-block h-3 w-4 rounded-sm border border-dashed border-accent" /> flexible event
            <span className="inline-block h-3 w-4 rounded-sm border border-dashed border-violet bg-violet/10" /> work session
            <span className="inline-block h-3 w-4 rounded-sm border border-line" /> ◆ deadline
          </span>
        </div>
      )}

      {offline && (events.events || blocks.blocks) && (
        <p className="rounded-sm border border-line bg-sunken/60 px-3 py-2 text-sm text-muted">Offline: changes are saved on this device and sync when you reconnect.</p>
      )}
      {error && !offline && (
        <p role="alert" className="text-sm text-bad">
          {error}
        </p>
      )}

      {summary && (
        <dl aria-label="Day summary" className="-mx-1 flex gap-2 overflow-x-auto px-1 pb-1">
          {[
            ["commitments", `${summary.commitments} · ${durationLabel(summary.commitmentMinutes)}`],
            ["planned work", `${summary.sessions} · ${durationLabel(summary.plannedMinutes)}`],
            ["work left", durationLabel(summary.remainingMinutes)],
            ["free (working hours)", durationLabel(summary.freeMinutes)],
            ["due", String(summary.due)],
          ].map(([k, v]) => (
            <div key={k} className="shrink-0 rounded-sm border border-line bg-panel px-3 py-1.5">
              <dt className="label text-[10px] whitespace-nowrap">{k}</dt>
              <dd className="text-sm font-medium">{v}</dd>
            </div>
          ))}
        </dl>
      )}

      <div className={`grid items-start gap-4 ${content ? "lg:grid-cols-[minmax(0,1fr)_22rem]" : ""}`}>
        <div className="min-w-0">
          {loading ? (
            <p className="text-sm text-muted">Loading calendar…</p>
          ) : failed ? (
            <div className="text-sm">
              <p className="text-muted">The calendar couldn't be loaded.</p>
              <Button
                className="mt-2 min-h-8 px-3 text-xs"
                onClick={() => {
                  events.reload();
                  blocks.reload();
                  tasks.reload();
                }}
              >
                retry
              </Button>
            </div>
          ) : view === "month" ? (
            <MonthView
              month={month}
              items={shown}
              today={today}
              selectedDate={panel?.kind === "day" ? panel.date : undefined}
              selectedKey={selectedKey}
              onSelectDate={(d) => {
                setAnchor(d);
                setPanel({ kind: "day", date: d });
              }}
              onOpenDay={openDay}
              onSelect={(i) => setPanel({ kind: "item", key: i.key })}
            />
          ) : (
            <TimeGrid
              days={days}
              items={shown}
              range={visible.range}
              now={now}
              rich={view === "day"}
              selectedKey={selectedKey}
              placing={placing?.title}
              allocs={planner.allocs}
              projectNames={projectNames}
              defaultMinutes={planner.hours.session_minutes}
              onSelect={(i: CalItem) => setPanel({ kind: "item", key: i.key })}
              onCreate={(spot) => setPanel({ kind: "create", spot })}
              onMove={(i, spot) => void planner.move(i, spot, allItems)}
              onDropTask={(id, date, start) => {
                const t = sources.tasks.find((x) => x.id === id);
                if (t) void place(t, date, start);
              }}
              onPlace={(date, start) => placing && void place(placing, date, start)}
              onOpenDay={openDay}
              onToggleDone={(i) =>
                i.block &&
                void planner.updateBlock(
                  i.block,
                  i.done ? { status: "planned", actual_minutes: null } : { status: "done", actual_minutes: (i.end ?? 0) - (i.start ?? 0) },
                  i.title,
                )
              }
            />
          )}
          {view !== "month" && (
            <p className="mt-2 text-[11px] text-muted">
              Drag across empty time to create · drag entries to move, their bottom edge to resize · drop tasks from Unscheduled · arrow keys move a focused entry
              (Shift resizes) · Ctrl+Z undoes. Times are in {planner.zone}.
            </p>
          )}
        </div>

        {content && (
          <aside
            aria-label={content.title}
            className="fixed inset-x-0 bottom-[calc(4.25rem+env(safe-area-inset-bottom))] z-30 max-h-[70dvh] overflow-y-auto rounded-t-lg border border-line bg-panel shadow-2xl md:bottom-0 lg:static lg:z-auto lg:max-h-none lg:overflow-visible lg:rounded-lg lg:shadow-none"
          >
            <header className="sticky top-0 z-10 flex min-h-11 items-center gap-2 border-b border-line bg-panel px-4">
              <h2 className="label flex-1">{content.title}</h2>
              {panel?.kind !== "inbox" && (
                <button type="button" onClick={close} aria-label="Close panel" className="grid size-9 place-items-center rounded-full text-muted hover:bg-sunken hover:text-ink">
                  ✕
                </button>
              )}
              {panel?.kind === "inbox" && isPhone() && (
                <button type="button" onClick={() => setPanel(undefined)} aria-label="Close panel" className="grid size-9 place-items-center rounded-full text-muted hover:bg-sunken">
                  ✕
                </button>
              )}
            </header>
            <div className="p-4">{content.body}</div>
          </aside>
        )}
      </div>

      {notice && (
        <div
          role="status"
          className={`fixed inset-x-4 bottom-[calc(5rem+env(safe-area-inset-bottom))] z-40 mx-auto flex max-w-lg items-center gap-3 rounded-md border bg-panel px-4 py-3 text-sm shadow-xl md:bottom-6 ${
            notice.tone === "bad" ? "border-bad text-bad" : notice.tone === "warn" ? "border-warn" : "border-line"
          }`}
        >
          <span className="flex-1">{notice.text}</span>
          {notice.undo && (
            <button type="button" onClick={() => void planner.undo()} className="min-h-8 rounded-full border border-line px-3 text-xs font-medium hover:bg-sunken">
              Undo
            </button>
          )}
          <button type="button" onClick={dismiss} aria-label="Dismiss" className="text-muted hover:text-ink">
            ✕
          </button>
        </div>
      )}
    </div>
  );
}
