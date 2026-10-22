// End-to-end API tests.
//
//   npm run smoke
//     Starts an isolated `wrangler dev` (fresh local D1/R2), then exercises owner
//     setup, passkey login (software authenticator), sessions, CSRF origin checks,
//     device tokens, private storage, and static assets.
//
//   BASE_URL=https://directory.<you>.workers.dev DIRECTORY_API_TOKEN=dir_... npm run smoke
//     Read-mostly checks against a deployed Worker using a device API token.
import { SoftAuthenticator } from "./lib/authenticator.mjs";
import { startDevServer } from "./lib/devserver.mjs";

let failures = 0;
async function check(name, fn) {
  try {
    await fn();
    console.log(`  ok   ${name}`);
  } catch (e) {
    failures++;
    console.log(`  FAIL ${name}\n       ${e.message}`);
  }
}
function expect(cond, msg) {
  if (!cond) throw new Error(msg);
}

/** Minimal client with a cookie jar for one session cookie. */
class Client {
  constructor(base, origin) {
    this.base = base;
    this.origin = origin;
    this.cookie = undefined;
  }
  async req(method, path, { body, headers = {}, origin = true, cookie = true } = {}) {
    const h = { ...headers };
    if (body !== undefined && !(body instanceof Uint8Array) && typeof body !== "string") h["Content-Type"] = "application/json";
    if (origin && this.origin) h.Origin = this.origin;
    if (cookie && this.cookie) h.Cookie = this.cookie;
    const res = await fetch(this.base + path, {
      method,
      headers: h,
      body: body === undefined ? undefined : typeof body === "string" || body instanceof Uint8Array ? body : JSON.stringify(body),
    });
    const set = res.headers.getSetCookie?.() ?? [];
    for (const c of set) {
      const [pair] = c.split(";");
      const [name, value] = pair.split("=");
      if (name === "__Host-dir_session") this.cookie = value ? pair : undefined;
    }
    let json;
    const text = await res.text();
    try {
      json = JSON.parse(text);
    } catch {
      json = undefined;
    }
    return { status: res.status, json, text, headers: res.headers, setCookie: set };
  }
}

async function staticChecks(base) {
  await check("static app shell, SPA fallback, manifest, and service worker are served", async () => {
    const index = await fetch(`${base}/`);
    expect(index.status === 200 && (await index.text()).includes('id="root"'), "index.html");
    const deep = await fetch(`${base}/some/client/route`);
    expect(deep.status === 200, `SPA fallback status ${deep.status}`);
    expect((await fetch(`${base}/manifest.webmanifest`)).status === 200, "manifest");
    const sw = await fetch(`${base}/sw.js`);
    const swText = await sw.text();
    expect(sw.status === 200 && swText.includes("directory-shell-") && !swText.includes("__PRECACHE_MANIFEST__"), "sw.js");
  });
}

