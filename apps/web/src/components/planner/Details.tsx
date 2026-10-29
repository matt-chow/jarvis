import { useMemo, useState, type FormEvent, type ReactNode } from "react";
import type { Planner, Spot } from "../../hooks/usePlanner";
import { BLOCK_STATUS_LABELS, type BlockStatus } from "../../lib/blocks";
import { addDays, dayNumber, REPEAT_LABELS, repeatLabel, type CalendarEvent, type NewEvent, type Repeat, type SplitScope } from "../../lib/calendar";
import { allocationLabel, atRisk, conflicts, conflictText, isTimed, suggestSlots, type CalItem, type Hours } from "../../lib/planner";
import { dueInfo, isOpen, parseEstimate, PRIORITY_LABELS, type Task, type TaskPriority } from "../../lib/tasks";
import { clockLabel, durationLabel, fromMinutes, toMinutes } from "../../lib/time";
import { Button, inputClass } from "../ui";
import { itemKindLabel, longDay, shortDate } from "./format";

const small = "min-h-8 px-3 text-xs";

export type EntryKind = "event" | "task" | "session" | "focus";

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex min-w-0 flex-col gap-1">
      <span className="label">{label}</span>
      {children}
    </label>
  );
}

/** Live overlap warning for a spot (allowed, never blocked). */
function Overlaps({ spot, items, ignore }: { spot?: Spot; items: CalItem[]; ignore?: (i: CalItem) => boolean }) {
  if (!spot || spot.end <= spot.start) return null;
  const found = conflicts({ ...spot, ignore }, items);
  if (!found.length) return null;
  return (
    <p role="status" className="rounded-sm border border-warn/40 bg-warn/10 px-2.5 py-1.5 text-xs text-warn">
      {conflictText(found)} You can still save it.
    </p>
  );
}

function ScopePicker({ value, onChange, first }: { value: "all" | SplitScope; onChange: (v: "all" | SplitScope) => void; first: boolean }) {
  return (
    <fieldset className="flex flex-col gap-1 rounded-sm border border-line p-2.5">
      <legend className="label px-1">repeating event: change</legend>
      {(
        [
          ["one", "This occurrence only"],
          ["following", first ? "This and following (the whole series)" : "This and following"],
          ["all", "Every occurrence"],
        ] as const
      ).map(([v, label]) => (
        <label key={v} className="flex min-h-8 items-center gap-2 text-sm">
          <input type="radio" name="scope" checked={value === v} onChange={() => onChange(v)} />
          {label}
        </label>
      ))}
    </fieldset>
  );
}

// ---------------------------------------------------------------------------
// Create / edit

