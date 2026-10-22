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
    await page.fill("your name", "Browser Owner");
    await page.fill("setup token", server.setupToken);
    await page.click("create passkey");
    await page.waitForText("Good to see you, Browser");
  });

  await step("health shows schema v2", async () => {
    await page.waitForText("schema v2");
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

  await step("Settings lists one passkey; device token is shown once", async () => {
    await page.hash("/settings");
    await page.waitForText("PASSKEYS · 1");
    await page.fill("Token name", "E2E device");
    await page.click("create");
    await page.waitForText("won't be shown again");
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
