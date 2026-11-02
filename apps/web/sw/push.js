/* Pure Web Push helpers for the service worker (D-049). The build inlines
 * this file ahead of service-worker.js with `export` removed; Vitest imports
 * it directly (src/lib/swPush.test.ts). No DOM or worker globals here. */

/** Snooze from a notification button, in minutes. */
export const NOTIFICATION_SNOOZE_MINUTES = 10;

/** Title and options for `showNotification` from a claimed delivery. */
export function notificationFor(shown) {
  const actions = shown.reminder_id
    ? [
        { action: "complete", title: "Complete" },
        { action: "snooze", title: `Snooze ${NOTIFICATION_SNOOZE_MINUTES} min` },
      ]
    : [];
  return {
    title: shown.title || "Reminder",
    options: {
      body: shown.body,
      tag: shown.tag,
      renotify: true,
      requireInteraction: false,
      icon: "/icons/icon-192.png",
      badge: "/icons/icon-192.png",
      data: {
        delivery_id: shown.delivery_id,
        reminder_id: shown.reminder_id,
        occurrence: shown.occurrence,
        url: shown.url,
      },
      actions,
    },
  };
}

/** Shown when a push arrives but nothing could be claimed (offline, signed
 * out, already handled elsewhere). Browsers require showing something. */
export function genericNotification() {
  return {
    title: "Directory",
    options: {
      body: "You have a reminder. Open Directory to see it.",
      tag: "directory-generic",
      icon: "/icons/icon-192.png",
      data: { url: "/#/reminders" },
    },
  };
}

/** The API call a notification action makes, or null for "open". */
export function actionRequest(action, data) {
  if (!data || !data.reminder_id) return null;
  const base = `/api/v1/reminders/${encodeURIComponent(data.reminder_id)}`;
  if (action === "complete") {
    return { path: `${base}/complete`, body: { occurrence: data.occurrence }, ack: "completed" };
  }
  if (action === "snooze") {
    return {
      path: `${base}/snooze`,
      body: { minutes: NOTIFICATION_SNOOZE_MINUTES, occurrence: data.occurrence },
      ack: "snoozed",
    };
  }
  return null;
}

/** Same-origin in-app path to open (anything else falls back to Reminders). */
export function openPath(data) {
  const url = data && typeof data.url === "string" ? data.url : "";
  return url.startsWith("/") && !url.startsWith("//") ? url : "/#/reminders";
}
