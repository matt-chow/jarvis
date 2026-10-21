// End-to-end smoke test against a running Worker (local `npm run dev:worker`
// or a deployed URL). Uses only Node built-ins.
//
//   npm run smoke                                   # http://127.0.0.1:8787, token from .dev.vars
//   BASE_URL=https://directory.<you>.workers.dev DIRECTORY_API_TOKEN=... npm run smoke
//
// Writes and then deletes one object under `smoke/` in the R2 bucket.
import { readFileSync } from "node:fs";

const BASE = (process.env.BASE_URL ?? "http://127.0.0.1:8787").replace(/\/$/, "");

function devVarsToken() {
  try {
    const line = readFileSync(new URL("../.dev.vars", import.meta.url), "utf8")
      .split(/\r?\n/)
      .find((l) => l.startsWith("DIRECTORY_API_TOKEN="));
    return line?.slice("DIRECTORY_API_TOKEN=".length).trim();
  } catch {
    return undefined;
  }
}
const TOKEN = process.env.DIRECTORY_API_TOKEN ?? devVarsToken();

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
const auth = (extra = {}) => ({ ...extra, Authorization: `Bearer ${TOKEN}` });

console.log(`smoke: ${BASE}`);

await check("GET /api/v1/health → ok with database + storage checks", async () => {
  const res = await fetch(`${BASE}/api/v1/health`);
  const body = await res.json();
  expect(res.status === 200, `status ${res.status}: ${JSON.stringify(body)}`);
  expect(body.status === "ok", `report status ${body.status}`);
  const names = body.checks.map((c) => c.name).sort().join(",");
  expect(names === "database,storage", `checks: ${names}`);
  expect(res.headers.get("cache-control") === "no-store", "API must be no-store");
});

await check("unknown API route → JSON 404", async () => {
  const res = await fetch(`${BASE}/api/v1/nope`);
  expect(res.status === 404, `status ${res.status}`);
  expect((await res.json()).error === "not_found", "error code");
});

await check("storage without token → 401", async () => {
  const res = await fetch(`${BASE}/api/v1/storage/objects`);
  expect(res.status === 401, `status ${res.status}`);
  expect(res.headers.get("www-authenticate") === "Bearer", "WWW-Authenticate header");
});

await check("storage with wrong token → 401", async () => {
  const res = await fetch(`${BASE}/api/v1/storage/objects`, {
    headers: { Authorization: "Bearer wrong-token-wrong-token-wrong-token" },
  });
  expect(res.status === 401, `status ${res.status}`);
});

if (!TOKEN) {
  failures++;
  console.log("  FAIL no DIRECTORY_API_TOKEN (env or .dev.vars); skipped authenticated checks");
} else {
  const key = `smoke/check-${Date.now()}.txt`;
  const payload = `directory smoke ${new Date().toISOString()}`;

  await check("PUT object → 201", async () => {
    const res = await fetch(`${BASE}/api/v1/storage/objects/${key}`, {
      method: "PUT",
      headers: auth({ "Content-Type": "text/plain; charset=utf-8" }),
      body: payload,
    });
    const body = await res.json();
    expect(res.status === 201, `status ${res.status}: ${JSON.stringify(body)}`);
    expect(body.key === key && body.size === Buffer.byteLength(payload), JSON.stringify(body));
  });

  await check("LIST objects → includes key", async () => {
    const res = await fetch(`${BASE}/api/v1/storage/objects?prefix=smoke/`, { headers: auth() });
    const body = await res.json();
    expect(res.status === 200, `status ${res.status}`);
    expect(body.objects.some((o) => o.key === key), "key missing from listing");
  });

  await check("GET object → same bytes, served as attachment", async () => {
    const res = await fetch(`${BASE}/api/v1/storage/objects/${key}`, { headers: auth() });
    expect(res.status === 200, `status ${res.status}`);
    expect((await res.text()) === payload, "body mismatch");
    expect(res.headers.get("content-type") === "text/plain", `content-type ${res.headers.get("content-type")}`);
    expect(res.headers.get("content-disposition") === "attachment", "must download, not render");
  });

  await check("invalid key → 400", async () => {
    const res = await fetch(`${BASE}/api/v1/storage/objects/bad%20key`, { headers: auth() });
    expect(res.status === 400, `status ${res.status}`);
  });

  await check("DELETE object → 204, then GET → 404", async () => {
    const del = await fetch(`${BASE}/api/v1/storage/objects/${key}`, { method: "DELETE", headers: auth() });
    expect(del.status === 204, `delete status ${del.status}`);
    const res = await fetch(`${BASE}/api/v1/storage/objects/${key}`, { headers: auth() });
    expect(res.status === 404, `get-after-delete status ${res.status}`);
  });
}

await check("static app shell, manifest, and service worker are served", async () => {
  const index = await fetch(`${BASE}/`);
  expect(index.status === 200 && (await index.text()).includes('id="root"'), "index.html");
  const deep = await fetch(`${BASE}/some/client/route`);
  expect(deep.status === 200, `SPA fallback status ${deep.status}`);
  const manifest = await fetch(`${BASE}/manifest.webmanifest`);
  expect(manifest.status === 200, "manifest");
  const sw = await fetch(`${BASE}/sw.js`);
  const swText = await sw.text();
  expect(sw.status === 200 && swText.includes("directory-shell-") && !swText.includes("__PRECACHE_MANIFEST__"), "sw.js");
});

console.log(failures ? `\n${failures} check(s) failed` : "\nall checks passed");
process.exit(failures ? 1 : 0);
