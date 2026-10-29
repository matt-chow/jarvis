import { useState, type ReactNode } from "react";
import type { ModuleProps } from "../App";
import { CaptureBox } from "../components/CaptureBox";
import { ModuleGrid } from "../components/ModuleGrid";
import { Button, Panel } from "../components/ui";
import { useInbox } from "../hooks/useInbox";
import { usePreferences } from "../hooks/usePreferences";
import { formatTime, greeting, longDay } from "../lib/format";
import { WIDGETS, moveWidget, widgetsOf, type WidgetId } from "../lib/preferences";
import { SuggestCard } from "./Suggest";
import { HabitsWidget, NotesWidget, TodayWidget } from "./widgets";

// Paper "Desktop · Home" and "Mobile · Home" (docs/DESIGN_HANDOFF.md). Widgets stay customizable (D-023).

const SW_TEXT: Record<ModuleProps["sw"]["kind"], string> = {
  ready: "App shell cached. Directory will open without a connection.",
  installing: "Caching the app shell for offline use…",
  update: "A newer version is cached and waiting.",
  disabled: "Service worker is off in the dev server. Use `npm run build` + `npm run dev:worker` to test offline.",
  unsupported: "This browser does not support service workers.",
  error: "Service worker failed to register.",
};

const small = "min-h-8 px-3 text-xs";
/** Widgets that span both columns on wide screens. */
const WIDE: ReadonlySet<WidgetId> = new Set(["capture", "modules"]);

