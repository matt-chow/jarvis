import { useEffect, useMemo, useState, type FormEvent } from "react";
import type { ModuleProps } from "../App";
import { LinkedSection } from "../components/LinkedSection";
import { Button, Panel, inputClass } from "../components/ui";
import { useHashParam } from "../hooks/useHashParam";
import { useProjects, type ProjectsState } from "../hooks/useProjects";
import { useTasks } from "../hooks/useTasks";
import { percent, projectProgress, type Progress, type Project } from "../lib/projects";
import type { Task } from "../lib/tasks";

// Provisional screen: awaiting design integration (see docs/MODULES.md › Design status).

const small = "min-h-7 px-2 text-[10px]";

function ProgressBar({ progress }: { progress: Progress }) {
  const pct = percent(progress);
  return (
    <div className="flex items-center gap-2">
      <div
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={pct}
        className="h-1.5 flex-1 overflow-hidden rounded-full bg-sunken"
      >
        <div className="h-full bg-accent transition-[width]" style={{ width: `${pct}%` }} />
      </div>
      <span className="w-24 text-right font-mono text-[11px] text-muted">
        {progress.done}/{progress.total} done
      </span>
    </div>
  );
}

export function Projects({ online }: ModuleProps) {
  const state = useProjects(online);
  const { tasks } = useTasks(online);
  const [selected, setSelected] = useState<string>();
  const [showArchived, setShowArchived] = useState(false);
  // Edits work offline: they apply locally and sync later (useProjects / D-017).
  const readOnly = !state.projects;
  const offline = state.stale || !online;

  const active = useMemo(() => (state.projects ?? []).filter((p) => p.status === "active"), [state.projects]);
  const archived = useMemo(() => (state.projects ?? []).filter((p) => p.status === "archived"), [state.projects]);
  const current = state.projects?.find((p) => p.id === selected);

  // Deep link (#/projects?project=…).
  const deepProject = useHashParam("project");
  useEffect(() => {
    const p = deepProject && state.projects?.find((x) => x.id === deepProject);
    if (!p) return;
    setSelected(p.id);
    if (p.status === "archived") setShowArchived(true);
  }, [deepProject, state.projects]);

  useEffect(() => {
    if (selected && state.projects && !current) setSelected(undefined);
  }, [selected, state.projects, current]);

  const row = (p: Project) => (
    <li key={p.id}>
      <button
        type="button"
        onClick={() => setSelected(p.id === selected ? undefined : p.id)}
        aria-expanded={p.id === selected}
        className={`flex w-full flex-col gap-1.5 rounded-sm px-2 py-2 text-left ${p.id === selected ? "bg-sunken" : "hover:bg-sunken/50"}`}
      >
        <span className={`truncate text-sm ${p.status === "archived" ? "text-muted" : ""}`}>{p.name}</span>
        <ProgressBar progress={projectProgress(tasks ?? [], p.id)} />
      </button>
    </li>
  );

  return (
    <div className="grid items-start gap-5 lg:grid-cols-[minmax(0,1fr)_24rem]">
      <Panel title={`projects${state.projects ? ` · ${active.length} active` : ""}`}>
        {offline && state.projects && (
          <p className="mb-3 rounded-sm border border-line bg-sunken/60 px-3 py-2 text-sm text-muted">
            Offline: changes are saved on this device and sync when you reconnect.
          </p>
        )}
        <NewProjectForm disabled={readOnly} onCreate={async (name) => (await state.create({ name })) !== undefined} />
        {state.error && !offline && (
          <p role="alert" className="mt-3 text-sm text-bad">
            {state.error}
          </p>
        )}
        {state.loading && !state.projects ? (
          <p className="mt-4 text-sm text-muted">Loading projects…</p>
        ) : !state.projects ? (
          <div className="mt-4 text-sm">
            <p className="text-muted">Projects couldn't be loaded.</p>
            <Button className={`${small} mt-2`} onClick={state.reload}>
              retry
            </Button>
          </div>
        ) : (
          <>
            {active.length === 0 ? (
              <p className="mt-4 text-sm text-muted">No active projects. Create one to group related tasks.</p>
            ) : (
              <ul className="-mx-2 mt-3 flex flex-col gap-0.5">{active.map(row)}</ul>
            )}
            {archived.length > 0 && (
              <div className="mt-4 border-t border-line pt-3">
                <button type="button" onClick={() => setShowArchived((v) => !v)} className="label hover:text-ink" aria-expanded={showArchived}>
                  {showArchived ? "▾" : "▸"} archived · {archived.length}
                </button>
                {showArchived && <ul className="-mx-2 mt-2 flex flex-col gap-0.5">{archived.map(row)}</ul>}
              </div>
            )}
          </>
        )}
      </Panel>

      {current && (
        <ProjectDetail
          key={current.id}
          project={current}
          tasks={tasks ?? []}
          state={state}
          readOnly={readOnly}
          offline={offline}
          onClose={() => setSelected(undefined)}
          onViewTasks={() => (location.hash = `/tasks?project=${current.id}`)}
        />
      )}
    </div>
  );
}

