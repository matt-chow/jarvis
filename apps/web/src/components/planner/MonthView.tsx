import { monthGrid } from "../../lib/calendar";
import type { CalItem } from "../../lib/planner";
import { clockLabel } from "../../lib/time";
import { itemKindLabel, itemTone, longDay } from "./format";

const WEEKDAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
/** Items listed in a cell before "+N more". */
const CELL_ITEMS = 3;

/** The month grid: events, sessions, and deadlines per day, from the shared items. */
export function MonthView(props: {
  month: string;
  items: CalItem[];
  today: string;
  selectedDate?: string;
  selectedKey?: string;
  onSelectDate: (date: string) => void;
  onOpenDay: (date: string) => void;
  onSelect: (item: CalItem) => void;
}) {
  const grid = monthGrid(props.month);
  return (
    <div aria-label="Month" className="grid grid-cols-7 gap-px overflow-hidden rounded-md border border-line bg-line">
      {WEEKDAYS.map((d) => (
        <div key={d} aria-hidden className="bg-panel py-1 text-center font-mono text-[10px] text-muted uppercase">
          {d}
        </div>
      ))}
      {grid.flat().map((d) => {
        const list = props.items.filter((i) => i.date === d);
        const inMonth = d.startsWith(props.month);
        const selected = d === props.selectedDate;
        return (
          <div
            key={d}
            onClick={(e) => e.target === e.currentTarget && props.onSelectDate(d)}
            onDoubleClick={() => props.onOpenDay(d)}
            className={`flex min-h-20 min-w-0 flex-col gap-0.5 bg-panel p-1 sm:min-h-24 ${selected ? "outline-2 -outline-offset-2 outline-accent" : ""} ${inMonth ? "" : "bg-sunken/40 opacity-60"}`}
          >
            <button
              type="button"
              aria-pressed={selected}
              aria-current={d === props.today ? "date" : undefined}
              aria-label={`${longDay(d)}${list.length ? `, ${list.length} item${list.length === 1 ? "" : "s"}` : ""}`}
              onClick={() => props.onSelectDate(d)}
              className={`grid size-7 place-items-center self-start rounded-full font-mono text-[11px] hover:bg-sunken ${d === props.today ? "bg-accent font-bold text-accent-ink hover:bg-accent" : "text-muted"}`}
            >
              {Number(d.slice(8))}
            </button>
            {list.slice(0, CELL_ITEMS).map((i) => (
              <button
                key={i.key}
                type="button"
                onClick={() => props.onSelect(i)}
                aria-label={`${i.title}, ${i.start !== null ? `${clockLabel(i.start)}, ` : ""}${itemKindLabel(i)}${i.done ? ", done" : ""}`}
                className={`truncate rounded-sm px-1 text-left text-[10px] leading-4 ${itemTone(i)} ${props.selectedKey === i.key ? "outline-2 outline-accent" : ""} ${i.skipped ? "opacity-50" : ""}`}
              >
                <span className={i.done ? "line-through" : ""}>
                  {i.kind === "deadline" ? "◆ " : i.start !== null ? `${clockLabel(i.start)} ` : ""}
                  {i.title}
                </span>
              </button>
            ))}
            {list.length > CELL_ITEMS && (
              <button type="button" onClick={() => props.onSelectDate(d)} className="px-1 text-left font-mono text-[10px] text-muted hover:text-ink">
                +{list.length - CELL_ITEMS} more
              </button>
            )}
          </div>
        );
      })}
    </div>
  );
}