export function Home(props: ModuleProps) {
  const { online, session } = props;
  const { prefs, error, update } = usePreferences(online);
  const [customizing, setCustomizing] = useState(false);
  const now = new Date();
  const widgets = widgetsOf(prefs);
  const hidden = WIDGETS.filter((w) => !widgets.includes(w.id));
  /** Apply `f` to the latest layout (not this render's), so quick edits all count. */
  const save = (f: (current: WidgetId[]) => WidgetId[]) => void update((p) => ({ dashboard: { widgets: f(widgetsOf(p)) } }));

  return (
    <div className="flex flex-col gap-5">
      {/* Paper "Desktop · Home": date eyebrow and a light display greeting. */}
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div className="min-w-0">
          <p className="label">{longDay(now)}</p>
          <h2 className="mt-2 text-[34px] leading-[1.08] font-light tracking-[-0.02em] md:text-[48px] xl:text-[56px]">
            {greeting(now.getHours())}
            {session.status.kind === "signed-in" ? `, ${session.status.user.displayName.split(" ")[0]}` : ""}.
          </h2>
        </div>
        <Button className={small} disabled={!prefs} aria-pressed={customizing} onClick={() => setCustomizing((v) => !v)}>
          {customizing ? "done" : "customize"}
        </Button>
      </div>

      <SuggestCard online={online} />
      {error && (
        <p role="alert" className="text-sm text-bad">
          {error}
        </p>
      )}

      {customizing && (
        <Panel title="dashboard layout">
          <p className="text-sm text-muted">Reorder or hide widgets below. Your layout is saved to your account and follows you to other devices.</p>
          {hidden.length > 0 && (
            <div className="mt-3 flex flex-wrap items-center gap-1.5">
              <span className="label mr-1">add</span>
              {hidden.map((w) => (
                <Button key={w.id} className={small} title={w.summary} onClick={() => save((cur) => [...cur, w.id])}>
                  + {w.label.toLowerCase()}
                </Button>
              ))}
            </div>
          )}
          <Button className={`${small} mt-3`} onClick={() => void update({ dashboard: null })}>
            reset to default
          </Button>
        </Panel>
      )}

      {widgets.length === 0 ? (
        <p className="text-sm text-muted">Your dashboard is empty. Use customize to add widgets.</p>
      ) : (
        <div className="grid gap-5 md:grid-cols-2">
          {widgets.map((id, i) => {
            const className = WIDE.has(id) ? "md:col-span-2" : undefined;
            const body = <Widget id={id} props={props} className={customizing ? undefined : className} />;
            if (!customizing) return <div key={id} className="contents">{body}</div>;
            const label = WIDGETS.find((w) => w.id === id)!.label;
            return (
              <div key={id} className={`flex flex-col gap-1.5 ${className ?? ""}`}>
                <div className="flex items-center gap-1.5">
                  <span className="label mr-auto">{label}</span>
                  <Button className={small} aria-label={`Move ${label} up`} disabled={i === 0} onClick={() => save((cur) => moveWidget(cur, id, -1))}>
                    ↑
                  </Button>
                  <Button className={small} aria-label={`Move ${label} down`} disabled={i === widgets.length - 1} onClick={() => save((cur) => moveWidget(cur, id, 1))}>
                    ↓
                  </Button>
                  <Button className={small} aria-label={`Hide ${label}`} onClick={() => save((cur) => cur.filter((w) => w !== id))}>
                    hide
                  </Button>
                </div>
                {body}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

function Widget({ id, props, className }: { id: WidgetId; props: ModuleProps; className?: string }): ReactNode {
  switch (id) {
    case "capture":
      return <CaptureWidget online={props.online} className={className} />;
    case "today":
      return <TodayWidget online={props.online} className={className} />;
    case "habits":
      return <HabitsWidget online={props.online} className={className} />;
    case "notes":
      return <NotesWidget online={props.online} className={className} />;
    case "status":
      return <StatusWidget props={props} className={className} />;
    case "modules":
      return <ModulesWidget navigate={props.navigate} className={className} />;
  }
}

function CaptureWidget({ online, className }: { online: boolean; className?: string }) {
  const inbox = useInbox(online);
  return (
    <Panel
      title="capture"
      className={className}
      actions={
        inbox.items && inbox.items.length > 0 ? (
          <a href="#/inbox" className="font-mono text-[11px] text-accent underline-offset-4 hover:underline">
            {inbox.items.length} in inbox →
          </a>
        ) : undefined
      }
    >
      <CaptureBox disabled={!inbox.items} onCapture={(t) => inbox.capture(t)} />
    </Panel>
  );
}

function StatusWidget({ props, className }: { props: ModuleProps; className?: string }) {
  const { health, online, sw } = props;
  const { report } = health;
  return (
    <Panel
      title="system status"
      className={className}
      actions={
        <Button onClick={health.refresh} disabled={health.loading || !online} className={small}>
          {health.loading ? "checking" : "recheck"}
        </Button>
      }
    >
      {report ? (
        <>
          <ul className="divide-y divide-line font-mono text-sm">
            {report.checks.map((c) => (
              <li key={c.name} className="flex items-center justify-between gap-4 py-2">
                <span className="text-muted">{c.name}</span>
                <span className={c.status === "ok" ? "text-ok" : "text-warn"}>
                  {c.status === "ok" ? "[ ok ]" : "[warn]"} {c.detail}
                </span>
              </li>
            ))}
          </ul>
          <p className="mt-3 font-mono text-[11px] text-muted">
            {report.service} v{report.version}
            {health.checkedAt && ` · checked ${formatTime(health.checkedAt)}`}
            {health.stale && " · last known (API unreachable)"}
          </p>
        </>
      ) : (
        <p className="font-mono text-sm text-muted">
          {health.loading ? "Contacting API…" : `API unreachable${health.error ? ` (${health.error})` : ""}.`}
        </p>
      )}
      <div className="mt-4 border-t border-line pt-3">
        <p className="label mb-1">offline</p>
        <p className="text-sm">{SW_TEXT[sw.kind]}</p>
        <p className="mt-2 text-sm text-muted">
          {online
            ? "Try it: install or load the app once, then go offline and reload."
            : "You are offline. Cached modules still work; changes stay on this device."}
        </p>
      </div>
    </Panel>
  );
}

function ModulesWidget({ navigate, className }: { navigate: ModuleProps["navigate"]; className?: string }) {
  return (
    <div className={className}>
      <p className="label mb-3">modules</p>
      <ModuleGrid navigate={navigate} exclude={["home", "modules"]} />
    </div>
  );
}
