// Browser end-to-end test against an isolated `wrangler dev` (fresh D1/R2),
// in headless Chrome/Edge with a virtual passkey authenticator:
//   setup → app shell → upload a file → settings → sign out → passkey sign-in
//   → service worker → server stopped → offline reload with cached session.
//
//   npm run build && npm run test:browser
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { audit } from "./lib/a11y.mjs";
import { launch, waitFor } from "./lib/cdp.mjs";
import { startDevServer } from "./lib/devserver.mjs";

const SHOTS = process.env.SCREENSHOT_DIR; // optional: save screenshots for design review
const shot = async (name, opts) => SHOTS && page.screenshot(join(SHOTS, `${name}.png`), opts);

/** The browser's local date (YYYY-MM-DD), as the journal uses it. */
const localToday = () => {
  const d = new Date();
  const p = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
};

/** A zip with stored (uncompressed) entries, e.g. a test ePub. CRCs are 0; readers here don't check them. */
function storedZip(files) {
  const parts = [];
  const central = [];
  let offset = 0;
  for (const [name, content] of Object.entries(files)) {
    const n = Buffer.from(name);
    const data = Buffer.from(content);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt32LE(data.length, 18);
    local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(n.length, 26);
    parts.push(local, n, data);
    const c = Buffer.alloc(46);
    c.writeUInt32LE(0x02014b50, 0);
    c.writeUInt32LE(data.length, 20);
    c.writeUInt32LE(data.length, 24);
    c.writeUInt16LE(n.length, 28);
    c.writeUInt32LE(offset, 42);
    central.push(c, n);
    offset += 30 + n.length + data.length;
  }
  const dir = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(Object.keys(files).length, 8);
  end.writeUInt16LE(Object.keys(files).length, 10);
  end.writeUInt32LE(dir.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...parts, dir, end]);
}

