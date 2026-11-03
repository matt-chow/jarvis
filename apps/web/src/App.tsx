import { Suspense, lazy, useEffect, useRef, useState, type ComponentType } from "react";
import { AuthScreen } from "./components/AuthScreen";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { Icon } from "./components/icons";
import { installGlobalErrorReporting } from "./hooks/reportError";
import { PlayerProvider } from "./components/Player";
import { QuickTask } from "./components/QuickTask";
import { ShortcutsDialog } from "./components/shell/ShortcutsDialog";
import { StatusIndicator } from "./components/shell/StatusIndicator";
import { StageTag } from "./components/ui";
import { useHealth, type HealthState } from "./hooks/useHealth";
import { useOnline } from "./hooks/useOnline";
import { useSession, type SessionApi } from "./hooks/useSession";
import { useSync } from "./hooks/useSync";
import { useServiceWorker, type SwState } from "./hooks/useServiceWorker";
import { useDevice } from "./lib/device";
import { noteRoute } from "./lib/focus";
import { MODULES, SIDEBAR_GROUPS, TABS, moduleFromHash, sidebarModules, type ModuleId } from "./lib/modules";
import { ariaKeys, isTyping, keyLabel, matchShortcut, shortcutById } from "./lib/shortcuts";
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
const Reminders = lazy(() => import("./modules/Reminders").then((m) => ({ default: m.Reminders })));

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
  reminders: Reminders,
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

const desktop = () => window.matchMedia("(min-width: 1024px)").matches;

