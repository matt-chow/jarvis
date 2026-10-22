import { useEffect, useState, type ComponentType } from "react";
import { AuthScreen } from "./components/AuthScreen";
import { StatusChip, StageTag, type Tone } from "./components/ui";
import { useHealth, type HealthState } from "./hooks/useHealth";
import { useOnline } from "./hooks/useOnline";
import { useSession, type SessionApi } from "./hooks/useSession";
import { useServiceWorker, type SwState } from "./hooks/useServiceWorker";
import { MODULES, moduleFromHash, type ModuleId } from "./lib/modules";
import { Files } from "./modules/Files";
import { Home } from "./modules/Home";
import { Jarvis } from "./modules/Jarvis";
import { Notes } from "./modules/Notes";
import { Projects } from "./modules/Projects";
import { Settings } from "./modules/Settings";
import { Tasks } from "./modules/Tasks";

export interface ModuleProps {
  online: boolean;
  health: HealthState;
  sw: SwState;
  session: SessionApi;
  navigate: (id: ModuleId) => void;
}

const VIEWS: Record<ModuleId, ComponentType<ModuleProps>> = {
  home: Home,
  tasks: Tasks,
  projects: Projects,
  notes: Notes,
  files: Files,
  jarvis: Jarvis,
  settings: Settings,
};

function useHashModule(): [ModuleId, (id: ModuleId) => void] {
  const [id, setId] = useState(() => moduleFromHash(location.hash));
  useEffect(() => {
    const onHash = () => setId(moduleFromHash(location.hash));
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);
  return [id, (next) => (location.hash = `/${next}`)];
}

export function swChip(sw: SwState): { tone: Tone; value: string } {
  switch (sw.kind) {
    case "ready":
      return { tone: "ok", value: "ready" };
    case "update":
      return { tone: "accent", value: "update" };
    case "installing":
      return { tone: "warn", value: "caching" };
    case "disabled":
      return { tone: "idle", value: "dev" };
    case "unsupported":
      return { tone: "idle", value: "n/a" };
    case "error":
      return { tone: "bad", value: "error" };
  }
}

function apiChip(online: boolean, health: HealthState): { tone: Tone; value: string } {
  if (health.loading && !health.report) return { tone: "idle", value: "…" };
  if (!online || (health.error && health.stale)) return { tone: "idle", value: "cached" };
  if (health.error) return { tone: "bad", value: "down" };
  return health.report?.status === "ok" ? { tone: "ok", value: "ok" } : { tone: "warn", value: "degraded" };
}

export function App() {
  const [active, navigate] = useHashModule();
  const online = useOnline();
  const health = useHealth(online);
  const sw = useServiceWorker();
  const session = useSession(online);
  const current = MODULES.find((m) => m.id === active)!;
  const View = VIEWS[active];
  const swState = swChip(sw);
  const api = apiChip(online, health);

  useEffect(() => {
    document.title = active === "home" ? "Directory" : `${current.label} · Directory`;
  }, [active, current.label]);

  const status = session.status;
  if (status.kind === "loading") {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="cursor font-mono text-sm tracking-[0.25em] text-muted">DIRECTORY</p>
      </div>
    );
  }
  if (status.kind !== "signed-in") {
    return (
      <AuthScreen
        setupRequired={status.kind === "signed-out" && status.setupRequired}
        unavailable={status.kind === "unavailable" ? status.message : undefined}
        onSignedIn={session.signedIn}
        onRetry={() => void session.refresh()}
      />
    );
  }

  return (
    <div className="flex h-full">
      {/* Sidebar (tablet / desktop) */}
      <aside className="hidden w-60 shrink-0 flex-col border-r border-line bg-panel/80 backdrop-blur md:flex">
        <div className="px-5 pt-6 pb-5">
          <p className="cursor font-mono text-sm font-semibold tracking-[0.25em]">DIRECTORY</p>
          <p className="label mt-1">personal operating env</p>
        </div>
        <nav aria-label="Modules" className="flex flex-1 flex-col gap-0.5 px-2">
          {MODULES.map((m) => {
            const isActive = m.id === active;
            return (
              <a
                key={m.id}
                href={`#/${m.id}`}
                aria-current={isActive ? "page" : undefined}
                className={`group flex items-center gap-3 rounded-sm px-3 py-2 text-sm transition ${
                  isActive ? "bg-sunken text-ink shadow-[inset_2px_0_0_var(--accent)]" : "text-muted hover:bg-sunken/60 hover:text-ink"
                }`}
              >
                <span aria-hidden className={`w-4 text-center font-mono ${isActive ? "text-accent" : ""}`}>
                  {m.glyph}
                </span>
                <span className="flex-1">{m.label}</span>
                {m.stage !== "ready" && <StageTag stage={m.stage} />}
              </a>
            );
          })}
        </nav>
        <div className="border-t border-line px-5 py-4 font-mono text-[11px] text-muted">
          <p className="truncate text-ink">{status.user.displayName}</p>
          <p title={status.verified ? undefined : "Server unreachable: using the session saved on this device"}>
            {status.verified ? "signed in" : "offline · unverified"} · v{health.report?.version ?? "0.0.1"}
          </p>
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="pt-safe sticky top-0 z-10 border-b border-line bg-canvas/85 backdrop-blur">
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 py-3 md:px-8">
            <div className="min-w-0 flex-1 basis-40">
              <p className="label md:hidden">directory</p>
              <h1 className="truncate font-mono text-base font-semibold tracking-wide">
                <span className="text-accent">{current.glyph}</span> {current.label}
              </h1>
            </div>
            <div className="flex flex-wrap gap-1.5" role="status" aria-live="polite">
              <StatusChip tone={online ? "ok" : "warn"} label="net" value={online ? "online" : "offline"} />
              <StatusChip tone={api.tone} label="api" value={api.value} />
              <StatusChip tone={swState.tone} label="offline" value={swState.value} title="Service worker / app-shell cache" />
            </div>
          </div>
          {sw.kind === "update" && (
            <div className="flex items-center justify-between gap-3 border-t border-line bg-sunken px-4 py-2 font-mono text-xs md:px-8">
              <span>A new version of Directory is ready.</span>
              <button type="button" onClick={sw.apply} className="text-accent underline underline-offset-4">
                Reload to update
              </button>
            </div>
          )}
        </header>

        <main className="flex-1 overflow-y-auto px-4 py-6 pb-28 md:px-8 md:pb-10">
          <div className="mx-auto max-w-5xl">
            <View online={online} health={health} sw={sw} session={session} navigate={navigate} />
          </div>
        </main>

        {/* Bottom tab bar (phone) */}
        <nav
          aria-label="Modules"
          className="pb-safe fixed inset-x-0 bottom-0 z-10 border-t border-line bg-panel/95 backdrop-blur md:hidden"
        >
          <ul className="flex overflow-x-auto">
            {MODULES.map((m) => (
              <li key={m.id} className="min-w-[4.25rem] flex-1">
                <a
                  href={`#/${m.id}`}
                  aria-current={m.id === active ? "page" : undefined}
                  className={`flex flex-col items-center gap-1 py-2.5 text-[10px] ${m.id === active ? "text-accent" : "text-muted"}`}
                >
                  <span aria-hidden className="font-mono text-base leading-none">
                    {m.glyph}
                  </span>
                  {m.label}
                </a>
              </li>
            ))}
          </ul>
        </nav>
      </div>
    </div>
  );
}
