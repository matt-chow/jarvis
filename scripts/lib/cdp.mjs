// Headless Chrome/Edge driven over the DevTools Protocol, with no dependencies.
import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

export function findBrowser() {
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

export async function waitFor(desc, fn, timeoutMs = 20_000) {
  const end = Date.now() + timeoutMs;
  let last;
  while (Date.now() < end) {
    try {
      if (await fn()) return;
    } catch (e) {
      last = e;
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error(`timed out waiting for ${desc}${last ? ` (${last.message})` : ""}`);
}

/** Minimal CDP client over Node's built-in WebSocket. */
async function connect(wsUrl) {
  const ws = new WebSocket(wsUrl);
  await new Promise((r, j) => {
    ws.onopen = r;
    ws.onerror = j;
  });
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
  const once = (method, sessionId) =>
    new Promise((r) => {
      const l = (m) => {
        if (m.method === method && m.sessionId === sessionId) {
          listeners.splice(listeners.indexOf(l), 1);
          r(m.params);
        }
      };
      listeners.push(l);
    });
  return { send, once, close: () => ws.close() };
}

/**
 * Launch a fresh headless browser profile and open one page.
 * Launcher stubs (msedge.exe on Windows) exit immediately, so we poll the
 * DevToolsActivePort file rather than watching the spawned process.
 */
export async function launch() {
  const exe = findBrowser();
  if (!exe) throw new Error("No Chrome/Edge found; set BROWSER_PATH.");
  const profile = mkdtempSync(join(tmpdir(), "directory-e2e-"));
  spawn(exe, ["--headless=new", "--remote-debugging-port=0", `--user-data-dir=${profile}`, "--no-first-run", "--no-default-browser-check", "--autoplay-policy=no-user-gesture-required", "about:blank"], {
    stdio: "ignore",
  }).unref();
  const portFile = join(profile, "DevToolsActivePort");
  let wsUrl;
  await waitFor("browser DevTools endpoint", () => {
    if (!existsSync(portFile)) return false;
    const [port, path] = readFileSync(portFile, "utf8").trim().split(/\r?\n/);
    if (port && path) wsUrl = `ws://127.0.0.1:${port}${path}`;
    return Boolean(wsUrl);
  }, 30_000);

  const cdp = await connect(wsUrl);
  const { targetId } = await cdp.send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await cdp.send("Target.attachToTarget", { targetId, flatten: true });
  const send = (method, params) => cdp.send(method, params, sessionId);
  await send("Page.enable");

  const withTimeout = (promise, what, ms = 30_000) =>
    Promise.race([promise, new Promise((_, j) => setTimeout(() => j(new Error(`timed out: ${what}`)), ms))]);

  const page = {
    name: exe.split(/[/\\]/).pop(),
    send,
    async evaluate(expression) {
      const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
      if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
      return r.result.value;
    },
    async navigate(url) {
      const loaded = cdp.once("Page.loadEventFired", sessionId);
      await send("Page.navigate", { url });
      await withTimeout(loaded, `load ${url}`);
    },
    /** In-app navigation (same document: no load event). */
    hash: (route) => page.evaluate(`location.hash = ${JSON.stringify(route)}`),
    async reload() {
      const loaded = cdp.once("Page.loadEventFired", sessionId);
      await send("Page.reload", {});
      await withTimeout(loaded, "reload");
    },
    text: () => page.evaluate("document.body.innerText"),
    waitForText: (text, timeoutMs) => waitFor(`text "${text}"`, async () => (await page.text()).includes(text), timeoutMs),
    /** Set a React-controlled input by its aria-label or label text. */
    async fill(label, value) {
      // Waits for the (enabled) field to exist: editors often mount after a fetch.
      await waitFor(`input "${label}"`, () =>
        page.evaluate(`(() => {
          const label = ${JSON.stringify(label)};
          const el = document.querySelector(\`input[aria-label="\${label}"], textarea[aria-label="\${label}"]\`)
            ?? [...document.querySelectorAll("label")].find((l) => l.innerText.trim().toLowerCase().startsWith(label.toLowerCase()))?.querySelector("input, textarea");
          if (!el || el.disabled) return false;
          const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
          Object.getOwnPropertyDescriptor(proto, "value").set.call(el, ${JSON.stringify(value)});
          el.dispatchEvent(new Event("input", { bubbles: true }));
          return true;
        })()`),
      );
    },
    /** Click the first enabled button or link whose text or aria-label contains `text`. */
    async click(text) {
      await waitFor(`clickable "${text}"`, () =>
        page.evaluate(`(() => {
          const needle = ${JSON.stringify(text.toLowerCase())};
          const candidates = [...document.querySelectorAll("button, a")].filter((b) => !b.disabled);
          const el = candidates.find((b) => b.innerText.toLowerCase().includes(needle))
            ?? candidates.find((b) => (b.getAttribute("aria-label") ?? "").toLowerCase().includes(needle));
          if (!el) return false;
          el.click();
          return true;
        })()`),
      );
    },
    async setFile(selector, path) {
      const { root } = await send("DOM.getDocument", {});
      const { nodeId } = await send("DOM.querySelector", { nodeId: root.nodeId, selector });
      await send("DOM.setFileInputFiles", { nodeId, files: [path] });
    },
    /** Save a PNG; `width` < 600 emulates a phone. */
    async screenshot(path, { width = 1280, height = 860 } = {}) {
      const mobile = width < 600;
      await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: mobile ? 2 : 1, mobile });
      await new Promise((r) => setTimeout(r, 400));
      const { data } = await send("Page.captureScreenshot", { format: "png" });
      writeFileSync(path, Buffer.from(data, "base64"));
      await send("Emulation.clearDeviceMetricsOverride", {});
    },
    /** Platform authenticator with resident keys and verified user (passkeys). */
    async addVirtualAuthenticator() {
      await send("WebAuthn.enable", { enableUI: false });
      return send("WebAuthn.addVirtualAuthenticator", {
        options: {
          protocol: "ctap2",
          transport: "internal",
          hasResidentKey: true,
          hasUserVerification: true,
          isUserVerified: true,
          automaticPresenceSimulation: true,
        },
      });
    },
  };

  const close = async () => {
    await cdp.send("Browser.close").catch(() => {});
    cdp.close();
    await new Promise((r) => setTimeout(r, 1000));
    try {
      rmSync(profile, { recursive: true, force: true });
    } catch {
      /* browser may still hold files */
    }
  };
  return { page, close };
}