async function local() {
  console.log("starting isolated wrangler dev (fresh D1/R2 state)…");
  const server = await startDevServer({ port: 8790, name: "smoke" });
  const { baseUrl, origin, rpId, setupToken } = server;
  console.log(`smoke: ${baseUrl}`);
  const api = new Client(baseUrl, origin);
  const key = new SoftAuthenticator({ rpId, origin });
  let deviceSecret;
  let deviceTokenId;

  try {
    await check("health ok, schema at expected version", async () => {
      const r = await api.req("GET", "/api/v1/health");
      expect(r.status === 200 && r.json.status === "ok", JSON.stringify(r.json));
      expect(/^schema v\d+$/.test(r.json.checks.find((c) => c.name === "database").detail), "schema");
      expect(r.headers.get("cache-control") === "no-store", "no-store");
    });

    await check("unknown API route → JSON 404", async () => {
      const r = await api.req("GET", "/api/v1/nope");
      expect(r.status === 404 && r.json.error === "not_found", `${r.status}`);
    });

    await check("fresh install: unauthenticated, setup required", async () => {
      const r = await api.req("GET", "/api/v1/auth/session");
      expect(r.status === 200 && !r.json.authenticated && r.json.setupRequired, JSON.stringify(r.json));
    });

    await check("private routes reject anonymous callers", async () => {
      for (const [m, p] of [["GET", "/api/v1/storage/objects"], ["GET", "/api/v1/auth/passkeys"], ["GET", "/api/v1/auth/tokens"]]) {
        const r = await api.req(m, p);
        expect(r.status === 401, `${m} ${p} → ${r.status}`);
      }
    });

    await check("setup rejects a wrong setup token", async () => {
      const r = await api.req("POST", "/api/v1/auth/setup/options", { body: { setupToken: "wrong" } });
      expect(r.status === 401 && r.json.error === "setup_token_invalid", `${r.status} ${r.text}`);
    });

    await check("setup registers the owner passkey and starts a session", async () => {
      const opts = await api.req("POST", "/api/v1/auth/setup/options", { body: { setupToken, displayName: "Smoke Owner" } });
      expect(opts.status === 200, `options ${opts.status} ${opts.text}`);
      expect(opts.json.publicKey.rp.id === rpId, "rp id");
      expect(opts.json.publicKey.authenticatorSelection.userVerification === "required", "uv required");
      const r = await api.req("POST", "/api/v1/auth/setup/verify", { body: key.register(opts.json, "Smoke key") });
      expect(r.status === 201, `verify ${r.status} ${r.text}`);
      const cookie = r.setCookie.find((c) => c.startsWith("__Host-dir_session="));
      expect(cookie && /HttpOnly/.test(cookie) && /Secure/.test(cookie) && /SameSite=Strict/.test(cookie), `cookie: ${cookie}`);
      const s = await api.req("GET", "/api/v1/auth/session");
      expect(s.json.authenticated && s.json.user.displayName === "Smoke Owner" && !s.json.setupRequired, JSON.stringify(s.json));
    });

    await check("setup is closed once an owner exists", async () => {
      const r = await api.req("POST", "/api/v1/auth/setup/options", { body: { setupToken } });
      expect(r.status === 409 && r.json.error === "setup_unavailable", `${r.status}`);
    });

    const objectKey = `smoke/check-${Date.now()}.txt`;
    const payload = `directory smoke ${new Date().toISOString()}`;

    await check("cookie-authenticated write without Origin → 403", async () => {
      const r = await api.req("PUT", `/api/v1/storage/objects/${objectKey}`, { body: payload, origin: false });
      expect(r.status === 403 && r.json.error === "origin_not_allowed", `${r.status}`);
      const evil = await fetch(`${baseUrl}/api/v1/storage/objects/${objectKey}`, {
        method: "PUT",
        headers: { Cookie: api.cookie, Origin: "https://evil.example" },
        body: payload,
      });
      expect(evil.status === 403, `foreign origin → ${evil.status}`);
    });

    await check("storage round trip with session: PUT, LIST, GET (attachment), DELETE", async () => {
      const put = await api.req("PUT", `/api/v1/storage/objects/${objectKey}`, {
        body: payload,
        headers: { "Content-Type": "text/plain; charset=utf-8" },
      });
      expect(put.status === 201 && put.json.key === objectKey, `put ${put.status} ${put.text}`);
      const list = await api.req("GET", "/api/v1/storage/objects?prefix=smoke/");
      expect(list.json.objects.some((o) => o.key === objectKey), "listed with logical key");
      const get = await api.req("GET", `/api/v1/storage/objects/${objectKey}`);
      expect(get.status === 200 && get.text === payload, "body");
      expect(get.headers.get("content-disposition") === "attachment", "attachment");
      const bad = await api.req("GET", "/api/v1/storage/objects/bad%20key");
      expect(bad.status === 400, `invalid key → ${bad.status}`);
      const del = await api.req("DELETE", `/api/v1/storage/objects/${objectKey}`);
      expect(del.status === 204, `delete ${del.status}`);
      expect((await api.req("GET", `/api/v1/storage/objects/${objectKey}`)).status === 404, "gone");
    });

    await check("tasks: create, validate, subtasks one level deep, filters, completion, clear due date, cascade delete", async () => {
      const bad = await api.req("POST", "/api/v1/tasks", { body: { title: "   " } });
      expect(bad.status === 400 && bad.json.error === "invalid_input" && /title/.test(bad.json.message), `blank title → ${bad.status}`);
      const badDate = await api.req("POST", "/api/v1/tasks", { body: { title: "x", due_date: "2026-02-30" } });
      expect(badDate.status === 400 && /due_date/.test(badDate.json.message), `bad date → ${badDate.status}`);

      const parent = await api.req("POST", "/api/v1/tasks", {
        body: { title: "Plan trip", priority: "high", due_date: "2026-12-01", notes: "flights\nhotel" },
      });
      expect(parent.status === 201 && parent.json.status === "todo" && parent.json.priority === "high", `create → ${parent.status} ${parent.text}`);
      expect(!("owner_id" in parent.json), "owner_id hidden");
      const child = await api.req("POST", "/api/v1/tasks", { body: { title: "Book flights", parent_id: parent.json.id } });
      expect(child.status === 201 && child.json.parent_id === parent.json.id, `subtask → ${child.status}`);
      const grandchild = await api.req("POST", "/api/v1/tasks", { body: { title: "Nope", parent_id: child.json.id } });
      expect(grandchild.status === 400, `grandchild → ${grandchild.status}`);

      const done = await api.req("PATCH", `/api/v1/tasks/${child.json.id}`, { body: { status: "done" } });
      expect(done.status === 200 && done.json.completed_at > 0, `complete → ${done.status} ${done.text}`);
      const open = await api.req("GET", "/api/v1/tasks?status=open");
      expect(open.json.tasks.some((t) => t.id === parent.json.id) && !open.json.tasks.some((t) => t.id === child.json.id), "open filter");
      const closed = await api.req("GET", "/api/v1/tasks?status=closed");
      expect(closed.json.tasks.some((t) => t.id === child.json.id), "closed filter");
      expect((await api.req("GET", "/api/v1/tasks?status=bogus")).status === 400, "bad filter");

      const cleared = await api.req("PATCH", `/api/v1/tasks/${parent.json.id}`, { body: { due_date: null, title: "Plan the trip" } });
      expect(cleared.json.due_date === null && cleared.json.title === "Plan the trip" && cleared.json.priority === "high", `patch → ${cleared.text}`);

      expect((await api.req("GET", `/api/v1/tasks/${"0".repeat(32)}`)).status === 404, "unknown id");
      expect((await api.req("DELETE", `/api/v1/tasks/${parent.json.id}`)).status === 204, "delete");
      expect((await api.req("GET", `/api/v1/tasks/${child.json.id}`)).status === 404, "subtask deleted with parent");
      expect((await fetch(`${baseUrl}/api/v1/tasks`)).status === 401, "anonymous → 401");
    });

    await check("device API token: create (shown once), use as bearer, cannot manage credentials", async () => {
      const r = await api.req("POST", "/api/v1/auth/tokens", { body: { name: "Smoke device" } });
      expect(r.status === 201 && r.json.secret.startsWith("dir_"), `${r.status} ${r.text}`);
      expect(!("token_hash" in r.json.token) && !("tokenHash" in r.json.token), "hash not exposed");
      deviceSecret = r.json.secret;
      deviceTokenId = r.json.token.id;
      const bearer = new Client(baseUrl);
      const list = await bearer.req("GET", "/api/v1/storage/objects", { headers: { Authorization: `Bearer ${deviceSecret}` } });
      expect(list.status === 200, `bearer list ${list.status}`);
      const mint = await bearer.req("POST", "/api/v1/auth/tokens", {
        body: {},
        headers: { Authorization: `Bearer ${deviceSecret}` },
      });
      expect(mint.status === 403 && mint.json.error === "session_required", `mint via bearer → ${mint.status}`);
      const wrong = await bearer.req("GET", "/api/v1/storage/objects", { headers: { Authorization: "Bearer dir_wrong" } });
      expect(wrong.status === 401, `wrong bearer → ${wrong.status}`);
    });

    const oldCookie = api.cookie;
    await check("logout clears the cookie and revokes the session server-side", async () => {
      const r = await api.req("POST", "/api/v1/auth/logout");
      expect(r.status === 204, `${r.status}`);
      expect(r.setCookie.some((c) => /Max-Age=0/.test(c)), "cookie cleared");
      const replay = await fetch(`${baseUrl}/api/v1/storage/objects`, { headers: { Cookie: oldCookie } });
      expect(replay.status === 401, `old cookie → ${replay.status}`);
    });

    await check("passkey login issues a new session", async () => {
      const opts = await api.req("POST", "/api/v1/auth/login/options", { body: {} });
      expect(opts.status === 200 && opts.json.publicKey.allowCredentials.length === 0, `${opts.status}`);
      const r = await api.req("POST", "/api/v1/auth/login/verify", { body: key.assert(opts.json) });
      expect(r.status === 200 && api.cookie && api.cookie !== oldCookie, `${r.status} ${r.text}`);
      const s = await api.req("GET", "/api/v1/auth/session");
      expect(s.json.authenticated, "authenticated");
    });

    await check("login rejects replayed challenges, bad signatures, and missing user verification", async () => {
      const opts = await api.req("POST", "/api/v1/auth/login/options", { body: {} });
      const good = key.assert(opts.json);
      expect((await api.req("POST", "/api/v1/auth/login/verify", { body: good, cookie: false })).status === 200, "first use");
      const replay = await api.req("POST", "/api/v1/auth/login/verify", { body: good, cookie: false });
      expect(replay.status === 400 && replay.json.error === "challenge_invalid", `replay → ${replay.status}`);

      const opts2 = await api.req("POST", "/api/v1/auth/login/options", { body: {} });
      const forged = { ...key.assert(opts2.json), signature: Buffer.alloc(70, 1).toString("base64url") };
      const bad = await api.req("POST", "/api/v1/auth/login/verify", { body: forged, cookie: false });
      expect(bad.status === 401 && bad.json.error === "authentication_failed", `forged → ${bad.status}`);

      const opts3 = await api.req("POST", "/api/v1/auth/login/options", { body: {} });
      const noUv = await api.req("POST", "/api/v1/auth/login/verify", { body: key.assert(opts3.json, { flags: 0x01 }), cookie: false });
      expect(noUv.status === 401, `no UV → ${noUv.status}`);
    });

    await check("passkeys: list, add a second, cannot delete the last", async () => {
      const list = await api.req("GET", "/api/v1/auth/passkeys");
      expect(list.status === 200 && list.json.passkeys.length === 1, JSON.stringify(list.json));
      const opts = await api.req("POST", "/api/v1/auth/passkeys/options", { body: {} });
      expect(opts.json.publicKey.excludeCredentials.length === 1, "excludes existing");
      const second = new SoftAuthenticator({ rpId, origin });
      const add = await api.req("POST", "/api/v1/auth/passkeys/verify", { body: second.register(opts.json, "Phone") });
      expect(add.status === 201 && add.json.name === "Phone", `${add.status} ${add.text}`);
      const del = await api.req("DELETE", `/api/v1/auth/passkeys/${add.json.id}`);
      expect(del.status === 204, `delete second → ${del.status}`);
      const last = (await api.req("GET", "/api/v1/auth/passkeys")).json.passkeys[0];
      const lastDel = await api.req("DELETE", `/api/v1/auth/passkeys/${last.id}`);
      expect(lastDel.status === 409 && lastDel.json.error === "last_passkey", `delete last → ${lastDel.status}`);
    });

    await check("revoked device token stops working", async () => {
      const del = await api.req("DELETE", `/api/v1/auth/tokens/${deviceTokenId}`);
      expect(del.status === 204, `revoke ${del.status}`);
      const r = await fetch(`${baseUrl}/api/v1/storage/objects`, { headers: { Authorization: `Bearer ${deviceSecret}` } });
      expect(r.status === 401, `after revoke → ${r.status}`);
    });

    await check("sign-in endpoints are rate limited per client (429 + Retry-After)", async () => {
      let limited;
      for (let i = 0; i < 40 && !limited; i++) {
        const r = await api.req("POST", "/api/v1/auth/login/options", { body: {}, cookie: false });
        if (r.status === 429) limited = r;
        else expect(r.status === 200, `unexpected ${r.status}`);
      }
      expect(limited, "never limited");
      expect(limited.json.error === "rate_limited" && Number(limited.headers.get("retry-after")) > 0, "Retry-After");
      const health = await api.req("GET", "/api/v1/health");
      expect(health.status === 200, "other routes unaffected");
    });

    await staticChecks(baseUrl);
  } finally {
    if (failures) console.log(`\n--- wrangler output (tail) ---\n${server.logs().split("\n").slice(-40).join("\n")}`);
    server.stop();
  }
}

