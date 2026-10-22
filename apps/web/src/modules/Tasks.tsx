import { useEffect, useMemo, useState, type FormEvent } from "react";
import type { ModuleProps } from "../App";
import { LinkedSection } from "../components/LinkedSection";
import { Button, Panel, inputClass } from "../components/ui";
import { useHashParam } from "../hooks/useHashParam";
import { useProjects } from "../hooks/useProjects";
import { useTasks, type TasksState } from "../hooks/useTasks";
import { hashParam } from "../lib/modules";
import type { Project } from "../lib/projects";
import {
  PRIORITY_LABELS,
  STATUS_LABELS,
  dueInfo,
  isOpen,
  localDate,
  RECENT_CLOSED_SHOWN,
  taskTree,
  type Task,
  type TaskFilter,
  type TaskNode,
  type TaskPriority,
  type TaskStatus,
} from "../lib/tasks";

// Provisional screen: awaiting design integration (see docs/MODULES.md › Design status).

const FILTERS: { id: TaskFilter; label: string }[] = [
  { id: "open", label: "Open" },
  { id: "closed", label: "Done" },
  { id: "all", label: "All" },
];

const DUE_TONE = { overdue: "text-bad", today: "text-accent", soon: "text-ink", later: "text-muted" } as const;
const PRIORITY_MARK: Record<TaskPriority, string> = { none: "", low: "!", medium: "!!", high: "!!!" };
const small = "min-h-7 px-2 text-[10px]";
/** Project filter: "" = all, "none" = unassigned, otherwise a project id. */
const NO_PROJECT = "none";

