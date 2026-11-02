/* Directory service worker (template). The build emits /sw.js from this file
 * with the placeholders below replaced; see vite.config.ts. */

const CACHE_VERSION = "__CACHE_VERSION__";
const PRECACHE = __PRECACHE_MANIFEST__;
const SHELL_PREFIX = "directory-shell-";
const SHELL_CACHE = SHELL_PREFIX + CACHE_VERSION;

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(SHELL_CACHE).then((cache) => cache.addAll(PRECACHE)));
});

// The page asks the waiting worker to take over when the user accepts an update.
self.addEventListener("message", (event) => {
  if (event.data === "SKIP_WAITING") self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      for (const key of await caches.keys()) {
        if (key.startsWith(SHELL_PREFIX) && key !== SHELL_CACHE) await caches.delete(key);
      }
      await self.clients.claim();
    })(),
  );
});

self.addEventListener("fetch", (event) => {
  const request = event.request;
  if (request.method !== "GET") return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;
  // API responses may contain private data: never cache them here.
  if (url.pathname.startsWith("/api/")) return;

  if (request.mode === "navigate") {
    // Network first so deploys show up immediately; fall back to the shell.
    event.respondWith(
      fetch(request).catch(async () => (await caches.match("/", { cacheName: SHELL_CACHE })) ?? Response.error()),
    );
    return;
  }

  // Hashed build assets are immutable: cache first.
  event.respondWith(caches.match(request, { cacheName: SHELL_CACHE }).then((hit) => hit ?? fetch(request)));
});

/* Web Push (D-049). Pushes carry no payload: ask the API what to show, with
 * this browser's session. The claim is what records "shown". Helpers come
 * from sw/push.js, inlined above by the build. */

async function claimShown() {
  const sub = await self.registration.pushManager.getSubscription();
  if (!sub) return [];
  const resp = await fetch("/api/v1/notifications/claim", {
    method: "POST",
    credentials: "same-origin",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ endpoint: sub.endpoint }),
  });
  if (!resp.ok) return [];
  const list = await resp.json();
  return Array.isArray(list) ? list : [];
}

self.addEventListener("push", (event) => {
  event.waitUntil(
    (async () => {
      let shown = [];
      try {
        shown = await claimShown();
      } catch {
        shown = [];
      }
      if (shown.length === 0) {
        // Browsers require a visible notification per push. Skip it if one is already up.
        if ((await self.registration.getNotifications()).length > 0) return;
        const g = genericNotification();
        await self.registration.showNotification(g.title, g.options);
        return;
      }
      for (const s of shown) {
        const n = notificationFor(s);
        await self.registration.showNotification(n.title, n.options);
      }
    })(),
  );
});

async function ack(deliveryId, action) {
  if (!deliveryId) return;
  try {
    await fetch(`/api/v1/notifications/deliveries/${encodeURIComponent(deliveryId)}/ack`, {
      method: "POST",
      credentials: "same-origin",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ action }),
    });
  } catch {
    // History only; the reminder itself is already up to date.
  }
}

async function openApp(path) {
  const url = new URL(path, self.location.origin).href;
  for (const client of await self.clients.matchAll({ type: "window", includeUncontrolled: true })) {
    if (new URL(client.url).origin === self.location.origin && "focus" in client) {
      await client.focus();
      if ("navigate" in client) await client.navigate(url).catch(() => undefined);
      return;
    }
  }
  await self.clients.openWindow(url);
}

self.addEventListener("notificationclick", (event) => {
  const notification = event.notification;
  const data = notification.data || {};
  notification.close();
  event.waitUntil(
    (async () => {
      const call = actionRequest(event.action, data);
      if (call) {
        let ok = false;
        try {
          const resp = await fetch(call.path, {
            method: "POST",
            credentials: "same-origin",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(call.body),
          });
          ok = resp.ok;
        } catch {
          ok = false;
        }
        if (ok) {
          await ack(data.delivery_id, call.ack);
          return;
        }
        // Offline, signed out, or the occurrence changed: let the person decide in the app.
      } else {
        await ack(data.delivery_id, "opened");
      }
      await openApp(openPath(data));
    })(),
  );
});

self.addEventListener("notificationclose", (event) => {
  const data = event.notification.data || {};
  event.waitUntil(ack(data.delivery_id, "dismissed"));
});

// The browser rotated the subscription: register the new one under the
// same session (the old endpoint expires server-side on its next 404/410).
self.addEventListener("pushsubscriptionchange", (event) => {
  event.waitUntil(
    (async () => {
      const sub =
        event.newSubscription ||
        (event.oldSubscription &&
          (await self.registration.pushManager.subscribe(event.oldSubscription.options).catch(() => null)));
      if (!sub) return;
      await fetch("/api/v1/notifications/subscriptions", {
        method: "POST",
        credentials: "same-origin",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ endpoint: sub.endpoint, label: "" }),
      }).catch(() => undefined);
    })(),
  );
});
