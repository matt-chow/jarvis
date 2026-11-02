import { describe, expect, it } from "vitest";
import { actionRequest, genericNotification, notificationFor, openPath, type Shown } from "../../sw/push.js";

const shown: Shown = {
  delivery_id: "d1",
  notice_id: "n1",
  reminder_id: "r1",
  occurrence: "2026-11-02",
  title: "Pay rent",
  body: "Due 9:00 AM",
  url: "/#/reminders?reminder=r1",
  tag: "r-r1-2026-11-02",
};

describe("service worker push helpers", () => {
  it("builds a reminder notification with complete and snooze actions", () => {
    const n = notificationFor(shown);
    expect(n.title).toBe("Pay rent");
    expect(n.options.tag).toBe("r-r1-2026-11-02");
    expect(n.options.actions?.map((a) => a.action)).toEqual(["complete", "snooze"]);
    expect(n.options.data).toEqual({ delivery_id: "d1", reminder_id: "r1", occurrence: "2026-11-02", url: shown.url });
  });

  it("has no actions on a test notification", () => {
    expect(notificationFor({ ...shown, reminder_id: null, occurrence: null }).options.actions).toEqual([]);
  });

  it("maps actions to the occurrence-specific reminder routes", () => {
    const data = notificationFor(shown).options.data;
    expect(actionRequest("complete", data)).toEqual({
      path: "/api/v1/reminders/r1/complete",
      body: { occurrence: "2026-11-02" },
      ack: "completed",
    });
    expect(actionRequest("snooze", data)).toEqual({
      path: "/api/v1/reminders/r1/snooze",
      body: { minutes: 10, occurrence: "2026-11-02" },
      ack: "snoozed",
    });
    expect(actionRequest("", data)).toBeNull();
    expect(actionRequest("complete", { url: "/" })).toBeNull();
  });

  it("opens only same-origin paths", () => {
    expect(openPath({ url: "/#/reminders?reminder=r1" })).toBe("/#/reminders?reminder=r1");
    expect(openPath({ url: "https://evil.example/" })).toBe("/#/reminders");
    expect(openPath({ url: "//evil.example/" })).toBe("/#/reminders");
    expect(openPath(genericNotification().options.data)).toBe("/#/reminders");
  });
});
