// Verifies the PWA works offline in a real browser, with no test dependencies:
//   1. serve apps/web/dist (+ a stub /api/v1/health) from a throwaway server
//   2. load it in headless Chrome/Edge via the DevTools Protocol
//   3. wait for the service worker to control the page and health to be cached
//   4. write a note to IndexedDB, shut the server down, reload
//   5. assert the shell renders from cache, the note survived, health shows cached
//
//   npm run build && npm run test:offline
// Set BROWSER_PATH to override browser discovery.
import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const DIST = fileURLToPath(new URL("../apps/web/dist/", import.meta.url));
const TYPES = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".svg": "image/svg+xml", ".png": "image/png", ".webmanifest": "application/manifest+json" };
const HEALTH = { status: "ok", service: "directory", version: "0.0.1", checks: [{ name: "database", status: "ok", detail: "schema v1" }] };

function findBrowser() {
  const candidates = [
    process.env.BROWSER_PATH,
    "C:/Program Files/Google/Chrome/Application/chrome.exe",
    "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
    "C:/Program Files/Microsoft/Edge/Application/msedge.exe",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
  ];
  return candidates.find((p) => p && existsSync(p));
}

function serveDist() {
  const sockets = new Set();
  const server = createServer((req, res) => {
    const path = new URL(req.url, "http://x").pathname;
    if (path === "/api/v1/health") {
      res.writeHead(200, { "Content-Type": "application/json", "Cache-Control": "no-store" });
      return res.end(JSON.stringify(HEALTH));
    }
    let file = join(DIST, normalize(path).replace(/^([/\\])+/, ""));
    if (!file.startsWith(DIST) || !existsSync(file) || path === "/") file = join(DIST, "index.html");
    res.writeHead(200, { "Content-Type": TYPES[extname(file)] ?? "application/octet-stream" });
    res.end(readFileSync(file));
  });
  server.on("connection", (s) => {
    sockets.add(s);
    s.on("close", () => sockets.delete(s));
  });
  return new Promise((resolve) =>
    server.listen(0, "127.0.0.1", () =>
      resolve({
        url: `http://127.0.0.1:${server.address().port}`,
        stop: () => new Promise((r) => { for (const s of sockets) s.destroy(); server.close(r); }),
      }),
    ),
  );
}

/** The browser writes `<port>\n<path>` to DevToolsActivePort. Launcher stubs
 * (e.g. msedge.exe on Windows) may exit immediately, so poll the file instead
 * of watching the spawned process. */
async function launchBrowser(exe, profileDir) {
  spawn(exe, ["--headless=new", "--remote-debugging-port=0", `--user-data-dir=${profileDir}`, "--no-first-run", "--no-default-browser-check", "about:blank"], { stdio: "ignore" }).unref();
  const portFile = join(profileDir, "DevToolsActivePort");
  let wsUrl;
  await waitFor("browser DevTools endpoint", () => {
    if (!existsSync(portFile)) return false;
    const [port, path] = readFileSync(portFile, "utf8").trim().split(/\r?\n/);
    if (port && path) wsUrl = `ws://127.0.0.1:${port}${path}`;
    return Boolean(wsUrl);
  }, 30_000);
  return { wsUrl };
}

/** Minimal CDP client over Node's built-in WebSocket. */
async function connect(wsUrl) {
  const ws = new WebSocket(wsUrl);
  await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
  let id = 0;
  const pending = new Map();
  const listeners = [];
  ws.onmessage = ({ data }) => {
    const msg = JSON.parse(data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      msg.error ? reject(new Error(msg.error.message)) : resolve(msg.result);
    } else listeners.forEach((l) => l(msg));
  };
  const send = (method, params = {}, sessionId) =>
    new Promise((resolve, reject) => {
      pending.set(++id, { resolve, reject });
      ws.send(JSON.stringify({ id, method, params, sessionId }));
    });
  const once = (method, sessionId) => new Promise((r) => {
    const l = (m) => { if (m.method === method && m.sessionId === sessionId) { listeners.splice(listeners.indexOf(l), 1); r(m.params); } };
    listeners.push(l);
  });
  return { send, once, close: () => ws.close() };
}