function useProjectFilter(): [string, (v: string) => void] {
  const read = () => hashParam(location.hash, "project") ?? "";
  const [value, setValue] = useState(read);
  useEffect(() => {
    const onHash = () => setValue(read());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);
  // Keep the filter in the URL so it survives reloads and can be linked to.
  const set = (v: string) => (location.hash = v ? `/tasks?project=${v}` : "/tasks");
  return [value, set];
}

export function Tasks({ online }: ModuleProps) {
  const state = useTasks(online);
  const { projects } = useProjects(online);
  const [project, setProject] = useProjectFilter();
  const [filter, setFilter] = useState<TaskFilter>("open");
  const [selected, setSelected] = useState<string>();
  const today = localDate();
  const tree = useMemo(
    () =>
      taskTree(state.tasks ?? [], filter).filter(
        ({ task }) => !project || (project === NO_PROJECT ? !task.project_id : task.project_id === project),
      ),
    [state.tasks, filter, project],
  );
  const projectName = useMemo(() => new Map((projects ?? []).map((p) => [p.id, p.name])), [projects]);
  const activeProjects = useMemo(() => (projects ?? []).filter((p) => p.status === "active" || p.id === project), [projects, project]);
  const selectedNode = useMemo(() => taskTree(state.tasks ?? [], "all").find((n) => n.task.id === selected), [state.tasks, selected]);
  // Edits work offline: they apply locally and sync later (useTasks / D-017).
  const readOnly = !state.tasks;
  const offline = state.stale || !online;

  // Deep link (#/tasks?task=…): open that task (or a subtask's parent).
  const deepTask = useHashParam("task");
  useEffect(() => {
    if (!deepTask || !state.tasks) return;
    const t = state.tasks.find((x) => x.id === deepTask);
    if (!t) return;
    setFilter("all");
    setSelected(t.parent_id ?? t.id);
  }, [deepTask, state.tasks]);

  // Drop the selection if the task disappears (deleted elsewhere).
  useEffect(() => {
    if (selected && state.tasks && !state.tasks.some((t) => t.id === selected)) setSelected(undefined);
  }, [selected, state.tasks]);

  return (
    <div className="grid items-start gap-5 lg:grid-cols-[minmax(0,1fr)_24rem]">
      <Panel
        title={`tasks${state.tasks ? ` · ${(state.tasks ?? []).filter((t) => !t.parent_id && isOpen(t)).length} open` : ""}`}
        actions={
          <div role="tablist" aria-label="Filter" className="flex gap-1">
            {FILTERS.map((f) => (
              <button
                key={f.id}
                role="tab"
                aria-selected={filter === f.id}
                onClick={() => setFilter(f.id)}
                className={`rounded-sm px-2 py-1 font-mono text-[10px] tracking-wide uppercase ${
                  filter === f.id ? "bg-accent text-accent-ink" : "text-muted hover:text-ink"
                }`}
              >
                {f.label}
              </button>
            ))}
          </div>
        }
      >
        {offline && state.tasks && (
          <p className="mb-3 rounded-sm border border-line bg-sunken/60 px-3 py-2 text-sm text-muted">
            Offline: changes are saved on this device and sync when you reconnect.
          </p>
        )}
        <label className="mb-3 flex items-center gap-2">
          <span className="label shrink-0">project</span>
          <select
            value={project}
            onChange={(e) => setProject(e.target.value)}
            aria-label="Project filter"
            className={`${inputClass} min-w-0 flex-1`}
          >
            <option value="">All projects</option>
            <option value={NO_PROJECT}>No project</option>
            {activeProjects.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
        <QuickAdd
          disabled={readOnly}
          onAdd={(title) => state.create({ title, project_id: project && project !== NO_PROJECT ? project : undefined })}
        />
        {state.error && !offline && (
          <p role="alert" className="mt-3 text-sm text-bad">
            {state.error}
          </p>
        )}

        {state.loading && !state.tasks ? (
          <p className="mt-4 text-sm text-muted">Loading tasks…</p>
        ) : !state.tasks ? (
          <div className="mt-4 text-sm">
            <p className="text-muted">Tasks couldn't be loaded.</p>
            <Button className={`${small} mt-2`} onClick={state.reload}>
              retry
            </Button>
          </div>
        ) : tree.length === 0 ? (
          <p className="mt-4 text-sm text-muted">
            {filter === "open" ? "Nothing open. Add a task above." : filter === "closed" ? "No completed tasks yet." : "No tasks yet."}
          </p>
        ) : (
          <ul className="-mx-2 mt-3 flex flex-col">
            {tree.map((node) => (
              <TaskRow
                key={node.task.id}
                node={node}
                today={today}
                selected={node.task.id === selected}
                readOnly={readOnly}
                projectName={project ? undefined : projectName.get(node.task.project_id ?? "")}
                onSelect={() => setSelected(node.task.id === selected ? undefined : node.task.id)}
                onToggle={() => void state.update(node.task.id, { status: isOpen(node.task) ? "done" : "todo" })}
              />
            ))}
          </ul>
        )}
        {filter !== "open" && (state.tasks ?? []).filter((t) => !isOpen(t)).length >= RECENT_CLOSED_SHOWN && (
          <p className="mt-3 text-xs text-muted">Showing the {RECENT_CLOSED_SHOWN} most recently completed tasks. Older ones are in your data export.</p>
        )}
      </Panel>

      {selectedNode && (
        <TaskDetail
          key={selectedNode.task.id}
          node={selectedNode}
          state={state}
          projects={activeProjects}
          readOnly={readOnly}
          onClose={() => setSelected(undefined)}
        />
      )}
    </div>
  );
}

function QuickAdd({ disabled, onAdd }: { disabled: boolean; onAdd: (title: string) => Promise<unknown> }) {
  const [title, setTitle] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!title.trim()) return;
    setBusy(true);
    const created = await onAdd(title);
    setBusy(false);
    if (created) setTitle("");
  };
  return (
    <form onSubmit={(e) => void submit(e)} className="flex gap-2">
      <input
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        placeholder="Add a task…"
        aria-label="New task"
        maxLength={200}
        disabled={disabled || busy}
        className={`${inputClass} min-w-0 flex-1`}
      />
      <Button type="submit" variant="primary" disabled={disabled || busy || !title.trim()}>
        add
      </Button>
    </form>
  );
}