/** A quiet 8 kHz mono 16-bit WAV of `seconds`, enough for the browser to play. */
function wav(seconds) {
  const rate = 8000;
  const samples = rate * seconds;
  const buf = Buffer.alloc(44 + samples * 2);
  buf.write("RIFF", 0);
  buf.writeUInt32LE(36 + samples * 2, 4);
  buf.write("WAVEfmt ", 8);
  buf.writeUInt32LE(16, 16);
  buf.writeUInt16LE(1, 20); // PCM
  buf.writeUInt16LE(1, 22); // mono
  buf.writeUInt32LE(rate, 24);
  buf.writeUInt32LE(rate * 2, 28);
  buf.writeUInt16LE(2, 32);
  buf.writeUInt16LE(16, 34);
  buf.write("data", 36);
  buf.writeUInt32LE(samples * 2, 40);
  for (let i = 0; i < samples; i++) buf.writeInt16LE(Math.round(800 * Math.sin((i * 2 * Math.PI * 440) / rate)), 44 + i * 2);
  return buf;
}

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

  await step("Notes: legacy device notes migrate; new note autosaves to the server", async () => {
    // A Sprint 00 Scratchpad note that only exists in this browser's IndexedDB.
    await page.evaluate(`new Promise((res, rej) => {
      const r = indexedDB.open("directory");
      r.onerror = () => rej(r.error);
      r.onsuccess = () => {
        const tx = r.result.transaction("notes", "readwrite");
        tx.objectStore("notes").put({ id: "legacy", body: "Legacy scratch\\nfrom sprint 00", createdAt: 1, updatedAt: 1 });
        tx.oncomplete = () => { r.result.close(); res(true); }; // a stray open connection would block deletes
      };
    })`);
    await page.hash("/notes");
    await page.waitForText("Moved 1 note from this device");
    await page.waitForText("Legacy scratch");
    await page.click("+ new");
    await page.waitForText("Untitled");
    await page.fill("Note body", "Meeting notes\nAgenda: launch plan");
    await page.waitForText("SAVED".toLowerCase());
    await page.waitForText("Agenda: launch plan");
    const notes = await page.evaluate(`fetch("/api/v1/notes").then((r) => r.json())`);
    const titles = notes.notes.map((n) => n.title);
    if (!titles.includes("Meeting notes") || !titles.includes("Legacy scratch")) throw new Error(`server notes: ${titles}`);
    await shot("notes");
  });

  await step("Wiki: a [[link]] to a missing note creates it; backlinks lead back", async () => {
    await page.fill("Note body", "Meeting notes\nAgenda: launch plan\nSee [[Launch checklist]]");
    await page.waitForText("+ create");
    await waitFor("note saved", () => page.evaluate(`[...document.querySelectorAll('[role=status]')].some((s) => s.textContent === "saved")`));
    await page.click("Create note “Launch checklist”");
    await waitFor("new note open", () => page.evaluate(`document.querySelector('input[aria-label="Note title"]')?.value === "Launch checklist"`));
    await page.waitForText("← Meeting notes");
    await page.click("Open backlink “Meeting notes”");
    await waitFor("back on the linking note", () => page.evaluate(`document.querySelector('textarea[aria-label="Note body"]')?.value.includes("[[Launch checklist]]")`));
    await waitFor("link resolved", async () => !(await page.text()).includes("+ create"));
    await shot("wiki");
  });

  await step("Journal: today's entry with mood autosaves; day navigation", async () => {
    await page.hash("/journal");
    await page.waitForText("No entries this month");
    await page.click("good");
    await page.fill("Journal entry", "Walked by the river.");
    await page.waitForText("Walked by the river.");
    await waitFor("journal saved", async () => (await page.text()).toLowerCase().includes("saved") && !(await page.text()).includes("unsaved"));
    const today = await page.evaluate(`(() => { const d = new Date(); const p = (n) => String(n).padStart(2, "0"); return d.getFullYear() + "-" + p(d.getMonth() + 1) + "-" + p(d.getDate()); })()`);
    const entry = await page.evaluate(`fetch("/api/v1/journal/${today}").then((r) => r.json())`);
    if (entry.mood !== 4 || entry.body !== "Walked by the river.") throw new Error(`server entry: ${JSON.stringify(entry)}`);
    await page.click("Previous day");
    await page.waitForText("NEW".toLowerCase());
    await shot("journal");
  });

  await step("Inbox: capture from Home, triage into a note", async () => {
    await page.hash("/home");
    await page.fill("Capture to inbox", "Gift ideas for Mum");
    await page.click("capture");
    await page.waitForText("1 in inbox");
    await page.click("1 in inbox");
    await page.waitForText("Gift ideas for Mum");
    await shot("inbox");
    await page.click("→ note");
    await page.waitForText("Saved as a note.");
    await page.waitForText("Inbox zero");
    const notes = await page.evaluate(`fetch("/api/v1/notes").then((r) => r.json())`);
    if (!notes.notes.some((n) => n.title === "Gift ideas for Mum")) throw new Error("note not created");
  });

  await step("Files: upload lists the file", async () => {
    const dir = mkdtempSync(join(tmpdir(), "directory-upload-"));
    const file = join(dir, "hello e2e.txt");
    writeFileSync(file, "uploaded from the browser e2e test");
    await page.hash("/files");
    await page.waitForText("No files yet");
    await page.click("new folder");
    await page.fill("Folder name", "uploads");
    await page.click("create");
    await page.waitForText("This folder is empty");
    await page.setFile('input[type="file"]', file);
    await page.waitForText("Uploaded 1 file.");
    await page.waitForText("hello e2e.txt");
    const text = await page.evaluate(`fetch(document.querySelector('a[download="hello e2e.txt"]').href).then((r) => r.text())`);
    if (text !== "uploaded from the browser e2e test") throw new Error(`download: ${text}`);
  });

  await step("Library: a PDF from Files appears; reading progress is saved", async () => {
    await page.evaluate(`fetch("/api/v1/files?folder=books&name=Dune.pdf", {
      method: "POST", headers: { "Content-Type": "application/pdf" }, body: "%PDF-1.4 e2e",
    }).then((r) => { if (!r.ok) throw new Error("upload " + r.status); })`);
    await page.hash("/library");
    await page.waitForText("Dune");
    await page.click("progress");
    await page.fill("Current page", "50");
    await page.fill("Total pages", "200");
    await page.click("save");
    await page.waitForText("p. 50 of 200");
    await page.waitForText("CURRENTLY READING");
    const lib = await page.evaluate(`fetch("/api/v1/library").then((r) => r.json())`);
    const item = lib.items.find((i) => i.file.name === "Dune.pdf");
    if (item?.progress?.percent !== 25 || item.progress.status !== "reading") throw new Error(`progress: ${JSON.stringify(item)}`);
    await shot("library");
  });

  await step("Library: an ePub opens in the reader, chapters turn, and progress is saved", async () => {
    const epub = storedZip({
      mimetype: "application/epub+zip",
      "META-INF/container.xml": `<container><rootfiles><rootfile full-path="book.opf"/></rootfiles></container>`,
      "book.opf": `<package><metadata><dc:title>Tiny Tale</dc:title></metadata><manifest><item id="a" href="a.xhtml" media-type="application/xhtml+xml"/><item id="b" href="b.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="a"/><itemref idref="b"/></spine></package>`,
      "a.xhtml": `<html><body><h1>Chapter one text</h1><script>window.parent.pwned = true</script></body></html>`,
      "b.xhtml": `<html><body><p>Chapter two text</p></body></html>`,
    });
    const id = await page.evaluate(`fetch("/api/v1/files?folder=books&name=Tiny.epub", { method: "POST", headers: { "Content-Type": "application/epub+zip", Origin: location.origin }, body: Uint8Array.from(atob("${epub.toString("base64")}"), (c) => c.charCodeAt(0)) }).then((r) => r.json()).then((f) => f.id)`);
    if (!id) throw new Error("ePub upload failed");
    await page.hash("/home"); // remount Library so it loads the new file
    await page.hash("/library");
    await page.waitForText("Tiny");
    await page.click("read");
    await page.waitForText("Chapter 1 of 2");
    const srcdoc = () => page.evaluate(`document.querySelector("iframe")?.getAttribute("srcdoc") ?? ""`);
    const first = await srcdoc();
    if (!first.includes("Chapter one text") || /<script/i.test(first)) throw new Error(`chapter 1: ${first.slice(0, 200)}`);
    if (await page.evaluate(`document.querySelector("iframe").getAttribute("sandbox")`) !== "") throw new Error("reader iframe must be fully sandboxed");
    await page.click("Next chapter");
    await page.waitForText("Chapter 2 of 2");
    if (!(await srcdoc()).includes("Chapter two text")) throw new Error("chapter 2 not shown");
    await waitFor("progress saved", async () => {
      const lib = await page.evaluate(`fetch("/api/v1/library").then((r) => r.json())`);
      const p = lib.items.find((i) => i.file.id === id)?.progress;
      return p && p.current_page === 2 && p.total_pages === 2 && p.percent === 100;
    });
    if (await page.evaluate(`window.pwned === true`)) throw new Error("ePub script ran");
    await page.click("Close the reader");

    // Offline: a saved book opens from the device; progress queues and syncs.
    await page.click("Save “Tiny” for offline");
    await page.waitForText("saved offline");
    await page.send("Network.enable", {});
    const net = (offline) => page.send("Network.emulateNetworkConditions", { offline, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
    await net(true);
    await waitFor("browser offline", () => page.evaluate("!navigator.onLine"));
    await page.waitForText("books saved on this device can be read");
    await page.click("read");
    await page.waitForText("Chapter 2 of 2"); // resumes where it was left
    await page.click("Previous chapter");
    await page.waitForText("Chapter 1 of 2");
    await page.waitForText("1 pending");
    await net(false);
    await waitFor("offline progress synced", async () => {
      const lib = await page.evaluate(`fetch("/api/v1/library").then((r) => r.json())`);
      // Finished books stay on their last page server-side; the position moves.
      return lib.items.find((i) => i.file.id === id)?.progress?.position === "epub:a.xhtml";
    }, 30_000);
    await page.click("Close the reader");
    await page.click("Remove “Tiny” from this device");
    await page.evaluate(`fetch("/api/v1/files/${id}", { method: "DELETE", headers: { Origin: location.origin } })`);
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

  await step("Tasks plan: an estimate saves; 'suggest' fills the free minutes with reasons (D-041)", async () => {
    await page.hash("/tasks");
    // The previous step left the "Done" filter and a task open.
    await page.click("Close task details");
    await page.evaluate(`[...document.querySelectorAll("[role=tab]")].find((t) => t.innerText.trim().toLowerCase() === "open").click()`);
    await page.waitForText("Nothing open");
    await page.fill("New task", "Plan e2e quick");
    await page.click("add");
    await page.waitForText("Plan e2e quick");
    await page.click("Plan e2e quick");
    await page.fill("estimate (minutes)", "15");
    await page.evaluate(`document.querySelector('input[placeholder="e.g. 30"]').dispatchEvent(new FocusEvent("focusout", { bubbles: true }))`);
    for (let i = 0; ; i++) {
      const all = await page.evaluate(`fetch("/api/v1/tasks?status=open").then((r) => r.json())`);
      if (all.tasks.some((t) => t.title === "Plan e2e quick" && t.estimate_minutes === 15)) break;
      if (i > 50) throw new Error(`estimate not saved: ${JSON.stringify(all)}`);
      await new Promise((r) => setTimeout(r, 200));
    }
    await page.click("Close task details");
    await page.fill("minutes free", "20");
    await page.click("suggest");
    await page.waitForText("1. Plan e2e quick");
    await page.waitForText("15 min of 20 min planned");
    await shot("tasks-plan");
    await page.click("Complete “Plan e2e quick”");
  });

  await step("Links: link a note to a task via the picker, follow the deep link, see the back-link", async () => {
    await page.hash("/notes");
    await page.click("Meeting notes");
    await page.waitForText("Nothing linked yet");
    await page.click("+ link");
    await page.fill("Link to", "quarterly");
    await page.click("Write the quarterly report");
    await waitFor("link saved", () =>
      page.evaluate(`[...document.querySelectorAll("a")].some((a) => a.innerText === "Write the quarterly report")`),
    );
    await page.evaluate(`[...document.querySelectorAll("a")].find((a) => a.innerText === "Write the quarterly report").click()`);
    await waitFor("task detail via deep link", () =>
      page.evaluate(`[...document.querySelectorAll("input")].some((i) => i.value === "Write the quarterly report")`),
    );
    await page.waitForText("Meeting notes");
  });

  await step("Offline edits: a task, an inbox capture, and a project made offline are queued, then synced", async () => {
    await page.hash("/tasks");
    await waitFor("tasks loaded", () => page.evaluate(`!!document.querySelector('input[aria-label="New task"]')`));
    await page.send("Network.enable", {});
    const net = (offline) => page.send("Network.emulateNetworkConditions", { offline, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
    await net(true);
    await waitFor("browser offline", () => page.evaluate("!navigator.onLine"));
    await page.waitForText("changes are saved on this device");
    await page.fill("New task", "Written offline");
    await page.click("add");
    await page.waitForText("Written offline");
    await page.waitForText("1 pending");
    await page.hash("/inbox");
    await page.waitForText("captures and sorting are saved on this device");
    await page.fill("Capture to inbox", "Idea from the train");
    await page.click("capture");
    await page.waitForText("Idea from the train");
    await page.waitForText("2 pending");
    await page.hash("/projects");
    await page.waitForText("changes are saved on this device");
    await page.fill("New project", "Planned offline");
    await page.click("create");
    await page.waitForText("Planned offline");
    await page.waitForText("3 pending");
    // Notes and journal: edits stay on the device as one coalesced save per document.
    await page.hash("/notes");
    await page.click("Meeting notes");
    await page.fill("Note body", "Meeting notes\nAgenda: launch plan\nAdded offline");
    await page.waitForText("saved on this device");
    await page.hash("/journal");
    await page.fill("Journal entry", "Walked by the river. Rain on the way back.");
    await page.waitForText("saved on this device");
    await page.fill("Journal entry", "Walked by the river. Rain on the way back, then sun.");
    await waitFor("journal edit queued", async () => (await page.text()).includes("5 pending") && (await page.text()).includes("saved on this device"));
    // Meanwhile another device changes today's entry.
    const { cookies } = await page.send("Network.getCookies", { urls: [server.origin] });
    const elsewhere = await fetch(`${server.origin}/api/v1/journal/${localToday()}`, {
      method: "PUT",
      headers: { cookie: cookies.map((c) => `${c.name}=${c.value}`).join("; "), origin: server.origin, "content-type": "application/json" },
      body: JSON.stringify({ body: "Written on my phone" }),
    });
    if (!elsewhere.ok) throw new Error(`edit from elsewhere → ${elsewhere.status}`);
    await net(false);
    // The sync chip reads "N pending" while queued and "syncing…" while replaying.
    await waitFor("synced", async () => !/pending|syncing…/i.test(await page.text()), 30_000);
    const tasks = await page.evaluate(`fetch("/api/v1/tasks").then((r) => r.json())`);
    if (!tasks.tasks.some((t) => t.title === "Written offline")) throw new Error("offline task not on server");
    const inbox = await page.evaluate(`fetch("/api/v1/inbox").then((r) => r.json())`);
    if (!inbox.items.some((i) => i.text === "Idea from the train")) throw new Error("offline capture not on server");
    const projects = await page.evaluate(`fetch("/api/v1/projects").then((r) => r.json())`);
    const planned = projects.projects.find((p) => p.name === "Planned offline");
    if (!planned) throw new Error("offline project not on server");
    const notes = await page.evaluate(`fetch("/api/v1/notes").then((r) => r.json())`);
    if (!notes.notes.some((n) => n.excerpt.includes("Added offline"))) throw new Error("offline note edit not on server");

    // The journal edit conflicted: the open editor offers both versions; keep mine.
    await page.waitForText("conflicts with a change made elsewhere");
    await page.waitForText("changed on another device");
    await page.click("keep mine");
    await waitFor("conflict resolved", async () => !(await page.text()).includes("changed on another device"));
    const day = await page.evaluate(`fetch("/api/v1/journal/${localToday()}").then((r) => r.json())`);
    if (day.body !== "Walked by the river. Rain on the way back, then sun.") throw new Error(`journal after keep mine: ${day.body}`);
    await page.click("dismiss");

    // Leave the inbox and projects empty for later steps.
    await page.hash("/projects");
    await page.click("Planned offline");
    await page.click("delete");
    await page.click("confirm delete");
    await waitFor("project deleted", async () => !(await page.text()).includes("Planned offline"));
    await page.evaluate(`fetch("/api/v1/inbox").then((r) => r.json()).then((b) => Promise.all(b.items.map((i) => fetch("/api/v1/inbox/" + i.id, { method: "DELETE", headers: { Origin: location.origin } }))))`);
  });

  await step("Journal offline: delete a day, write it again, and both sync in order (D-035)", async () => {
    const yesterday = await page.evaluate(`(() => { const d = new Date(); d.setDate(d.getDate() - 1); const p = (n) => String(n).padStart(2, "0"); return d.getFullYear() + "-" + p(d.getMonth() + 1) + "-" + p(d.getDate()); })()`);
    const serverDay = () => page.evaluate(`fetch("/api/v1/journal/${yesterday}").then((r) => (r.ok ? r.json() : null))`);
    await page.hash("/journal");
    await page.click("Previous day");
    await page.fill("Journal entry", "Yesterday, first version");
    await waitFor("first version saved", async () => (await serverDay())?.body === "Yesterday, first version");
    await page.send("Network.enable", {});
    const net = (offline) => page.send("Network.emulateNetworkConditions", { offline, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
    await net(true);
    await waitFor("browser offline", () => page.evaluate("!navigator.onLine"));
    await page.click("delete entry");
    await page.click("confirm delete");
    await page.waitForText("1 pending");
    await waitFor("day emptied", () => page.evaluate(`document.querySelector('textarea[aria-label="Journal entry"]')?.value === ""`));
    await page.fill("Journal entry", "Yesterday, rewritten");
    await page.waitForText("2 pending");
    await net(false);
    await waitFor("synced", async () => !/pending|syncing…/i.test(await page.text()), 30_000);
    const day = await serverDay();
    if (day?.body !== "Yesterday, rewritten") throw new Error(`after sync: ${JSON.stringify(day)}`);
    await page.evaluate(`fetch("/api/v1/journal/${yesterday}", { method: "DELETE", headers: { Origin: location.origin } })`);
  });

  await step("Inbox offline: capture and sort into a task without a connection, then it syncs in order (D-036)", async () => {
    await page.hash("/inbox");
    // Wait for the server's (empty) inbox: the cached copy can still list items deleted by API calls above.
    await page.waitForText("Inbox zero");
    await page.send("Network.enable", {});
    const net = (offline) => page.send("Network.emulateNetworkConditions", { offline, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
    await net(true);
    await waitFor("browser offline", () => page.evaluate("!navigator.onLine"));
    await page.waitForText("captures and sorting are saved on this device");
    await page.fill("Capture to inbox", "Buy stamps offline");
    await page.click("capture");
    await page.waitForText("Buy stamps offline");
    await page.click("→ task");
    await page.waitForText("It becomes a task when you reconnect.");
    await page.waitForText("2 pending");
    await net(false);
    await waitFor("synced", async () => !/pending|syncing…/i.test(await page.text()), 30_000);
    const tasks = await page.evaluate(`fetch("/api/v1/tasks").then((r) => r.json())`);
    const made = tasks.tasks.find((t) => t.title === "Buy stamps offline");
    if (!made) throw new Error("triaged task not on server");
    const inbox = await page.evaluate(`fetch("/api/v1/inbox").then((r) => r.json())`);
    if (inbox.items.some((i) => i.text === "Buy stamps offline")) throw new Error("item still in the inbox");
    await page.evaluate(`fetch("/api/v1/tasks/${made.id}", { method: "DELETE", headers: { Origin: location.origin } })`);
  });

  await step("Links offline: unlink and relink without a connection; both replay in order (D-037)", async () => {
    const notes = await page.evaluate(`fetch("/api/v1/notes").then((r) => r.json())`);
    const meeting = notes.notes.find((n) => n.title === "Meeting notes");
    const serverLinks = () => page.evaluate(`fetch("/api/v1/links?kind=note&id=${meeting.id}").then((r) => r.json()).then((b) => b.links)`);
    if (!(await serverLinks()).some((l) => l.title === "Write the quarterly report")) throw new Error("expected the link from the Links step");
    await page.hash("/notes");
    await page.click("Meeting notes");
    await waitFor("links listed", () => page.evaluate(`[...document.querySelectorAll("a")].some((a) => a.innerText === "Write the quarterly report")`));
    await page.send("Network.enable", {});
    const net = (offline) => page.send("Network.emulateNetworkConditions", { offline, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
    await net(true);
    await waitFor("browser offline", () => page.evaluate("!navigator.onLine"));
    await page.click("Unlink “Write the quarterly report”");
    await page.waitForText("1 pending");
    await page.click("+ link");
    await page.fill("Link to", "quarterly");
    await page.click("Write the quarterly report"); // offered from the copy on this device
    await page.waitForText("2 pending");
    await waitFor("relinked locally", () => page.evaluate(`[...document.querySelectorAll("a")].some((a) => a.innerText === "Write the quarterly report")`));
    await net(false);
    await waitFor("synced", async () => !/pending|syncing…/i.test(await page.text()), 30_000);
    const after = await serverLinks();
    if (after.filter((l) => l.title === "Write the quarterly report").length !== 1) throw new Error(`links after sync: ${JSON.stringify(after)}`);
  });

  await step("Habits: add one, check off today, streak shows and reaches the server", async () => {
    await page.hash("/habits");
    await page.waitForText("No habits yet");
    await page.fill("New habit", "Stretch");
    await page.click("add");
    await page.waitForText("Every day · no streak yet");
    await page.click("(today)");
    await page.waitForText("1-day streak");
    await page.waitForText("1/1 TODAY"); // panel titles are uppercased by CSS
    const today = localToday();
    const list = await page.evaluate(`fetch("/api/v1/habits?today=${today}").then((r) => r.json())`);
    if (!list.habits.some((h) => h.name === "Stretch" && h.checkins.includes(today) && h.streak === 1)) throw new Error(`server habits: ${JSON.stringify(list)}`);
    await shot("habits");
  });

  await step("Calendar: add an event on today, it shows in the month and day, then delete it", async () => {
    await page.hash("/calendar");
    await page.waitForText("Nothing scheduled.");
    await page.click("+ event");
    await page.fill("Event title", "Dentist");
    await page.click("add event");
    await waitFor("event in today's cell", () => page.evaluate(`[...document.querySelectorAll("button[aria-current=date]")].some((b) => b.getAttribute("aria-label").endsWith("1 item"))`));
    await page.waitForText("All day");
    const events = await page.evaluate(`fetch("/api/v1/events").then((r) => r.json())`);
    if (!events.events.some((e) => e.title === "Dentist" && e.start_date === localToday())) throw new Error(`server events: ${JSON.stringify(events)}`);
    await shot("calendar");
    await page.click("Edit “Dentist”");
    await page.click("delete");
    await page.click("confirm delete");
    await page.waitForText("Nothing scheduled.");
  });

  await step("Dashboard: widgets show today's habit, layout changes persist across reloads", async () => {
    await page.hash("/home");
    await page.waitForText("Stretch"); // habits widget, from the Habits step
    await page.waitForText("Nothing scheduled or due today.");
    await page.click("customize");
    await page.click("Hide Recent notes");
    await page.click("Move Habits up");
    await page.click("done");
    await waitFor("notes widget hidden", async () => !(await page.text()).includes("RECENT NOTES"));
    // Both edits reach the server, in order (they are sent one at a time).
    await waitFor("server layout", async () => (await page.evaluate(`fetch("/api/v1/preferences").then((r) => r.json())`)).dashboard?.widgets.join() === "capture,habits,today,status,modules");
    await page.reload();
    await page.waitForText("Stretch");
    if ((await page.text()).includes("RECENT NOTES")) throw new Error("layout not kept after reload");
    await shot("dashboard");
    await page.click("customize");
    await page.click("reset to default");
    await page.click("done");
    await page.waitForText("RECENT NOTES");
  });

  await step("Automations: add a rule in JARVIS, it runs when a task is completed, then delete it", async () => {
    await page.hash("/jarvis");
    await page.waitForText("No automations yet.");
    await page.fill("Automation name", "Log wins");
    await page.fill("Automation command", "capture finished {title}");
    await page.click("add automation");
    await page.waitForText("When any task is completed:");
    // Complete a task the way the Tasks screen does (with today's local date).
    const today = localToday();
    await page.evaluate(`(async () => {
      const h = { "content-type": "application/json", Origin: location.origin };
      const t = await (await fetch("/api/v1/tasks", { method: "POST", headers: h, body: JSON.stringify({ title: "Water ferns" }) })).json();
      await fetch("/api/v1/tasks/" + t.id + "?today=${today}", { method: "PATCH", headers: h, body: JSON.stringify({ status: "done" }) });
    })()`);
    const inbox = await page.evaluate(`fetch("/api/v1/inbox").then((r) => r.json())`);
    if (!inbox.items.some((i) => i.text === "finished Water ferns")) throw new Error(`inbox: ${JSON.stringify(inbox)}`);
    await page.reload();
    await page.waitForText("· ok · Captured to your inbox.");
    await shot("automations");
    await page.click("Delete “Log wins”");
    await page.click("confirm delete");
    await page.waitForText("No automations yet.");
    await page.evaluate(`fetch("/api/v1/inbox").then((r) => r.json()).then((b) => Promise.all(b.items.map((i) => fetch("/api/v1/inbox/" + i.id, { method: "DELETE", headers: { Origin: location.origin } }))))`);
  });

  await step("Music: upload a track, play it, save it offline, add it to a playlist", async () => {
    const dir = mkdtempSync(join(tmpdir(), "directory-music-"));
    const file = join(dir, "Tone.wav");
    writeFileSync(file, wav(2));
    await page.hash("/music");
    await page.waitForText("No music yet.");
    await page.setFile('input[aria-label="Upload audio"]', file);
    await page.click("Play “Tone”");
    await waitFor("audio playing", () => page.evaluate(`(() => { const a = document.querySelector("audio"); return !!a && a.currentTime > 0 && !a.paused; })()`), 20_000);
    await page.waitForText("1/1");
    await page.click("Pause");
    await page.click("Save “Tone” for offline");
    await page.waitForText("saved offline");
    // Saved tracks play from the device (a blob: URL the page CSP must allow).
    await page.click("Close player");
    await page.click("Play “Tone”");
    await waitFor("saved copy playing", () => page.evaluate(`(() => { const a = document.querySelector("audio"); return !!a && a.currentSrc.startsWith("blob:") && a.currentTime > 0 && !a.paused; })()`), 20_000);
    await page.click("Pause");
    await page.fill("New playlist", "Focus");
    await page.click("create");
    await page.waitForText("Empty. Add tracks");
    await page.evaluate(`(() => {
      const s = document.querySelector('select[aria-label="Add “Tone” to a playlist"]');
      const set = Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value").set;
      set.call(s, s.options[1].value);
      s.dispatchEvent(new Event("change", { bubbles: true }));
    })()`);
    await waitFor("track in playlist", () => page.evaluate(`!!document.querySelector('button[aria-label="Remove “Tone” from the playlist"]')`));
    const lib = await page.evaluate(`fetch("/api/v1/music").then((r) => r.json())`);
    const focus = lib.playlists.find((p) => p.name === "Focus");
    if (!focus || focus.tracks.length !== 1 || lib.tracks[0].name !== "Tone.wav") throw new Error(`music: ${JSON.stringify(lib)}`);
    await shot("music");
    await page.click("Close player");
  });

  await step("Restore: an export file adds missing records and reports what it did", async () => {
    const dir = mkdtempSync(join(tmpdir(), "directory-restore-"));
    const file = join(dir, "export.json");
    const note = { id: "5e".repeat(16), title: "", body: "Restored from a file", pinned: false, created_at: 1_700_000_000_000, updated_at: 1_700_000_000_000 };
    writeFileSync(file, JSON.stringify({ format: "directory-export", format_version: 1, notes: [note], files: [{}] }));
    await page.hash("/settings");
    await page.setFile('input[aria-label="Export file to restore"]', file);
    await page.waitForText("This file has 1 notes.");
    await page.waitForText("upload those again in Files");
    await page.click("restore");
    await page.waitForText("Restore finished.");
    await page.waitForText("Notes: 1 added, 0 already here");
    const n = await page.evaluate(`fetch("/api/v1/notes/${note.id}").then((r) => r.json())`);
    if (n.body !== note.body || n.created_at !== note.created_at) throw new Error(`restored note: ${JSON.stringify(n)}`);
    await page.evaluate(`fetch("/api/v1/notes/${note.id}", { method: "DELETE", headers: { Origin: location.origin } })`);
  });

  await step("Accessibility: on every module, controls have accessible names and the page has a heading", async () => {
    const problems = [];
    for (const m of ["home", "search", "inbox", "tasks", "projects", "calendar", "habits", "notes", "journal", "library", "music", "files", "jarvis", "settings"]) {
      await page.hash(`/${m}`);
      await waitFor(`${m} loaded`, async () => !/Loading/.test(await page.text()), 10_000).catch(() => {});
      for (const p of await audit(page)) problems.push(`${m}: ${p}`);
    }
    if (problems.length) throw new Error(`\n${[...new Set(problems)].join("\n")}`);
    // The audit itself must catch an unnamed control and a missing heading.
    await page.evaluate(`(() => { const b = document.createElement("button"); b.id = "a11y-probe"; document.body.append(b); })()`);
    const caught = await audit(page);
    await page.evaluate(`document.getElementById("a11y-probe").remove()`);
    if (!caught.some((p) => p.includes("a11y-probe"))) throw new Error(`audit missed an unnamed button: ${JSON.stringify(caught)}`);
    // Keyboard: the first Tab reaches "Skip to content", which moves focus to the main region.
    // (Reload so focus navigation starts at the top of the document.)
    await page.reload();
    await page.waitForText("Skip to content");
    for (const type of ["keyDown", "keyUp"]) await page.send("Input.dispatchKeyEvent", { type, key: "Tab", code: "Tab", windowsVirtualKeyCode: 9 });
    const first = await page.evaluate(`document.activeElement?.textContent ?? ""`);
    if (first.trim() !== "Skip to content") throw new Error(`first Tab focused: ${first}`);
    // `text` makes it a real key press, which activates the focused button.
    await page.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13, text: "\r" });
    await page.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
    await waitFor("main focused", () => page.evaluate(`document.activeElement?.tagName === "MAIN"`));
  });

  await step("Errors: a crashing module is contained and recoverable; errors reach Settings › diagnostics", async () => {
    // Make date formatting throw so the Notes list crashes while rendering.
    await page.evaluate(`(() => { window.__fmt = Date.prototype.toLocaleDateString; Date.prototype.toLocaleDateString = () => { throw new Error("e2e boundary probe"); }; })()`);
    await page.hash("/notes");
    await page.waitForText("This screen hit a problem");
    await page.waitForText("e2e boundary probe");
    if (!(await page.text()).includes("Settings")) throw new Error("the shell went down with the module");
    await page.evaluate(`Date.prototype.toLocaleDateString = window.__fmt`);
    await page.click("try again");
    await page.waitForText("Meeting notes"); // the list renders again
    // An unhandled promise rejection is reported too.
    await page.evaluate(`setTimeout(() => Promise.reject(new Error("e2e unhandled probe")), 0)`);
    await page.hash("/settings");
    await waitFor("reports listed", async () => {
      await page.click("refresh");
      const t = await page.text();
      return t.includes("e2e boundary probe") && t.includes("e2e unhandled probe");
    }, 15_000);
    await page.click("clear");
    await page.waitForText("No errors reported.");
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

  await step("Search: '/' opens search, results are highlighted and open the item", async () => {
    await page.hash("/home");
    await page.evaluate(`document.activeElement?.blur(); window.dispatchEvent(new KeyboardEvent("keydown", { key: "/" }))`);
    await waitFor("search module", () => page.evaluate(`location.hash.startsWith("#/search")`));
    await page.fill("Search", "quarterly");
    await page.waitForText("Write the quarterly report");
    const marked = await page.evaluate(`[...document.querySelectorAll("mark")].map((m) => m.innerText.toLowerCase())`);
    if (!marked.includes("quarterly")) throw new Error(`highlight: ${marked}`);
    await shot("search");
    await page.evaluate(`[...document.querySelectorAll("a")].find((a) => a.innerText.includes("Write the quarterly report")).click()`);
    await waitFor("task opened", () =>
      page.evaluate(`[...document.querySelectorAll("input")].some((i) => i.value === "Write the quarterly report")`),
    );
  });

  await step("JARVIS: command preview, approve, results link to items", async () => {
    await page.hash("/jarvis");
    await page.waitForText("No commands yet");
    await page.fill("Command", "dance wildly");
    await page.click("run");
    await page.waitForText("I don't know how to");
    await page.fill("Command", "add task Plan the offsite due tomorrow; journal started planning the offsite");
    await page.click("run");
    await page.waitForText("Create task “Plan the offsite” due");
    const before = await page.evaluate(`fetch("/api/v1/tasks").then((r) => r.json())`);
    if (before.tasks.some((t) => t.title === "Plan the offsite")) throw new Error("created before approval");
    await shot("jarvis-preview");
    await page.click("approve");
    await page.waitForText("Created task “Plan the offsite”.");
    await page.waitForText("EXECUTED"); // innerText reflects CSS text-transform
    const after = await page.evaluate(`fetch("/api/v1/tasks").then((r) => r.json())`);
    if (!after.tasks.some((t) => t.title === "Plan the offsite" && t.due_date)) throw new Error("task missing after approval");

    // Save it as a preset, then run the preset: it previews again and needs approval.
    await page.click("save as preset");
    await page.fill("Preset name", "Offsite kickoff");
    await page.click("save");
    await page.waitForText("▸ Offsite kickoff");
    await page.click("▸ Offsite kickoff");
    await page.waitForText("PENDING");
    await page.click("cancel");
    await page.waitForText("CANCELLED");
  });

  await step("Settings lists one passkey; device token is shown once", async () => {
    await page.hash("/settings");
    await page.waitForText("PASSKEYS · 1");
    await page.fill("Token name", "E2E device");
    await page.click("create");
    await page.waitForText("won't be shown again");
    await shot("settings");
    await page.click("export my data");
    await new Promise((r) => setTimeout(r, 1000));
    const alert = await page.evaluate(`document.querySelector('[role="alert"]')?.innerText ?? ""`);
    if (alert) throw new Error(`export error: ${alert}`);
    const exported = await page.evaluate(`fetch("/api/v1/export").then((r) => r.json())`);
    if (exported.format !== "directory-export" || !exported.notes.some((n) => n.body.includes("Agenda: launch plan"))) {
      throw new Error("export content");
    }
    const secret = await page.evaluate(`document.querySelector("code.select-all")?.innerText`);
    if (!secret?.startsWith("dir_")) throw new Error(`secret: ${secret}`);
    const r = await fetch(`${server.origin}/api/v1/files?folder=uploads`, { headers: { Authorization: `Bearer ${secret}` } });
    const body = await r.json();
    if (!body.files?.some((f) => f.name === "hello e2e.txt")) throw new Error(`token list: ${JSON.stringify(body)}`);

    // A JARVIS-only token (D-042): Settings shows the Shortcut URL; the token
    // works for /jarvis/ask and nothing else.
    await page.click("done");
    await page.fill("Token name", "E2E Shortcut");
    await page.evaluate(`(() => {
      const el = document.querySelector('select[aria-label="Token access"]');
      Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype, "value").set.call(el, "jarvis");
      el.dispatchEvent(new Event("change", { bubbles: true }));
    })()`);
    await page.click("create");
    await page.waitForText("/api/v1/jarvis/ask");
    const jarvisSecret = await page.evaluate(`document.querySelector("code.select-all")?.innerText`);
    const asked = await fetch(`${server.origin}/api/v1/jarvis/ask`, {
      method: "POST",
      headers: { Authorization: `Bearer ${jarvisSecret}`, "Content-Type": "application/json" },
      body: JSON.stringify({ text: "list tasks", today: "2026-10-21" }),
    });
    const spoken = await asked.json();
    if (spoken.status !== "executed" || typeof spoken.speech !== "string") throw new Error(`ask: ${JSON.stringify(spoken)}`);
    const refused = await fetch(`${server.origin}/api/v1/files?folder=uploads`, { headers: { Authorization: `Bearer ${jarvisSecret}` } });
    if (refused.status !== 403) throw new Error(`JARVIS-only token on files → ${refused.status}`);
    await page.waitForText("JARVIS only · created");
  });

  await step("sign out and forget this device: asks first, then deletes the browser's copy", async () => {
    const cached = `indexedDB.databases().then((dbs) => dbs.some((d) => d.name === "directory") && new Promise((res) => { const r = indexedDB.open("directory"); r.onsuccess = () => { const db = r.result; if (!db.objectStoreNames.contains("kv")) { db.close(); return res(false); } const g = db.transaction("kv").objectStore("kv").get("tasks:last"); g.onsuccess = () => { db.close(); res(g.result !== undefined); }; }; }))`;
    if (!(await page.evaluate(cached))) throw new Error("expected cached tasks before forgetting");
    await page.click("sign out and forget this device");
    await page.waitForText("Delete everything saved in this browser?");
    await page.click("sign out and forget");
    await page.waitForText("sign in with passkey".toUpperCase());
    if (await page.evaluate(cached)) throw new Error("cached tasks survived 'forget this device'");
  });

  await step("passkey sign-in restores the session", async () => {
    await page.click("sign in with passkey");
    // The route is preserved across sign-in: back on Settings.
    await page.waitForText("Signed in as Browser Owner");
    // The session is saved on the device again (the offline relaunch needs it).
    const kv = `indexedDB.databases().then((dbs) => !dbs.some((d) => d.name === "directory") ? {} : new Promise((res) => { const r = indexedDB.open("directory"); r.onsuccess = () => { const db = r.result; if (!db.objectStoreNames.contains("kv")) { db.close(); return res({}); } const tx = db.transaction("kv"); const k = tx.objectStore("kv").getAllKeys(); const s = tx.objectStore("kv").get("session:last"); tx.oncomplete = () => { db.close(); res({ keys: k.result, session: s.result }); }; }; }))`;
    await waitFor("session saved on the device", async () => !!(await page.evaluate(kv)).session?.user, 10_000).catch(async () => {
      throw new Error(`kv after sign-in: ${JSON.stringify(await page.evaluate(kv))}`);
    });
    // Rebuild the offline copies the last step relies on.
    await page.hash("/notes");
    await page.click("Meeting notes");
    await waitFor("note body cached", async () => (await page.evaluate(`document.querySelector('textarea[aria-label="Note body"]')?.value ?? ""`)).includes("Agenda: launch plan"));
    await page.hash("/tasks");
    await waitFor("tasks loaded", () => page.evaluate(`!!document.querySelector('input[aria-label="New task"]:not([disabled])')`));
    await page.hash("/files?folder=uploads");
    await page.waitForText("hello e2e.txt");
    await page.hash("/settings"); // the offline step expects to reopen on Settings
  });

  await step("service worker controls the page", async () => {
    await waitFor("service worker", () => page.evaluate("!!navigator.serviceWorker.controller"), 30_000);
  });

  await step("offline: with the server stopped, the app opens from cache with the saved session and cached data", async () => {
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
    await page.waitForText("Links need a connection");
    await page.click("Meeting notes");
    await waitFor("cached note body", async () =>
      ((await page.evaluate(`document.querySelector('textarea[aria-label="Note body"]')?.value ?? ""`))).includes("Agenda: launch plan"),
    );
    await page.fill("Note title", "Edited with the server down");
    await page.waitForText("saved on this device");
    // Pinning is saved with the text, so it works offline too (D-034).
    await page.click("pin");
    await waitFor("pinned offline", () => page.evaluate(`[...document.querySelectorAll("button")].some((b) => b.innerText.toLowerCase() === "unpin" && b.getAttribute("aria-pressed") === "true")`));
    await page.waitForText("saved on this device");
    await page.hash("/tasks");
    await page.waitForText("Offline: changes are saved on this device");
    await page.click("All");
    await page.waitForText("Write the quarterly report");
    // Search falls back to what this device has cached.
    await page.hash("/search");
    await page.fill("Search", "quarterly");
    await page.waitForText("showing matches from what this device has saved");
    await page.waitForText("Write the quarterly report");
    await page.fill("Search", "launch plan"); // a note body, cached when the note was opened
    await page.waitForText("Agenda: launch plan");
    // Files: the last listing seen on this device, read-only.
    await page.hash("/files?folder=uploads");
    await page.waitForText("last copy of this folder seen on this device");
    await page.waitForText("hello e2e.txt");
    await page.waitForText("not on this device");
    if (!(await page.evaluate(`[...document.querySelectorAll("button")].find((b) => b.innerText.toLowerCase() === "upload")?.disabled`)))
      throw new Error("upload should be disabled offline");
    await page.hash("/files?folder=never-opened");
    await page.waitForText("hasn't been opened on this device yet");
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
