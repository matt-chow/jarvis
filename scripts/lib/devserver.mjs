// Starts an isolated `wrangler dev` (fresh local D1/R2 state, own vars) for tests.
import { spawn, spawnSync } from "node:child_process";
import { randomBytes } from "node:crypto";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { delimiter, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("../..", import.meta.url));
const IS_WIN = process.platform === "win32";

function env() {
  // worker-build lives in ~/.cargo/bin, which may not be on PATH in every shell.
  return { ...process.env, PATH: [join(homedir(), ".cargo", "bin"), process.env.PATH].join(delimiter) };
}

// Arguments are fixed strings and generated paths (no user input), so joining them
// into one shell command is safe; a single string avoids Node's DEP0190 warning.
const command = (args) => ["npx", "wrangler", ...args].join(" ");

function wrangler(args) {
  return spawnSync(command(args), { cwd: ROOT, env: env(), shell: true, encoding: "utf8" });
}

function killTree(child) {
  if (child.exitCode !== null) return;
  if (IS_WIN) spawnSync("taskkill", ["/PID", String(child.pid), "/T", "/F"], { stdio: "ignore" });
  else process.kill(-child.pid, "SIGTERM");
}

/**
 * @param {{ port?: number, name?: string, vars?: Record<string, string> }} opts
 *   `vars` override the test defaults (the eval passes a real free model only when asked).
 * @returns {Promise<{ baseUrl: string, origin: string, rpId: string, setupToken: string, stop: () => void, logs: () => string }>}
 */
export async function startDevServer({ port = 8790, name = "smoke", vars = {} } = {}) {
  const stateDir = join(".wrangler", `${name}-state`);
  rmSync(join(ROOT, stateDir), { recursive: true, force: true });
  mkdirSync(join(ROOT, stateDir), { recursive: true });

  const setupToken = randomBytes(24).toString("hex");
  const origin = `http://localhost:${port}`;
  const envFile = join(ROOT, ".wrangler", `${name}.env`);
  writeFileSync(
    envFile,
    [
      `DIRECTORY_SETUP_TOKEN=${setupToken}`,
      "DIRECTORY_RP_ID=localhost",
      `DIRECTORY_ORIGINS=${origin}`,
      // Small quota so tests can exercise the limit.
      "DIRECTORY_QUOTA_BYTES=1000000",
      // AI intent parsing configured with a paid model id: it must stay off (D-043),
      // so no request ever leaves for OpenRouter during tests.
      "OPENROUTER_API_KEY=sk-or-test-not-a-real-key",
      "JARVIS_AI_MODEL=example/paid-model",
    ]
      .filter((line) => !(line.split("=")[0] in vars))
      .concat(Object.entries(vars).map(([k, v]) => `${k}=${v}`))
      .join("\n"),
  );

  // On Windows a just-stopped run can still hold the state files for a moment
  // (SQLITE_READONLY): wait, wipe, and try again.
  let migrate;
  for (let attempt = 1; ; attempt++) {
    migrate = wrangler(["d1", "migrations", "apply", "DB", "--local", "--persist-to", stateDir]);
    if (migrate.status === 0 || attempt === 3 || !/SQLITE_READONLY|SQLITE_BUSY|EBUSY/.test(`${migrate.stdout}${migrate.stderr}`)) break;
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 3000);
    rmSync(join(ROOT, stateDir), { recursive: true, force: true });
    mkdirSync(join(ROOT, stateDir), { recursive: true });
  }
  if (migrate.status !== 0) throw new Error(`migrations failed:\n${migrate.stdout}\n${migrate.stderr}`);

  let output = "";
  const child = spawn(
    command(["dev", "--port", String(port), "--ip", "127.0.0.1", "--persist-to", stateDir, "--env-file", `"${envFile}"`]),
    { cwd: ROOT, env: env(), shell: true, detached: !IS_WIN, stdio: ["ignore", "pipe", "pipe"] },
  );
  child.stdout.on("data", (d) => (output += d));
  child.stderr.on("data", (d) => (output += d));
  const stop = () => killTree(child);
  process.on("exit", stop);

  const deadline = Date.now() + 5 * 60_000; // first run compiles the Rust worker
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`wrangler dev exited early:\n${output}`);
    try {
      const res = await fetch(`${origin}/api/v1/health`);
      if (res.status === 200 || res.status === 503) break;
    } catch {
      /* not up yet */
    }
    await new Promise((r) => setTimeout(r, 1000));
  }
  if (Date.now() >= deadline) {
    stop();
    throw new Error(`wrangler dev did not start:\n${output}`);
  }
  return { baseUrl: origin, origin, rpId: "localhost", setupToken, stop, logs: () => output };
}
