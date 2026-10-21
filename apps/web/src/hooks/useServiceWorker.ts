import { useEffect, useState } from "react";

export type SwState =
  | { kind: "unsupported" }
  | { kind: "disabled" } // dev server: no service worker
  | { kind: "installing" }
  | { kind: "ready" } // app shell cached; works offline
  | { kind: "update"; apply: () => void }
  | { kind: "error"; message: string };

export function useServiceWorker(): SwState {
  const [state, setState] = useState<SwState>(() =>
    !("serviceWorker" in navigator)
      ? { kind: "unsupported" }
      : import.meta.env.DEV
        ? { kind: "disabled" }
        : { kind: "installing" },
  );

  useEffect(() => {
    if (!("serviceWorker" in navigator) || import.meta.env.DEV) return;
    let cancelled = false;
    const sw = navigator.serviceWorker;

    const offerUpdate = (waiting: ServiceWorker) =>
      setState({ kind: "update", apply: () => waiting.postMessage("SKIP_WAITING") });

    // Reload once the new worker takes control so the page matches its cache.
    let reloading = false;
    const onControllerChange = () => {
      if (reloading) return;
      reloading = true;
      window.location.reload();
    };

    sw.register("/sw.js")
      .then((reg) => {
        if (cancelled) return;
        if (reg.waiting && sw.controller) offerUpdate(reg.waiting);
        reg.addEventListener("updatefound", () => {
          const next = reg.installing;
          next?.addEventListener("statechange", () => {
            if (next.state !== "installed") return;
            if (sw.controller) {
              sw.addEventListener("controllerchange", onControllerChange);
              offerUpdate(next);
            } else {
              setState({ kind: "ready" });
            }
          });
        });
        return sw.ready.then(() => {
          if (!cancelled && !reg.waiting) setState({ kind: "ready" });
        });
      })
      .catch((e: unknown) => {
        if (!cancelled) setState({ kind: "error", message: e instanceof Error ? e.message : String(e) });
      });

    return () => {
      cancelled = true;
      sw.removeEventListener("controllerchange", onControllerChange);
    };
  }, []);

  return state;
}
