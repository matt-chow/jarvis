import { useMemo } from "react";
import { AGENDA_LABELS, allocationLabel, taskAgenda, type Allocation } from "../../lib/planner";
import { dueInfo, type Task } from "../../lib/tasks";
import { TASK_DRAG } from "./TimeGrid";

/**
 * The tasks half of the split view (D-050): open tasks by due date beside the
 * calendar. "Due" is the deadline; "planned" is time on the calendar. Drag a
 * task onto the grid, or use Place (keyboard and touch) or Suggest times.
 */
export function SplitTasks(props: {
  tasks: Task[];
  allocs: Map<string, Allocation>;
  today: string;
  placingId?: string;
  onPlace: (t: Task) => void;
  onPlan: (t: Task) => void;
  onComplete: (t: Task) => void;
}) {
  const groups = useMemo(() => taskAgenda(props.tasks, props.today), [props.tasks, props.today]);
  const btn = "min-h-8 rounded-full border border-line px-2.5 text-xs hover:bg-sunken";

  return (
    <section aria-label="Tasks beside the calendar" className="rounded-lg border border-line bg-panel">
      <header className="flex min-h-11 items-center gap-2 border-b border-line px-4">
        <h2 className="label flex-1">tasks</h2>
        <a href="#/tasks" className="font-mono text-[11px] text-accent underline-offset-4 hover:underline">
          all tasks →
        </a>
      </header>
      {groups.length === 0 ? (
        <p className="p-4 text-sm text-muted">No open tasks.</p>
      ) : (
        <div className="flex flex-col gap-4 p-4">
          {groups.map(({ group, tasks }) => (
            <div key={group}>
              <h3 className={`mb-1.5 text-xs ${group === "overdue" ? "text-bad" : "text-muted"}`}>
                {AGENDA_LABELS[group]} · {tasks.length}
              </h3>
              <ul className="flex flex-col gap-1">
                {tasks.map((t) => {
                  const a = props.allocs.get(t.id);
                  const due = dueInfo(t.due_date, props.today);
                  const placing = props.placingId === t.id;
                  return (
                    <li
                      key={t.id}
                      draggable
                      onDragStart={(e) => {
                        e.dataTransfer.setData(TASK_DRAG, t.id);
                        e.dataTransfer.effectAllowed = "copy";
                      }}
                      className={`group flex cursor-grab flex-col gap-1 rounded-sm border px-2 py-1.5 ${placing ? "border-accent" : "border-transparent hover:border-line"}`}
                    >
                      <span className="flex items-start gap-2">
                        <button
                          type="button"
                          aria-label={`Complete “${t.title}”`}
                          onClick={() => props.onComplete(t)}
                          className="mt-0.5 size-[16px] shrink-0 rounded-full border-[1.5px] border-muted transition hover:border-accent hover:bg-accent/10"
                        />
                        <a href={`#/tasks?task=${t.id}`} className="min-w-0 flex-1 text-sm hover:underline">
                          {t.title}
                        </a>
                      </span>
                      <span className="flex flex-wrap gap-x-2 pl-6 font-mono text-[11px] text-muted">
                        {due && <span className={due.tone === "overdue" ? "text-bad" : due.tone === "today" ? "text-warn" : ""}>Due {due.label}</span>}
                        <span>{a && a.sessions > 0 ? allocationLabel(a) : "No time planned"}</span>
                      </span>
                      <span className="flex gap-1.5 pl-6">
                        <button type="button" aria-pressed={placing} onClick={() => props.onPlace(t)} className={btn}>
                          {placing ? "Cancel placing" : "Place"}
                        </button>
                        <button type="button" onClick={() => props.onPlan(t)} className={btn}>
                          Suggest times
                        </button>
                      </span>
                    </li>
                  );
                })}
              </ul>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}
