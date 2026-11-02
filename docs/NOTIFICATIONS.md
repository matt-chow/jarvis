# Reminder notifications (Web Push)

How reminder alerts reach your devices, how to set them up, and how to check
them by hand. Decisions: DECISIONS D-049 › Notifications.

**Web Push is best-effort.** The browser's push service (Apple, Google,
Mozilla, Microsoft) can delay or drop a message, and a phone in low-power
mode may hold it. A due reminder is always visible on Home and in Reminders,
whether or not its notification arrived.

## How it works

1. Saving a reminder writes its upcoming alerts as **notices** (one per
   reminder, occurrence, and alert slot) in the same D1 batch. A unique key
   means a replayed save can't create a second notice.
2. A **cron trigger** (`* * * * *`, Workers Free) runs the scheduled handler
   about once a minute. It never assumes second-level precision: anything due
   since the last run is sent on this run. It:
   - advances reminders whose occurrence has passed (missed policies);
   - checks each due notice against the reminder as it is *now*: completed,
     snoozed past, deleted, or moved on → `resolved`; more than 12 hours late
     → `stale`; an older alert of the same reminder that came due with a newer
     one → `superseded`; no VAPID key → `not_configured`; no devices →
     `no_devices`; otherwise one **delivery** per subscribed device;
   - sends due deliveries and prunes history older than 30 days (hourly).
3. **Payload-free push.** The push message carries no reminder text. The
   service worker wakes, calls `POST /api/v1/notifications/claim` with its
   endpoint and the session cookie, and shows what the server returns. The
   body shows only the time; the title is the reminder's title.
4. Notification buttons: **Complete** and **Snooze 10 min** call the
   reminder routes for that exact occurrence; tapping the notification opens
   the reminder. Anything that fails opens the app instead, where every
   snooze option (10 min, 1 hour, tomorrow, custom) is available.

### What each state means (Settings › notifications › recent alerts)

| Delivery | Meaning |
| --- | --- |
| queued / retry | Waiting to send, or retrying after 429, 5xx, or a network error (1, 2, 4, 8 min; 5 tries) |
| accepted | The push service took it (2xx). **Not** proof it was shown |
| shown | The device's service worker claimed it, so it reached the device |
| opened / completed / snoozed / dismissed | What you did with it |
| expired | The push service said the subscription is gone (404/410); the device is removed |
| failed | Refused (for example 401/403: the server key changed) |
| cancelled | Resolved before the device showed it |

The app never says "delivered" because of an accepted push.

### Security and privacy

- Every route authenticates and is scoped to the signed-in user. A claim for
  an endpoint that isn't yours returns an empty list (it doesn't reveal
  whose it is).
- Endpoints are accepted only for known push services, over HTTPS on the
  default port, at most 1024 characters, so the Worker can't be made to
  POST elsewhere.
- Endpoints are never returned to the browser; the device list shows a label.
- No reminder text is sent through the push service, and logs carry counts
  and error kinds only.
- The VAPID private key is a Worker secret. The public key is derived from it
  and served by `GET /api/v1/notifications/config`.

## Set up (owner)

1. Generate a key pair (nothing is written to disk):

   ```powershell
   node scripts/vapid-keys.mjs
   ```

2. Store the private key as a secret (paste the value after
   `VAPID_PRIVATE_KEY=` when prompted):

   ```powershell
   npx wrangler secret put VAPID_PRIVATE_KEY --env staging
   ```

3. Optional: set `VAPID_SUBJECT` (a `mailto:` or `https://` contact) under
   `[env.staging.vars]`. Without it, the first `https://` origin in
   `DIRECTORY_ORIGINS` is used.
4. Deploy (migration 0027 and the cron trigger come with it; see
   docs/STAGING.md › 5 and 6).

Changing the key makes every existing subscription fail with 401/403. Each
device must then turn notifications off and on again in Settings.

Locally, put the key in `.dev.vars` (see `.dev.vars.example`). `npm run
smoke` runs its own mock push service and key.

## Turn on a device

- **Computer (Chrome, Edge, Firefox, Safari 16+):** Settings ›
  notifications › *turn on for this device*, then allow notifications.
- **iPhone / iPad (iOS 16.4+):** open the site in Safari, Share › *Add to
  Home Screen*, open Directory **from the Home Screen icon**, sign in, then
  Settings › notifications › *turn on for this device* and allow. In Safari
  itself the panel explains this, because iOS offers Web Push only to Home
  Screen apps.

Each device is listed with when it was added and when a push was last
accepted. *remove* stops alerts to that device.

## Manual verification (pending)

Core unit tests (retries, backoff, states) and `npm run smoke` with a mock push
service (VAPID headers, payload-free sends, expiry, claim-once,
the cron path, no duplicate pushes) cover the server. **Live delivery to real devices is
pending manual verification.** After deploying with a key:

1. On a computer, turn on notifications and press *send test*. A "Directory test notification"
   should appear within seconds, and recent alerts
   should change from "Accepted…" to "Shown on 1 of 1 device".
2. Create a reminder due in 3 minutes. Within about a minute of its time,
   check that the notification shows its title and time, then press
   **Complete**. The reminder should be completed, and history should say
   "Completed from a notification".
3. Repeat with **Snooze**: it should come back about 10 minutes later.
4. iPhone: steps 1–3 from the Home Screen app, with the screen locked.
5. Turn off for a device, then send a test: that device should get nothing.

## Limits

- Timing is within about a minute (cron) plus the push service's delay. A
  notice more than 12 hours late is not sent.
- Workers Free allows 10 ms CPU per invocation. Each cron run handles at most
  50 reminders, 50 notices, and 25 sends; anything left over goes next
  minute.
- Offline: the app can't subscribe or test, and says so. Alerts already
  delivered still work, and their buttons fall back to opening the app.
- Up to 20 devices per account.
