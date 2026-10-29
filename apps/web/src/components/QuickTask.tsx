import { useEffect, useRef, useState, type FormEvent } from "react";
import { TASKS_CHANGED_EVENT, useTasks } from "../hooks/useTasks";
import { parseEstimate, PRIORITY_LABELS, type TaskPriority } from "../lib/tasks";
import { Button, inputClass } from "./ui";

/**
 * Global quick capture for tasks (D-047): a header button on every screen,
 * and Alt+N on a keyboard. The task appears at once in Tasks and in the
 * calendar's Unscheduled list; it works offline like any task change.
 */
export function QuickTask({ online }: { online: boolean }) {
  const [open, setOpen] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.altKey && !e.ctrlKey && !e.metaKey && e.code === "KeyN") {
        e.preventDefault();
        setOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  useEffect(() => {
    const d = dialog.current;
    if (!d) return;
    if (open && !d.open) d.showModal?.();
    if (!open && d.open) d.close();
  }, [open]);

  return (
    <>
      <button
        type="button"
        onClick={() => setOpen(true)}
        aria-label="Quick add task"
        aria-keyshortcuts="Alt+N"
        className="inline-flex min-h-8 items-center gap-1.5 rounded-full border border-line bg-panel px-3 text-[13px] transition hover:border-accent"
      >
        <span aria-hidden>+</span> Task
      </button>
      <dialog
        ref={dialog}
        onClose={() => setOpen(false)}
        aria-label="Quick add task"
        className="m-auto w-[min(28rem,calc(100vw-2rem))] rounded-lg border border-line bg-panel p-0 text-ink shadow-2xl backdrop:bg-ink/30"
      >
        {open && <QuickTaskForm online={online} onDone={() => setOpen(false)} />}
      </dialog>
    </>
  );
}

function QuickTaskForm({ online, onDone }: { online: boolean; onDone: () => void }) {
  const tasks = useTasks(online);
  const [title, setTitle] = useState("");
  const [due, setDue] = useState("");
  const [estimate, setEstimate] = useState("");
  const [priority, setPriority] = useState<TaskPriority>("none");
  const [busy, setBusy] = useState(false);
  const estimateValue = parseEstimate(estimate);
  const invalid = !title.trim() || estimateValue === undefined;

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (invalid) return;
    setBusy(true);
    const made = await tasks.create({ title, due_date: due || null, estimate_minutes: estimateValue ?? null, priority });
    setBusy(false);
    if (!made) return;
    window.dispatchEvent(new Event(TASKS_CHANGED_EVENT));
    onDone();
  };

  return (
    <form onSubmit={(e) => void submit(e)} className="flex flex-col gap-3 p-5">
      <h2 className="label">quick task</h2>
      <input
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        maxLength={200}
        required
        autoFocus
        placeholder="What needs doing?"
        aria-label="Task title"
        className={inputClass}
      />
      <div className="grid grid-cols-3 gap-2">
        <label className="flex min-w-0 flex-col gap-1">
          <span className="label">due</span>
          <input type="date" value={due} onChange={(e) => setDue(e.target.value)} aria-label="Due date" className={inputClass} />
        </label>
        <label className="flex min-w-0 flex-col gap-1">
          <span className="label">estimate (min)</span>
          <input value={estimate} onChange={(e) => setEstimate(e.target.value)} inputMode="numeric" aria-label="Estimate in minutes" className={inputClass} />
        </label>
        <label className="flex min-w-0 flex-col gap-1">
          <span className="label">priority</span>
          <select value={priority} onChange={(e) => setPriority(e.target.value as TaskPriority)} aria-label="Priority" className={inputClass}>
            {(Object.keys(PRIORITY_LABELS) as TaskPriority[]).map((p) => (
              <option key={p} value={p}>
                {PRIORITY_LABELS[p]}
              </option>
            ))}
          </select>
        </label>
      </div>
      {tasks.error && (
        <p role="alert" className="text-sm text-bad">
          {tasks.error}
        </p>
      )}
      <div className="flex gap-1.5">
        <Button type="submit" variant="primary" disabled={invalid || busy}>
          add task
        </Button>
        <Button onClick={onDone}>cancel</Button>
      </div>
    </form>
  );
}