async function waitFor(desc, fn, timeoutMs = 20_000) {
  const end = Date.now() + timeoutMs;
  while (Date.now() < end) {
    if (await fn()) return;
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(`timed out waiting for ${desc}`);
}

let failures = 0;
const check = (name, ok, detail = "") => {
  if (!ok) failures++;
  console.log(`  ${ok ? "ok  " : "FAIL"} ${name}${!ok && detail ? `\n       ${detail}` : ""}`);
};

if (!existsSync(join(DIST, "sw.js"))) {
  console.error("apps/web/dist/sw.js missing — run `npm run build` first.");
  process.exit(1);
}
const exe = findBrowser();
if (!exe) {
  console.error("No Chrome/Edge found; set BROWSER_PATH.");
  process.exit(1);
}

const profile = mkdtempSync(join(tmpdir(), "directory-offline-"));
const server = await serveDist();
const { wsUrl } = await launchBrowser(exe, profile);
const cdp = await connect(wsUrl);

try {
  console.log(`offline check: ${server.url} in ${exe.split(/[/\\]/).pop()}`);
  const { targetId } = await cdp.send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await cdp.send("Target.attachToTarget", { targetId, flatten: true });
  const page = (method, params) => cdp.send(method, params, sessionId);
  const evaluate = async (expression) =>
    (await page("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true })).result.value;
  await page("Page.enable");

  const loaded = cdp.once("Page.loadEventFired", sessionId);
  await page("Page.navigate", { url: server.url });
  await loaded;

  await waitFor("service worker to control the page", () => evaluate("!!navigator.serviceWorker.controller"));
  check("service worker installed and controlling", true);

  const idb = (body) => `new Promise((res, rej) => { const r = indexedDB.open("directory"); r.onerror = () => rej(r.error); r.onsuccess = () => { const db = r.result; ${body} }; })`;
  await waitFor("health report cached in IndexedDB", () =>
    evaluate(idb(`const q = db.transaction("kv").objectStore("kv").get("health:last"); q.onsuccess = () => res(!!q.result);`)),
  );
  check("health report cached on device", true);

  await evaluate(idb(`const tx = db.transaction("notes", "readwrite"); tx.objectStore("notes").put({ id: "offline-check", body: "written online", createdAt: 1, updatedAt: Date.now() }); tx.oncomplete = () => res(true);`));

  await server.stop();
  const net = await fetch(server.url).then(() => "reachable", () => "unreachable");
  check("server is down", net === "unreachable", net);

  const reloaded = cdp.once("Page.loadEventFired", sessionId);
  await page("Page.reload", { ignoreCache: false });
  await reloaded;

  await waitFor("app to render offline", () => evaluate(`document.body.innerText.includes("DIRECTORY")`)).catch(() => {});
  const text = await evaluate("document.body.innerText");
  check("app shell renders with server down", text.includes("DIRECTORY") && text.includes("Scratchpad"), text.slice(0, 200));
  await waitFor("cached health", () => evaluate(`document.body.innerText.includes("last known")`)).catch(() => {});
  check("system status falls back to last known report", (await evaluate("document.body.innerText")).includes("last known"));
  const note = await evaluate(idb(`const q = db.transaction("notes").objectStore("notes").get("offline-check"); q.onsuccess = () => res(q.result?.body);`));
  check("IndexedDB note persisted across offline reload", note === "written online", String(note));
} catch (e) {
  failures++;
  console.log(`  FAIL ${e.message}`);
} finally {
  await cdp.send("Browser.close").catch(() => {});
  cdp.close();
  await new Promise((r) => setTimeout(r, 1000));
  try { rmSync(profile, { recursive: true, force: true }); } catch { /* browser may still hold files */ }
}

console.log(failures ? `\n${failures} check(s) failed` : "\nall checks passed");
process.exit(failures ? 1 : 0);