export function App() {
  const [active, navigate] = useHashModule();
  useEffect(() => noteRoute(active), [active]);
  const online = useOnline();
  const health = useHealth(online);
  const sw = useServiceWorker();
  const session = useSession(online);
  const sync = useSync(online);
  const [device, setDevice] = useDevice();
  const [help, setHelp] = useState(false);
  const main = useRef<HTMLElement>(null);
  const activeRef = useRef(active);
  activeRef.current = active;
  useEffect(() => installGlobalErrorReporting(() => activeRef.current), []);
  const current = MODULES.find((m) => m.id === active)!;
  const View = VIEWS[active];
  // The side panel shows beside other screens; on Ask itself it would be a duplicate.
  const panelOpen = device.jarvisPanel && active !== "ask";
  const togglePanel = () => {
    if (!desktop()) {
      location.hash = "/ask";
      return;
    }
    setDevice({ jarvisPanel: !device.jarvisPanel });
  };
  const toggleRef = useRef(togglePanel);
  toggleRef.current = togglePanel;

  useEffect(() => {
    document.title = active === "home" ? "Directory" : `${current.label} · Directory`;
  }, [active, current.label]);

  // Global shortcuts (lib/shortcuts.ts); never while typing in a field.
  useEffect(() => {
    let pending: string | null = null;
    let timer: number | undefined;
    const onKey = (e: KeyboardEvent) => {
      if (e.defaultPrevented || e.repeat) return;
      const chord = e.metaKey || e.ctrlKey || e.altKey;
      if (isTyping(e.target) && !chord) return;
      if (isTyping(e.target) && e.altKey) return;
      const hit = matchShortcut(e, pending);
      pending = null;
      window.clearTimeout(timer);
      if (hit === "pending") {
        pending = "g";
        timer = window.setTimeout(() => (pending = null), 1200);
        return;
      }
      if (!hit) return;
      // Alt+N belongs to QuickTask's own listener.
      if (hit.id === "task") return;
      e.preventDefault();
      if (hit.id === "help") setHelp(true);
      else if (hit.id === "panel") toggleRef.current();
      else if (hit.to) location.hash = `/${hit.to}`;
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.clearTimeout(timer);
    };
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
  const panelKeys = shortcutById("panel");

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
        {/* Sidebar (tablet and desktop). */}
        <aside className="hidden w-60 shrink-0 flex-col border-r border-line bg-panel md:flex">
          <a href="#/home" className="flex items-center gap-3 px-6 pt-6 pb-5">
            <span aria-hidden className="bg-tidal size-6 rounded-[7px]" />
            <span className="text-[15px] font-medium">Directory</span>
          </a>
          <nav aria-label="Modules" className="flex flex-1 flex-col overflow-y-auto px-3 pb-4">
            {SIDEBAR_GROUPS.map(({ group, label }) => (
              <div key={group} className={label ? "mt-5" : undefined}>
                {label && <p className="px-3 pb-1.5 text-xs text-muted">{label}</p>}
                <ul className="flex flex-col gap-px">
                  {sidebarModules(group).map((m) => {
                    const isActive = m.id === active;
                    return (
                      <li key={m.id}>
                        <a
                          href={`#/${m.id}`}
                          aria-current={isActive ? "page" : undefined}
                          className={`flex min-h-9 items-center gap-3 rounded-sm px-3 text-[15px] transition-colors ${
                            isActive ? "bg-sunken text-ink" : "text-muted hover:bg-sunken/60 hover:text-ink"
                          }`}
                        >
                          <Icon name={m.icon} className={isActive ? "text-accent" : ""} />
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
          <button
            type="button"
            onClick={() => setHelp(true)}
            aria-keyshortcuts="Shift+?"
            className="mx-3 flex min-h-9 items-center gap-3 rounded-sm px-3 text-sm text-muted transition-colors hover:bg-sunken/60 hover:text-ink"
          >
            <Icon name="keyboard" />
            <span className="flex-1 text-left">Shortcuts</span>
            <kbd className="font-mono text-xs">?</kbd>
          </button>
          {/* Account footer: opens Settings. */}
          <a
            href="#/settings"
            aria-current={active === "settings" ? "page" : undefined}
            className={`m-3 flex items-center gap-3 rounded-md px-3 py-3 transition-colors hover:bg-sunken ${active === "settings" ? "bg-sunken" : "bg-sunken/60"}`}
          >
            <span aria-hidden className="grid size-8 shrink-0 place-items-center rounded-full bg-periwinkle text-sm font-medium text-[#1c1e2e]">
              {user.displayName.trim().charAt(0).toUpperCase() || "·"}
            </span>
            <span className="min-w-0 flex-1">
              <span className="block truncate text-sm">{user.displayName}</span>
              <span className="block text-xs text-muted" title={status.verified ? undefined : "Server unreachable: using the session saved on this device"}>
                Settings · {status.verified ? `v${health.report?.version ?? "0.0.1"}` : "offline, unverified"}
              </span>
            </span>
          </a>
        </aside>

        <div className="flex min-w-0 flex-1 flex-col">
          <header className="pt-safe sticky top-0 z-20 bg-canvas/90 backdrop-blur">
            <div className="flex items-center gap-2 px-4 pt-3 pb-2 md:px-8 md:pt-5">
              <h1 className={active === "home" ? "sr-only" : "min-w-0 flex-1 truncate text-[22px] font-medium tracking-tight"}>{current.label}</h1>
              {active === "home" && <span className="flex-1" />}
              <StatusIndicator online={online} health={health} sw={sw} sync={sync} />
              {active !== "ask" && <QuickTask online={online} />}
              {active !== "ask" && (
                <button
                  type="button"
                  onClick={togglePanel}
                  aria-pressed={panelOpen}
                  aria-keyshortcuts={ariaKeys(panelKeys)}
                  title={`JARVIS side panel (${panelKeys.keys.map((k) => keyLabel(k)).join(" ")})`}
                  className={`press hidden min-h-9 items-center gap-2 rounded-full border px-3 text-[13px] transition-colors md:inline-flex ${
                    panelOpen ? "border-accent bg-accent text-accent-ink" : "border-line bg-panel hover:border-accent"
                  }`}
                >
                  <Icon name="ask" size={16} />
                  JARVIS
                </button>
              )}
              <button type="button" onClick={() => setHelp(true)} aria-label="Keyboard shortcuts and help" className="grid size-9 place-items-center rounded-full text-muted hover:bg-sunken hover:text-ink md:hidden">
                <Icon name="help" />
              </button>
            </div>
            {sync.rejected.length > 0 && (
              <div role="alert" className="flex items-start justify-between gap-3 border-y border-line bg-panel px-4 py-2 text-sm md:px-8">
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
              <div className="flex items-center justify-between gap-3 border-y border-line bg-panel px-4 py-2 text-sm md:px-8">
                <span>A new version of Directory is ready.</span>
                <button type="button" onClick={sw.apply} className="min-h-8 font-medium text-accent underline underline-offset-4">
                  Reload to update
                </button>
              </div>
            )}
          </header>

          <div className="flex min-h-0 flex-1">
            <main ref={main} tabIndex={-1} className="min-w-0 flex-1 overflow-y-auto px-4 pt-3 pb-40 focus:outline-none md:px-8 md:pb-24">
              <div className="mx-auto max-w-6xl">
                {/* A crash stays inside the module; switching modules starts fresh. */}
                <ErrorBoundary key={active} module={active}>
                  <Suspense fallback={<p className="text-sm text-muted">Loading…</p>}>
                    <View online={online} health={health} sw={sw} session={session} navigate={navigate} />
                  </Suspense>
                </ErrorBoundary>
              </div>
            </main>
            {panelOpen && (
              <aside aria-label="JARVIS side panel" className="drawer hidden w-[24rem] shrink-0 border-l border-line lg:block">
                <ErrorBoundary module="ask">
                  <Suspense fallback={<p className="p-5 text-sm text-muted">Loading…</p>}>
                    <Ask online={online} panel={{ onClose: () => setDevice({ jarvisPanel: false }) }} />
                  </Suspense>
                </ErrorBoundary>
              </aside>
            )}
          </div>

          {/* Tab bar (phone). JARVIS capture is the raised middle button. */}
          <nav aria-label="Tabs" className="pb-safe fixed inset-x-0 bottom-0 z-20 border-t border-line bg-panel/95 backdrop-blur md:hidden">
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
                        className="press bg-tidal -mt-6 mb-2 grid size-14 place-items-center rounded-full ring-4 ring-canvas"
                      >
                        <Icon name="ask" size={26} />
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
                      <Icon name={m.icon} size={22} className={isActive ? "text-accent" : ""} />
                      {m.label}
                    </a>
                  </li>
                );
              })}
            </ul>
          </nav>
        </div>
      </div>
      <ShortcutsDialog open={help} onClose={() => setHelp(false)} />
    </PlayerProvider>
  );
}
