import { Suspense, lazy, useEffect, useRef, useState, type ComponentType } from "react";
import { AuthScreen } from "./components/AuthScreen";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { installGlobalErrorReporting } from "./hooks/reportError";
import { PlayerProvider } from "./components/Player";
import { StageTag, StatusChip, type Tone } from "./components/ui";
import { useHealth, type HealthState } from "./hooks/useHealth";
import { useOnline } from "./hooks/useOnline";
import { useSession, type SessionApi } from "./hooks/useSession";
import { useSync } from "./hooks/useSync";
import { useServiceWorker, type SwState } from "./hooks/useServiceWorker";
import { MODULES, SIDEBAR_GROUPS, TABS, moduleFromHash, sidebarModules, type ModuleId } from "./lib/modules";
import { Home } from "./modules/Home";

// Modules load on first use (each is its own chunk; the service worker
// precaches them all, so they still open offline). Home is the default route.
const Ask = lazy(() => import("./modules/Ask").then((m) => ({ default: m.Ask })));
const Files = lazy(() => import("./modules/Files").then((m) => ({ default: m.Files })));
const Inbox = lazy(() => import("./modules/Inbox").then((m) => ({ default: m.Inbox })));
const Jarvis = lazy(() => import("./modules/Jarvis").then((m) => ({ default: m.Jarvis })));
const Journal = lazy(() => import("./modules/Journal").then((m) => ({ default: m.Journal })));
const Library = lazy(() => import("./modules/Library").then((m) => ({ default: m.Library })));
const Modules = lazy(() => import("./modules/Modules").then((m) => ({ default: m.Modules })));
const Music = lazy(() => import("./modules/Music").then((m) => ({ default: m.Music })));
const Notes = lazy(() => import("./modules/Notes").then((m) => ({ default: m.Notes })));
const Calendar = lazy(() => import("./modules/Calendar").then((m) => ({ default: m.Calendar })));
const Habits = lazy(() => import("./modules/Habits").then((m) => ({ default: m.Habits })));
const Projects = lazy(() => import("./modules/Projects").then((m) => ({ default: m.Projects })));
const Search = lazy(() => import("./modules/Search").then((m) => ({ default: m.Search })));
const Settings = lazy(() => import("./modules/Settings").then((m) => ({ default: m.Settings })));
const Tasks = lazy(() => import("./modules/Tasks").then((m) => ({ default: m.Tasks })));

export interface ModuleProps {
  online: boolean;
  health: HealthState;
  sw: SwState;
  session: SessionApi;
  navigate: (id: ModuleId) => void;
}

