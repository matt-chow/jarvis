import { useEffect, useMemo, useState, type FormEvent, type KeyboardEvent } from "react";
import type { ModuleProps } from "../App";
import { CategoryChips, CategoryManager, CategoryPicker } from "../components/Categories";
import { SnoozeMenu } from "../components/SnoozeMenu";
import { Button, Panel, inputClass } from "../components/ui";
import { useCategories, type CategoriesState } from "../hooks/useCategories";
import { useHashParam } from "../hooks/useHashParam";
import { useProjects } from "../hooks/useProjects";
import { useReminders, type RemindersState } from "../hooks/useReminders";
import { useTasks } from "../hooks/useTasks";
import { api } from "../lib/api";
import { NO_CATEGORY, PALETTE, type Category } from "../lib/categories";
import {
  ALERT_PRESETS,
  MAX_ALERTS,
  MAX_CHECKLIST,
  MAX_LINKS,
  MAX_LIST_NAME,
  MAX_TITLE,
  PRIORITY_LABELS,
  PRIORITY_MARK,
  SMART_VIEWS,
  SORTS,
  WEEKDAY_SHORT,
  alertLabel,
  deviceZone,
  dueLabel,
  fieldLabel,
  fieldValue,
  inView,
  isOpen,
  isOverdue,
  isSnoozed,
  listNameProblem,
  ruleSummary,
  sameAlert,
  selectReminders,
  type Advance,
  type Freq,
  type HistoryEntry,
  type Missed,
  type Priority,
  type Reminder,
  type ReminderList,
  type SmartView,
  type SortKey,
} from "../lib/reminders";
import { clientId, reminderConflictKey, type ReminderConflict } from "../lib/sync";
import { isOpen as taskOpen, localDate } from "../lib/tasks";

// Restyled in the D-050 pass (no separate Paper design): built from the Tasks
// screen's rows, pills, and panels, in sentence case.

const small = "min-h-7 px-2 text-[10px]";
const NO_LIST = "none";

/** Ticks once a minute so overdue and snoozed states stay current. */
function useNow(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 60_000);
    return () => window.clearInterval(t);
  }, []);
  return now;
}

const ACTION_WORDS: Record<Extract<ReminderConflict, { kind: "stale" }>["op"]["kind"], string> = {
  "reminder.complete": "completion",
  "reminder.uncomplete": "reopening",
  "reminder.snooze": "snooze",
};

/**
 * Reminder changes that clashed with another device's (D-049 M7). Nothing
 * is dropped until the owner picks a version here.
 */
