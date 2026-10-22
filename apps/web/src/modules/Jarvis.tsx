import { useCallback, useEffect, useState, type FormEvent } from "react";
import type { ModuleProps } from "../App";
import { Button, Panel, inputClass } from "../components/ui";
import { describeError } from "../hooks/useCachedList";
import { reportIfUnauthorized } from "../hooks/useSession";
import { api, ApiError } from "../lib/api";
import { formatTime } from "../lib/format";
import { STATUS_TONE, type Plan, type Preset } from "../lib/jarvis";
import { entityRoute } from "../lib/links";
import { localDate } from "../lib/tasks";

// Provisional screen: awaiting design integration (see docs/MODULES.md › Design status).

const small = "min-h-7 px-2 text-[10px]";

interface Problem {
  message: string;
  examples?: string[];
}

function SaveAsPreset({ onSave }: { onSave: (name: string) => Promise<boolean> }) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState("");
  if (!open) {
    return (
      <button type="button" onClick={() => setOpen(true)} className="font-mono text-[11px] text-muted hover:text-accent">
        save as preset
      </button>
    );
  }
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        void onSave(name).then((ok) => ok && setOpen(false));
      }}
      className="flex items-center gap-1.5"
    >
      <input
        autoFocus
        value={name}
        maxLength={60}
        onChange={(e) => setName(e.target.value)}
        placeholder="Preset name"
        aria-label="Preset name"
        className={`${inputClass} min-h-7 w-36 text-xs`}
      />
      <Button type="submit" className={small} disabled={!name.trim()}>
        save
      </Button>
    </form>
  );
}

function PlanCard({
  plan,
  busy,
  onApprove,
  onCancel,
  onSave,
}: {
  plan: Plan;
  busy: boolean;
  onApprove?: () => void;
  onCancel?: () => void;
  onSave?: (name: string) => Promise<boolean>;
}) {
  return (
    <div className="rounded-sm border border-line bg-sunken/40 p-3">
      <div className="flex items-baseline justify-between gap-3 font-mono text-xs">
        <span className="truncate text-muted">&gt; {plan.input}</span>
        <span className={`shrink-0 uppercase ${STATUS_TONE[plan.status]}`}>{plan.status}</span>
      </div>
      <ol className="mt-2 flex flex-col gap-1">
        {plan.actions.map((a, i) => {
          const result = plan.results[i];
          return (
            <li key={i} className="text-sm">
              <span className="flex items-start gap-2">
                <span aria-hidden className={`font-mono ${result ? (result.ok ? "text-ok" : "text-bad") : "text-muted"}`}>
                  {result ? (result.ok ? "✓" : "✗") : `${i + 1}.`}
                </span>
                <span className="min-w-0 flex-1">
                  {a.summary}
                  {a.risk !== "read" && !result && <span className="label ml-2 text-warn">[{a.risk}]</span>}
                  {result && (
                    <span className={`block text-xs ${result.ok ? "text-muted" : "text-bad"}`}>
                      {result.message}{" "}
                      {result.entity && (
                        <a href={entityRoute(result.entity)} className="text-accent underline underline-offset-2">
                          open
                        </a>
                      )}
                    </span>
                  )}
                </span>
              </span>
            </li>
          );
        })}
      </ol>
      {plan.status === "pending" && onApprove && (
        <div className="mt-3 flex items-center gap-2">
          <Button variant="primary" className={small} disabled={busy} onClick={onApprove}>
            approve
          </Button>
          <Button className={small} disabled={busy} onClick={onCancel}>
            cancel
          </Button>
          <span className="ml-auto font-mono text-[11px] text-muted">expires {formatTime(plan.expires_at)}</span>
        </div>
      )}
      {onSave && (
        <div className="mt-2 flex justify-end">
          <SaveAsPreset onSave={onSave} />
        </div>
      )}
    </div>
  );
}