export function EntryForm(props: {
  planner: Planner;
  items: CalItem[];
  /** Creating: the selected slot. */
  spot?: Spot;
  kind?: EntryKind;
  /** Editing an event occurrence or a block. */
  item?: CalItem;
  onDone: () => void;
}) {
  const { planner, items, item } = props;
  const editingEvent = item?.kind === "event" ? item.event : undefined;
  const editingBlock = item?.kind === "block" ? item.block : undefined;
  const [kind, setKind] = useState<EntryKind>(editingEvent ? "event" : editingBlock ? (editingBlock.task_id ? "session" : "focus") : (props.kind ?? "event"));
  const [more, setMore] = useState(!!item);

  const startSpot = props.spot ?? (item && isTimed(item) ? { date: item.date, start: item.start, end: item.end } : undefined);
  const [title, setTitle] = useState(editingEvent?.title ?? editingBlock?.title ?? "");
  const [date, setDate] = useState(editingEvent ? (item?.occurrence ?? editingEvent.start_date) : (startSpot?.date ?? props.spot?.date ?? ""));
  const [endDate, setEndDate] = useState(
    editingEvent ? addDays(item?.occurrence ?? editingEvent.start_date, dayNumber(editingEvent.end_date) - dayNumber(editingEvent.start_date)) : (startSpot?.date ?? ""),
  );
  const [allDay, setAllDay] = useState(editingEvent ? !editingEvent.start_time : !startSpot);
  const [start, setStart] = useState(editingEvent?.start_time ?? (startSpot ? fromMinutes(startSpot.start) : "09:00"));
  const [end, setEnd] = useState(editingEvent?.end_time ?? (startSpot ? fromMinutes(Math.min(startSpot.end, 1439)) : "10:00"));
  const [fixed, setFixed] = useState(editingEvent ? editingEvent.fixed !== false : (editingBlock?.fixed ?? false));
  const [eventFixed, setEventFixed] = useState(editingEvent ? editingEvent.fixed !== false : true);
  const [location, setLocation] = useState(editingEvent?.location ?? "");
  const [notes, setNotes] = useState(editingEvent?.notes ?? editingBlock?.notes ?? "");
  const [repeat, setRepeat] = useState<Repeat>(editingEvent?.repeat ?? "none");
  const [every, setEvery] = useState(String(editingEvent?.repeat_interval ?? 1));
  const [ends, setEnds] = useState<"never" | "on" | "after">(editingEvent?.repeat_count ? "after" : editingEvent?.repeat_until ? "on" : "never");
  const [until, setUntil] = useState(editingEvent?.repeat_until ?? "");
  const [count, setCount] = useState(String(editingEvent?.repeat_count ?? 10));
  const [scope, setScope] = useState<"all" | SplitScope>(editingEvent && editingEvent.repeat !== "none" ? "one" : "all");
  // Tasks
  const [due, setDue] = useState(props.spot?.date ?? "");
  const [estimate, setEstimate] = useState("");
  const [priority, setPriority] = useState<TaskPriority>("none");
  const [project, setProject] = useState("");
  const [taskRepeat, setTaskRepeat] = useState<Repeat>("none");
  const [scheduleNow, setScheduleNow] = useState(!!props.spot);
  // Sessions
  const openTasks = useMemo(() => (planner.tasks.tasks ?? []).filter((t) => isOpen(t) && !t.parent_id), [planner.tasks.tasks]);
  const [taskId, setTaskId] = useState(editingBlock?.task_id ?? openTasks[0]?.id ?? "");
  const [status, setStatus] = useState<BlockStatus>(editingBlock?.status ?? "planned");
  const [actual, setActual] = useState(editingBlock?.actual_minutes != null ? String(editingBlock.actual_minutes) : "");
  const [busy, setBusy] = useState(false);

  const timed = kind !== "event" || !allDay;
  const spot: Spot | undefined = timed && date && start && end ? { date, start: toMinutes(start), end: end === "00:00" ? 1440 : toMinutes(end) } : undefined;
  const ignore = (i: CalItem) => !!item && i.kind === item.kind && i.id === item.id && (item.kind !== "event" || i.occurrence === item.occurrence);
  const estimateValue = parseEstimate(estimate);
  const n = Number(every);
  const invalid =
    (kind !== "session" && !title.trim()) ||
    (kind === "session" && !taskId) ||
    (kind !== "task" && !date) ||
    (timed && kind !== "task" && (!spot || spot.end <= spot.start)) ||
    (kind === "event" && endDate < date) ||
    (kind === "event" && repeat !== "none" && (!Number.isInteger(n) || n < 1 || n > 99)) ||
    (kind === "task" && (estimateValue === undefined || (taskRepeat !== "none" && !due))) ||
    (kind === "task" && scheduleNow && (!spot || spot.end <= spot.start));

  const eventFields = (): NewEvent => ({
    title,
    start_date: date,
    end_date: endDate < date ? date : endDate,
    start_time: allDay ? null : start,
    // Events can't end at 24:00; a midnight end is the last minute of the day.
    end_time: allDay ? null : end === "00:00" ? "23:59" : end,
    location,
    notes,
    repeat,
    repeat_interval: repeat === "none" ? 1 : n,
    repeat_until: repeat !== "none" && ends === "on" && until ? until : null,
    repeat_count: repeat !== "none" && ends === "after" ? Math.max(1, Math.min(500, Number(count) || 1)) : null,
    fixed: eventFixed,
    timezone: allDay ? null : planner.zone,
  });

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (invalid || busy) return;
    setBusy(true);
    try {
      if (kind === "event") {
        const fields = eventFields();
        if (editingEvent) {
          let save = fields;
          // "Every occurrence": move the series by as much as this occurrence moved.
          if (scope === "all" && item?.occurrence) {
            const shift = dayNumber(date) - dayNumber(item.occurrence);
            save = { ...fields, start_date: addDays(editingEvent.start_date, shift), end_date: addDays(editingEvent.start_date, shift + dayNumber(fields.end_date!) - dayNumber(date)) };
          }
          await planner.saveEvent(editingEvent, save, scope, item?.occurrence);
        } else await planner.createEvent(fields, items);
      } else if (kind === "task") {
        await planner.createTask(
          {
            title,
            notes,
            due_date: due || null,
            estimate_minutes: estimateValue ?? null,
            priority,
            ...(project ? { project_id: project } : {}),
            ...(taskRepeat !== "none" ? { repeat: taskRepeat } : {}),
          },
          scheduleNow ? spot : undefined,
          items,
        );
      } else if (editingBlock) {
        await planner.updateBlock(
          editingBlock,
          {
            task_id: kind === "session" ? taskId : null,
            title,
            date,
            start_time: start,
            end_time: fromMinutes(spot!.end),
            fixed,
            notes,
            status,
            actual_minutes: status === "done" && actual !== "" ? Math.max(0, Math.min(1440, Number(actual) || 0)) : status === "done" ? editingBlock.actual_minutes : null,
            timezone: planner.zone,
          },
          title || item!.title,
          items,
        );
      } else {
        const t = openTasks.find((x) => x.id === taskId);
        await planner.createBlock(
          { task_id: kind === "session" ? taskId : null, title, date, start_time: start, end_time: fromMinutes(spot!.end), fixed, notes },
          items,
          kind === "session" ? `a session for “${t?.title ?? "the task"}”` : `“${title}”`,
        );
      }
      props.onDone();
    } finally {
      setBusy(false);
    }
  };

  const kinds: [EntryKind, string][] = item ? [] : [
    ["event", "Event"],
    ["task", "Task"],
    ["session", "Work session"],
    ["focus", "Focus block"],
  ];

  return (
    <form onSubmit={(e) => void submit(e)} className="flex flex-col gap-3" aria-label={item ? `Edit ${item.title}` : "New entry"}>
      {kinds.length > 0 && (
        <div role="radiogroup" aria-label="What to create" className="flex flex-wrap gap-1.5">
          {kinds.map(([k, label]) => (
            <button
              key={k}
              type="button"
              role="radio"
              aria-checked={kind === k}
              onClick={() => setKind(k)}
              className={`min-h-8 rounded-full border px-3 text-xs ${kind === k ? "border-ink bg-ink text-canvas" : "border-line hover:bg-sunken"}`}
            >
              {label}
            </button>
          ))}
        </div>
      )}

      {kind === "session" ? (
        <Field label="task">
          <select value={taskId} onChange={(e) => setTaskId(e.target.value)} aria-label="Task for this session" className={inputClass} required>
            {openTasks.length === 0 && <option value="">No open tasks</option>}
            {openTasks.map((t) => (
              <option key={t.id} value={t.id}>
                {t.title}
              </option>
            ))}
          </select>
        </Field>
      ) : null}
      {(kind !== "session" || more) && (
        <Field label={kind === "session" ? "label (optional)" : "title"}>
          <input
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            maxLength={200}
            required={kind !== "session"}
            aria-label={kind === "task" ? "Task title" : kind === "event" ? "Event title" : "Title"}
            autoFocus={!item}
            className={inputClass}
          />
        </Field>
      )}

      {kind === "task" && (
        <div className="grid grid-cols-2 gap-2">
          <Field label="due">
            <input type="date" value={due} onChange={(e) => setDue(e.target.value)} aria-label="Due date" className={inputClass} />
          </Field>
          <Field label="estimate (min)">
            <input
              value={estimate}
              onChange={(e) => setEstimate(e.target.value)}
              inputMode="numeric"
              placeholder="e.g. 90"
              aria-label="Estimate in minutes"
              aria-invalid={estimateValue === undefined}
              className={inputClass}
            />
          </Field>
        </div>
      )}

      {(kind !== "task" || scheduleNow) && (
        <>
          <div className="grid grid-cols-2 gap-2">
            <Field label={kind === "event" && !allDay && endDate !== date ? "starts" : "date"}>
              <input
                type="date"
                value={date}
                onChange={(e) => {
                  setDate(e.target.value);
                  if (endDate < e.target.value) setEndDate(e.target.value);
                }}
                required
                aria-label={kind === "task" ? "Session date" : "Date"}
                className={inputClass}
              />
            </Field>
            {kind === "event" && more ? (
              <Field label="ends">
                <input type="date" value={endDate} min={date} onChange={(e) => setEndDate(e.target.value)} aria-label="End date" className={inputClass} />
              </Field>
            ) : (
              <div />
            )}
          </div>
          {kind === "event" && (
            <label className="flex min-h-8 items-center gap-2 text-sm">
              <input type="checkbox" checked={allDay} onChange={(e) => setAllDay(e.target.checked)} />
              All day
            </label>
          )}
          {timed && (
            <div className="grid grid-cols-2 gap-2">
              <Field label="from">
                <input type="time" value={start} step={300} onChange={(e) => setStart(e.target.value)} required aria-label="Start time" className={inputClass} />
              </Field>
              <Field label="to">
                <input type="time" value={end} step={300} onChange={(e) => setEnd(e.target.value)} required aria-label="End time" className={inputClass} />
              </Field>
            </div>
          )}
          {spot && spot.end > spot.start && (
            <p className="font-mono text-[11px] text-muted">
              {durationLabel(spot.end - spot.start)} · {longDay(date)}
            </p>
          )}
          <Overlaps spot={timed ? spot : undefined} items={items} ignore={ignore} />
        </>
      )}

      {kind === "task" && (
        <label className="flex min-h-8 items-center gap-2 text-sm">
          <input type="checkbox" checked={scheduleNow} onChange={(e) => setScheduleNow(e.target.checked)} />
          Schedule a work session now
        </label>
      )}

      {kind === "event" ? (
        <label className="flex min-h-8 items-start gap-2 text-sm">
          <input type="checkbox" className="mt-1" checked={eventFixed} onChange={(e) => setEventFixed(e.target.checked)} />
          <span>
            Fixed commitment
            <span className="block text-xs text-muted">Meetings, classes, appointments. Suggestions always work around it.</span>
          </span>
        </label>
      ) : kind !== "task" ? (
        <label className="flex min-h-8 items-start gap-2 text-sm">
          <input type="checkbox" className="mt-1" checked={fixed} onChange={(e) => setFixed(e.target.checked)} />
          <span>
            Fixed time
            <span className="block text-xs text-muted">Off: flexible work you can move or reschedule.</span>
          </span>
        </label>
      ) : null}

      {!more && (
        <button type="button" onClick={() => setMore(true)} className="self-start text-xs text-accent underline-offset-2 hover:underline">
          More options
        </button>
      )}

      {more && kind === "event" && (
        <>
          <div className="grid grid-cols-2 gap-2">
            <Field label="repeat">
              <select value={repeat} onChange={(e) => setRepeat(e.target.value as Repeat)} aria-label="Repeat" className={inputClass}>
                {(Object.keys(REPEAT_LABELS) as Repeat[]).map((r) => (
                  <option key={r} value={r}>
                    {REPEAT_LABELS[r]}
                  </option>
                ))}
              </select>
            </Field>
            {repeat !== "none" && (
              <Field label="every">
                <input value={every} onChange={(e) => setEvery(e.target.value)} inputMode="numeric" aria-label="Repeat every N" className={inputClass} />
              </Field>
            )}
          </div>
          {repeat !== "none" && (
            <div className="grid grid-cols-2 gap-2">
              <Field label="ends">
                <select value={ends} onChange={(e) => setEnds(e.target.value as typeof ends)} aria-label="Repeat ends" className={inputClass}>
                  <option value="never">Never</option>
                  <option value="on">On a date</option>
                  <option value="after">After a number of times</option>
                </select>
              </Field>
              {ends === "on" && (
                <Field label="last date">
                  <input type="date" value={until} min={date} onChange={(e) => setUntil(e.target.value)} aria-label="Repeat until" className={inputClass} />
                </Field>
              )}
              {ends === "after" && (
                <Field label="times">
                  <input value={count} onChange={(e) => setCount(e.target.value)} inputMode="numeric" aria-label="Number of occurrences" className={inputClass} />
                </Field>
              )}
            </div>
          )}
          <Field label="location">
            <input value={location} onChange={(e) => setLocation(e.target.value)} maxLength={200} aria-label="Location" className={inputClass} />
          </Field>
        </>
      )}

      {more && kind === "task" && (
        <div className="grid grid-cols-2 gap-2">
          <Field label="priority">
            <select value={priority} onChange={(e) => setPriority(e.target.value as TaskPriority)} aria-label="Priority" className={inputClass}>
              {(Object.keys(PRIORITY_LABELS) as TaskPriority[]).map((p) => (
                <option key={p} value={p}>
                  {PRIORITY_LABELS[p]}
                </option>
              ))}
            </select>
          </Field>
          <Field label="project">
            <select value={project} onChange={(e) => setProject(e.target.value)} aria-label="Project" className={inputClass}>
              <option value="">None</option>
              {(planner.projects.projects ?? [])
                .filter((p) => p.status === "active")
                .map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name}
                  </option>
                ))}
            </select>
          </Field>
          <Field label="repeat">
            <select value={taskRepeat} onChange={(e) => setTaskRepeat(e.target.value as Repeat)} aria-label="Task repeats" className={inputClass}>
              {(Object.keys(REPEAT_LABELS) as Repeat[]).map((r) => (
                <option key={r} value={r}>
                  {REPEAT_LABELS[r]}
                </option>
              ))}
            </select>
          </Field>
          {taskRepeat !== "none" && !due && <p className="col-span-2 text-xs text-warn">A repeating task needs a due date.</p>}
        </div>
      )}

      {more && editingBlock && (
        <div className="grid grid-cols-2 gap-2">
          <Field label="status">
            <select value={status} onChange={(e) => setStatus(e.target.value as BlockStatus)} aria-label="Session status" className={inputClass}>
              {(Object.keys(BLOCK_STATUS_LABELS) as BlockStatus[]).map((s) => (
                <option key={s} value={s}>
                  {BLOCK_STATUS_LABELS[s]}
                </option>
              ))}
            </select>
          </Field>
          {status === "done" && (
            <Field label="actual (min)">
              <input value={actual} onChange={(e) => setActual(e.target.value)} inputMode="numeric" aria-label="Actual minutes" className={inputClass} />
            </Field>
          )}
        </div>
      )}

      {more && (
        <Field label="notes">
          <textarea value={notes} onChange={(e) => setNotes(e.target.value)} maxLength={5000} rows={3} aria-label="Notes" className={`${inputClass} resize-y py-2`} />
        </Field>
      )}

      {editingEvent && editingEvent.repeat !== "none" && item?.occurrence && (
        <ScopePicker value={scope} onChange={setScope} first={item.occurrence === editingEvent.start_date} />
      )}

      <div className="flex flex-wrap gap-1.5 pt-1">
        <Button type="submit" variant="primary" className={small} disabled={invalid || busy}>
          {item ? "save" : kind === "task" ? (scheduleNow ? "add and schedule" : "add task") : kind === "event" ? "add event" : "schedule"}
        </Button>
        <Button className={small} onClick={props.onDone}>
          cancel
        </Button>
      </div>
    </form>
  );
}

