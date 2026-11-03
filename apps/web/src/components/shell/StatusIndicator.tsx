import { useEffect, useId, useRef, useState } from "react";
import type { HealthState } from "../../hooks/useHealth";
import type { SwState } from "../../hooks/useServiceWorker";
import type { SyncState } from "../../hooks/useSync";
import { Icon } from "../icons";
import type { Tone } from "../ui";

export interface StatusSummary {
  tone: Tone;
  /** Short text on the button: "Synced", "3 pending", "Offline". */
  text: string;
}

/** One line for the header, most urgent first (D-050). */
export function summarize(online: boolean, health: HealthState, sync: Pick<SyncState, "pending" | "syncing">): StatusSummary {
  if (sync.syncing) return { tone: "accent", text: "Syncing…" };
  if (!online) return { tone: "warn", text: sync.pending ? `Offline · ${sync.pending} pending` : "Offline" };
  if (sync.pending) return { tone: "warn", text: `${sync.pending} pending` };
  if (health.error && !health.stale) return { tone: "bad", text: "Server unreachable" };
  return { tone: "ok", text: "Synced" };
}

const dot: Record<Tone, string> = { ok: "bg-ok", warn: "bg-warn", bad: "bg-bad", idle: "bg-muted", accent: "bg-accent" };

function apiText(online: boolean, health: HealthState): [Tone, string] {
  if (health.loading && !health.report) return ["idle", "Checking…"];
  if (!online || (health.error && health.stale)) return ["idle", "Using saved data"];
  if (health.error) return ["bad", "Not responding"];
  return health.report?.status === "ok" ? ["ok", "Working"] : ["warn", "Degraded"];
}

function swText(sw: SwState): [Tone, string] {
  switch (sw.kind) {
    case "ready":
      return ["ok", "Ready: opens without a connection"];
    case "update":
      return ["accent", "Update ready"];
    case "installing":
      return ["warn", "Saving the app for offline use…"];
    case "disabled":
      return ["idle", "Off in development"];
    case "unsupported":
      return ["idle", "Not supported in this browser"];
    case "error":
      return ["bad", "Couldn't save the app for offline use"];
  }
}

/**
 * The always-visible sync indicator: a small dot and a word in the header,
 * with the detail (connection, server, offline copy, queued changes) on demand.
 */
export function StatusIndicator({ online, health, sw, sync }: { online: boolean; health: HealthState; sw: SwState; sync: SyncState }) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const id = useId();
  const s = summarize(online, health, sync);
  const [apiTone, api] = apiText(online, health);
  const [swTone, swLine] = swText(sw);

  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    const esc = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setOpen(false);
        box.current?.querySelector("button")?.focus();
      }
    };
    document.addEventListener("pointerdown", away);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("pointerdown", away);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);

  const rows: [string, Tone, string][] = [
    ["Connection", online ? "ok" : "warn", online ? "Online" : "Offline: changes are saved on this device"],
    ["Server", apiTone, api],
    ["Offline copy", swTone, swLine],
    ["Waiting to sync", sync.pending ? "warn" : "ok", sync.pending ? `${sync.pending} change${sync.pending === 1 ? "" : "s"}` : "Nothing"],
  ];

  return (
    <div ref={box} className="relative">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={id}
        onClick={() => setOpen((o) => !o)}
        title="Sync and offline status"
        className="inline-flex min-h-9 items-center gap-2 rounded-full px-3 text-[13px] text-muted transition-colors hover:bg-sunken hover:text-ink"
      >
        <span aria-hidden className={`size-2 rounded-full ${dot[s.tone]} ${s.tone === "accent" ? "animate-pulse motion-reduce:animate-none" : ""}`} />
        <span role="status" aria-live="polite">
          {s.text}
        </span>
      </button>
      {open && (
        <div
          id={id}
          role="dialog"
          aria-label="Sync and offline status"
          className="pop absolute top-full right-0 z-40 mt-2 w-[min(20rem,calc(100vw-2rem))] origin-top-right rounded-md border border-line bg-panel p-4 shadow-lift"
        >
          <dl className="flex flex-col gap-3 text-sm">
            {rows.map(([k, tone, v]) => (
              <div key={k} className="flex gap-3">
                <span aria-hidden className={`mt-1.5 size-2 shrink-0 rounded-full ${dot[tone]}`} />
                <div className="min-w-0">
                  <dt className="text-muted">{k}</dt>
                  <dd>{v}</dd>
                </div>
              </div>
            ))}
          </dl>
          {sw.kind === "update" && (
            <button type="button" onClick={sw.apply} className="mt-4 inline-flex min-h-9 items-center gap-2 text-sm font-medium text-accent underline underline-offset-4">
              <Icon name="sync" size={16} /> Reload to update
            </button>
          )}
          <p className="mt-4 border-t border-line pt-3 text-xs text-muted">Changes made offline are kept on this device and sent when you reconnect.</p>
        </div>
      )}
    </div>
  );
}
