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