function SyncConflicts({ state, names }: { state: RemindersState; names: (id: string) => string | undefined }) {
  const [busy, setBusy] = useState<string>();
  const act = async (c: ReminderConflict, keep: "mine" | "theirs") => {
    setBusy(reminderConflictKey(c));
    try {
      await state.resolve(c, keep);
    } finally {
      setBusy(undefined);
    }
  };
  return (
    <section aria-label="Sync conflicts" className="mb-3 rounded-sm border border-warn bg-sunken/60 px-3 py-2 text-sm">
      <p className="label mb-2 text-warn">sync conflicts · {state.conflicts.length}</p>
      <ul className="flex flex-col gap-3">
        {state.conflicts.map((c) => {
          const key = reminderConflictKey(c);
          // A snooze whose time has passed has nothing left to apply.
          const expired = c.kind === "stale" && c.op.kind === "reminder.snooze" && c.op.until <= Date.now();
          return (
            <li key={key} className="flex flex-col gap-1.5">
              {c.kind === "fields" ? (
                <>
                  <p>
                    “{c.theirs.title}” was changed on this device and on another. The rest of your edit was saved.
                  </p>
                  <table className="w-full table-fixed text-left text-xs">
                    <thead className="text-muted">
                      <tr>
                        <th className="w-1/4 font-normal">field</th>
                        <th className="font-normal">this device</th>
                        <th className="font-normal">other device (kept now)</th>
                      </tr>
                    </thead>
                    <tbody>
                      {c.fields.map((f) => (
                        <tr key={f} className="align-top">
                          <td className="text-muted">{fieldLabel(f)}</td>
                          <td className="break-words">{fieldValue(f, (c.mine as Record<string, unknown>)[f], names)}</td>
                          <td className="break-words">{fieldValue(f, (c.theirs as unknown as Record<string, unknown>)[f], names)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </>
              ) : (
                <p>
                  Your offline {ACTION_WORDS[c.op.kind]} of “{c.theirs.title}”{c.op.occurrence ? ` for ${c.op.occurrence}` : ""} didn't apply: {c.message}.
                  {c.theirs.due_date ? ` It's now due ${c.theirs.due_date}.` : ""}
                </p>
              )}
              <div className="flex flex-wrap gap-2">
                {!expired && (
                  <Button className={small} disabled={busy !== undefined} onClick={() => void act(c, "mine")}>
                    {c.kind === "fields" ? "Use this device's" : `apply to ${c.theirs.due_date ? "the current one" : "it now"}`}
                  </Button>
                )}
                <Button className={small} disabled={busy !== undefined} onClick={() => void act(c, "theirs")}>
                  {c.kind === "fields" ? "Keep other device's" : "Discard"}
                </Button>
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

const timeOf = (ms: number) => new Date(ms).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
const whenOf = (ms: number) => new Date(ms).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });

export function Reminders({ online }: ModuleProps) {
  const state = useReminders(online);
  const cats = useCategories(online);
  const [view, setView] = useState<SmartView>("today");
  const [list, setList] = useState("");
  const [category, setCategory] = useState("");
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<SortKey>("due");
  const [selected, setSelected] = useState<string>();
  const [managing, setManaging] = useState<"lists" | "categories">();
  const now = useNow();
  const today = localDate(new Date(now));
  const offline = state.stale || !online;
  const readOnly = !state.reminders;

  const shown = useMemo(
    () => selectReminders(state.reminders ?? [], { view, list, category, search, sort }, today),
    [state.reminders, view, list, category, search, sort, today],
  );
  const counts = useMemo(() => {
    const all = state.reminders ?? [];
    return Object.fromEntries(SMART_VIEWS.map((v) => [v.id, all.filter((r) => inView(r, v.id, today)).length])) as Record<SmartView, number>;
  }, [state.reminders, today]);
  const listName = useMemo(() => new Map((state.lists ?? []).map((l) => [l.id, l.name])), [state.lists]);
  const catById = useMemo(() => new Map((cats.categories ?? []).map((c) => [c.id, c])), [cats.categories]);
  const current = state.reminders?.find((r) => r.id === selected);

  // Deep link (#/reminders?reminder=…), e.g. from a notification or the calendar.
  const deep = useHashParam("reminder");
  useEffect(() => {
    if (!deep || !state.reminders) return;
    const r = state.reminders.find((x) => x.id === deep);
    if (!r) return;
    setSearch("");
    setView(r.deleted_at ? "deleted" : r.completed_at ? "completed" : r.due_date ? "scheduled" : "all");
    setSelected(r.id);
  }, [deep, state.reminders]);

  // Drop the selection if the reminder is purged elsewhere.
  useEffect(() => {
    if (selected && state.reminders && !state.reminders.some((r) => r.id === selected)) setSelected(undefined);
  }, [selected, state.reminders]);

  const empty =
    search.trim() !== ""
      ? "No reminders match that search."
      : {
          today: "Nothing due today.",
          scheduled: "No dated reminders.",
          all: "No reminders yet. Add one above.",
          flagged: "No flagged reminders.",
          completed: "No completed reminders.",
          deleted: "Nothing deleted. Deleted reminders stay here until you delete them for good.",
        }[view];

  return (
    <div className="grid items-start gap-5 lg:grid-cols-[minmax(0,1fr)_24rem]">
      <Panel
        title={`reminders${state.reminders ? ` · ${counts.all} open` : ""}`}
        actions={
          <div role="tablist" aria-label="View" className="flex flex-wrap gap-1">
            {SMART_VIEWS.map((v) => (
              <button
                key={v.id}
                role="tab"
                aria-selected={view === v.id}
                onClick={() => setView(v.id)}
                className={`rounded-sm px-2 py-1 font-mono text-[10px] tracking-wide uppercase ${
                  view === v.id ? "bg-accent text-accent-ink" : "text-muted hover:text-ink"
                }`}
              >
                {v.label}
                {state.reminders && v.id !== "all" && counts[v.id] > 0 && <span className="ml-1 opacity-70">{counts[v.id]}</span>}
              </button>
            ))}
          </div>
        }
      >
        {offline && state.reminders && (
          <p className="mb-3 rounded-sm border border-line bg-sunken/60 px-3 py-2 text-sm text-muted">
            Offline: changes are saved on this device and sync when you reconnect. Notifications need the server, so none arrive while
            this device is offline.
          </p>
        )}
        {state.conflicts.length > 0 && <SyncConflicts state={state} names={(id) => listName.get(id) ?? catById.get(id)?.name} />}
        <div className="mb-3 grid gap-2 sm:grid-cols-2">
          <label className="flex min-w-0 items-center gap-2">
            <span className="label shrink-0">list</span>
            <select value={list} onChange={(e) => setList(e.target.value)} aria-label="List filter" className={`${inputClass} min-w-0 flex-1`}>
              <option value="">All lists</option>
              <option value={NO_LIST}>No list</option>
              {(state.lists ?? []).map((l) => (
                <option key={l.id} value={l.id}>
                  {l.name}
                </option>
              ))}
            </select>
          </label>
          <label className="flex min-w-0 items-center gap-2">
            <span className="label shrink-0">category</span>
            <select value={category} onChange={(e) => setCategory(e.target.value)} aria-label="Category filter" className={`${inputClass} min-w-0 flex-1`}>
              <option value="">All categories</option>
              <option value={NO_CATEGORY}>No category</option>
              {(cats.categories ?? []).map((c) => (
                <option key={c.id} value={c.id}>
                  {c.name}
                </option>
              ))}
            </select>
          </label>
          <label className="flex min-w-0 items-center gap-2">
            <span className="label shrink-0">sort</span>
            <select value={sort} onChange={(e) => setSort(e.target.value as SortKey)} aria-label="Sort" className={`${inputClass} min-w-0 flex-1`}>
              {SORTS.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.label}
                </option>
              ))}
            </select>
          </label>
          <input
            type="search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search, including completed…"
            aria-label="Search reminders"
            className={`${inputClass} min-w-0`}
          />
        </div>
        <div className="mb-3 flex flex-wrap gap-2">
          <Button className={small} aria-pressed={managing === "lists"} onClick={() => setManaging(managing === "lists" ? undefined : "lists")}>
            Lists
          </Button>
          <Button className={small} aria-pressed={managing === "categories"} onClick={() => setManaging(managing === "categories" ? undefined : "categories")}>
            Categories
          </Button>
        </div>
        {view !== "completed" && view !== "deleted" && (
          <QuickAdd
            disabled={readOnly}
            today={today}
            defaultDate={view === "today" ? today : ""}
            onAdd={(n) =>
              state.create({
                ...n,
                list_id: list && list !== NO_LIST ? list : null,
                category_ids: category && category !== NO_CATEGORY ? [category] : [],
                flagged: view === "flagged",
              })
            }
          />
        )}
        {state.error && (
          <p role="alert" className="mt-3 text-sm text-bad">
            {state.error}
          </p>
        )}

        {state.loading && !state.reminders ? (
          <p className="mt-4 text-sm text-muted">Loading reminders…</p>
        ) : !state.reminders ? (
          <div className="mt-4 text-sm">
            <p className="text-muted">Reminders couldn't be loaded.</p>
            <Button className={`${small} mt-2`} onClick={state.reload}>
              Try again
            </Button>
          </div>
        ) : shown.length === 0 ? (
          <p className="mt-4 text-sm text-muted">{empty}</p>
        ) : (
          <ul aria-label="Reminders" className="-mx-2 mt-3 flex flex-col">
            {shown.map((r) => (
              <ReminderRow
                key={r.id}
                reminder={r}
                now={now}
                today={today}
                selected={selected === r.id}
                readOnly={readOnly}
                listName={r.list_id ? listName.get(r.list_id) : undefined}
                categories={r.category_ids.map((id) => catById.get(id)).filter((c): c is NonNullable<typeof c> => !!c)}
                state={state}
                onSelect={() => setSelected(selected === r.id ? undefined : r.id)}
              />
            ))}
          </ul>
        )}
      </Panel>

      <div className="flex flex-col gap-5">
        {managing === "lists" && (
          <Panel title="lists" actions={<Button className={small} onClick={() => setManaging(undefined)}>Close</Button>}>
            <ListManager state={state} disabled={!state.lists} />
          </Panel>
        )}
        {managing === "categories" && (
          <Panel title="categories" actions={<Button className={small} onClick={() => setManaging(undefined)}>Close</Button>}>
            <CategoryManager state={cats} disabled={!cats.categories} />
          </Panel>
        )}
        {current && (
          <ReminderDetail
            key={current.id}
            reminder={current}
            state={state}
            cats={cats}
            online={online && !state.stale}
            readOnly={readOnly}
            onClose={() => setSelected(undefined)}
          />
        )}
      </div>
    </div>
  );
}

function QuickAdd(props: {
  disabled: boolean;
  today: string;
  defaultDate: string;
  onAdd: (r: { title: string; due_date: string | null; due_time: string | null }) => Promise<unknown>;
}) {
  const [title, setTitle] = useState("");
  const [date, setDate] = useState(props.defaultDate);
  const [time, setTime] = useState("");
  useEffect(() => setDate(props.defaultDate), [props.defaultDate]);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!title.trim()) return;
    if (await props.onAdd({ title, due_date: date || null, due_time: date && time ? time : null })) {
      setTitle("");
      setTime("");
    }
  };
  return (
    <form onSubmit={(e) => void submit(e)} className="flex flex-wrap gap-2">
      <input
        value={title}
        disabled={props.disabled}
        maxLength={MAX_TITLE}
        onChange={(e) => setTitle(e.target.value)}
        placeholder="Remind me to…"
        aria-label="New reminder"
        className={`${inputClass} min-w-40 flex-1`}
      />
      <input type="date" value={date} disabled={props.disabled} onChange={(e) => setDate(e.target.value)} aria-label="New reminder date" className={inputClass} />
      <input
        type="time"
        value={time}
        disabled={props.disabled || !date}
        onChange={(e) => setTime(e.target.value)}
        aria-label="New reminder time"
        className={inputClass}
      />
      <Button type="submit" variant="primary" disabled={props.disabled || !title.trim()}>
        Add
      </Button>
    </form>
  );
}

function Check(props: { reminder: Reminder; disabled: boolean; onToggle: () => void }) {
  const done = props.reminder.completed_at !== null;
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={done}
      aria-label={`${done ? "Reopen" : "Complete"} “${props.reminder.title}”`}
      disabled={props.disabled}
      onClick={props.onToggle}
      className={`grid size-5 shrink-0 place-items-center rounded-full border font-mono text-[11px] leading-none transition ${
        done ? "border-ok bg-ok text-canvas" : "border-line hover:border-accent"
      } disabled:opacity-50`}
    >
      {done ? "✓" : ""}
    </button>
  );
}

function ReminderRow(props: {
  reminder: Reminder;
  now: number;
  today: string;
  selected: boolean;
  readOnly: boolean;
  listName?: string;
  categories: Category[];
  state: RemindersState;
  onSelect: () => void;
}) {
  const { reminder: r, state, readOnly } = props;
  const overdue = isOverdue(r, props.now, props.today);
  const due = dueLabel(r, props.today);
  const done = r.checklist.filter((c) => c.done).length;
  const deleted = r.deleted_at !== null;
  return (
    <li className={`flex items-start gap-3 rounded-sm px-2 py-2 ${props.selected ? "bg-sunken" : "hover:bg-sunken/50"}`}>
      {!deleted && (
        <span className="pt-0.5">
          <Check reminder={r} disabled={readOnly} onToggle={() => void (r.completed_at ? state.uncomplete(r) : state.complete(r))} />
        </span>
      )}
      <button type="button" onClick={props.onSelect} aria-expanded={props.selected} className="min-w-0 flex-1 text-left">
        <span className={`block truncate text-sm ${isOpen(r) ? "" : "text-muted line-through"}`}>
          {r.flagged && (
            <span className="mr-1 text-warn" title="Flagged">
              ⚑
            </span>
          )}
          {r.title}
        </span>
        <span className="flex flex-wrap gap-x-3 font-mono text-[11px] text-muted">
          {due && <span className={overdue ? "text-bad" : r.due_date === props.today ? "text-accent" : ""}>{overdue ? `overdue · ${due}` : due}</span>}
          {r.recurrence && <span title={ruleSummary(r.recurrence)}>↻ {ruleSummary(r.recurrence)}</span>}
          {isSnoozed(r, props.now) && <span className="text-accent">snoozed until {timeOf(r.snoozed_until!)}</span>}
          {props.listName && <span>☰ {props.listName}</span>}
          <CategoryChips categories={props.categories} />
          {r.priority !== "none" && (
            <span className="text-warn" title={`${PRIORITY_LABELS[r.priority]} priority`}>
              {PRIORITY_MARK[r.priority]}
            </span>
          )}
          {r.checklist.length > 0 && (
            <span>
              {done}/{r.checklist.length}
            </span>
          )}
          {r.task_id && <span title="Linked to a task">✓ task</span>}
          {r.notes && <span title="Has notes">¶</span>}
        </span>
      </button>
      {deleted ? (
        <span className="flex gap-1.5">
          <Button className={small} disabled={readOnly} onClick={() => void state.restore(r.id)} aria-label={`Restore “${r.title}”`}>
            Restore
          </Button>
        </span>
      ) : (
        <SnoozeMenu reminder={r} disabled={readOnly} onSnooze={(until) => void state.snooze(r, until)} />
      )}
    </li>
  );
}

function ListManager({ state, disabled }: { state: RemindersState; disabled: boolean }) {
  const [name, setName] = useState("");
  const problem = name.trim() ? listNameProblem(state.lists, name) : undefined;
  const add = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim() || problem) return;
    if (await state.createList(name, PALETTE[(state.lists?.length ?? 0) % PALETTE.length].color)) setName("");
  };
  return (
    <div className="flex flex-col gap-3">
      {state.lists && state.lists.length === 0 && <p className="text-sm text-muted">No lists yet. A reminder belongs to at most one list.</p>}
      <ul className="flex flex-col gap-1">
        {(state.lists ?? []).map((l) => (
          <ListRow key={l.id} list={l} state={state} disabled={disabled} />
        ))}
      </ul>
      <form onSubmit={(e) => void add(e)} className="flex gap-2">
        <input
          value={name}
          disabled={disabled}
          maxLength={MAX_LIST_NAME}
          onChange={(e) => setName(e.target.value)}
          placeholder="New list…"
          aria-label="New list"
          className={`${inputClass} min-w-0 flex-1`}
        />
        <Button type="submit" className={small} disabled={disabled || !name.trim() || !!problem}>
          Add
        </Button>
      </form>
      {problem && <p className="text-xs text-bad">{problem}</p>}
    </div>
  );
}

