import { api } from "../lib/api";
import { Throttle, describeThrown } from "../lib/errors";

const throttle = new Throttle();

/** Send an error to the owner's diagnostics log. Best effort; never throws. */
export function reportError(e: unknown, module: string): void {
  const { message, stack } = describeThrown(e);
  if (!navigator.onLine || !throttle.allow(`${module}:${message}`)) return;
  api
    .reportError({ message, module, stack: stack?.slice(0, 4000), app_version: import.meta.env?.MODE ?? "" })
    .catch(() => {
      /* signed out, offline, or the server is down: nothing more to do */
    });
}

let installed = false;

/** Report uncaught errors and unhandled promise rejections (once per page). */
export function installGlobalErrorReporting(currentModule: () => string): void {
  if (installed) return;
  installed = true;
  window.addEventListener("error", (ev) => reportError(ev.error ?? ev.message, currentModule()));
  window.addEventListener("unhandledrejection", (ev) => reportError(ev.reason, currentModule()));
}
