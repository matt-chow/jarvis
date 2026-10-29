import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import { usePreferences } from "../hooks/usePreferences";
import { DEFAULT_HOURS } from "../lib/planner";
import { jarvisForm, parseJarvisForm, type JarvisForm } from "../lib/preferences";
import { durationLabel } from "../lib/time";
import { Button, Panel, inputClass } from "./ui";

// Provisional (D-013): Settings fields in the existing style.

const small = "min-h-8 px-3 text-xs";
const DAYS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

function Field({ label, hint, children }: { label: string; hint: string; children: ReactNode }) {
  return (
    <label className="flex flex-col gap-1">
      <span className="label">{label}</span>
      {children}
      <span className="text-xs text-muted">{hint}</span>
    </label>
  );
}

/** How JARVIS schedules work sessions (D-048): visible, editable, removable. */
export function JarvisSettings({ online }: { online: boolean }) {
  const { prefs, error, update } = usePreferences(online);
  const saved = prefs?.jarvis;
  const [form, setForm] = useState<JarvisForm>(jarvisForm(saved));
  const [message, setMessage] = useState<string>();
  const savedKey = JSON.stringify(saved ?? null);
  // A change saved elsewhere (another device, or a JARVIS plan you approved) shows here.
  useEffect(() => setForm(jarvisForm(saved)), [savedKey]); // eslint-disable-line react-hooks/exhaustive-deps

  const hours = prefs?.planner ?? DEFAULT_HOURS;
  const parsed = parseJarvisForm(form);
  // Saving the defaults unchanged still records them, so only an edit enables save.
  const changed = JSON.stringify(form) !== JSON.stringify(jarvisForm(saved));

  const save = (e: FormEvent) => {
    e.preventDefault();
    if (!("prefs" in parsed)) return;
    setMessage(undefined);
    void update({ jarvis: parsed.prefs }).then(() => setMessage(online ? "Saved." : "Saved on this device; it syncs when you're online."));
  };

  const reset = () => {
    setMessage(undefined);
    void update({ jarvis: null }).then(() => setMessage("JARVIS uses its defaults again."));
  };

  const set = (patch: Partial<JarvisForm>) => {
    setMessage(undefined);
    setForm({ ...form, ...patch });
  };

  return (
    <Panel title="jarvis scheduling" className="lg:col-span-2">
      {!prefs ? (
        <p className="cursor font-mono text-sm text-muted">loading</p>
      ) : (
        <form onSubmit={save} className="flex flex-col gap-4">
          <p className="text-sm text-muted">
            JARVIS plans sessions inside your working hours ({hours.day_start}–{hours.day_end}, {hours.work_days.map((d) => DAYS[d]).join(" ")}) in{" "}
            {durationLabel(hours.session_minutes)} sessions.{" "}
            <a href="#/calendar" className="text-accent underline-offset-2 hover:underline">
              change them in Calendar
            </a>
            . These refine them. JARVIS only changes them when you approve a plan that says so.
          </p>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label="no sessions after" hint="Blank: up to the end of your working day.">
              <input type="time" value={form.avoid_after} onChange={(e) => set({ avoid_after: e.target.value })} aria-label="No sessions after" className={inputClass} />
            </Field>
            <Field label="break around sessions (min)" hint="Free time kept before and after, 0–120.">
              <input
                value={form.break_minutes}
                onChange={(e) => set({ break_minutes: e.target.value })}
                inputMode="numeric"
                aria-label="Break around sessions in minutes"
                className={inputClass}
              />
            </Field>
            <Field label="default task length (min)" hint="For tasks without an estimate, 15–480. Blank: 60.">
              <input
                value={form.default_task_minutes}
                onChange={(e) => set({ default_task_minutes: e.target.value })}
                inputMode="numeric"
                aria-label="Default task length in minutes"
                className={inputClass}
              />
            </Field>
            <Field label="most per day (min)" hint="Session minutes JARVIS plans on one day, 30–960. Blank: no limit.">
              <input
                value={form.max_daily_minutes}
                onChange={(e) => set({ max_daily_minutes: e.target.value })}
                inputMode="numeric"
                aria-label="Most session minutes per day"
                className={inputClass}
              />
            </Field>
          </div>
          <label className="flex items-center gap-2 text-sm">
            <input type="checkbox" checked={form.earliest_first} onChange={(e) => set({ earliest_first: e.target.checked })} className="size-4 accent-accent" />
            Fill the earliest free time first (otherwise sessions spread across days)
          </label>
          {"error" in parsed && (
            <p role="alert" className="text-sm text-bad">
              {parsed.error}
            </p>
          )}
          {error && (
            <p role="alert" className="text-sm text-bad">
              {error}
            </p>
          )}
          {message && (
            <p role="status" className="text-sm text-ok">
              {message}
            </p>
          )}
          <div className="flex flex-wrap gap-2">
            <Button type="submit" variant="primary" className={small} disabled={!changed || !("prefs" in parsed)}>
              save jarvis settings
            </Button>
            {saved && (
              <Button type="button" className={small} onClick={reset}>
                use defaults
              </Button>
            )}
          </div>
        </form>
      )}
    </Panel>
  );
}