function NewProjectForm({ disabled, onCreate }: { disabled: boolean; onCreate: (name: string) => Promise<boolean> }) {
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    setBusy(true);
    if (await onCreate(name)) setName("");
    setBusy(false);
  };
  return (
    <form onSubmit={(e) => void submit(e)} className="flex gap-2">
      <input
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder="New project name…"
        aria-label="New project"
        maxLength={120}
        disabled={disabled || busy}
        className={`${inputClass} min-w-0 flex-1`}
      />
      <Button type="submit" variant="primary" disabled={disabled || busy || !name.trim()}>
        create
      </Button>
    </form>
  );
}

function ProjectDetail(props: {
  project: Project;
  tasks: Task[];
  state: ProjectsState;
  readOnly: boolean;
  /** Links need a connection. */
  offline: boolean;
  onClose: () => void;
  onViewTasks: () => void;
}) {
  const { project, state, readOnly } = props;
  const [name, setName] = useState(project.name);
  const [description, setDescription] = useState(project.description);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const progress = projectProgress(props.tasks, project.id);
  const archivedProject = project.status === "archived";

  const saveName = () => {
    if (name.trim() && name.trim() !== project.name) void state.update(project.id, { name });
    else setName(project.name);
  };

  return (
    <Panel
      title="project"
      actions={
        <Button onClick={props.onClose} className={small} aria-label="Close project details">
          close
        </Button>
      }
    >
      <div className="flex flex-col gap-3">
        <label className="flex flex-col gap-1">
          <span className="label">name</span>
          <input
            value={name}
            disabled={readOnly}
            maxLength={120}
            onChange={(e) => setName(e.target.value)}
            onBlur={saveName}
            onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
            className={inputClass}
          />
        </label>
        <label className="flex flex-col gap-1">
          <span className="label">description</span>
          <textarea
            value={description}
            disabled={readOnly}
            maxLength={5000}
            rows={3}
            onChange={(e) => setDescription(e.target.value)}
            onBlur={() => description !== project.description && void state.update(project.id, { description })}
            className={`${inputClass} resize-y py-2 leading-relaxed`}
          />
        </label>
        <div>
          <p className="label mb-1">progress</p>
          <ProgressBar progress={progress} />
          <p className="mt-1 font-mono text-[11px] text-muted">{progress.open} open</p>
        </div>
        <Button onClick={props.onViewTasks}>view tasks →</Button>
        <LinkedSection entity={{ kind: "project", id: project.id }} disabled={props.offline} />

        <div className="flex items-center justify-between gap-2 border-t border-line pt-3">
          <Button
            className={small}
            disabled={readOnly}
            onClick={() => void state.update(project.id, { status: archivedProject ? "active" : "archived" })}
          >
            {archivedProject ? "restore" : "archive"}
          </Button>
          {confirmDelete ? (
            <span className="flex items-center gap-1.5">
              <span className="text-xs text-muted">Tasks are kept, unassigned.</span>
              <Button className={small} onClick={() => setConfirmDelete(false)}>
                keep
              </Button>
              <Button variant="danger" className={small} onClick={() => void state.remove(project.id)}>
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
