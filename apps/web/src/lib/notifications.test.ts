import { describe, expect, it } from "vitest";
import { deliverySummary, keyBytes, noticeSummary, pushSupport, type Delivery, type HistoryItem } from "./notifications";

const d = (state: Delivery["state"]): Delivery => ({
  id: state,
  notice_id: "n",
  subscription_id: "s",
  state,
  attempts: 1,
  next_try: null,
  status: null,
  created_at: 0,
  updated_at: 0,
});

const item = (state: HistoryItem["state"], deliveries: Delivery[] = []): HistoryItem => ({
  id: "n",
  reminder_id: "r",
  occurrence: "2026-11-02",
  slot: "b0",
  due_at: 0,
  state,
  created_at: 0,
  title: "Pay rent",
  open: true,
  deliveries,
});

describe("delivery wording", () => {
  it("never calls a push-service acceptance a delivery", () => {
    const s = deliverySummary([d("accepted"), d("queued")]);
    expect(s).toContain("Accepted by the push service for 1 of 2 devices");
    expect(s).toContain("not confirmed shown");
  });

  it("reports shown, actions, retries, and failures", () => {
    expect(deliverySummary([d("shown"), d("accepted")])).toBe("Shown on 1 of 2 devices");
    expect(deliverySummary([d("completed")])).toBe("Completed from a notification");
    expect(deliverySummary([d("retry")])).toBe("Sending (1 of 1 device)");
    expect(deliverySummary([d("cancelled")])).toContain("Cancelled");
    expect(deliverySummary([d("expired")])).toContain("expired");
    expect(deliverySummary([d("failed")])).toContain("refused");
    expect(deliverySummary([])).toBe("Sent to no devices");
  });

  it("explains notices that were never sent", () => {
    expect(noticeSummary(item("no_devices"))).toContain("no device");
    expect(noticeSummary(item("not_configured"))).toContain("isn't set up");
    expect(noticeSummary(item("resolved"))).toContain("already done");
    expect(noticeSummary(item("stale"))).toContain("12 hours");
    expect(noticeSummary(item("dispatched", [d("shown")]))).toBe("Shown on 1 of 1 device");
  });
});

describe("push support", () => {
  const base = { hasServiceWorker: true, hasPushManager: true, hasNotification: true, standalone: false };
  it("needs the Home Screen app on iPhone", () => {
    const ua = "Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 Version/18.0 Mobile/15E148 Safari/604.1";
    expect(pushSupport({ ...base, userAgent: ua })).toBe("ios-browser");
    expect(pushSupport({ ...base, userAgent: ua, standalone: true })).toBe("supported");
  });
  it("needs the push APIs elsewhere", () => {
    const ua = "Mozilla/5.0 (Windows NT 10.0) Chrome/140.0 Safari/537.36";
    expect(pushSupport({ ...base, userAgent: ua })).toBe("supported");
    expect(pushSupport({ ...base, userAgent: ua, hasPushManager: false })).toBe("unsupported");
  });
});

describe("keyBytes", () => {
  it("decodes base64url without padding", () => {
    expect(Array.from(keyBytes("BAEC_-8"))).toEqual([4, 1, 2, 255, 239]);
  });
});
