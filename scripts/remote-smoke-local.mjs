// Exercises `npm run smoke` remote mode (the staging checks) against an isolated
// local `wrangler dev`: sets up a throwaway owner, mints a full and a
// JARVIS-only device token, then runs smoke.mjs with BASE_URL and both tokens.
// Usage: node scripts/remote-smoke-local.mjs
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { SoftAuthenticator } from "./lib/authenticator.mjs";
import { startDevServer } from "./lib/devserver.mjs";

const server = await startDevServer({ port: 8792, name: "remote-local" });
let code = 1;
try {
  const { origin, rpId, setupToken, baseUrl } = server;
  const post = async (path, body, cookie) => {
    const res = await fetch(`${baseUrl}${path}`, {
      method: "POST",
      headers: { "Content-Type": "application/json", Origin: origin, ...(cookie ? { Cookie: cookie } : {}) },
      body: JSON.stringify(body),
    });
    return { res, json: await res.json().catch(() => null) };
  };
  const key = new SoftAuthenticator({ rpId, origin });
  const opts = await post("/api/v1/auth/setup/options", { setupToken, displayName: "Remote Smoke Owner" });
  const done = await post("/api/v1/auth/setup/verify", key.register(opts.json, "Remote smoke key"));
  const cookie = done.res.headers.getSetCookie().find((c) => c.startsWith("__Host-dir_session=")).split(";")[0];
  const full = await post("/api/v1/auth/tokens", { name: "Remote smoke", scope: "full" }, cookie);
  const jarvis = await post("/api/v1/auth/tokens", { name: "Remote smoke Shortcut", scope: "jarvis" }, cookie);
  if (!full.json?.secret || !jarvis.json?.secret) throw new Error("token setup failed");
  const run = spawnSync(process.execPath, [fileURLToPath(new URL("./smoke.mjs", import.meta.url))], {
    stdio: "inherit",
    env: { ...process.env, BASE_URL: baseUrl, DIRECTORY_API_TOKEN: full.json.secret, DIRECTORY_JARVIS_TOKEN: jarvis.json.secret },
  });
  code = run.status ?? 1;
} finally {
  server.stop();
}
process.exit(code);