const VIEWS: Record<ModuleId, ComponentType<ModuleProps>> = {
  home: Home,
  ask: Ask,
  search: Search,
  inbox: Inbox,
  tasks: Tasks,
  projects: Projects,
  calendar: Calendar,
  habits: Habits,
  notes: Notes,
  journal: Journal,
  library: Library,
  music: Music,
  files: Files,
  jarvis: Jarvis,
  settings: Settings,
  modules: Modules,
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

const shortcutLabel = /Mac|iPhone|iPad/.test(navigator.platform) ? "⌘J" : "Ctrl J";

export function App() {
  const [active, navigate] = useHashModule();
  const online = useOnline();
  const health = useHealth(online);
  const sw = useServiceWorker();
  const session = useSession(online);
  const sync = useSync(online);
  const main = useRef<HTMLElement>(null);
  const activeRef = useRef(active);
  activeRef.current = active;
  useEffect(() => installGlobalErrorReporting(() => activeRef.current), []);
  const current = MODULES.find((m) => m.id === active)!;
  const View = VIEWS[active];
  const swState = swChip(sw);
  const api = apiChip(online, health);

  useEffect(() => {
    document.title = active === "home" ? "Directory" : `${current.label} · Directory`;
  }, [active, current.label]);

  // "/" opens search from anywhere (except while typing in a field); ⌘J / Ctrl+J opens Ask.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const el = e.target as HTMLElement | null;
      const typing = el && (el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName));
      if (e.key === "/" && !typing && !e.metaKey && !e.ctrlKey && !e.altKey) {
        e.preventDefault();
        location.hash = "/search";
      }
      if (e.key.toLowerCase() === "j" && (e.metaKey || e.ctrlKey) && !e.altKey && !e.shiftKey) {
        e.preventDefault();
        location.hash = "/ask";
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

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

  const user = status.user;
  // Phone: only chips that need attention (desktop shows all four).
  const quiet = { net: online, api: api.tone === "ok", sw: swState.tone === "ok" };

  return (
    <PlayerProvider>
    {/* Keyboard users can jump past the navigation (a link would change the hash route). */}
    <button
      type="button"
      onClick={() => main.current?.focus()}
      className="sr-only z-50 rounded-full bg-ink px-4 py-2 text-sm text-canvas focus:not-sr-only focus:fixed focus:top-2 focus:left-2"
    >
      Skip to content
    </button>
    <div className="flex h-full">
      {/* Sidebar (tablet / desktop): Paper "Desktop · Home" */}
      <aside className="hidden w-60 shrink-0 flex-col border-r border-line bg-panel md:flex">
        <a href="#/home" className="flex items-center gap-3 px-6 pt-7 pb-6">
          <span aria-hidden className="bg-tidal size-6 rounded-[7px]" />
          <span className="text-[15px] font-medium">Directory</span>
        </a>
        <nav aria-label="Modules" className="flex flex-1 flex-col overflow-y-auto px-3 pb-4">
          {SIDEBAR_GROUPS.map(({ group, label }) => (
            <div key={group} className={label ? "mt-6" : undefined}>
              {label && <p className="label px-3 pb-2">{label}</p>}
              <ul className="flex flex-col gap-0.5">
                {sidebarModules(group).map((m) => {
                  const isActive = m.id === active;
                  return (
                    <li key={m.id}>
                      <a
                        href={`#/${m.id}`}
                        aria-current={isActive ? "page" : undefined}
                        className={`flex min-h-9 items-center gap-3 rounded-sm px-3 text-[15px] transition ${
                          isActive ? "bg-sunken text-ink" : "text-muted hover:bg-sunken/60 hover:text-ink"
                        }`}
                      >
                        <span aria-hidden className={`w-4 shrink-0 text-center font-mono text-sm ${isActive ? "text-accent" : ""}`}>
                          {m.glyph}
                        </span>
                        <span className="flex-1">{m.label}</span>
                        {m.stage === "planned" && <StageTag stage={m.stage} />}
                      </a>
                    </li>
                  );
                })}
              </ul>
            </div>
          ))}
        </nav>
        {/* Account footer: opens Settings. */}
        <a
          href="#/settings"
          aria-current={active === "settings" ? "page" : undefined}
          className={`m-3 flex items-center gap-3 rounded-md px-3 py-3 transition hover:bg-sunken ${active === "settings" ? "bg-sunken" : "bg-sunken/60"}`}
        >
          <span aria-hidden className="grid size-8 shrink-0 place-items-center rounded-full bg-periwinkle text-sm font-medium text-[#1c1e2e]">
            {user.displayName.trim().charAt(0).toUpperCase() || "·"}
          </span>
          <span className="min-w-0 flex-1">
            <span className="block truncate text-sm">{user.displayName}</span>
            <span
              className="block font-mono text-[11px] text-muted"
              title={status.verified ? undefined : "Server unreachable: using the session saved on this device"}
            >
              Settings · {status.verified ? `v${health.report?.version ?? "0.0.1"}` : "offline · unverified"}
            </span>
          </span>
        </a>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="pt-safe sticky top-0 z-10 bg-canvas/85 backdrop-blur">
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2 px-4 pt-4 pb-3 md:px-10 md:pt-6">
            <h1 className={active === "home" ? "sr-only" : "min-w-0 flex-1 basis-40 truncate text-[22px] tracking-tight"}>{current.label}</h1>
            {active === "home" && <span className="flex-1" />}
            <div className="flex flex-wrap items-center gap-1.5" role="status" aria-live="polite">
              <span className={quiet.net ? "hidden md:inline-flex" : "inline-flex"}>
                <StatusChip tone={online ? "ok" : "warn"} label="net" value={online ? "online" : "offline"} />
              </span>
              <span className={quiet.api ? "hidden md:inline-flex" : "inline-flex"}>
                <StatusChip tone={api.tone} label="api" value={api.value} />
              </span>
              <span className={quiet.sw ? "hidden md:inline-flex" : "inline-flex"}>
                <StatusChip tone={swState.tone} label="offline" value={swState.value} title="Service worker / app-shell cache" />
              </span>
              {sync.pending > 0 && (
                <StatusChip tone="warn" label="sync" value={sync.syncing ? "syncing…" : `${sync.pending} pending`} title="Changes saved on this device, waiting to sync" />
              )}
              {active !== "ask" && (
                <a
                  href="#/ask"
                  className="ml-1 hidden min-h-8 items-center gap-2 rounded-full border border-line bg-panel px-3 text-[13px] transition hover:border-accent md:inline-flex"
                >
                  <span aria-hidden className="size-1.5 rounded-full bg-accent" />
                  Ask JARVIS
                  <kbd className="font-mono text-[11px] text-muted">{shortcutLabel}</kbd>
                </a>
              )}
            </div>
          </div>
          {sync.rejected.length > 0 && (
            <div role="alert" className="flex items-start justify-between gap-3 border-y border-line bg-panel px-4 py-2 text-sm md:px-10">
              <ul className="text-bad">
                {sync.rejected.map((r, i) => (
                  <li key={i}>{r}</li>
                ))}
              </ul>
              <button type="button" onClick={sync.dismiss} className="min-h-8 text-muted underline underline-offset-4">
                Dismiss
              </button>
            </div>
          )}
          {sw.kind === "update" && (
            <div className="flex items-center justify-between gap-3 border-y border-line bg-panel px-4 py-2 text-sm md:px-10">
              <span>A new version of Directory is ready.</span>
              <button type="button" onClick={sw.apply} className="min-h-8 font-medium text-accent underline underline-offset-4">
                Reload to update
              </button>
            </div>
          )}
        </header>

        <main ref={main} tabIndex={-1} className="flex-1 overflow-y-auto px-4 pt-3 pb-40 focus:outline-none md:px-10 md:pb-24">
          <div className="mx-auto max-w-6xl">
            {/* A crash stays inside the module; switching modules starts fresh. */}
            <ErrorBoundary key={active} module={active}>
              <Suspense fallback={<p className="text-sm text-muted">Loading…</p>}>
                <View online={online} health={health} sw={sw} session={session} navigate={navigate} />
              </Suspense>
            </ErrorBoundary>
          </div>
        </main>

        {/* Tab bar (phone): Paper "Mobile · Home". Ask is the raised JARVIS button. */}
        <nav aria-label="Tabs" className="pb-safe fixed inset-x-0 bottom-0 z-10 border-t border-line bg-panel/95 backdrop-blur md:hidden">
          <ul className="mx-auto flex max-w-md items-end">
            {TABS.map((id) => {
              const m = MODULES.find((x) => x.id === id)!;
              const isActive = id === active || (id === "modules" && !TABS.includes(active));
              if (id === "ask") {
                return (
                  <li key={id} className="flex flex-1 justify-center">
                    <a
                      href="#/ask"
                      aria-label="Ask JARVIS"
                      aria-current={isActive ? "page" : undefined}
                      className="bg-tidal -mt-7 mb-2 grid size-16 place-items-center rounded-full shadow-[0_8px_24px_-8px_rgb(59_63_168/60%)] ring-4 ring-canvas"
                    >
                      <svg aria-hidden viewBox="0 0 24 24" className="size-7" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
                        <path d="M4 10v4M8 7v10M12 4v16M16 7v10M20 10v4" />
                      </svg>
                    </a>
                  </li>
                );
              }
              return (
                <li key={id} className="flex-1">
                  <a
                    href={`#/${id}`}
                    aria-current={isActive ? "page" : undefined}
                    className={`flex min-h-14 flex-col items-center justify-center gap-1 text-[11px] ${isActive ? "text-ink" : "text-muted"}`}
                  >
                    <span
                      aria-hidden
                      className={`grid size-6 place-items-center rounded-[7px] border font-mono text-xs leading-none ${
                        isActive ? "border-accent bg-accent text-accent-ink" : "border-current"
                      }`}
                    >
                      {m.glyph}
                    </span>
                    {m.label}
                  </a>
                </li>
              );
            })}
          </ul>
        </nav>
      </div>
    </div>
    </PlayerProvider>
  );
}
