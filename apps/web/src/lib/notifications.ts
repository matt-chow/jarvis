/**
 * Reminder notifications (D-049): Web Push device registration, plus the
 * honest wording of delivery states. A push service's "accepted" is not
 * "delivered"; only the device's claim ("shown") says it reached the screen.
 */

export interface PushConfig {
  enabled: boolean;
  public_key: string | null;
}

export interface Device {
  id: string;
  label: string;
  created_at: number;
  updated_at: number;
  last_ok_at: number | null;
  last_error: string | null;
  failures: number;
}

export type DeliveryState =
  | "queued"
  | "retry"
  | "accepted"
  | "shown"
  | "opened"
  | "completed"
  | "snoozed"
  | "dismissed"
  | "cancelled"
  | "failed"
  | "expired";

export interface Delivery {
  id: string;
  notice_id: string;
  subscription_id: string;
  state: DeliveryState;
  attempts: number;
  next_try: number | null;
  status: number | null;
  created_at: number;
  updated_at: number;
}

export type NoticeState = "pending" | "dispatched" | "no_devices" | "not_configured" | "resolved" | "superseded" | "stale";

export interface HistoryItem {
  id: string;
  reminder_id: string;
  occurrence: string;
  slot: string;
  due_at: number;
  state: NoticeState;
  created_at: number;
  title: string | null;
  open: boolean;
  deliveries: Delivery[];
}

/** One line for a notice in the history list. */
export function noticeSummary(item: HistoryItem): string {
  switch (item.state) {
    case "pending":
      return "Waiting to send";
    case "no_devices":
      return "Not sent: no device has notifications on";
    case "not_configured":
      return "Not sent: push isn't set up on the server";
    case "resolved":
      return "Not sent: already done or snoozed";
    case "superseded":
      return "Skipped: a later alert replaced it";
    case "stale":
      return "Not sent: more than 12 hours late";
    case "dispatched":
      return deliverySummary(item.deliveries);
  }
}

const SEEN: DeliveryState[] = ["shown", "opened", "completed", "snoozed", "dismissed"];

/** The best state across devices, worded without overclaiming. */
export function deliverySummary(ds: Delivery[]): string {
  if (ds.length === 0) return "Sent to no devices";
  const n = (s: DeliveryState[]) => ds.filter((d) => s.includes(d.state)).length;
  const of = ` of ${ds.length} device${ds.length === 1 ? "" : "s"}`;
  if (n(["completed"])) return `Completed from a notification`;
  if (n(["snoozed"])) return `Snoozed from a notification`;
  if (n(SEEN)) return `Shown on ${n(SEEN)}${of}`;
  if (n(["accepted"])) return `Accepted by the push service for ${n(["accepted"])}${of} (not confirmed shown)`;
  if (n(["queued", "retry"])) return `Sending (${n(["queued", "retry"])}${of})`;
  if (n(["cancelled"])) return "Cancelled: resolved before it was shown";
  if (n(["expired"])) return "Not delivered: the device subscription expired";
  return "Not delivered: the push service refused it";
}

export type PushSupport = "supported" | "unsupported" | "ios-browser";

/**
 * Web Push needs a service worker, PushManager, and Notification. On iPhone
 * and iPad it works only from the Home Screen app (iOS 16.4+).
 */
export function pushSupport(env: {
  hasServiceWorker: boolean;
  hasPushManager: boolean;
  hasNotification: boolean;
  userAgent: string;
  standalone: boolean;
}): PushSupport {
  const ios = /iPad|iPhone|iPod/.test(env.userAgent) || (/Macintosh/.test(env.userAgent) && /Mobile/.test(env.userAgent));
  if (ios && !env.standalone) return "ios-browser";
  return env.hasServiceWorker && env.hasPushManager && env.hasNotification ? "supported" : "unsupported";
}

export function currentSupport(): PushSupport {
  if (typeof window === "undefined") return "unsupported";
  const nav = navigator as Navigator & { standalone?: boolean };
  return pushSupport({
    hasServiceWorker: "serviceWorker" in navigator,
    hasPushManager: "PushManager" in window,
    hasNotification: "Notification" in window,
    userAgent: navigator.userAgent,
    standalone: nav.standalone === true || window.matchMedia?.("(display-mode: standalone)").matches === true,
  });
}

/** base64url → bytes, for `applicationServerKey`. */
export function keyBytes(b64url: string): Uint8Array<ArrayBuffer> {
  const b64 = b64url.replace(/-/g, "+").replace(/_/g, "/").padEnd(Math.ceil(b64url.length / 4) * 4, "=");
  const bin = atob(b64);
  const out = new Uint8Array(new ArrayBuffer(bin.length));
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

async function registration(): Promise<ServiceWorkerRegistration> {
  const reg = await navigator.serviceWorker.getRegistration();
  if (!reg) throw new Error("The app isn't installed for offline use yet; reload and try again.");
  return reg;
}

/** This browser's current push endpoint, if subscribed. */
export async function currentEndpoint(): Promise<string | null> {
  if (currentSupport() !== "supported") return null;
  const reg = await navigator.serviceWorker.getRegistration();
  const sub = await reg?.pushManager.getSubscription();
  return sub?.endpoint ?? null;
}

/** Ask permission and subscribe; returns the endpoint to register. */
export async function subscribe(publicKey: string): Promise<string> {
  const permission = await Notification.requestPermission();
  if (permission !== "granted") {
    throw new Error(permission === "denied" ? "Notifications are blocked for this site in the browser settings." : "Permission wasn't granted.");
  }
  const reg = await registration();
  const existing = await reg.pushManager.getSubscription();
  const key = keyBytes(publicKey);
  // A subscription made with another server key can't receive our pushes.
  if (existing) {
    const current = existing.options.applicationServerKey;
    const same = current && new Uint8Array(current).every((b, i) => b === key[i]) && current.byteLength === key.length;
    if (same) return existing.endpoint;
    await existing.unsubscribe();
  }
  const sub = await reg.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey: key });
  return sub.endpoint;
}

/** Unsubscribe this browser (the server row is removed separately). */
export async function unsubscribe(): Promise<void> {
  const reg = await navigator.serviceWorker.getRegistration();
  const sub = await reg?.pushManager.getSubscription();
  await sub?.unsubscribe();
}