// ---------------------------------------------------------------------------
// Suggestions for a task (or moving a flexible session)

export function ScheduleTask(props: {
  planner: Planner;
  task: Task;
  items: CalItem[];
  today: string;
  now: { date: string; minutes: number };
  placing: boolean;
  onPlace: () => void;
  onDone: () => void;
  /** Move this session instead of creating one. */
  moving?: CalItem;
}) {
  const { planner, task, moving } = props;
  const a = planner.allocs.get(task.id);
  const remaining = a?.remaining ?? null;
  const defaultLen = moving && isTimed(moving) ? moving.end - moving.start : Math.max(15, Math.min(remaining || planner.hours.session_minutes, planner.hours.session_minutes));
  const [length, setLength] = useState(String(defaultLen));
  const [horizon, setHorizon] = useState(task.due_date && task.due_date >= props.today ? "due" : "7");
  const minutes = Math.max(15, Math.min(600, Number(length) || defaultLen));
  const to = horizon === "due" && task.due_date ? task.due_date : addDays(props.today, Number(horizon) - 1);
  const ignore = moving ? (i: CalItem) => i.key === moving.key : undefined;
  const slots = useMemo(
    () =>
      suggestSlots({
        items: ignore ? props.items.filter((i) => !ignore(i)) : props.items,
        from: props.today,
        to: to < props.today ? props.today : to,
        minutes,
        hours: planner.hours,
        now: props.now,
        due: task.due_date,
      }),
    [props.items, props.today, to, minutes, planner.hours, props.now.date, task.due_date],
  );
  const [busy, setBusy] = useState(false);
  const due = dueInfo(task.due_date, props.today);

  const pick = async (s: { date: string; start: number; end: number }) => {
    setBusy(true);
    if (moving) await planner.move(moving, s, props.items);
    else
      await planner.createBlock({ task_id: task.id, date: s.date, start_time: fromMinutes(s.start), end_time: fromMinutes(s.end) }, props.items, `a session for “${task.title}”`);
    setBusy(false);
    props.onDone();
  };

  return (
    <div className="flex flex-col gap-3">
      <div>
        <h3 className="text-base font-semibold">{moving ? `Reschedule “${moving.title}”` : `Schedule “${task.title}”`}</h3>
        <p className="mt-1 text-sm text-muted">
          {a ? allocationLabel(a) : "Not scheduled yet"}
          {due && ` · due ${due.label}`}
        </p>
        {atRisk(task, a) && <p className="mt-1 text-sm text-warn">Not enough time is scheduled before it's due.</p>}
      </div>
      <div className="grid grid-cols-2 gap-2">
        <Field label="session (min)">
          <input value={length} onChange={(e) => setLength(e.target.value)} inputMode="numeric" aria-label="Session length in minutes" className={inputClass} />
        </Field>
        <Field label="look in">
          <select value={horizon} onChange={(e) => setHorizon(e.target.value)} aria-label="Look for time in" className={inputClass}>
            {task.due_date && task.due_date >= props.today && <option value="due">Until it's due</option>}
            <option value="3">Next 3 days</option>
            <option value="7">Next 7 days</option>
            <option value="14">Next 14 days</option>
          </select>
        </Field>
      </div>
      <p className="text-xs text-muted">
        Free time within your working hours ({planner.hours.day_start}–{planner.hours.day_end}), around everything already on the calendar. Nothing is
        scheduled until you choose.
      </p>
      {slots.length === 0 ? (
        <p className="text-sm text-muted">No free time in that range. Try a longer range or a shorter session.</p>
      ) : (
        <ul aria-label="Suggested times" className="flex flex-col gap-1.5">
          {slots.map((s) => (
            <li key={`${s.date}${s.start}`} className="flex items-center gap-2 rounded-sm border border-line p-2">
              <span className="min-w-0 flex-1">
                <span className="block text-sm font-medium">
                  {shortDate(s.date)} · {clockLabel(s.start)}–{clockLabel(s.end)}
                </span>
                <span className={`block text-xs ${s.late || s.short ? "text-warn" : "text-muted"}`}>{s.reason}</span>
              </span>
              <Button className={small} variant="primary" disabled={busy} onClick={() => void pick(s)} aria-label={`Schedule ${shortDate(s.date)} at ${clockLabel(s.start)}`}>
                {moving ? "move here" : "schedule"}
              </Button>
            </li>
          ))}
        </ul>
      )}
      <div className="flex flex-wrap gap-1.5">
        {!moving && (
          <Button className={small} onClick={props.onPlace} aria-pressed={props.placing}>
            {props.placing ? "cancel placing" : "tap a time instead"}
          </Button>
        )}
        <Button className={small} onClick={props.onDone}>
          close
        </Button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// A selected item

export function ItemDetails(props: {
  planner: Planner;
  item: CalItem;
  items: CalItem[];
  today: string;
  onEdit: () => void;
  onSchedule: (t: Task, moving?: CalItem) => void;
  onClose: () => void;
}) {
  const { planner, item, items } = props;
  const [confirm, setConfirm] = useState<"all" | SplitScope>();
  const [actual, setActual] = useState("");
  const task = item.task;
  const a = item.taskId ? planner.allocs.get(item.taskId) : undefined;
  const e: CalendarEvent | undefined = item.event;
  const b = item.block;
  const timed = isTimed(item);
  const nudge = (minutes: number, days = 0) => {
    if (!timed) return;
    const len = item.end! - item.start!;
    const start = Math.max(0, Math.min(1440 - len, item.start! + minutes));
    void planner.move(item, { date: addDays(item.date, days), start, end: start + len }, items);
    props.onClose();
  };
  const found = timed ? conflicts({ date: item.date, start: item.start!, end: item.end!, ignore: (i) => i.key === item.key }, items) : [];

  return (
    <div className="flex flex-col gap-3">
      <div>
        <p className="label">{itemKindLabel(item)}</p>
        <h3 className={`mt-1 text-lg font-semibold ${item.done ? "line-through" : ""}`}>{item.title}</h3>
        <p className="mt-1 font-mono text-xs text-muted">
          {longDay(item.date)}
          {timed ? ` · ${clockLabel(item.start!)}–${clockLabel(item.end!)} (${durationLabel(item.end! - item.start!)})` : item.kind === "event" ? " · all day" : ""}
        </p>
        {item.zone && <p className="mt-1 text-xs text-muted">Set in {item.zone}; shown in your time zone ({planner.zone}).</p>}
        {e && e.repeat !== "none" && <p className="mt-1 text-xs text-muted">{repeatLabel(e)}{e.repeat_until ? ` until ${e.repeat_until}` : ""}</p>}
        {e?.location && <p className="mt-1 text-sm">{e.location}</p>}
        {(e?.notes || b?.notes) && <p className="mt-2 text-sm whitespace-pre-wrap">{e?.notes || b?.notes}</p>}
      </div>

      {found.length > 0 && (
        <p role="status" className="rounded-sm border border-warn/40 bg-warn/10 px-2.5 py-1.5 text-xs text-warn">
          {conflictText(found)}
        </p>
      )}

      {task && (
        <div className="rounded-sm border border-line p-2.5 text-sm">
          <p className="font-medium">{item.kind === "deadline" ? "Task" : `Task: ${task.title}`}</p>
          <p className="text-xs text-muted">
            {[task.due_date ? `Due ${dueInfo(task.due_date, props.today)?.label}` : "", task.priority !== "none" ? `${PRIORITY_LABELS[task.priority]} priority` : "", isOpen(task) ? "" : "Done"]
              .filter(Boolean)
              .join(" · ")}
          </p>
          {a && <p className="mt-1 text-xs">{allocationLabel(a)}{a.actual ? ` · ${durationLabel(a.actual)} worked` : ""}</p>}
          {atRisk(task, a) && <p className="mt-1 text-xs text-warn">Not enough time scheduled before it's due.</p>}
          <div className="mt-2 flex flex-wrap gap-1.5">
            <Button className={small} onClick={() => void planner.setTaskDone(task, isOpen(task))}>
              {isOpen(task) ? "complete task" : "reopen task"}
            </Button>
            {isOpen(task) && (
              <Button className={small} onClick={() => props.onSchedule(task)}>
                plan time
              </Button>
            )}
            <a href={`#/tasks?task=${task.id}`} className="inline-flex min-h-8 items-center rounded-full border border-line px-3 text-xs hover:bg-sunken">
              Open in Tasks
            </a>
          </div>
        </div>
      )}

      {b && (
        <div className="flex flex-wrap items-end gap-1.5">
          {b.status !== "done" ? (
            <>
              <label className="flex flex-col gap-1">
                <span className="label">worked (min)</span>
                <input
                  value={actual}
                  onChange={(ev) => setActual(ev.target.value)}
                  placeholder={String(item.end! - item.start!)}
                  inputMode="numeric"
                  aria-label="Minutes actually worked"
                  className={`${inputClass} w-24`}
                />
              </label>
              <Button
                className={small}
                variant="primary"
                onClick={() =>
                  void planner.updateBlock(b, { status: "done", actual_minutes: actual ? Math.max(0, Math.min(1440, Number(actual) || 0)) : item.end! - item.start! }, item.title)
                }
              >
                session done
              </Button>
            </>
          ) : (
            <Button className={small} onClick={() => void planner.updateBlock(b, { status: "planned", actual_minutes: null }, item.title)}>
              mark not done
            </Button>
          )}
          {b.status !== "skipped" && b.status !== "done" && (
            <Button className={small} onClick={() => void planner.updateBlock(b, { status: "skipped" }, item.title)}>
              skip
            </Button>
          )}
          {b.status === "skipped" && (
            <Button className={small} onClick={() => void planner.updateBlock(b, { status: "planned" }, item.title)}>
              restore
            </Button>
          )}
        </div>
      )}

      {timed && (
        <div className="flex flex-col gap-1.5">
          <span className="label">move {e && e.repeat !== "none" ? "(this occurrence)" : ""}</span>
          <div className="flex flex-wrap gap-1.5">
            <Button className={small} onClick={() => nudge(-15)} aria-label="15 minutes earlier">
              −15 min
            </Button>
            <Button className={small} onClick={() => nudge(15)} aria-label="15 minutes later">
              +15 min
            </Button>
            <Button className={small} onClick={() => nudge(0, -1)} aria-label="One day earlier">
              −1 day
            </Button>
            <Button className={small} onClick={() => nudge(0, 1)} aria-label="One day later">
              +1 day
            </Button>
            {b && !item.fixed && task && (
              <Button className={small} onClick={() => props.onSchedule(task, item)}>
                find another time
              </Button>
            )}
          </div>
          <p className="text-[11px] text-muted">On a computer you can also drag it, drag its bottom edge, or use the arrow keys (Shift to resize).</p>
        </div>
      )}

      <div className="flex flex-wrap gap-1.5 border-t border-line pt-3">
        {item.kind !== "deadline" && (
          <Button className={small} onClick={props.onEdit}>
            edit
          </Button>
        )}
        {item.kind !== "deadline" &&
          (confirm ? (
            <span className="flex flex-wrap items-center gap-1.5">
              <span className="text-xs text-muted">
                {e && e.repeat !== "none" ? (confirm === "one" ? "Cancel this occurrence?" : confirm === "following" ? "Delete this and later ones?" : "Delete every occurrence?") : "Delete it?"}
              </span>
              <Button
                variant="danger"
                className={small}
                onClick={() => {
                  if (b) void planner.deleteBlock(b, item.title);
                  else if (e) void planner.deleteEvent(e, confirm, item.occurrence);
                  props.onClose();
                }}
              >
                confirm delete
              </Button>
              <Button className={small} onClick={() => setConfirm(undefined)}>
                keep
              </Button>
            </span>
          ) : e && e.repeat !== "none" ? (
            <>
              <Button variant="danger" className={small} onClick={() => setConfirm("one")}>
                cancel this one
              </Button>
              <Button variant="danger" className={small} onClick={() => setConfirm(item.occurrence === e.start_date ? "all" : "following")}>
                delete from here
              </Button>
              <Button variant="danger" className={small} onClick={() => setConfirm("all")}>
                delete all
              </Button>
            </>
          ) : (
            <Button variant="danger" className={small} onClick={() => setConfirm("all")}>
              delete
            </Button>
          ))}
        <Button className={small} onClick={props.onClose}>
          close
        </Button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// A day's list (month view), and planner settings

export function DayAgenda(props: { date: string; items: CalItem[]; onSelect: (i: CalItem) => void; onNew: () => void; onOpenDay: () => void }) {
  const list = props.items.filter((i) => i.date === props.date);
  return (
    <div className="flex flex-col gap-3">
      <h3 className="text-base font-semibold">{longDay(props.date)}</h3>
      {list.length === 0 ? (
        <p className="text-sm text-muted">Nothing planned.</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {list.map((i) => (
            <li key={i.key}>
              <button type="button" onClick={() => props.onSelect(i)} className="w-full rounded-sm px-2 py-1.5 text-left hover:bg-sunken">
                <span className={`block text-sm ${i.done ? "line-through" : ""}`}>
                  {i.kind === "deadline" ? "◆ " : ""}
                  {i.title}
                </span>
                <span className="font-mono text-[11px] text-muted">
                  {isTimed(i) ? `${clockLabel(i.start)}–${clockLabel(i.end)} · ` : i.kind === "deadline" ? "due · " : "all day · "}
                  {itemKindLabel(i)}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
      <div className="flex gap-1.5">
        <Button className={small} variant="primary" onClick={props.onNew}>
          + new
        </Button>
        <Button className={small} onClick={props.onOpenDay}>
          open day
        </Button>
      </div>
    </div>
  );
}

const DAY_NAMES = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

export function PlannerSettings(props: { hours: Hours; visible: [number, number]; onHours: (h: Hours) => void; onVisible: (v: [number, number]) => void }) {
  const [h, setH] = useState(props.hours);
  const changed = JSON.stringify(h) !== JSON.stringify(props.hours);
  const valid = toMinutes(h.day_end) > toMinutes(h.day_start) && h.session_minutes >= 15 && h.session_minutes <= 480;
  const hourOptions = Array.from({ length: 25 }, (_, i) => i);
  return (
    <div className="flex flex-col gap-3">
      <fieldset className="flex flex-col gap-2">
        <legend className="label mb-1">working hours (all your devices)</legend>
        <div className="grid grid-cols-2 gap-2">
          <Field label="start">
            <input type="time" value={h.day_start} onChange={(e) => setH({ ...h, day_start: e.target.value })} aria-label="Working day starts" className={inputClass} />
          </Field>
          <Field label="end">
            <input type="time" value={h.day_end} onChange={(e) => setH({ ...h, day_end: e.target.value })} aria-label="Working day ends" className={inputClass} />
          </Field>
        </div>
        <div role="group" aria-label="Working days" className="flex flex-wrap gap-1">
          {DAY_NAMES.map((d, i) => (
            <button
              key={d}
              type="button"
              aria-pressed={h.work_days.includes(i)}
              onClick={() => setH({ ...h, work_days: h.work_days.includes(i) ? h.work_days.filter((x) => x !== i) : [...h.work_days, i].sort((a, b) => a - b) })}
              className={`min-h-8 min-w-10 rounded-full border px-2 text-xs ${h.work_days.includes(i) ? "border-ink bg-ink text-canvas" : "border-line"}`}
            >
              {d}
            </button>
          ))}
        </div>
        <Field label="preferred session (min)">
          <input
            value={String(h.session_minutes)}
            onChange={(e) => setH({ ...h, session_minutes: Number(e.target.value) || 0 })}
            inputMode="numeric"
            aria-label="Preferred session length in minutes"
            className={inputClass}
          />
        </Field>
        <Button className={small} variant="primary" disabled={!changed || !valid} onClick={() => props.onHours(h)}>
          save working hours
        </Button>
      </fieldset>
      <fieldset className="flex flex-col gap-2">
        <legend className="label mb-1">visible hours (this device)</legend>
        <div className="grid grid-cols-2 gap-2">
          <Field label="from">
            <select
              value={props.visible[0] / 60}
              onChange={(e) => props.onVisible([Number(e.target.value) * 60, Math.max(props.visible[1], Number(e.target.value) * 60 + 60)])}
              aria-label="Show hours from"
              className={inputClass}
            >
              {hourOptions.slice(0, 24).map((x) => (
                <option key={x} value={x}>
                  {fromMinutes(x * 60)}
                </option>
              ))}
            </select>
          </Field>
          <Field label="to">
            <select
              value={props.visible[1] / 60}
              onChange={(e) => props.onVisible([Math.min(props.visible[0], Number(e.target.value) * 60 - 60), Number(e.target.value) * 60])}
              aria-label="Show hours until"
              className={inputClass}
            >
              {hourOptions.slice(1).map((x) => (
                <option key={x} value={x}>
                  {fromMinutes(x * 60)}
                </option>
              ))}
            </select>
          </Field>
        </div>
      </fieldset>
    </div>
  );
}
