import { useEffect, useMemo, useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import type { Spot } from "../../hooks/usePlanner";
import { allocationLabel, isTimed, layoutDay, type Allocation, type CalItem } from "../../lib/planner";
import { clockLabel, snap } from "../../lib/time";
import { dueInfo, PRIORITY_LABELS } from "../../lib/tasks";
import { itemTone, itemKindLabel, longDay, shortDay } from "./format";

/** MIME type for an unscheduled task dragged from the inbox. */
export const TASK_DRAG = "application/x-directory-task";

export interface TimeGridProps {
  days: string[];
  /** Filtered items for every day shown. */
  items: CalItem[];
  /** Visible minutes of the day, e.g. [0, 1440]. */
  range: [number, number];
  now: { date: string; minutes: number };
  /** The day view: wider columns and richer blocks. */
  rich: boolean;
  selectedKey?: string;
  /** Tap-to-schedule: the next tap on the grid places this. */
  placing?: string;
  allocs: Map<string, Allocation>;
  projectNames: Map<string, string>;
  defaultMinutes: number;
  onSelect: (item: CalItem) => void;
  onCreate: (spot: Spot) => void;
  onMove: (item: CalItem, to: Spot) => void;
  onDropTask: (taskId: string, date: string, start: number) => void;
  onPlace: (date: string, start: number) => void;
  onOpenDay: (date: string) => void;
  onToggleDone: (item: CalItem) => void;
}

interface Drag {
  item: CalItem & { start: number; end: number };
  mode: "move" | "resize";
  pointerId: number;
  x0: number;
  y0: number;
  day: number;
  colWidth: number;
  moved: boolean;
  preview: Spot;
}

const SNAP = 15;

/** "9 AM" / "21" (the hour only, in the user's locale). */
const hourLabel = (h: number) => new Date(Date.UTC(2000, 0, 1, h)).toLocaleTimeString([], { hour: "numeric", timeZone: "UTC" });

/** The week and day views: one time-positioned grid and scheduling engine (D-047). */
export function TimeGrid(props: TimeGridProps) {
  const { days, items, range, now, rich } = props;
  const ppm = rich ? 1.1 : 0.8; // px per minute
  const height = (range[1] - range[0]) * ppm;
  const scroller = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<Drag>();
  const [sel, setSel] = useState<{ day: number; a: number; b: number }>();
  const dragged = useRef(false);
  const pointerType = useRef("mouse");

  const byDay = useMemo(() => days.map((d) => items.filter((i) => i.date === d)), [days, items]);
  const placed = useMemo(() => byDay.map((list) => layoutDay(list)), [byDay]);

  // Open at the current time (or the start of the working day).
  const first = days[0];
  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const at = days.includes(now.date) ? now.minutes - 90 : 8 * 60;
    el.scrollTop = Math.max(0, (at - range[0]) * ppm);
    // Only when the shown days or zoom change, not every minute.
  }, [first, days.length, ppm, range[0]]);

  const minutesAt = (clientY: number, el: HTMLElement) => {
    const r = el.getBoundingClientRect();
    return Math.max(range[0], Math.min(range[1], range[0] + (clientY - r.top) / ppm));
  };

  const colWidth = () => scroller.current?.querySelector("[role=group]")?.getBoundingClientRect().width ?? 100;

  // ---- Empty slots: select a range (mouse), tap (touch), or drop a task. ----

  const onColumnDown = (e: PointerEvent<HTMLDivElement>, day: number) => {
    pointerType.current = e.pointerType;
    if (e.target !== e.currentTarget || e.pointerType === "touch" || e.button !== 0) return;
    const m = minutesAt(e.clientY, e.currentTarget);
    if (props.placing) {
      props.onPlace(days[day], Math.floor(m / SNAP) * SNAP);
      return;
    }
    e.currentTarget.setPointerCapture(e.pointerId);
    setSel({ day, a: m, b: m });
  };

  const onColumnMove = (e: PointerEvent<HTMLDivElement>) => {
    if (sel) setSel({ ...sel, b: minutesAt(e.clientY, e.currentTarget) });
  };

  const onColumnUp = () => {
    if (!sel) return;
    const [lo, hi] = [Math.min(sel.a, sel.b), Math.max(sel.a, sel.b)];
    const start = Math.floor(lo / SNAP) * SNAP;
    const end = hi - lo < 10 ? Math.min(1440, start + props.defaultMinutes) : Math.max(start + SNAP, Math.ceil(hi / SNAP) * SNAP);
    setSel(undefined);
    props.onCreate({ date: days[sel.day], start, end });
  };

  const onColumnClick = (e: React.MouseEvent<HTMLDivElement>, day: number) => {
    // Mouse and pen use the pointer handlers; this is a tap.
    if (pointerType.current !== "touch" || e.target !== e.currentTarget) return;
    const m = Math.floor(minutesAt(e.clientY, e.currentTarget) / SNAP) * SNAP;
    if (props.placing) props.onPlace(days[day], m);
    else props.onCreate({ date: days[day], start: m, end: Math.min(1440, m + props.defaultMinutes) });
  };

  // ---- Items: drag to move, drag the bottom edge to resize, keys too. ----

  const onItemDown = (e: PointerEvent<HTMLElement>, item: CalItem & { start: number; end: number }, day: number) => {
    e.stopPropagation();
    pointerType.current = e.pointerType;
    if (e.button !== 0 || props.placing) return;
    // On touch, select first; a selected item can then be dragged (no accidental moves while scrolling).
    if (e.pointerType === "touch" && props.selectedKey !== item.key) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    const mode = (e.target as HTMLElement).dataset.resize ? "resize" : "move";
    setDrag({
      item,
      mode,
      pointerId: e.pointerId,
      x0: e.clientX,
      y0: e.clientY,
      day,
      colWidth: colWidth(),
      moved: false,
      preview: { date: item.date, start: item.start, end: item.end },
    });
  };

  const onItemMove = (e: PointerEvent<HTMLElement>) => {
    if (!drag || drag.pointerId !== e.pointerId) return;
    const dy = e.clientY - drag.y0;
    const dx = e.clientX - drag.x0;
    const dm = snap(dy / ppm, SNAP);
    const { item } = drag;
    let preview: Spot;
    if (drag.mode === "resize") {
      preview = { date: item.date, start: item.start, end: Math.max(item.start + SNAP, Math.min(1440, item.end + dm)) };
    } else {
      const dd = days.length > 1 ? Math.round(dx / drag.colWidth) : 0;
      const day = Math.max(0, Math.min(days.length - 1, drag.day + dd));
      const len = item.end - item.start;
      const start = Math.max(0, Math.min(1440 - len, item.start + dm));
      preview = { date: days[day], start, end: start + len };
    }
    setDrag({ ...drag, preview, moved: drag.moved || Math.abs(dy) > 4 || Math.abs(dx) > 4 });
  };

  const onItemUp = (e: PointerEvent<HTMLElement>) => {
    if (!drag || drag.pointerId !== e.pointerId) return;
    const { item, preview, moved } = drag;
    setDrag(undefined);
    const changed = preview.date !== item.date || preview.start !== item.start || preview.end !== item.end;
    if (moved) {
      dragged.current = true;
      if (changed) props.onMove(item, preview);
    }
  };

  const onItemClick = (item: CalItem) => {
    if (dragged.current) {
      dragged.current = false;
      return;
    }
    props.onSelect(item);
  };

  const onItemKey = (e: KeyboardEvent<HTMLElement>, item: CalItem & { start: number; end: number }, day: number) => {
    const len = item.end - item.start;
    let to: Spot | undefined;
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      const d = e.key === "ArrowUp" ? -SNAP : SNAP;
      to = e.shiftKey
        ? { date: item.date, start: item.start, end: Math.max(item.start + SNAP, Math.min(1440, item.end + d)) }
        : { date: item.date, start: Math.max(0, Math.min(1440 - len, item.start + d)), end: 0 };
    } else if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && days.length > 1) {
      const next = day + (e.key === "ArrowLeft" ? -1 : 1);
      if (next >= 0 && next < days.length) to = { date: days[next], start: item.start, end: item.end };
    }
    if (!to) return;
    e.preventDefault();
    if (!e.shiftKey && (e.key === "ArrowUp" || e.key === "ArrowDown")) to.end = to.start + len;
    if (to.date !== item.date || to.start !== item.start || to.end !== item.end) props.onMove(item, to);
  };

  const hours = [];
  for (let h = Math.ceil(range[0] / 60); h * 60 < range[1]; h++) hours.push(h);
  const cols = `3.5rem repeat(${days.length}, minmax(0, 1fr))`;
  const outside = (list: CalItem[]) => list.filter((i) => isTimed(i) && (i.end <= range[0] || i.start >= range[1])).length;

  return (
    <div className="flex flex-col overflow-hidden rounded-md border border-line">
      {/* Day headers */}
      <div className="grid border-b border-line bg-panel" style={{ gridTemplateColumns: cols }}>
        <div />
        {days.map((d, i) => (
          <button
            key={d}
            type="button"
            onClick={() => props.onOpenDay(d)}
            aria-current={d === now.date ? "date" : undefined}
            aria-label={`${longDay(d)}${rich ? "" : ", open the day"}`}
            className={`flex min-h-11 flex-col items-center justify-center border-l border-line py-1 ${d === now.date ? "text-accent" : "text-muted"} hover:bg-sunken`}
          >
            <span className="font-mono text-[10px] tracking-wider uppercase">{shortDay(d)}</span>
            <span className={`text-sm font-medium ${d === now.date ? "grid size-7 place-items-center rounded-full bg-accent text-accent-ink" : "text-ink"}`}>
              {Number(d.slice(8))}
            </span>
            {outside(byDay[i]) > 0 && <span className="font-mono text-[9px] text-muted">+{outside(byDay[i])} outside hours</span>}
          </button>
        ))}
      </div>

      {/* All-day items and deadlines */}
      <div className="grid border-b border-line bg-panel" style={{ gridTemplateColumns: cols }}>
        <div className="label self-center px-1 text-[9px]">all day</div>
        {byDay.map((list, i) => (
          <ul key={days[i]} className="flex min-h-8 min-w-0 flex-col gap-0.5 border-l border-line p-0.5">
            {list
              .filter((x) => !isTimed(x))
              .map((x) => (
                <li key={x.key} className="min-w-0">
                  <button
                    type="button"
                    onClick={() => props.onSelect(x)}
                    aria-label={`${x.title}, ${itemKindLabel(x)}${x.done ? ", done" : ""}`}
                    className={`block w-full truncate rounded-sm px-1.5 py-0.5 text-left text-[11px] leading-4 ${itemTone(x)} ${props.selectedKey === x.key ? "outline-2 outline-accent" : ""}`}
                  >
                    {x.kind === "deadline" ? "◆ " : ""}
                    <span className={x.done ? "line-through" : ""}>{x.title}</span>
                  </button>
                </li>
              ))}
          </ul>
        ))}
      </div>

      {/* The time grid */}
      <div ref={scroller} className="relative max-h-[min(68dvh,760px)] overflow-y-auto overscroll-contain bg-panel">
        {props.placing && (
          <p role="status" className="sticky top-0 z-30 border-b border-accent bg-accent px-3 py-1.5 text-center text-xs text-accent-ink">
            Tap a time to schedule “{props.placing}”
          </p>
        )}
        <div className="relative grid" style={{ gridTemplateColumns: cols, height }}>
          <div className="relative" aria-hidden>
            {hours.map((h) => (
              <span key={h} className="absolute right-1.5 -translate-y-1/2 font-mono text-[10px] whitespace-nowrap text-muted" style={{ top: (h * 60 - range[0]) * ppm }}>
                {h === 0 ? "" : hourLabel(h)}
              </span>
            ))}
          </div>
          <div className="contents">
            {days.map((d, day) => (
              <div
                key={d}
                role="group"
                aria-label={longDay(d)}
                onPointerDown={(e) => onColumnDown(e, day)}
                onPointerMove={onColumnMove}
                onPointerUp={onColumnUp}
                onClick={(e) => onColumnClick(e, day)}
                onDragOver={(e) => {
                  if (e.dataTransfer.types.includes(TASK_DRAG)) {
                    e.preventDefault();
                    e.dataTransfer.dropEffect = "copy";
                  }
                }}
                onDrop={(e) => {
                  const id = e.dataTransfer.getData(TASK_DRAG);
                  if (!id) return;
                  e.preventDefault();
                  props.onDropTask(id, d, Math.floor(minutesAt(e.clientY, e.currentTarget) / SNAP) * SNAP);
                }}
                className={`relative border-l border-line ${d === now.date ? "bg-accent/[0.03]" : ""} ${props.placing ? "cursor-copy" : "cursor-cell"}`}
                style={{
                  backgroundImage: "linear-gradient(to bottom, var(--color-line) 1px, transparent 1px)",
                  backgroundSize: `100% ${60 * ppm}px`,
                  backgroundPositionY: `${((60 - (range[0] % 60)) % 60) * ppm}px`,
                }}
              >
                {/* Range being selected */}
                {sel?.day === day && (
                  <div
                    aria-hidden
                    className="pointer-events-none absolute inset-x-1 rounded-sm border border-accent bg-accent/15"
                    style={{ top: (Math.min(sel.a, sel.b) - range[0]) * ppm, height: Math.max(4, Math.abs(sel.b - sel.a) * ppm) }}
                  />
                )}

                {placed[day].map(({ item, col, cols: n }) => {
                  if (item.end <= range[0] || item.start >= range[1]) return null;
                  const top = (Math.max(item.start, range[0]) - range[0]) * ppm;
                  const h = Math.max(18, (Math.min(item.end, range[1]) - Math.max(item.start, range[0])) * ppm - 2);
                  const dragging = drag?.item.key === item.key;
                  const selected = props.selectedKey === item.key;
                  return (
                    <GridItem
                      key={item.key}
                      item={item}
                      style={{ top, height: h, left: `calc(${(col / n) * 100}% + 2px)`, width: `calc(${100 / n}% - 4px)` }}
                      selected={selected}
                      faded={dragging}
                      rich={rich}
                      tall={h >= 56}
                      allocation={item.taskId ? props.allocs.get(item.taskId) : undefined}
                      project={item.projectId ? props.projectNames.get(item.projectId) : undefined}
                      today={now.date}
                      onPointerDown={(e) => onItemDown(e, item, day)}
                      onPointerMove={onItemMove}
                      onPointerUp={onItemUp}
                      onPointerCancel={() => setDrag(undefined)}
                      onClick={() => onItemClick(item)}
                      onKeyDown={(e) => onItemKey(e, item, day)}
                      onToggleDone={() => props.onToggleDone(item)}
                    />
                  );
                })}

                {/* Where a dragged item will land */}
                {drag && drag.moved && drag.preview.date === d && (
                  <div
                    aria-hidden
                    className="pointer-events-none absolute inset-x-1 z-20 rounded-sm border-2 border-accent bg-accent/20 px-1.5 py-0.5 text-[11px] font-medium text-ink shadow-lg"
                    style={{ top: (drag.preview.start - range[0]) * ppm, height: Math.max(18, (drag.preview.end - drag.preview.start) * ppm - 2) }}
                  >
                    {clockLabel(drag.preview.start)}–{clockLabel(drag.preview.end)} · {drag.item.title}
                  </div>
                )}

                {d === now.date && now.minutes >= range[0] && now.minutes <= range[1] && (
                  <div aria-hidden className="pointer-events-none absolute inset-x-0 z-10 h-0.5 bg-bad" style={{ top: (now.minutes - range[0]) * ppm }}>
                    <span className="absolute -top-1 -left-1 size-2.5 rounded-full bg-bad" />
                  </div>
                )}
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

function GridItem(props: {
  item: CalItem & { start: number; end: number };
  style: React.CSSProperties;
  selected: boolean;
  faded: boolean;
  rich: boolean;
  tall: boolean;
  allocation?: Allocation;
  project?: string;
  today: string;
  onPointerDown: (e: PointerEvent<HTMLElement>) => void;
  onPointerMove: (e: PointerEvent<HTMLElement>) => void;
  onPointerUp: (e: PointerEvent<HTMLElement>) => void;
  onPointerCancel: () => void;
  onClick: () => void;
  onKeyDown: (e: KeyboardEvent<HTMLElement>) => void;
  onToggleDone: () => void;
}) {
  const { item, rich, tall, allocation: a } = props;
  const time = `${clockLabel(item.start)}–${clockLabel(item.end)}`;
  const task = item.task;
  const due = task ? dueInfo(task.due_date, props.today) : undefined;
  const progress = a?.estimate ? Math.min(100, Math.round((a.scheduled / a.estimate) * 100)) : undefined;
  const label = `${item.title}, ${time}, ${itemKindLabel(item)}${item.done ? ", done" : ""}${item.skipped ? ", skipped" : ""}${item.zone ? `, set in ${item.zone}` : ""}`;
  return (
    <div
      className={`absolute z-[1] overflow-hidden rounded-sm text-[11px] leading-tight select-none ${itemTone(item)} ${props.selected ? "z-[5] shadow-md ring-2 ring-accent" : ""} ${
        props.faded ? "opacity-40" : ""
      } ${item.done ? "opacity-70" : ""} ${item.skipped ? "opacity-45" : ""}`}
      style={props.style}
    >
      <div
        role="button"
        tabIndex={0}
        aria-label={label}
        aria-pressed={props.selected}
        aria-keyshortcuts="ArrowUp ArrowDown ArrowLeft ArrowRight Shift+ArrowUp Shift+ArrowDown"
        onPointerDown={props.onPointerDown}
        onPointerMove={props.onPointerMove}
        onPointerUp={props.onPointerUp}
        onPointerCancel={props.onPointerCancel}
        onClick={props.onClick}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            props.onClick();
          } else props.onKeyDown(e);
        }}
        className={`absolute inset-0 flex flex-col px-1.5 py-0.5 text-left focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent ${
          item.kind === "block" ? "pl-6" : ""
        } ${props.selected ? "cursor-grab" : "cursor-pointer"}`}
        style={{ touchAction: props.selected ? "none" : "auto" }}
      >
      <span className={`flex items-start gap-1 font-medium ${item.done ? "line-through" : ""}`}>
        {item.fixed && item.kind !== "deadline" && <span aria-hidden>🔒︎</span>}
        <span className="min-w-0 flex-1 truncate">{item.title}</span>
      </span>
      {(tall || rich) && <span className="truncate font-mono text-[10px] opacity-80">{time}{item.zone ? " ⌖" : ""}</span>}
      {rich && tall && (
        <span className="mt-0.5 flex flex-col gap-0.5 text-[11px] opacity-90">
          {(props.project || (task && task.priority !== "none") || due) && (
            <span className="truncate">
              {[props.project, task && task.priority !== "none" ? `${PRIORITY_LABELS[task.priority]} priority` : "", due ? `Due ${due.label}` : ""].filter(Boolean).join(" · ")}
            </span>
          )}
          {a && <span className="truncate">{allocationLabel(a)}</span>}
          {progress !== undefined && (
            <span aria-hidden className="h-1 overflow-hidden rounded-full bg-current/20">
              <span className="block h-full bg-current" style={{ width: `${progress}%` }} />
            </span>
          )}
          {(item.block?.notes || item.event?.notes || task?.notes) && (
            <span className="line-clamp-2 opacity-80">{item.block?.notes || item.event?.notes || task?.notes}</span>
          )}
        </span>
      )}
      {props.selected && (
        <span
          data-resize="1"
          aria-hidden
          className="absolute inset-x-0 bottom-0 h-2.5 cursor-ns-resize bg-current/25"
          title="Drag to change the length"
        />
      )}
      </div>
      {item.kind === "block" && (
        <button
          type="button"
          aria-label={item.done ? `Mark “${item.title}” not done` : `Mark session “${item.title}” done`}
          onPointerDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.stopPropagation();
            props.onToggleDone();
          }}
          className={`absolute top-0.5 left-1 z-[2] grid size-4 place-items-center rounded-full border text-[9px] ${item.done ? "border-ok bg-ok text-white" : "border-current"}`}
        >
          {item.done ? "✓" : ""}
        </button>
      )}
    </div>
  );
}
