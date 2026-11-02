import { useCallback, useEffect, useState } from "react";
import { reportIfUnauthorized } from "../hooks/useSession";
import { api, ApiError, isNetworkError } from "../lib/api";
import { formatDate, formatTime } from "../lib/format";
import {
  currentEndpoint,
  currentSupport,
  noticeSummary,
  subscribe,
  unsubscribe,
  type Device,
  type HistoryItem,
  type PushConfig,
} from "../lib/notifications";
import { deviceLabel } from "./AuthScreen";
import { Button, Panel, StatusChip } from "./ui";

// Provisional (D-013): Settings panel in the existing style.

const small = "min-h-8 px-3 text-xs";

function describe(e: unknown): string {
  if (e instanceof ApiError) {
    if (e.code === "busy") return "A test was just sent. Wait half a minute and try again.";
    if (e.code === "too_many") return "Too many devices. Remove one first.";
    return `Request failed (${e.code}).`;
  }
  if (isNetworkError(e)) return "Can't reach the server right now.";
  return e instanceof Error ? e.message : "Something went wrong.";
}

function permission(): NotificationPermission | "unavailable" {
  return typeof Notification === "undefined" ? "unavailable" : Notification.permission;
}

/** Reminder alerts by Web Push (D-049): this device, all devices, a test, and history. */
export function NotificationSettings({ online }: { online: boolean }) {
  const [config, setConfig] = useState<PushConfig>();
  const [devices, setDevices] = useState<Device[]>();
  const [history, setHistory] = useState<HistoryItem[]>();
  const [endpoint, setEndpoint] = useState<string | null>(null);
  const [error, setError] = useState<string>();
  const [message, setMessage] = useState<string>();
  const [busy, setBusy] = useState(false);
  const support = currentSupport();

  const load = useCallback(async () => {
    if (!online) return;
    try {
      const [c, d, h, e] = await Promise.all([api.pushConfig(), api.pushDevices(), api.notificationHistory(20), currentEndpoint()]);
      setConfig(c);
      setDevices(d);
      setHistory(h);
      setEndpoint(e);
      setError(undefined);
    } catch (e) {
      reportIfUnauthorized(e);
      setError(describe(e));
    }
  }, [online]);

  useEffect(() => void load(), [load]);

  // The device list has no endpoints, so this browser finds its row by renewing its
  // registration (idempotent; it also moves the device to this account).
  const [thisDevice, setThisDevice] = useState<Device>();
  useEffect(() => {
    if (!online || !endpoint || !config?.enabled || permission() !== "granted") {
      setThisDevice(undefined);
      return;
    }
    let live = true;
    api
      .registerPush(endpoint, deviceLabel())
      .then((d) => {
        if (live) setThisDevice(d);
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [online, endpoint, config?.enabled]);

  const run = async (what: () => Promise<string | undefined>) => {
    setBusy(true);
    setMessage(undefined);
    setError(undefined);
    try {
      setMessage(await what());
      await load();
    } catch (e) {
      reportIfUnauthorized(e);
      setError(describe(e));
    } finally {
      setBusy(false);
    }
  };

  const enable = () =>
    run(async () => {
      if (!config?.public_key) throw new Error("Push isn't set up on the server.");
      const ep = await subscribe(config.public_key);
      await api.registerPush(ep, deviceLabel());
      return "Notifications are on for this device.";
    });

  const disable = () =>
    run(async () => {
      const mine = devices?.find((d) => d.id === thisDevice?.id);
      await unsubscribe();
      if (mine) await api.removePushDevice(mine.id);
      return "Notifications are off for this device.";
    });

  const remove = (d: Device) =>
    run(async () => {
      await api.removePushDevice(d.id);
      return `Removed ${d.label}.`;
    });

  const test = () =>
    run(async () => {
      await api.testPush();
      return devices && devices.length > 0
        ? "Test sent. If no notification appears within a minute, check the history below."
        : "Test logged, but no device has notifications on.";
    });

  const on = Boolean(thisDevice);
  const perm = permission();

  return (
    <Panel
      title="notifications"
      className="lg:col-span-2"
      actions={
        config && (
          <StatusChip
            tone={!config.enabled ? "idle" : on ? "ok" : "warn"}
            label="push"
            value={!config.enabled ? "not set up" : on ? "on here" : "off here"}
          />
        )
      }
    >
      <p className="text-sm text-muted">
        Reminder alerts arrive by Web Push on each device you turn on. Push is best-effort: the browser's push service may delay or drop
        an alert, so due reminders also show on Home and in Reminders.
      </p>
      {!online && <p className="mt-3 text-sm text-muted">Offline. Notification settings need a connection.</p>}
      {online && !config && !error && <p className="cursor mt-3 font-mono text-sm text-muted">loading</p>}
      {error && (
        <p role="alert" className="mt-3 text-sm text-bad">
          {error}
        </p>
      )}
      {message && (
        <p role="status" className="mt-3 text-sm">
          {message}
        </p>
      )}

      {config && !config.enabled && (
        <p className="mt-3 text-sm">Push isn't set up on this server yet (the owner sets a VAPID key; see docs/NOTIFICATIONS.md).</p>
      )}

      {config?.enabled && (
        <div className="mt-3 flex flex-wrap items-center gap-2">
          {support === "ios-browser" && (
            <p className="text-sm">On iPhone and iPad, add Directory to the Home Screen and open it from there to turn on notifications.</p>
          )}
          {support === "unsupported" && <p className="text-sm">This browser doesn't support Web Push.</p>}
          {support === "supported" && perm === "denied" && (
            <p className="text-sm">Notifications are blocked for this site. Allow them in the browser's site settings, then reload.</p>
          )}
          {support === "supported" && perm !== "denied" && !on && (
            <Button variant="primary" disabled={busy || !online} onClick={() => void enable()}>
              turn on for this device
            </Button>
          )}
          {support === "supported" && on && (
            <Button disabled={busy || !online} onClick={() => void disable()}>
              turn off for this device
            </Button>
          )}
          <Button disabled={busy || !online} onClick={() => void test()}>
            send test
          </Button>
        </div>
      )}

      {devices && (
        <>
          <h3 className="label mt-5">devices · {devices.length}</h3>
          {devices.length === 0 ? (
            <p className="mt-2 text-sm text-muted">No device has notifications on.</p>
          ) : (
            <ul className="mt-2 divide-y divide-line">
              {devices.map((d) => (
                <li key={d.id} className="flex items-center gap-3 py-2">
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm">
                      {d.label}
                      {thisDevice?.id === d.id ? " (this device)" : ""}
                    </span>
                    <span className="font-mono text-[11px] text-muted">
                      added {formatDate(d.created_at)}
                      {d.last_ok_at ? ` · last accepted ${formatDate(d.last_ok_at)}` : ""}
                    </span>
                    {d.last_error && <span className="block text-xs text-warn">{d.last_error}</span>}
                  </span>
                  <Button variant="danger" className={small} disabled={busy || !online} onClick={() => void remove(d)}>
                    remove
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </>
      )}

      {history && (
        <>
          <h3 className="label mt-5">recent alerts</h3>
          {history.length === 0 ? (
            <p className="mt-2 text-sm text-muted">No alerts yet.</p>
          ) : (
            <ul className="mt-2 divide-y divide-line">
              {history.map((h) => (
                <li key={h.id} className="py-2">
                  <span className="block truncate text-sm">{h.reminder_id ? (h.title ?? "Deleted reminder") : "Test notification"}</span>
                  <span className="font-mono text-[11px] text-muted">
                    {formatDate(h.due_at)} {formatTime(h.due_at)} · {noticeSummary(h)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </Panel>
  );
}