function Check({ task, disabled, onToggle }: { task: Task; disabled: boolean; onToggle: () => void }) {
  const done = !isOpen(task);
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={done}
      aria-label={`${done ? "Reopen" : "Complete"} “${task.title}”`}
      disabled={disabled}
      onClick={onToggle}
      className={`grid size-5 shrink-0 place-items-center rounded-sm border font-mono text-[11px] leading-none transition ${
        done ? "border-ok bg-ok text-canvas" : "border-line hover:border-accent"
      } disabled:opacity-50`}
    >
      {done ? "✓" : ""}
    </button>
  );
}

function TaskRow(props: {
  node: TaskNode;
  today: string;
  selected: boolean;
  readOnly: boolean;
  projectName?: string;
  onSelect: () => void;
  onToggle: () => void;
}) {
  const { task, subtasks } = props.node;
  const due = isOpen(task) ? dueInfo(task.due_date, props.today) : undefined;
  const subDone = subtasks.filter((s) => !isOpen(s)).length;
  return (
    <li className={`flex items-start gap-3 rounded-sm px-2 py-2 ${props.selected ? "bg-sunken" : "hover:bg-sunken/50"}`}>
      <span className="pt-0.5">
        <Check task={task} disabled={props.readOnly} onToggle={props.onToggle} />
      </span>
      <button type="button" onClick={props.onSelect} aria-expanded={props.selected} className="min-w-0 flex-1 text-left">
        <span className={`block truncate text-sm ${isOpen(task) ? "" : "text-muted line-through"}`}>{task.title}</span>
        <span className="flex flex-wrap gap-x-3 font-mono text-[11px] text-muted">
          {props.projectName && <span>▦ {props.projectName}</span>}
          {task.status === "in_progress" && <span className="text-accent">in progress</span>}
          {due && <span className={DUE_TONE[due.tone]}>{due.label}</span>}
          {task.priority !== "none" && (
            <span className="text-warn" title={`${PRIORITY_LABELS[task.priority]} priority`}>
              {PRIORITY_MARK[task.priority]}
            </span>
          )}
          {subtasks.length > 0 && (
            <span>
              {subDone}/{subtasks.length} subtasks
            </span>
          )}
          {task.notes && <span title="Has notes">¶</span>}
        </span>
      </button>
    </li>
  );
}

