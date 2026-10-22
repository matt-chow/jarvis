import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import { Button, Panel, inputClass } from "../components/ui";
import { describeError } from "../hooks/useCachedList";
import { useHabits } from "../hooks/useHabits";
import { useProjects } from "../hooks/useProjects";
import { reportIfUnauthorized } from "../hooks/useSession";
import { api } from "../lib/api";
import { TEMPLATE_HINT, describeTrigger, type Rule, type Trigger } from "../lib/automations";
import { formatDate, formatTime } from "../lib/format";

// Provisional panel: awaiting design integration (see docs/MODULES.md › Design status).

const small = "min-h-7 px-2 text-[10px]";

/** Rules that run a JARVIS command when a task is completed or a habit is checked in (D-024). */
export function AutomationsPanel({ online }: { online: boolean }) {
  const [rules, setRules] = useState<Rule[]>();
  const [error, setError] = useState<string>();
  const { projects } = useProjects(online);
  const { habits } = useHabits(online);
  const names = useMemo(
    () => ({
      projects: new Map((projects ?? []).map((p) => [p.id, p.name])),
      habits: new Map((habits ?? []).map((h) => [h.id, h.name])),
    }),
    [projects, habits],
  );

  const load = useCallback(() => {
    api.automations().then(setRules, (e: unknown) => {
      reportIfUnauthorized(e);
      setError(describeError(e, "automation"));
    });
  }, []);

  useEffect(() => {
    if (online) load();
  }, [online, load]);

  /** Run a change; show its error. `true` on success. */
  const act = async (fn: () => Promise<void>) => {
    setError(undefined);
    try {
      await fn();
      return true;
    } catch (e) {
      reportIfUnauthorized(e);
      setError(describeError(e, "automation"));
      return false;
    }
  };
  const replace = (r: Rule) => setRules((cur) => (cur ?? []).map((x) => (x.id === r.id ? r : x)));

  return (
    <Panel title={`automations${rules ? ` · ${rules.length}` : ""}`}>
      <p className="mb-3 text-sm text-muted">
        When something happens, run a command for you. Saving a rule approves its command, so it runs without asking each time.
      </p>
      {!online ? (
        <p className="text-sm text-muted">Automations need a connection to view or change. They still run on the server.</p>
      ) : !rules ? (
        <p className="text-sm text-muted">{error ?? "Loading…"}</p>
      ) : (
        <>
          {rules.length === 0 ? (
            <p className="text-sm text-muted">No automations yet.</p>
          ) : (
            <ul className="mb-4 flex flex-col divide-y divide-line">
              {rules.map((r) => (
                <RuleRow
                  key={r.id}
                  rule={r}
                  trigger={describeTrigger(r.trigger, names)}
                  onToggle={() => void act(async () => replace(await api.updateAutomation(r.id, { enabled: !r.enabled })))}
                  onDelete={() =>
                    void act(async () => {
                      await api.deleteAutomation(r.id);
                      setRules((cur) => cur?.filter((x) => x.id !== r.id));
                    })
                  }
                />
              ))}
            </ul>
          )}
          {error && (
            <p role="alert" className="mb-3 text-sm text-bad">
              {error}
            </p>
          )}
          <NewRuleForm
            projects={(projects ?? []).filter((p) => p.status === "active")}
            habits={(habits ?? []).filter((h) => !h.archived)}
            onCreate={(rule) =>
              act(async () => {
                const created = await api.createAutomation(rule);
                setRules((cur) => [...(cur ?? []), created]);
              })
            }
          />
        </>
      )}
    </Panel>
  );
}

function RuleRow({ rule, trigger, onToggle, onDelete }: { rule: Rule; trigger: string; onToggle: () => void; onDelete: () => void }) {
  const [confirm, setConfirm] = useState(false);
  return (
    <li className="py-2.5">
      <div className="flex flex-wrap items-center gap-2">
        <span className={`mr-auto text-sm ${rule.enabled ? "" : "text-muted"}`}>{rule.name}</span>
        <Button className={small} aria-pressed={rule.enabled} aria-label={`${rule.enabled ? "Turn off" : "Turn on"} “${rule.name}”`} onClick={onToggle}>
          {rule.enabled ? "on" : "off"}
        </Button>
        {confirm ? (
          <>
            <Button className={small} onClick={() => setConfirm(false)}>
              keep
            </Button>
            <Button variant="danger" className={small} onClick={onDelete}>
              confirm delete
            </Button>
          </>
        ) : (
          <Button variant="danger" className={small} aria-label={`Delete “${rule.name}”`} onClick={() => setConfirm(true)}>
            delete
          </Button>
        )}
      </div>
      <p className="mt-1 text-xs text-muted">{trigger}:</p>
      <p className="font-mono text-xs">{rule.command}</p>
      {rule.last_run_at && (
        <p className={`mt-1 font-mono text-[11px] ${rule.last_ok ? "text-muted" : "text-bad"}`}>
          last run {formatDate(rule.last_run_at)} {formatTime(rule.last_run_at)} · {rule.last_ok ? "ok" : "failed"}
          {rule.last_message && ` · ${rule.last_message}`}
        </p>
      )}
    </li>
  );
}

function NewRuleForm(props: {
  projects: { id: string; name: string }[];
  habits: { id: string; name: string }[];
  onCreate: (r: { name: string; trigger: Trigger; command: string }) => Promise<boolean>;
}) {
  const [name, setName] = useState("");
  const [kind, setKind] = useState<Trigger["kind"]>("task_completed");
  const [filter, setFilter] = useState("");
  const [command, setCommand] = useState("");
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !command.trim()) return;
    const trigger: Trigger = kind === "task_completed" ? { kind, project_id: filter || null } : { kind, habit_id: filter || null };
    setBusy(true);
    if (await props.onCreate({ name, trigger, command })) {
      setName("");
      setCommand("");
      setFilter("");
    }
    setBusy(false);
  };

  const options = kind === "task_completed" ? props.projects : props.habits;
  return (
    <form onSubmit={(e) => void submit(e)} className="flex flex-col gap-2 border-t border-line pt-3">
      <p className="label">new automation</p>
      <input value={name} onChange={(e) => setName(e.target.value)} maxLength={80} placeholder="Name" aria-label="Automation name" className={inputClass} />
      <div className="grid gap-2 sm:grid-cols-2">
        <select
          value={kind}
          onChange={(e) => {
            setKind(e.target.value as Trigger["kind"]);
            setFilter("");
          }}
          aria-label="When"
          className={inputClass}
        >
          <option value="task_completed">When a task is completed</option>
          <option value="habit_checked">When a habit is checked in</option>
        </select>
        <select value={filter} onChange={(e) => setFilter(e.target.value)} aria-label={kind === "task_completed" ? "Project" : "Habit"} className={inputClass}>
          <option value="">{kind === "task_completed" ? "in any project" : "any habit"}</option>
          {options.map((o) => (
            <option key={o.id} value={o.id}>
              {kind === "task_completed" ? `in “${o.name}”` : o.name}
            </option>
          ))}
        </select>
      </div>
      <input
        value={command}
        onChange={(e) => setCommand(e.target.value)}
        maxLength={500}
        placeholder="journal Finished {title}"
        aria-label="Automation command"
        className={`${inputClass} font-mono`}
      />
      <p className="text-xs text-muted">{TEMPLATE_HINT}</p>
      <div>
        <Button type="submit" variant="primary" className={small} disabled={busy || !name.trim() || !command.trim()}>
          add automation
        </Button>
      </div>
    </form>
  );
}
