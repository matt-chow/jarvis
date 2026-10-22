// End-to-end API tests.
//
//   npm run smoke
//     Starts an isolated `wrangler dev` (fresh local D1/R2), then exercises owner
//     setup, passkey login (software authenticator), sessions, CSRF origin checks,
//     device tokens, private files, and static assets.
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
      for (const [m, p] of [["GET", "/api/v1/files"], ["GET", "/api/v1/auth/passkeys"], ["GET", "/api/v1/auth/tokens"]]) {
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

    const payload = `directory smoke ${new Date().toISOString()}`;

    await check("cookie-authenticated write without Origin → 403", async () => {
      const r = await api.req("POST", "/api/v1/tasks", { body: { title: "csrf" }, origin: false });
      expect(r.status === 403 && r.json.error === "origin_not_allowed", `${r.status}`);
      const evil = await fetch(`${baseUrl}/api/v1/tasks`, {
        method: "POST",
        headers: { Cookie: api.cookie, Origin: "https://evil.example", "Content-Type": "application/json" },
        body: JSON.stringify({ title: "csrf" }),
      });
      expect(evil.status === 403, `foreign origin → ${evil.status}`);
    });

    await check("files: streamed upload, folders, download (attachment), rename/move, conflicts, quota, delete", async () => {
      const upload = (folder, name, body, type = "text/plain; charset=utf-8") =>
        api.req("POST", `/api/v1/files?folder=${encodeURIComponent(folder)}&name=${encodeURIComponent(name)}`, {
          body,
          headers: { "Content-Type": type },
        });
      const up = await upload("docs/2026", "Résumé.txt", payload);
      expect(up.status === 201 && up.json.size === Buffer.byteLength(payload) && up.json.content_type === "text/plain", `upload → ${up.text}`);
      expect((await upload("docs/2026", "Résumé.txt", "dup")).status === 409, "duplicate name");
      expect((await upload("", "../evil", "x")).status === 400, "bad name");

      const root = await api.req("GET", "/api/v1/files");
      expect(root.json.folders.includes("docs") && root.json.files.length === 0, `root → ${root.text}`);
      const docs = await api.req("GET", "/api/v1/files?folder=docs");
      expect(docs.json.folders.join() === "2026", "nested folder");

      const get = await api.req("GET", `/api/v1/files/${up.json.id}/content`);
      expect(get.status === 200 && get.text === payload, "content streamed back");
      expect(/^attachment; filename="R_sum_\.txt"; filename\*=UTF-8''R%C3%A9sum%C3%A9\.txt$/.test(get.headers.get("content-disposition")), get.headers.get("content-disposition"));
      expect(get.headers.get("content-security-policy").includes("sandbox"), "sandboxed");

      const moved = await api.req("PATCH", `/api/v1/files/${up.json.id}`, { body: { folder: "archive", name: "cv.txt" } });
      expect(moved.json.folder === "archive" && moved.json.name === "cv.txt", `move → ${moved.text}`);
      expect((await api.req("GET", `/api/v1/files/${up.json.id}/content`)).text === payload, "content survives move");

      const usage = await api.req("GET", "/api/v1/files/usage");
      expect(usage.json.used === Buffer.byteLength(payload) && usage.json.quota === 1_000_000, `usage → ${usage.text}`);
      const big = await upload("", "big.bin", new Uint8Array(1_100_000), "application/octet-stream");
      expect(big.status === 507 && big.json.error === "quota_exceeded", `over quota → ${big.status}`);

      expect((await api.req("DELETE", `/api/v1/files/${up.json.id}`)).status === 204, "delete");
      expect((await api.req("GET", `/api/v1/files/${up.json.id}/content`)).status === 404, "gone");
      expect((await api.req("GET", "/api/v1/files/usage")).json.used === 0, "usage released");
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

    await check("projects: create, assign tasks (top-level only), archive, delete unassigns tasks", async () => {
      expect((await api.req("POST", "/api/v1/projects", { body: { name: "" } })).status === 400, "blank name");
      const proj = await api.req("POST", "/api/v1/projects", { body: { name: "Kitchen remodel", description: "Q1" } });
      expect(proj.status === 201 && proj.json.status === "active", `create → ${proj.status} ${proj.text}`);
      const task = await api.req("POST", "/api/v1/tasks", { body: { title: "Get quotes", project_id: proj.json.id } });
      expect(task.status === 201 && task.json.project_id === proj.json.id, `assigned → ${task.text}`);
      const unknown = await api.req("POST", "/api/v1/tasks", { body: { title: "x", project_id: "f".repeat(32) } });
      expect(unknown.status === 400 && /project_id/.test(unknown.json.message), `unknown project → ${unknown.status}`);
      const sub = await api.req("POST", "/api/v1/tasks", {
        body: { title: "Call contractor", parent_id: task.json.id, project_id: proj.json.id },
      });
      expect(sub.status === 400, `subtask with project → ${sub.status}`);

      const archived = await api.req("PATCH", `/api/v1/projects/${proj.json.id}`, { body: { status: "archived" } });
      expect(archived.json.status === "archived", "archive");
      const active = await api.req("GET", "/api/v1/projects");
      expect(!active.json.projects.some((p) => p.id === proj.json.id), "archived hidden by default");
      const all = await api.req("GET", "/api/v1/projects?include=archived");
      expect(all.json.projects.some((p) => p.id === proj.json.id), "include=archived");

      expect((await api.req("DELETE", `/api/v1/projects/${proj.json.id}`)).status === 204, "delete project");
      const kept = await api.req("GET", `/api/v1/tasks/${task.json.id}`);
      expect(kept.status === 200 && kept.json.project_id === null, `task kept, unassigned → ${kept.text}`);
      await api.req("DELETE", `/api/v1/tasks/${task.json.id}`);
    });

    await check("notes: summaries without bodies, optimistic concurrency (409 with current), pin, delete", async () => {
      const n = await api.req("POST", "/api/v1/notes", { body: { body: "# Packing list\n\npassport\ncharger" } });
      expect(n.status === 201 && n.json.title === "", `create → ${n.status} ${n.text}`);
      const list = await api.req("GET", "/api/v1/notes");
      const row = list.json.notes.find((x) => x.id === n.json.id);
      expect(row && row.title === "Packing list" && row.excerpt === "passport charger" && !("body" in row), `summary → ${JSON.stringify(row)}`);

      const v1 = n.json.updated_at;
      const saved = await api.req("PATCH", `/api/v1/notes/${n.json.id}`, { body: { body: "edited on laptop", expected_updated_at: v1 } });
      expect(saved.status === 200 && saved.json.updated_at > v1, `save → ${saved.status}`);
      const stale = await api.req("PATCH", `/api/v1/notes/${n.json.id}`, { body: { body: "edited on phone", expected_updated_at: v1 } });
      expect(stale.status === 409 && stale.json.error === "conflict" && stale.json.note.body === "edited on laptop", `stale → ${stale.status} ${stale.text}`);

      const pinned = await api.req("PATCH", `/api/v1/notes/${n.json.id}`, { body: { pinned: true } });
      expect(pinned.json.pinned === true && pinned.json.body === "edited on laptop", "pin keeps body");
      const big = await api.req("POST", "/api/v1/notes", { body: { body: "x".repeat(100_001) } });
      expect(big.status === 400 && /body/.test(big.json.message), `oversize body → ${big.status} ${big.text.slice(0, 80)}`);
      expect((await api.req("DELETE", `/api/v1/notes/${n.json.id}`)).status === 204, "delete");
      expect((await api.req("GET", `/api/v1/notes/${n.json.id}`)).status === 404, "gone");
    });

    await check("journal: upsert by date, mood, ranged list, conflicts, validation", async () => {
      expect((await api.req("GET", "/api/v1/journal/2026-10-21")).status === 404, "no entry yet");
      const created = await api.req("PUT", "/api/v1/journal/2026-10-21", { body: { body: "Shipped the journal.\nFelt good.", mood: 4 } });
      expect(created.status === 200 && created.json.mood === 4 && created.json.date === "2026-10-21", `create → ${created.text}`);
      // Second device creating the same day without a version: conflict carries the first entry.
      const race = await api.req("PUT", "/api/v1/journal/2026-10-21", { body: { body: "other device", expected_updated_at: 1 } });
      expect(race.status === 409 && race.json.entry.body.startsWith("Shipped"), `stale → ${race.status}`);
      const edited = await api.req("PUT", "/api/v1/journal/2026-10-21", {
        body: { body: "Shipped the journal. Long walk.", expected_updated_at: created.json.updated_at },
      });
      expect(edited.status === 200 && edited.json.mood === 4, `edit keeps mood → ${edited.text}`);
      await api.req("PUT", "/api/v1/journal/2026-10-02", { body: { body: "Earlier" } });
      await api.req("PUT", "/api/v1/journal/2026-11-01", { body: { body: "Next month" } });
      const oct = await api.req("GET", "/api/v1/journal?from=2026-10-01&to=2026-10-31");
      expect(oct.json.entries.map((e) => e.date).join() === "2026-10-21,2026-10-02", `range → ${oct.text}`);
      expect(!("body" in oct.json.entries[0]) && oct.json.entries[0].excerpt.startsWith("Shipped"), "summaries only");
      expect((await api.req("PUT", "/api/v1/journal/2026-02-30", { body: { body: "x" } })).status === 400, "bad date");
      expect((await api.req("PUT", "/api/v1/journal/2026-10-21", { body: { mood: 9 } })).status === 400, "bad mood");
      expect((await api.req("GET", "/api/v1/journal?from=2024-01-01&to=2026-01-01")).status === 400, "range too long");
      expect((await api.req("DELETE", "/api/v1/journal/2026-11-01")).status === 204, "delete");
    });

    await check("inbox: capture, triage into task / note / journal (item removed), validation", async () => {
      expect((await api.req("POST", "/api/v1/inbox", { body: { text: "   " } })).status === 400, "blank capture");
      const cap = async (text) => (await api.req("POST", "/api/v1/inbox", { body: { text } })).json;
      const a = await cap("Renew passport\nform is on the desk");
      const b = await cap("Idea: weekly review template");
      const c = await cap("Saw a heron by the river");
      const list = await api.req("GET", "/api/v1/inbox");
      expect(list.json.items.map((i) => i.id).join() === [a.id, b.id, c.id].join(), "oldest first");

      const t = await api.req("POST", `/api/v1/inbox/${a.id}/triage`, { body: { to: "task" } });
      expect(t.status === 200 && t.json.kind === "task" && t.json.task.title === "Renew passport" && t.json.task.notes === "form is on the desk", `task → ${t.text}`);
      const n = await api.req("POST", `/api/v1/inbox/${b.id}/triage`, { body: { to: "note" } });
      expect(n.json.kind === "note" && n.json.note.body.startsWith("Idea:"), `note → ${n.text}`);
      const noDate = await api.req("POST", `/api/v1/inbox/${c.id}/triage`, { body: { to: "journal" } });
      expect(noDate.status === 400, "journal needs a date");
      const j = await api.req("POST", `/api/v1/inbox/${c.id}/triage`, { body: { to: "journal", date: "2026-10-21" } });
      expect(j.json.kind === "journal" && j.json.entry.body.endsWith("\n\nSaw a heron by the river"), `journal append → ${j.text}`);

      expect((await api.req("GET", "/api/v1/inbox")).json.items.length === 0, "inbox empty");
      expect((await api.req("POST", `/api/v1/inbox/${a.id}/triage`, { body: { to: "task" } })).status === 404, "already triaged");
      await api.req("DELETE", `/api/v1/tasks/${t.json.task.id}`);
      await api.req("DELETE", `/api/v1/notes/${n.json.note.id}`);
    });

    await check("links: connect task, note, project, journal; symmetric, idempotent, titles resolved, cleaned on delete", async () => {
      const task = (await api.req("POST", "/api/v1/tasks", { body: { title: "Draft talk" } })).json;
      const note = (await api.req("POST", "/api/v1/notes", { body: { body: "# Talk outline\nintro" } })).json;
      const proj = (await api.req("POST", "/api/v1/projects", { body: { name: "Conference" } })).json;
      await api.req("PUT", "/api/v1/journal/2026-10-21", { body: { body: "worked on the talk" } });
      const link = (from, to) => api.req("POST", "/api/v1/links", { body: { from, to } });

      const l1 = await link({ kind: "task", id: task.id }, { kind: "note", id: note.id });
      expect(l1.status === 201 && l1.json.title === "Talk outline" && l1.json.kind === "note", `task→note → ${l1.text}`);
      const again = await link({ kind: "note", id: note.id }, { kind: "task", id: task.id });
      expect(again.json.link_id === l1.json.link_id, "reverse is the same link");
      await link({ kind: "task", id: task.id }, { kind: "project", id: proj.id });
      await link({ kind: "journal", id: "2026-10-21" }, { kind: "task", id: task.id });

      const fromTask = await api.req("GET", `/api/v1/links?kind=task&id=${task.id}`);
      expect(fromTask.json.links.map((l) => `${l.kind}:${l.title}`).join() === "journal:2026-10-21,note:Talk outline,project:Conference", `list → ${fromTask.text}`);
      const fromNote = await api.req("GET", `/api/v1/links?kind=note&id=${note.id}`);
      expect(fromNote.json.links[0].title === "Draft talk", "visible from the other end");

      expect((await link({ kind: "task", id: task.id }, { kind: "task", id: task.id })).status === 400, "self link");
      expect((await link({ kind: "task", id: task.id }, { kind: "note", id: "e".repeat(32) })).status === 400, "unknown target");
      expect((await api.req("GET", "/api/v1/links?kind=bogus&id=x")).status === 400, "bad kind");

      await api.req("DELETE", `/api/v1/notes/${note.id}`);
      const afterDelete = await api.req("GET", `/api/v1/links?kind=task&id=${task.id}`);
      expect(!afterDelete.json.links.some((l) => l.kind === "note"), "dangling link hidden");
      expect((await api.req("DELETE", `/api/v1/links/${l1.json.link_id}`)).status === 404, "and cleaned up");
      await api.req("DELETE", `/api/v1/tasks/${task.id}`);
      await api.req("DELETE", `/api/v1/projects/${proj.id}`);
    });

    await check("export: one JSON download with every record, no credential material", async () => {
      const task = (await api.req("POST", "/api/v1/tasks", { body: { title: "Export me" } })).json;
      const note = (await api.req("POST", "/api/v1/notes", { body: { body: "Exported note body" } })).json;
      await api.req("POST", "/api/v1/links", { body: { from: { kind: "task", id: task.id }, to: { kind: "note", id: note.id } } });
      await api.req("POST", "/api/v1/inbox", { body: { text: "still in the inbox" } });
      const file = (await api.req("POST", "/api/v1/files?folder=export&name=check.txt", { body: "file", headers: { "Content-Type": "text/plain" } })).json;

      const r = await api.req("GET", "/api/v1/export");
      expect(r.status === 200 && /attachment; filename="directory-export-\d{4}-\d{2}-\d{2}\.json"/.test(r.headers.get("content-disposition")), "download headers");
      const doc = r.json;
      expect(doc.format === "directory-export" && doc.format_version === 1 && doc.schema_version >= 9, "header fields");
      expect(doc.user.display_name === "Smoke Owner", "user");
      expect(doc.tasks.some((t) => t.id === task.id) && doc.notes.some((n) => n.body === "Exported note body"), "tasks + full note bodies");
      expect(doc.journal.some((e) => e.date === "2026-10-21" && e.body.length > 0), "journal with bodies");
      expect(doc.links.length >= 1 && doc.inbox.some((i) => i.text === "still in the inbox"), "links + inbox");
      expect(doc.files.some((f) => f.folder === "export" && f.name === "check.txt"), "file metadata");
      for (const secret of ["token_hash", "public_key", "sign_count", "owner_id", "dir_", "__Host-"]) {
        expect(!r.text.includes(secret), `leaked ${secret}`);
      }
      await api.req("DELETE", `/api/v1/files/${file.id}`);
    });

    await check("jarvis: read plans run now; write plans need approval, run once; cancel; history; parse errors", async () => {
      const plan = (input) => api.req("POST", "/api/v1/jarvis/plan", { body: { input, today: "2026-10-21" } });

      const read = await plan("list tasks");
      expect(read.status === 200 && read.json.status === "executed" && !read.json.requires_approval, `read → ${read.text}`);

      const write = await plan("add task Buy stamps due tomorrow; capture check postage rates");
      expect(write.json.status === "pending" && write.json.requires_approval && write.json.actions.length === 2, `write → ${write.text}`);
      expect(write.json.actions[0].summary === "Create task “Buy stamps” due 2026-10-22", write.json.actions[0].summary);
      const before = await api.req("GET", "/api/v1/tasks");
      expect(!before.json.tasks.some((t) => t.title === "Buy stamps"), "nothing written before approval");

      const ran = await api.req("POST", `/api/v1/jarvis/plans/${write.json.id}/execute`);
      expect(ran.status === 200 && ran.json.status === "executed" && ran.json.results.every((r) => r.ok), `execute → ${ran.text}`);
      const task = (await api.req("GET", "/api/v1/tasks")).json.tasks.find((t) => t.title === "Buy stamps");
      expect(task && task.due_date === "2026-10-22", "task created with due date");
      const twice = await api.req("POST", `/api/v1/jarvis/plans/${write.json.id}/execute`);
      expect(twice.status === 409 && twice.json.message === "executed", `second execute → ${twice.status}`);

      const complete = await plan("done stamps");
      expect(complete.json.actions[0].summary === "Complete task “Buy stamps”", complete.text);
      const cancelled = await api.req("POST", `/api/v1/jarvis/plans/${complete.json.id}/cancel`);
      expect(cancelled.json.status === "cancelled", "cancel");
      expect((await api.req("POST", `/api/v1/jarvis/plans/${complete.json.id}/execute`)).status === 409, "cancelled can't run");

      const unknown = await plan("dance wildly");
      expect(unknown.status === 400 && unknown.json.error === "not_understood" && unknown.json.examples.length > 0, `parse error → ${unknown.text}`);
      const unresolved = await plan("complete nonexistent thing");
      expect(unresolved.status === 422 && unresolved.json.error === "unresolved", `unresolved → ${unresolved.status}`);

      const history = await api.req("GET", "/api/v1/jarvis/history");
      expect(history.json.plans[0].id === complete.json.id && history.json.plans.length >= 3, "history newest first");
      expect((await fetch(`${baseUrl}/api/v1/jarvis/history`)).status === 401, "anonymous → 401");
      await api.req("DELETE", `/api/v1/tasks/${task.id}`);
    });

    await check("jarvis presets: save (validated by parsing), list, delete", async () => {
      const bad = await api.req("POST", "/api/v1/jarvis/presets", { body: { name: "Bad", input: "dance" } });
      expect(bad.status === 400 && bad.json.error === "not_understood", `unparseable → ${bad.status}`);
      const p = await api.req("POST", "/api/v1/jarvis/presets", { body: { name: "Morning", input: "list tasks; journal started the day" } });
      expect(p.status === 201 && p.json.name === "Morning", `create → ${p.text}`);
      const list = await api.req("GET", "/api/v1/jarvis/presets");
      expect(list.json.presets.some((x) => x.id === p.json.id), "listed");
      expect((await api.req("DELETE", `/api/v1/jarvis/presets/${p.json.id}`)).status === 204, "delete");
      expect((await api.req("DELETE", `/api/v1/jarvis/presets/${p.json.id}`)).status === 404, "gone");
    });

    await check("search: across kinds, title matches first, LIKE wildcards escaped, jarvis find", async () => {
      const task = (await api.req("POST", "/api/v1/tasks", { body: { title: "Renew passport", notes: "form 100% done" } })).json;
      const note = (await api.req("POST", "/api/v1/notes", { body: { body: "Travel docs\nPassport is in the top drawer" } })).json;
      const proj = (await api.req("POST", "/api/v1/projects", { body: { name: "Japan trip", description: "need passport photos" } })).json;
      const r = await api.req("GET", "/api/v1/search?q=passport");
      expect(r.status === 200, `search → ${r.status}`);
      const kinds = r.json.hits.map((h) => h.kind);
      expect(kinds[0] === "task" && r.json.hits[0].title_match && kinds.includes("note") && kinds.includes("project"), `hits → ${r.text}`);
      const noteHit = r.json.hits.find((h) => h.kind === "note");
      expect(noteHit.title === "Travel docs" && /Passport is in the top drawer/.test(noteHit.snippet), "derived title + snippet");
      const pct = await api.req("GET", "/api/v1/search?q=100%25");
      expect(pct.json.hits.length === 1 && pct.json.hits[0].id === task.id, `literal % → ${pct.text}`);
      const underscore = await api.req("GET", "/api/v1/search?q=_a");
      expect(underscore.json.hits.length === 0, "literal _ is not a wildcard");
      expect((await api.req("GET", "/api/v1/search?q=a")).status === 400, "too short");
      const find = await api.req("POST", "/api/v1/jarvis/plan", { body: { input: "find passport", today: "2026-10-21" } });
      expect(find.json.status === "executed" && find.json.results[0].entity.id === task.id, `jarvis find → ${find.text}`);
      await api.req("DELETE", `/api/v1/tasks/${task.id}`);
      await api.req("DELETE", `/api/v1/notes/${note.id}`);
      await api.req("DELETE", `/api/v1/projects/${proj.id}`);
    });

    await check("library: PDF/ePub files with reading progress; non-documents rejected; exported", async () => {
      const up = async (name, type) =>
        (await api.req("POST", `/api/v1/files?folder=books&name=${encodeURIComponent(name)}`, { body: "%PDF-1.4 smoke", headers: { "Content-Type": type } })).json;
      const pdf = await up("Dune.pdf", "application/pdf");
      const epub = await up("Novel.epub", "application/octet-stream");
      const jpg = await up("cover.jpg", "image/jpeg");
      const lib = await api.req("GET", "/api/v1/library");
      const names = lib.json.items.map((i) => i.file.name);
      expect(names.includes("Dune.pdf") && names.includes("Novel.epub") && !names.includes("cover.jpg"), `library → ${names}`);

      const p1 = await api.req("PUT", `/api/v1/library/${pdf.id}/progress`, { body: { current_page: 50, total_pages: 200 } });
      expect(p1.status === 200 && p1.json.status === "reading" && p1.json.percent === 25, `progress → ${p1.text}`);
      const p2 = await api.req("PUT", `/api/v1/library/${pdf.id}/progress`, { body: { current_page: 200 } });
      expect(p2.json.status === "finished" && p2.json.percent === 100 && p2.json.finished_at > 0, "finished at last page");
      const bad = await api.req("PUT", `/api/v1/library/${pdf.id}/progress`, { body: { current_page: 201 } });
      expect(bad.status === 400, `beyond last page → ${bad.status}`);
      expect((await api.req("PUT", `/api/v1/library/${jpg.id}/progress`, { body: {} })).status === 400, "not a document");

      const ordered = (await api.req("GET", "/api/v1/library")).json.items;
      expect(ordered[ordered.length - 1].file.id === pdf.id && ordered[ordered.length - 1].progress.status === "finished", "finished sorts last");
      const doc = (await api.req("GET", "/api/v1/export")).json;
      expect(doc.reading_progress.some((r) => r.file_id === pdf.id), "progress exported");

      expect((await api.req("DELETE", `/api/v1/library/${pdf.id}/progress`)).status === 204, "clear");
      for (const f of [pdf, epub, jpg]) await api.req("DELETE", `/api/v1/files/${f.id}`);
      expect((await api.req("GET", "/api/v1/library")).json.items.length === 0, "library empties with files");
    });

    await check("offline sync support: client ids make replayed creates idempotent (tasks, inbox, projects, notes); journal create-only", async () => {
      const id = "ab".repeat(16);
      const first = await api.req("POST", "/api/v1/tasks", { body: { id, title: "From the outbox" } });
      expect(first.status === 201 && first.json.id === id, `create → ${first.text}`);
      const replayed = await api.req("POST", "/api/v1/tasks", { body: { id, title: "From the outbox" } });
      expect(replayed.status === 409 && replayed.json.error === "exists", `replay → ${replayed.status}`);
      expect((await api.req("POST", "/api/v1/tasks", { body: { id: "zz", title: "x" } })).status === 400, "malformed id");
      await api.req("DELETE", `/api/v1/tasks/${id}`);

      const iid = "cd".repeat(16);
      const cap = await api.req("POST", "/api/v1/inbox", { body: { id: iid, text: "Captured offline" } });
      expect(cap.status === 201 && cap.json.id === iid, `capture → ${cap.status} ${cap.text}`);
      const again = await api.req("POST", "/api/v1/inbox", { body: { id: iid, text: "Captured offline" } });
      expect(again.status === 409 && again.json.error === "exists", `capture replay → ${again.status}`);
      expect((await api.req("DELETE", `/api/v1/inbox/${iid}`)).status === 204, "discard");
      expect((await api.req("DELETE", `/api/v1/inbox/${iid}`)).status === 404, "discard replay → 404");

      const pid = "ef".repeat(16);
      expect((await api.req("POST", "/api/v1/projects", { body: { id: pid, name: "Offline plan" } })).status === 201, "project create");
      const pAgain = await api.req("POST", "/api/v1/projects", { body: { id: pid, name: "Offline plan" } });
      expect(pAgain.status === 409 && pAgain.json.error === "exists", `project replay → ${pAgain.status}`);
      await api.req("DELETE", `/api/v1/projects/${pid}`);

      const nid = "0a".repeat(16);
      expect((await api.req("POST", "/api/v1/notes", { body: { id: nid } })).status === 201, "note create");
      const nAgain = await api.req("POST", "/api/v1/notes", { body: { id: nid } });
      expect(nAgain.status === 409 && nAgain.json.error === "exists", `note replay → ${nAgain.status}`);
      await api.req("DELETE", `/api/v1/notes/${nid}`);

      // Journal: expected_updated_at 0 = create only (an offline write to a day that had no entry).
      const fresh = await api.req("PUT", "/api/v1/journal/2001-01-01", { body: { body: "offline", expected_updated_at: 0 } });
      expect(fresh.status === 200 || fresh.status === 201, `create-only → ${fresh.status}`);
      const clash = await api.req("PUT", "/api/v1/journal/2001-01-01", { body: { body: "other device", expected_updated_at: 0 } });
      expect(clash.status === 409 && clash.json.entry.body === "offline", `create-only on existing → ${clash.status}`);
      await api.req("DELETE", "/api/v1/journal/2001-01-01");
    });

    await check("device API token: create (shown once), use as bearer, cannot manage credentials", async () => {
      const r = await api.req("POST", "/api/v1/auth/tokens", { body: { name: "Smoke device" } });
      expect(r.status === 201 && r.json.secret.startsWith("dir_"), `${r.status} ${r.text}`);
      expect(!("token_hash" in r.json.token) && !("tokenHash" in r.json.token), "hash not exposed");
      deviceSecret = r.json.secret;
      deviceTokenId = r.json.token.id;
      const bearer = new Client(baseUrl);
      const list = await bearer.req("GET", "/api/v1/files", { headers: { Authorization: `Bearer ${deviceSecret}` } });
      expect(list.status === 200, `bearer list ${list.status}`);
      const mint = await bearer.req("POST", "/api/v1/auth/tokens", {
        body: {},
        headers: { Authorization: `Bearer ${deviceSecret}` },
      });
      expect(mint.status === 403 && mint.json.error === "session_required", `mint via bearer → ${mint.status}`);
      const wrong = await bearer.req("GET", "/api/v1/files", { headers: { Authorization: "Bearer dir_wrong" } });
      expect(wrong.status === 401, `wrong bearer → ${wrong.status}`);
    });

    const oldCookie = api.cookie;
    await check("logout clears the cookie and revokes the session server-side", async () => {
      const r = await api.req("POST", "/api/v1/auth/logout");
      expect(r.status === 204, `${r.status}`);
      expect(r.setCookie.some((c) => /Max-Age=0/.test(c)), "cookie cleared");
      const replay = await fetch(`${baseUrl}/api/v1/files`, { headers: { Cookie: oldCookie } });
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
      const r = await fetch(`${baseUrl}/api/v1/files`, { headers: { Authorization: `Bearer ${deviceSecret}` } });
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
    expect((await api.req("GET", "/api/v1/files")).status === 401, "files");
  });
  if (token) {
    await check("device token file round trip", async () => {
      const auth = { Authorization: `Bearer ${token}` };
      const name = `remote-${Date.now()}.txt`;
      const put = await api.req("POST", `/api/v1/files?folder=smoke&name=${name}`, { body: "remote smoke", headers: { ...auth, "Content-Type": "text/plain" } });
      expect(put.status === 201, `upload ${put.status}`);
      const get = await api.req("GET", `/api/v1/files/${put.json.id}/content`, { headers: auth });
      expect(get.text === "remote smoke", "body");
      expect((await api.req("DELETE", `/api/v1/files/${put.json.id}`, { headers: auth })).status === 204, "delete");
    });
  } else {
    console.log("  skip file round trip (set DIRECTORY_API_TOKEN to a device token)");
  }
  await staticChecks(base);
}

if (process.env.BASE_URL) await remote(process.env.BASE_URL.replace(/\/$/, ""));
else await local();

console.log(failures ? `\n${failures} check(s) failed` : "\nall checks passed");
process.exit(failures ? 1 : 0);