function TaskDetail(props: { node: TaskNode; state: TasksState; projects: Project[]; readOnly: boolean; onClose: () => void }) {
  const { node, state, readOnly, onClose } = props;
  const { task, subtasks } = node;
  const [title, setTitle] = useState(task.title);
  const [notes, setNotes] = useState(task.notes);
  const [subtask, setSubtask] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);

  const saveTitle = () => {
    if (title.trim() && title.trim() !== task.title) void state.update(task.id, { title });
    else setTitle(task.title);
  };
  const saveNotes = () => {
    if (notes !== task.notes) void state.update(task.id, { notes });
  };
  const addSubtask = async (e: FormEvent) => {
    e.preventDefault();
    if (!subtask.trim()) return;
    if (await state.create({ title: subtask, parent_id: task.id })) setSubtask("");
  };

  return (
    <Panel
      title="task"
      actions={
        <Button onClick={onClose} className={small} aria-label="Close task details">
          close
        </Button>
      }
    >
      <div className="flex flex-col gap-3">
        <label className="flex flex-col gap-1">
          <span className="label">title</span>
          <input
            value={title}
            disabled={readOnly}
            maxLength={200}
            onChange={(e) => setTitle(e.target.value)}
            onBlur={saveTitle}
            onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
            className={inputClass}
          />
        </label>

        <div className="grid grid-cols-2 gap-3">
          <label className="flex flex-col gap-1">
            <span className="label">status</span>
            <select
              value={task.status}
              disabled={readOnly}
              onChange={(e) => void state.update(task.id, { status: e.target.value as TaskStatus })}
              className={inputClass}
            >
              {Object.entries(STATUS_LABELS).map(([v, l]) => (
                <option key={v} value={v}>
                  {l}
                </option>
              ))}
            </select>
          </label>
          <label className="flex flex-col gap-1">
            <span className="label">priority</span>
            <select
              value={task.priority}
              disabled={readOnly}
              onChange={(e) => void state.update(task.id, { priority: e.target.value as TaskPriority })}
              className={inputClass}
            >
              {Object.entries(PRIORITY_LABELS).map(([v, l]) => (
                <option key={v} value={v}>
                  {l}
                </option>
              ))}
            </select>
          </label>
        </div>

        {!task.parent_id && (
          <label className="flex flex-col gap-1">
            <span className="label">project</span>
            <select
              value={task.project_id ?? ""}
              disabled={readOnly}
              onChange={(e) => void state.update(task.id, { project_id: e.target.value || null })}
              className={inputClass}
            >
              <option value="">No project</option>
              {props.projects.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </label>
        )}

        <label className="flex flex-col gap-1">
          <span className="label">due date</span>
          <span className="flex gap-2">
            <input
              type="date"
              value={task.due_date ?? ""}
              disabled={readOnly}
              onChange={(e) => void state.update(task.id, { due_date: e.target.value || null })}
              className={`${inputClass} min-w-0 flex-1`}
            />
            {task.due_date && (
              <Button className={small} disabled={readOnly} onClick={() => void state.update(task.id, { due_date: null })}>
                clear
              </Button>
            )}
          </span>
        </label>

        <label className="flex flex-col gap-1">
          <span className="label">notes</span>
          <textarea
            value={notes}
            disabled={readOnly}
            maxLength={10000}
            rows={4}
            onChange={(e) => setNotes(e.target.value)}
            onBlur={saveNotes}
            className={`${inputClass} resize-y py-2 leading-relaxed`}
          />
        </label>

        <LinkedSection entity={{ kind: "task", id: task.id }} disabled={readOnly} />

        {!task.parent_id && (
          <div>
            <p className="label mb-1">subtasks</p>
            {subtasks.length > 0 && (
              <ul className="mb-2 flex flex-col gap-1">
                {subtasks.map((s) => (
                  <li key={s.id} className="flex items-center gap-2">
                    <Check
                      task={s}
                      disabled={readOnly}
                      onToggle={() => void state.update(s.id, { status: isOpen(s) ? "done" : "todo" })}
                    />
                    <span className={`min-w-0 flex-1 truncate text-sm ${isOpen(s) ? "" : "text-muted line-through"}`}>{s.title}</span>
                    <button
                      type="button"
                      disabled={readOnly}
                      onClick={() => void state.remove(s.id)}
                      aria-label={`Delete subtask “${s.title}”`}
                      className="font-mono text-xs text-muted hover:text-bad disabled:opacity-50"
                    >
                      ×
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <form onSubmit={(e) => void addSubtask(e)} className="flex gap-2">
              <input
                value={subtask}
                disabled={readOnly}
                maxLength={200}
                onChange={(e) => setSubtask(e.target.value)}
                placeholder="Add a subtask…"
                aria-label="New subtask"
                className={`${inputClass} min-w-0 flex-1`}
              />
              <Button type="submit" className={small} disabled={readOnly || !subtask.trim()}>
                add
              </Button>
            </form>
          </div>
        )}

        <div className="flex items-center justify-between border-t border-line pt-3">
          <span className="font-mono text-[11px] text-muted">created {new Date(task.created_at).toLocaleDateString()}</span>
          {confirmDelete ? (
            <span className="flex gap-1.5">
              <Button className={small} onClick={() => setConfirmDelete(false)}>
                keep
              </Button>
              <Button variant="danger" className={small} onClick={() => void state.remove(task.id)}>
                confirm delete
              </Button>
            </span>
          ) : (
            <Button variant="danger" className={small} disabled={readOnly} onClick={() => setConfirmDelete(true)}>
              delete
            </Button>
          )}
        </div>
      </div>
    </Panel>
  );
}
