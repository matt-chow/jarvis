import { useState } from "react";
import { isOpen, SNOOZE_PRESETS, snoozeUntil, type Reminder } from "../lib/reminders";
import { Button, inputClass } from "./ui";

const small = "min-h-7 px-2 text-[10px]";

/** Snooze presets, plus a custom time. */
export function SnoozeMenu(props: { reminder: Reminder; disabled: boolean; onSnooze: (until: number) => void }) {
  const [open, setOpen] = useState(false);
  const [custom, setCustom] = useState("");
  const r = props.reminder;
  if (!r.due_date || !isOpen(r)) return null;
  const pick = (until: number | null) => {
    if (until === null || until <= Date.now()) return;
    props.onSnooze(until);
    setOpen(false);
    setCustom("");
  };
  return (
    <span className="relative">
      <Button className={small} disabled={props.disabled} aria-expanded={open} aria-label={`Snooze “${r.title}”`} onClick={() => setOpen(!open)}>
        snooze
      </Button>
      {open && (
        <span
          role="menu"
          aria-label="Snooze for"
          onKeyDown={(e) => e.key === "Escape" && setOpen(false)}
          className="absolute right-0 z-10 mt-1 flex w-52 flex-col gap-1 rounded-sm border border-line bg-panel p-2 shadow-lg"
        >
          {SNOOZE_PRESETS.map((p) => (
            <Button key={p.id} role="menuitem" className={`${small} justify-start`} onClick={() => pick(snoozeUntil(p.id, r.timezone))}>
              {p.label}
            </Button>
          ))}
          <span className="flex gap-1">
            <input
              type="datetime-local"
              value={custom}
              onChange={(e) => setCustom(e.target.value)}
              aria-label="Snooze until"
              className={`${inputClass} min-w-0 flex-1 text-xs`}
            />
            <Button className={small} disabled={!custom || new Date(custom).getTime() <= Date.now()} onClick={() => pick(new Date(custom).getTime())}>
              set
            </Button>
          </span>
        </span>
      )}
    </span>
  );
}