async function remote(base) {
  const token = process.env.DIRECTORY_API_TOKEN;
  console.log(`smoke (remote): ${base}`);
  const api = new Client(base);
  await check("health ok", async () => {
    const r = await api.req("GET", "/api/v1/health");
    expect(r.status === 200 && r.json.status === "ok", `${r.status} ${r.text}`);
  });
  await check("anonymous access to private routes → 401", async () => {
    expect((await api.req("GET", "/api/v1/storage/objects")).status === 401, "storage");
  });
  if (token) {
    await check("device token storage round trip", async () => {
      const auth = { Authorization: `Bearer ${token}` };
      const key = `smoke/remote-${Date.now()}.txt`;
      const put = await api.req("PUT", `/api/v1/storage/objects/${key}`, { body: "remote smoke", headers: auth });
      expect(put.status === 201, `put ${put.status}`);
      const get = await api.req("GET", `/api/v1/storage/objects/${key}`, { headers: auth });
      expect(get.text === "remote smoke", "body");
      expect((await api.req("DELETE", `/api/v1/storage/objects/${key}`, { headers: auth })).status === 204, "delete");
    });
  } else {
    console.log("  skip storage round trip (set DIRECTORY_API_TOKEN to a device token)");
  }
  await staticChecks(base);
}

if (process.env.BASE_URL) await remote(process.env.BASE_URL.replace(/\/$/, ""));
else await local();

console.log(failures ? `\n${failures} check(s) failed` : "\nall checks passed");
process.exit(failures ? 1 : 0);