function ListRow({ list, state, disabled }: { list: ReminderList; state: RemindersState; disabled: boolean }) {
  const [name, setName] = useState(list.name);
  const [confirm, setConfirm] = useState(false);
  const problem = listNameProblem(state.lists, name, list.id);
  const save = () => {
    if (!problem && name.trim() !== list.name) void state.updateList(list.id, { name });
    else setName(list.name);
  };
  return (
    <li className="flex items-center gap-2">
      <span aria-hidden className="inline-block size-2 shrink-0 rounded-full" style={{ backgroundColor: list.color }} />
      <input
        value={name}
        disabled={disabled}
        maxLength={MAX_LIST_NAME}
        onChange={(e) => setName(e.target.value)}
        onBlur={save}
        onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
        aria-label={`Rename list “${list.name}”`}
        className={`${inputClass} min-w-0 flex-1`}
      />
      {confirm ? (
        <>
          <Button className={small} onClick={() => setConfirm(false)}>
            Keep
          </Button>
          <Button variant="danger" className={small} onClick={() => void state.removeList(list.id)}>
            Confirm
          </Button>
        </>
      ) : (
        <Button variant="danger" className={small} disabled={disabled} onClick={() => setConfirm(true)} aria-label={`Delete list “${list.name}”`}>
          Delete
        </Button>
      )}
    </li>
  );
}

