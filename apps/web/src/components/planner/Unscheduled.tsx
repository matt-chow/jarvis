import type { Project } from "../../lib/projects";
import { allocationLabel, atRisk, type Allocation, type InboxSort } from "../../lib/planner";
import { dueInfo, PRIORITY_LABELS, type Task } from "../../lib/tasks";
import { durationLabel } from "../../lib/time";
import { inputClass } from "../ui";
import { TASK_DRAG } from "./TimeGrid";

const SORTS: Record<InboxSort, string> = { due: "Due date", priority: "Priority", project: "Project", remaining: "Most remaining" };

/**
 * Tasks with no session, or fewer scheduled minutes than estimated. Drag one
 * onto the grid, tap Place then a time (phones), or ask for suggestions.
 */
export function Unscheduled(props: {
  tasks: Task[];
  allocs: Map<string, Allocation>;
  projects: Project[];
  today: string;
  sort: InboxSort;
  project: string;
  placingId?: string;
  onSort: (s: InboxSort) => void;
  onProject: (id: string) => void;
  onPlace: (t: Task) => void;
  onPlan: (t: Task) => void;
}) {
  const names = new Map(props.projects.map((p) => [p.id, p.name]));
  const list = props.project ? props.tasks.filter((t) => t.project_id === props.project) : props.tasks;
  return (
    <div className="flex flex-col gap-3">
      <div className="grid grid-cols-2 gap-2">
        <label className="flex flex-col gap-1">
          <span className="label">sort</span>
          <select value={props.sort} onChange={(e) => props.onSort(e.target.value as InboxSort)} aria-label="Sort unscheduled tasks" className={inputClass}>
            {(Object.keys(SORTS) as InboxSort[]).map((s) => (
              <option key={s} value={s}>
                {SORTS[s]}
              </option>
            ))}
          </select>
        </label>
        <label className="flex flex-col gap-1">
          <span className="label">project</span>
          <select value={props.project} onChange={(e) => props.onProject(e.target.value)} aria-label="Unscheduled tasks in project" className={inputClass}>
            <option value="">All</option>
            {props.projects.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
      </div>
      {list.length === 0 ? (
        <p className="text-sm text-muted">Every open task has its time scheduled. Nice.</p>
      ) : (
        <ul aria-label="Unscheduled tasks" className="flex flex-col gap-2">
          {list.map((t) => {
            const a = props.allocs.get(t.id);
            const due = dueInfo(t.due_date, props.today);
            const risk = atRisk(t, a);
            return (
              <li
                key={t.id}
                draggable
                onDragStart={(e) => {
                  e.dataTransfer.setData(TASK_DRAG, t.id);
                  e.dataTransfer.effectAllowed = "copy";
                }}
                className={`flex flex-col gap-1.5 rounded-sm border bg-panel p-2.5 ${props.placingId === t.id ? "border-accent" : "border-line"} cursor-grab`}
              >
                <span className="flex items-start gap-2">
                  <span aria-hidden className="mt-0.5 font-mono text-muted">
                    ⠿
                  </span>
                  <span className="min-w-0 flex-1 text-sm font-medium">{t.title}</span>
                </span>
                <span className="flex flex-wrap gap-x-2 gap-y-0.5 pl-5 font-mono text-[11px] text-muted">
                  {due && <span className={due.tone === "overdue" ? "text-bad" : due.tone === "today" ? "text-warn" : ""}>Due {due.label}</span>}
                  {t.priority !== "none" && <span>{PRIORITY_LABELS[t.priority]}</span>}
                  {t.project_id && names.get(t.project_id) && <span>{names.get(t.project_id)}</span>}
                  {a?.remaining ? <span>{durationLabel(a.remaining)} left</span> : null}
                </span>
                <span className={`pl-5 text-[11px] ${risk ? "text-warn" : "text-muted"}`}>
                  {a ? allocationLabel(a) : "Not scheduled"}
                  {risk && " · not enough time before it's due"}
                </span>
                <span className="flex gap-1.5 pl-5">
                  <button
                    type="button"
                    onClick={() => props.onPlace(t)}
                    aria-pressed={props.placingId === t.id}
                    className="min-h-8 rounded-full border border-line px-3 text-xs hover:bg-sunken"
                  >
                    {props.placingId === t.id ? "Cancel placing" : "Place on calendar"}
                  </button>
                  <button type="button" onClick={() => props.onPlan(t)} className="min-h-8 rounded-full border border-line px-3 text-xs hover:bg-sunken">
                    Suggest times
                  </button>
                </span>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