/** Text command console: preview → approve → execute, with history. */
export function Jarvis({ online }: ModuleProps) {
  const [input, setInput] = useState("");
  const [current, setCurrent] = useState<Plan>();
  const [history, setHistory] = useState<Plan[]>();
  const [problem, setProblem] = useState<Problem>();
  const [busy, setBusy] = useState(false);
  const [presets, setPresets] = useState<Preset[]>();

  const loadHistory = useCallback(() => {
    api.jarvisHistory().then(setHistory, reportIfUnauthorized);
  }, []);

  useEffect(() => {
    if (!online) return;
    loadHistory();
    api.presets().then(setPresets, reportIfUnauthorized);
  }, [online, loadHistory]);

  const savePreset = (input: string) => async (name: string) => {
    try {
      const created = await api.createPreset(name, input);
      setPresets((ps) => [...(ps ?? []), created]);
      return true;
    } catch (e) {
      reportIfUnauthorized(e);
      setProblem({ message: describeError(e, "preset") });
      return false;
    }
  };

  const removePreset = async (id: string) => {
    try {
      await api.deletePreset(id);
      setPresets((ps) => ps?.filter((p) => p.id !== id));
    } catch (e) {
      reportIfUnauthorized(e);
    }
  };

  const run = async (fn: () => Promise<Plan>) => {
    setBusy(true);
    setProblem(undefined);
    try {
      setCurrent(await fn());
      loadHistory();
    } catch (e) {
      reportIfUnauthorized(e);
      if (e instanceof ApiError && e.body && typeof e.body === "object") {
        const b = e.body as { message?: string; examples?: string[] };
        setProblem({ message: b.message ?? describeError(e, "plan"), examples: b.examples });
      } else setProblem({ message: describeError(e, "plan") });
    } finally {
      setBusy(false);
    }
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!input.trim()) return;
    void run(async () => {
      const plan = await api.jarvisPlan(input, localDate());
      setInput("");
      return plan;
    });
  };

  return (
    <div className="mx-auto flex max-w-3xl flex-col gap-5">
      <Panel title="jarvis · commands">
        <form onSubmit={submit} className="flex gap-2">
          <span aria-hidden className="cursor self-center font-mono text-accent">
            &gt;
          </span>
          <input
            value={input}
            onChange={(e) => setInput(e.target.value)}
            disabled={!online || busy}
            maxLength={1000}
            aria-label="Command"
            placeholder="add task call the dentist due friday"
            className={`${inputClass} min-w-0 flex-1 font-mono`}
          />
          <Button type="submit" variant="primary" disabled={!online || busy || !input.trim()}>
            run
          </Button>
        </form>
        {presets && presets.length > 0 && (
          <ul aria-label="Presets" className="mt-3 flex flex-wrap gap-1.5">
            {presets.map((p) => (
              <li key={p.id} className="flex items-center rounded-sm border border-line">
                <button
                  type="button"
                  title={p.input}
                  disabled={!online || busy}
                  onClick={() => void run(() => api.jarvisPlan(p.input, localDate()))}
                  className="px-2 py-0.5 font-mono text-xs hover:text-accent disabled:opacity-50"
                >
                  ▸ {p.name}
                </button>
                <button
                  type="button"
                  aria-label={`Delete preset “${p.name}”`}
                  onClick={() => void removePreset(p.id)}
                  className="border-l border-line px-1.5 font-mono text-xs text-muted hover:text-bad"
                >
                  ×
                </button>
              </li>
            ))}
          </ul>
        )}
        {!online && <p className="mt-3 text-sm text-muted">JARVIS needs a connection.</p>}
        <p className="mt-2 text-xs text-muted">
          Changes are previewed and only happen when you approve. Chain steps with “;” or “then”.
        </p>

        {problem && (
          <div role="alert" className="mt-3 text-sm">
            <p className="text-bad">{problem.message}</p>
            {problem.examples && (
              <ul className="mt-2 flex flex-wrap gap-1.5">
                {problem.examples.map((ex) => (
                  <li key={ex}>
                    <button
                      type="button"
                      onClick={() => setInput(ex)}
                      className="rounded-sm border border-line px-2 py-0.5 font-mono text-xs text-muted hover:border-accent hover:text-ink"
                    >
                      {ex}
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}

        {current && (
          <div className="mt-4">
            <PlanCard
              plan={current}
              busy={busy}
              onApprove={() => void run(() => api.jarvisExecute(current.id))}
              onCancel={() => void run(() => api.jarvisCancel(current.id))}
              onSave={savePreset(current.input)}
            />
          </div>
        )}
      </Panel>

      <Panel title={`history${history ? ` · ${history.length}` : ""}`}>
        {!history ? (
          <p className="text-sm text-muted">{online ? "Loading…" : "Unavailable offline."}</p>
        ) : history.length === 0 ? (
          <p className="text-sm text-muted">No commands yet.</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {history.map((p) => (
              <li key={p.id}>
                <PlanCard plan={p} busy={busy} onSave={savePreset(p.input)} />
              </li>
            ))}
          </ul>
        )}
      </Panel>
    </div>
  );
}
