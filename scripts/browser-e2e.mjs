// Browser end-to-end test against an isolated `wrangler dev` (fresh D1/R2),
// in headless Chrome/Edge with a virtual passkey authenticator:
//   setup → app shell → upload a file → settings → sign out → passkey sign-in
//   → service worker → server stopped → offline reload with cached session.
//
//   npm run build && npm run test:browser
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { launch, waitFor } from "./lib/cdp.mjs";
import { startDevServer } from "./lib/devserver.mjs";

const SHOTS = process.env.SCREENSHOT_DIR; // optional: save screenshots for design review
const shot = async (name, opts) => SHOTS && page.screenshot(join(SHOTS, `${name}.png`), opts);

let failures = 0;
async function step(name, fn) {
  try {
    await fn();
    console.log(`  ok   ${name}`);
  } catch (e) {
    failures++;
    const text = await page.text().catch(() => "(page unavailable)");
    console.log(`  FAIL ${name}\n       ${e.message}\n       page: ${text.replace(/\s+/g, " ").slice(0, 300)}`);
    throw e; // later steps depend on earlier ones
  }
}

console.log("starting isolated wrangler dev…");
const server = await startDevServer({ port: 8791, name: "browser" });
const { page, close } = await launch();
console.log(`browser e2e: ${server.origin} in ${page.name}`);

try {
  await page.addVirtualAuthenticator();
  await page.navigate(server.origin);

  await step("first run shows setup; passkey setup signs the owner in", async () => {
    await page.waitForText("first-run setup".toUpperCase());
    await shot("setup", { width: 390, height: 760 });
    await page.fill("your name", "Browser Owner");
    await page.fill("setup token", server.setupToken);
    await page.click("create passkey");
    await page.waitForText("Good to see you, Browser");
  });

  await step("health shows database and storage ok", async () => {
    await page.waitForText("[ ok ] schema v");
  });

  await step("Files: upload lists the file", async () => {
    const dir = mkdtempSync(join(tmpdir(), "directory-upload-"));
    const file = join(dir, "hello e2e.txt");
    writeFileSync(file, "uploaded from the browser e2e test");
    await page.hash("/files");
    await page.waitForText("Choose a file to upload");
    await page.setFile('input[type="file"]', file);
    await page.waitForText("Uploaded uploads/hello-e2e.txt");
  });

  await step("Tasks: quick add, subtask, complete, filters", async () => {
    await page.hash("/tasks");
    await page.waitForText("Add a task");
    await page.fill("New task", "Write the quarterly report");
    await page.click("add");
    await page.waitForText("Write the quarterly report");
    await page.click("Write the quarterly report");
    await page.fill("New subtask", "Gather numbers");
    await page.evaluate(`document.querySelector('input[aria-label="New subtask"]').form.requestSubmit()`);
    await page.waitForText("0/1 subtasks");
    await page.click("Complete “Gather numbers”");
    await page.waitForText("1/1 subtasks");
    await shot("tasks-desktop");
    await shot("tasks-phone", { width: 390, height: 844 });
    await page.click("Complete “Write the quarterly report”");
    await page.waitForText("Nothing open");
    await page.click("Done");
    await page.waitForText("Write the quarterly report");
    // Persisted server-side, not just in UI state.
    const tasks = await page.evaluate(`fetch("/api/v1/tasks").then((r) => r.json())`);
    const parent = tasks.tasks.find((t) => t.title === "Write the quarterly report");
    if (!parent || parent.status !== "done" || !tasks.tasks.some((t) => t.parent_id === parent.id && t.status === "done")) {
      throw new Error(`server state: ${JSON.stringify(tasks)}`);
    }
  });

  await step("Projects: create, add a task from the project, progress updates", async () => {
    await page.hash("/projects");
    await page.waitForText("No active projects");
    await page.fill("New project", "Garden");
    await page.click("create");
    await page.waitForText("0/0 done");
    await page.click("Garden");
    await page.click("view tasks");
    await page.waitForText("Add a task");
    await page.fill("New task", "Plant tomatoes");
    await page.click("add");
    await page.waitForText("Plant tomatoes");
    await page.click("Complete “Plant tomatoes”");
    await page.hash("/projects");
    await page.waitForText("1/1 done");
    await shot("projects");
  });

  await step("Settings lists one passkey; device token is shown once", async () => {
    await page.hash("/settings");
    await page.waitForText("PASSKEYS · 1");
    await page.fill("Token name", "E2E device");
    await page.click("create");
    await page.waitForText("won't be shown again");
    await shot("settings");
    const secret = await page.evaluate(`document.querySelector("code.select-all")?.innerText`);
    if (!secret?.startsWith("dir_")) throw new Error(`secret: ${secret}`);
    const r = await fetch(`${server.origin}/api/v1/storage/objects`, { headers: { Authorization: `Bearer ${secret}` } });
    const body = await r.json();
    if (!body.objects?.some((o) => o.key === "uploads/hello-e2e.txt")) throw new Error(`token list: ${JSON.stringify(body)}`);
  });

  await step("sign out returns to the sign-in screen", async () => {
    await page.click("sign out");
    await page.waitForText("sign in with passkey".toUpperCase());
  });

  await step("passkey sign-in restores the session", async () => {
    await page.click("sign in with passkey");
    // The route is preserved across sign-in: back on Settings.
    await page.waitForText("Signed in as Browser Owner");
  });

  await step("service worker controls the page", async () => {
    await waitFor("service worker", () => page.evaluate("!!navigator.serviceWorker.controller"), 30_000);
  });

  await step("offline: with the server stopped, the app opens from cache with the saved session", async () => {
    await page.evaluate(`new Promise((res, rej) => {
      const r = indexedDB.open("directory");
      r.onerror = () => rej(r.error);
      r.onsuccess = () => {
        const tx = r.result.transaction("notes", "readwrite");
        tx.objectStore("notes").put({ id: "e2e", body: "written online", createdAt: 1, updatedAt: Date.now() });
        tx.oncomplete = () => res(true);
      };
    })`);
    server.stop();
    await waitFor("server down", async () => {
      try {
        await fetch(server.origin);
        return false;
      } catch {
        return true;
      }
    });
    await page.reload();
    await page.waitForText("session saved on this device");
    await page.hash("/notes");
    await page.waitForText("written online");
  });
} catch {
  // reported by step()
} finally {
  await close();
  server.stop();
  if (failures) console.log(`\n--- wrangler output (tail) ---\n${server.logs().split("\n").slice(-30).join("\n")}`);
}

console.log(failures ? `\n${failures} step(s) failed` : "\nall steps passed");
process.exit(failures ? 1 : 0);