const FREQS: { id: Freq | ""; label: string }[] = [
  { id: "", label: "Never" },
  { id: "daily", label: "Daily" },
  { id: "weekly", label: "Weekly" },
  { id: "monthly", label: "Monthly" },
  { id: "yearly", label: "Yearly" },
];
const ADVANCE_LABELS: Record<Advance, string> = { calendar: "On schedule", completion: "From when I complete it" };
const MISSED_LABELS: Record<Missed, string> = { one: "Carry one forward", each: "Keep each one", skip: "Skip missed ones" };

function zoneChoices(): string[] {
  const f = (Intl as unknown as { supportedValuesOf?: (k: string) => string[] }).supportedValuesOf;
  try {
    return f ? f("timeZone") : [];
  } catch {
    return [];
  }
}

function ReminderDetail(props: {
  reminder: Reminder;
  state: RemindersState;
  cats: CategoriesState;
  online: boolean;
  readOnly: boolean;
  onClose: () => void;
}) {
  const { reminder: r, state, readOnly, onClose } = props;
  const projects = useProjects(props.online);
  const tasks = useTasks(props.online);
  const [title, setTitle] = useState(r.title);
  const [notes, setNotes] = useState(r.notes);
  const [zone, setZone] = useState(r.timezone);
  const [link, setLink] = useState("");
  const [item, setItem] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [history, setHistory] = useState<HistoryEntry[]>();
  const zones = useMemo(zoneChoices, []);
  const set = (patch: Parameters<RemindersState["update"]>[1]) => void state.update(r.id, patch);
  const deleted = r.deleted_at !== null;
  const ro = readOnly || deleted;

  // Edited elsewhere (or synced): show the new text unless it's being typed.
  useEffect(() => setTitle(r.title), [r.title]);
  useEffect(() => setNotes(r.notes), [r.notes]);
  useEffect(() => setZone(r.timezone), [r.timezone]);

  useEffect(() => {
    if (!props.online) return;
    let live = true;
    api
      .reminderHistory(r.id)
      .then((h) => live && setHistory(h))
      .catch(() => live && setHistory(undefined));
    return () => {
      live = false;
    };
  }, [r.id, r.version, props.online]);

  const rule = r.recurrence;
  const setRule = (patch: Partial<NonNullable<Reminder["recurrence"]>> | null) =>
    set({ recurrence: patch === null ? null : { freq: "daily", interval: 1, weekdays: [], until: null, ...rule, ...patch } });
  const openTasks = (tasks.tasks ?? []).filter((t) => !t.parent_id && (taskOpen(t) || t.id === r.task_id));

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") onClose();
  };

  return (
    <Panel
      title={deleted ? "reminder · deleted" : "reminder"}
      actions={
        <Button onClick={onClose} className={small} aria-label="Close reminder details">
          Close
        </Button>
      }
    >
      <div className="flex flex-col gap-3" onKeyDown={onKey}>
        {deleted && (
          <p className="rounded-sm border border-line bg-sunken/60 px-3 py-2 text-sm text-muted">
            Deleted {new Date(r.deleted_at!).toLocaleDateString()}. Restore it to edit, or delete it for good below.
          </p>
        )}
        <label className="flex flex-col gap-1">
          <span className="label">title</span>
          <input
            value={title}
            disabled={ro}
            maxLength={MAX_TITLE}
            onChange={(e) => setTitle(e.target.value)}
            onBlur={() => (title.trim() && title.trim() !== r.title ? set({ title }) : setTitle(r.title))}
            onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
            className={inputClass}
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="label">notes</span>
          <textarea
            value={notes}
            disabled={ro}
            rows={3}
            onChange={(e) => setNotes(e.target.value)}
            onBlur={() => notes !== r.notes && set({ notes })}
            className={inputClass}
          />
        </label>

        <div className="grid grid-cols-2 gap-3">
          <label className="flex flex-col gap-1">
            <span className="label">date</span>
            <input
              type="date"
              value={r.due_date ?? ""}
              disabled={ro}
              onChange={(e) => set({ due_date: e.target.value || null })}
              aria-label="Due date"
              className={inputClass}
            />
          </label>
          <label className="flex flex-col gap-1">
            <span className="label">time</span>
            <span className="flex gap-1">
              <input
                type="time"
                value={r.due_time ?? ""}
                disabled={ro || !r.due_date}
                onChange={(e) => set({ due_time: e.target.value || null })}
                aria-label="Due time"
                className={`${inputClass} min-w-0 flex-1`}
              />
            </span>
          </label>
        </div>
        {r.due_date && (
          <label className="flex flex-col gap-1">
            <span className="label">time zone</span>
            <input
              value={zone}
              disabled={ro}
              list="reminder-zones"
              onChange={(e) => setZone(e.target.value)}
              onBlur={() => (zone.trim() && zone !== r.timezone ? set({ timezone: zone.trim() }) : setZone(r.timezone))}
              onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
              aria-label="Time zone"
              className={inputClass}
            />
            <datalist id="reminder-zones">
              {zones.map((z) => (
                <option key={z} value={z} />
              ))}
            </datalist>
            {r.timezone !== deviceZone() && <span className="text-xs text-muted">This device is in {deviceZone()}.</span>}
          </label>
        )}
        {r.due_date && (
          <div className="flex gap-2">
            <Button className={small} disabled={ro} onClick={() => set({ due_date: null })}>
              Remove date
            </Button>
          </div>
        )}

        <div>
          <p className="label mb-1">alerts</p>
          {!r.due_date ? (
            <p className="text-sm text-muted">Undated reminders never notify. Give it a date to add alerts.</p>
          ) : (
            <>
              {r.alerts.length === 0 && <p className="mb-1 text-sm text-muted">No alerts: this reminder won't notify.</p>}
              <ul className="mb-2 flex flex-col gap-1">
                {r.alerts.map((a, i) => (
                  <li key={i} className="flex items-center justify-between gap-2 text-sm">
                    <span>{alertLabel(a)}</span>
                    <button
                      type="button"
                      disabled={ro}
                      onClick={() => set({ alerts: r.alerts.filter((_, j) => j !== i) })}
                      aria-label={`Remove alert “${alertLabel(a)}”`}
                      className="font-mono text-xs text-muted hover:text-bad disabled:opacity-50"
                    >
                      ×
                    </button>
                  </li>
                ))}
              </ul>
              {r.alerts.length < MAX_ALERTS && (
                <select
                  value=""
                  disabled={ro}
                  onChange={(e) => {
                    const p = ALERT_PRESETS[Number(e.target.value)];
                    if (p && !r.alerts.some((a) => sameAlert(a, p.alert))) set({ alerts: [...r.alerts, p.alert] });
                  }}
                  aria-label="Add alert"
                  className={inputClass}
                >
                  <option value="">Add an alert…</option>
                  {ALERT_PRESETS.map((p, i) => (
                    <option key={p.label} value={i} disabled={r.alerts.some((a) => sameAlert(a, p.alert))}>
                      {p.label}
                    </option>
                  ))}
                </select>
              )}
            </>
          )}
        </div>

        {r.due_date && (
          <div className="flex flex-col gap-2">
            <div className="grid grid-cols-2 gap-3">
              <label className="flex flex-col gap-1">
                <span className="label">repeat</span>
                <select
                  value={rule?.freq ?? ""}
                  disabled={ro}
                  onChange={(e) => setRule(e.target.value ? { freq: e.target.value as Freq, weekdays: [] } : null)}
                  aria-label="Repeat"
                  className={inputClass}
                >
                  {FREQS.map((f) => (
                    <option key={f.id} value={f.id}>
                      {f.label}
                    </option>
                  ))}
                </select>
              </label>
              {rule && (
                <label className="flex flex-col gap-1">
                  <span className="label">every</span>
                  <input
                    type="number"
                    min={1}
                    max={999}
                    value={rule.interval}
                    disabled={ro}
                    onChange={(e) => {
                      const n = Math.trunc(Number(e.target.value));
                      if (n >= 1 && n <= 999) setRule({ interval: n });
                    }}
                    aria-label="Repeat interval"
                    className={inputClass}
                  />
                </label>
              )}
            </div>
            {rule?.freq === "weekly" && (
              <div role="group" aria-label="Weekdays" className="flex flex-wrap gap-1">
                {WEEKDAY_SHORT.map((d, i) => {
                  const on = rule.weekdays.includes(i);
                  return (
                    <button
                      key={d}
                      type="button"
                      aria-pressed={on}
                      disabled={ro}
                      onClick={() => setRule({ weekdays: on ? rule.weekdays.filter((x) => x !== i) : [...rule.weekdays, i].sort() })}
                      className={`min-h-7 rounded-full border px-2.5 text-[12px] ${on ? "border-accent bg-sunken text-ink" : "border-line text-muted hover:text-ink"}`}
                    >
                      {d}
                    </button>
                  );
                })}
              </div>
            )}
            {rule && (
              <div className="grid grid-cols-2 gap-3">
                <label className="flex flex-col gap-1">
                  <span className="label">until</span>
                  <input
                    type="date"
                    value={rule.until ?? ""}
                    min={r.due_date}
                    disabled={ro}
                    onChange={(e) => setRule({ until: e.target.value || null })}
                    aria-label="Repeat until"
                    className={inputClass}
                  />
                </label>
                <label className="flex flex-col gap-1">
                  <span className="label">next one</span>
                  <select value={r.advance} disabled={ro} onChange={(e) => set({ advance: e.target.value as Advance })} aria-label="Advance" className={inputClass}>
                    {Object.entries(ADVANCE_LABELS).map(([v, l]) => (
                      <option key={v} value={v}>
                        {l}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="col-span-2 flex flex-col gap-1">
                  <span className="label">missed occurrences</span>
                  <select value={r.missed} disabled={ro} onChange={(e) => set({ missed: e.target.value as Missed })} aria-label="Missed occurrences" className={inputClass}>
                    {Object.entries(MISSED_LABELS).map(([v, l]) => (
                      <option key={v} value={v}>
                        {l}
                      </option>
                    ))}
                  </select>
                </label>
                {r.backlog > 0 && <p className="col-span-2 text-xs text-warn">{r.backlog} earlier occurrence(s) still open.</p>}
              </div>
            )}
          </div>
        )}

        <div className="grid grid-cols-2 gap-3">
          <label className="flex flex-col gap-1">
            <span className="label">priority</span>
            <select value={r.priority} disabled={ro} onChange={(e) => set({ priority: e.target.value as Priority })} aria-label="Priority" className={inputClass}>
              {Object.entries(PRIORITY_LABELS).map(([v, l]) => (
                <option key={v} value={v}>
                  {l}
                </option>
              ))}
            </select>
          </label>
          <label className="flex flex-col gap-1">
            <span className="label">list</span>
            <select value={r.list_id ?? ""} disabled={ro} onChange={(e) => set({ list_id: e.target.value || null })} aria-label="List" className={inputClass}>
              <option value="">No list</option>
              {(state.lists ?? []).map((l) => (
                <option key={l.id} value={l.id}>
                  {l.name}
                </option>
              ))}
            </select>
          </label>
        </div>
        <label className="flex items-center gap-2 text-sm">
          <input type="checkbox" checked={r.flagged} disabled={ro} onChange={(e) => set({ flagged: e.target.checked })} />
          Flagged
        </label>

        <CategoryPicker state={props.cats} selected={r.category_ids} disabled={ro} onChange={(ids) => set({ category_ids: ids })} />

        <div>
          <p className="label mb-1">checklist</p>
          {r.checklist.length > 0 && (
            <ul className="mb-2 flex flex-col gap-1">
              {r.checklist.map((c) => (
                <li key={c.id} className="flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={c.done}
                    disabled={ro}
                    aria-label={`Done: ${c.text}`}
                    onChange={(e) => set({ checklist: r.checklist.map((x) => (x.id === c.id ? { ...x, done: e.target.checked } : x)) })}
                  />
                  <span className={`min-w-0 flex-1 truncate ${c.done ? "text-muted line-through" : ""}`}>{c.text}</span>
                  <button
                    type="button"
                    disabled={ro}
                    onClick={() => set({ checklist: r.checklist.filter((x) => x.id !== c.id) })}
                    aria-label={`Remove “${c.text}”`}
                    className="font-mono text-xs text-muted hover:text-bad disabled:opacity-50"
                  >
                    ×
                  </button>
                </li>
              ))}
            </ul>
          )}
          {r.checklist.length < MAX_CHECKLIST && (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                if (!item.trim()) return;
                set({ checklist: [...r.checklist, { id: clientId().slice(0, 12), text: item.trim(), done: false }] });
                setItem("");
              }}
              className="flex gap-2"
            >
              <input
                value={item}
                disabled={ro}
                maxLength={200}
                onChange={(e) => setItem(e.target.value)}
                placeholder="Add a step…"
                aria-label="New checklist item"
                className={`${inputClass} min-w-0 flex-1`}
              />
              <Button type="submit" className={small} disabled={ro || !item.trim()}>
                Add
              </Button>
            </form>
          )}
        </div>

        <div>
          <p className="label mb-1">links</p>
          <ul className="mb-2 flex flex-col gap-1">
            {r.links.map((l) => (
              <li key={l} className="flex items-center gap-2 text-sm">
                <a href={l} target="_blank" rel="noopener noreferrer" className="min-w-0 flex-1 truncate text-accent hover:underline">
                  {l}
                </a>
                <button
                  type="button"
                  disabled={ro}
                  onClick={() => set({ links: r.links.filter((x) => x !== l) })}
                  aria-label={`Remove link ${l}`}
                  className="font-mono text-xs text-muted hover:text-bad disabled:opacity-50"
                >
                  ×
                </button>
              </li>
            ))}
          </ul>
          {r.links.length < MAX_LINKS && (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                const v = link.trim();
                if (!/^https?:\/\//i.test(v) || r.links.includes(v)) return;
                set({ links: [...r.links, v] });
                setLink("");
              }}
              className="flex gap-2"
            >
              <input
                value={link}
                disabled={ro}
                maxLength={2000}
                onChange={(e) => setLink(e.target.value)}
                placeholder="https://…"
                aria-label="New link"
                className={`${inputClass} min-w-0 flex-1`}
              />
              <Button type="submit" className={small} disabled={ro || !/^https?:\/\//i.test(link.trim())}>
                Add
              </Button>
            </form>
          )}
        </div>

        <div className="grid grid-cols-2 gap-3">
          <label className="flex flex-col gap-1">
            <span className="label">project</span>
            <select value={r.project_id ?? ""} disabled={ro} onChange={(e) => set({ project_id: e.target.value || null })} aria-label="Project" className={inputClass}>
              <option value="">No project</option>
              {(projects.projects ?? [])
                .filter((p) => p.status === "active" || p.id === r.project_id)
                .map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
            </select>
          </label>
          <label className="flex flex-col gap-1">
            <span className="label">task</span>
            <select value={r.task_id ?? ""} disabled={ro} onChange={(e) => set({ task_id: e.target.value || null })} aria-label="Task" className={inputClass}>
              <option value="">No task</option>
              {openTasks.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.title}
                </option>
              ))}
            </select>
          </label>
        </div>
        {r.task_id && (
          <label className="flex items-center gap-2 text-sm">
            <input type="checkbox" checked={r.complete_with_task} disabled={ro} onChange={(e) => set({ complete_with_task: e.target.checked })} />
            Complete this reminder when the task is done
          </label>
        )}

        {history && history.length > 0 && (
          <div>
            <p className="label mb-1">history</p>
            <ul className="flex flex-col gap-0.5 font-mono text-[11px] text-muted">
              {history.slice(0, 10).map((h) => (
                <li key={h.id}>
                  {whenOf(h.at)} · {h.action}
                  {h.occurrence !== "-" ? ` ${h.occurrence}` : ""}
                  {h.until ? ` until ${whenOf(h.until)}` : ""}
                </li>
              ))}
            </ul>
          </div>
        )}

        <div className="flex items-center justify-between border-t border-line pt-3">
          <span className="font-mono text-[11px] text-muted">created {new Date(r.created_at).toLocaleDateString()}</span>
          {deleted ? (
            <span className="flex gap-1.5">
              <Button className={small} disabled={readOnly} onClick={() => void state.restore(r.id)}>
                Restore
              </Button>
              {confirmDelete ? (
                <Button variant="danger" className={small} onClick={() => void state.remove(r.id, true)}>
                  Confirm delete for good
                </Button>
              ) : (
                <Button variant="danger" className={small} disabled={readOnly} onClick={() => setConfirmDelete(true)}>
                  Delete for good
                </Button>
              )}
            </span>
          ) : (
            <Button variant="danger" className={small} disabled={readOnly} onClick={() => void state.remove(r.id)}>
              Delete
            </Button>
          )}
        </div>
      </div>
    </Panel>
  );
}
