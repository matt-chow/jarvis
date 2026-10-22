import type { ModuleProps } from "../App";
import { Button, Panel, StageTag } from "../components/ui";
import { formatTime } from "../lib/format";
import { MODULES } from "../lib/modules";

const SW_TEXT: Record<ModuleProps["sw"]["kind"], string> = {
  ready: "App shell cached. Directory will open without a connection.",
  installing: "Caching the app shell for offline use…",
  update: "A newer version is cached and waiting.",
  disabled: "Service worker is off in the dev server. Use `npm run build` + `npm run dev:worker` to test offline.",
  unsupported: "This browser does not support service workers.",
  error: "Service worker failed to register.",
};

export function Home({ online, health, sw, session, navigate }: ModuleProps) {
  const { report } = health;
  return (
    <div className="grid gap-5 lg:grid-cols-5">
      <div className="lg:col-span-5">
        <p className="label">boot sequence complete</p>
        <h2 className="mt-1 text-2xl font-semibold tracking-tight md:text-3xl">
          Good to see you{session.status.kind === "signed-in" ? `, ${session.status.user.displayName.split(" ")[0]}` : ""}.
        </h2>
        <p className="mt-2 max-w-2xl text-sm text-muted">
          Directory is your modular personal workspace. This is the Sprint 00 foundation: the shell, offline cache,
          on-device storage, and the private cloud API.
        </p>
      </div>

      <Panel
        title="system status"
        className="lg:col-span-3"
        actions={
          <Button onClick={health.refresh} disabled={health.loading || !online} className="min-h-7 px-2 text-[10px]">
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
      </Panel>

      <Panel title="offline" className="lg:col-span-2">
        <p className="text-sm">{SW_TEXT[sw.kind]}</p>
        <p className="mt-3 text-sm text-muted">
          {online
            ? "Try it: install or load the app once, then go offline and reload."
            : "You are offline. Cached modules still work; changes stay on this device."}
        </p>
      </Panel>

      <div className="lg:col-span-5">
        <p className="label mb-2">modules</p>
        <ul className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {MODULES.filter((m) => m.id !== "home").map((m) => (
            <li key={m.id}>
              <button
                type="button"
                onClick={() => navigate(m.id)}
                className="group flex h-full w-full flex-col rounded-md border border-line bg-panel p-4 text-left transition hover:border-accent"
              >
                <span className="flex items-center justify-between">
                  <span aria-hidden className="font-mono text-xl text-accent">
                    {m.glyph}
                  </span>
                  <StageTag stage={m.stage} />
                </span>
                <span className="mt-3 font-medium">{m.label}</span>
                <span className="mt-1 text-sm text-muted">{m.summary}</span>
                <span className="label mt-3">{m.offline ? "works offline" : "needs connection"}</span>
              </button>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
