/**
 * Where the installed app (Home Screen / standalone) opens. A per-device
 * choice, stored in localStorage (not a secret). Browser tabs are never
 * redirected: they keep the dashboard as the landing page.
 *
 * iOS opens a Home Screen app at the URL saved when it was installed and
 * ignores manifest shortcuts, so the route is chosen here, at startup, before
 * the shell reads `location.hash` (see docs/JARVIS_ASK.md › Home Screen launch).
 */

export const LAUNCH_KEY = "launch:module";
const HANDLED_KEY = "launch:handled";

export type LaunchChoice = "ask" | "home";
export const LAUNCH_CHOICES: readonly LaunchChoice[] = ["ask", "home"];

export interface LaunchEnv {
  hash: string;
  /** Running as an installed app (iOS `navigator.standalone`, or `display-mode: standalone`). */
  standalone: boolean;
  /** iOS Home Screen app (`navigator.standalone === true`): defaults to Ask. */
  ios: boolean;
  /** `PerformanceNavigationTiming.type`: only a fresh launch ("navigate") is redirected. */
  navigationType: string | undefined;
  /** The launch route was already decided in this app session (survives reloads). */
  handled: boolean;
  saved: string | null;
}

/** Hashes that mean "no route chosen": the default landing, as saved at install time. */
const UNROUTED = new Set(["", "#", "#/", "#/home"]);

export function parseChoice(value: string | null): LaunchChoice | undefined {
  return LAUNCH_CHOICES.find((c) => c === value);
}

export function defaultChoice(ios: boolean): LaunchChoice {
  return ios ? "ask" : "home";
}

/** The hash to open at launch, or undefined to leave the URL alone. */
export function launchHash(env: LaunchEnv): string | undefined {
  if (!env.standalone || env.handled) return undefined;
  if (env.navigationType !== undefined && env.navigationType !== "navigate") return undefined;
  if (!UNROUTED.has(env.hash)) return undefined;
  const choice = parseChoice(env.saved) ?? defaultChoice(env.ios);
  return choice === "home" ? undefined : `#/${choice}`;
}

const iosNavigator = () => navigator as Navigator & { standalone?: boolean };

function iosStandalone(): boolean {
  return iosNavigator().standalone === true;
}

/** iPhone or iPad (Safari exposes `navigator.standalone` there; Mac has no touch points). */
function iosDevice(): boolean {
  return iosNavigator().standalone !== undefined && navigator.maxTouchPoints > 0;
}

function storage(kind: "localStorage" | "sessionStorage"): Storage | undefined {
  try {
    return window[kind];
  } catch {
    return undefined; // blocked storage: fall back to the default choice
  }
}

function read(store: Storage | undefined, key: string): string | null {
  try {
    return store?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

/** This device's choice (or its default). */
export function readLaunchChoice(): LaunchChoice {
  return parseChoice(read(storage("localStorage"), LAUNCH_KEY)) ?? defaultChoice(iosStandalone() || iosDevice());
}

export function writeLaunchChoice(choice: LaunchChoice) {
  try {
    storage("localStorage")?.setItem(LAUNCH_KEY, choice);
  } catch {
    /* private mode: the default applies */
  }
}

/** Run once, before the app renders: open the device's launch module in an installed app. */
export function applyLaunchRoute() {
  const ios = iosStandalone();
  const standalone = ios || (typeof matchMedia === "function" && matchMedia("(display-mode: standalone)").matches);
  if (!standalone) return;
  const session = storage("sessionStorage");
  const nav = performance.getEntriesByType?.("navigation")[0] as PerformanceNavigationTiming | undefined;
  const target = launchHash({
    hash: location.hash,
    standalone,
    ios,
    navigationType: nav?.type,
    handled: read(session, HANDLED_KEY) === "1",
    saved: read(storage("localStorage"), LAUNCH_KEY),
  });
  try {
    session?.setItem(HANDLED_KEY, "1");
  } catch {
    /* without sessionStorage, the reload check above still applies */
  }
  // replaceState: no hashchange, no extra history entry (Back doesn't return to the dashboard).
  if (target) history.replaceState(history.state, "", target);
}
