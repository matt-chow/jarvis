# Staging deployment guide

Staging is a private, test-only copy of Directory on Cloudflare's free tier.
You use it to try the app on real devices (iPhone Safari and the JARVIS
Shortcut) before any production deploy. It has its own Worker
(`directory-staging`), its own D1 database (`directory-db-staging`), and its
own private R2 bucket (`directory-files-staging`), so it never touches
production data. They are defined in `wrangler.toml` under `[env.staging]`.

**Status (2026-10-24):** deployed at
`https://directory-staging.mchowcl.workers.dev` (Worker `directory-staging`,
D1 `directory-db-staging` at schema v22, private R2
`directory-files-staging` with r2.dev disabled, 100 MB per-owner quota).
- §1–§6 done: resources created, migrations applied, Worker deployed.
- §7 done by the owner: setup token set, first-run setup with an iPhone
  passkey, Windows sign-in, and two device tokens (`Staging smoke`, Full;
  `iPhone Shortcut`, JARVIS only). The public session endpoint reports
  `setupRequired: false`.
- §8: the owner reports `npm run smoke` against staging exited 0. The
  tokenless checks (health, anonymous 401s, app shell, security headers)
  were re-run separately and passed. Whether the token checks ran or were
  skipped wasn't recorded; see the note in §8.
- Not yet done: the by-hand checks in §8, §9 (including a confirmed "yes"
  through `/ask`), and the on-iPhone Shortcut (docs/IPHONE_SHORTCUT.md).

How to read this guide:

- 🧑 **Owner action** means a manual step, in a browser, the Cloudflare
  dashboard, or a decision.
- 💻 **Command** means something you run in PowerShell, from the repository
  folder. Each one says what it should print.
- Placeholders such as `<subdomain>`, `<database-id>`, and `dir_…` are
  values you'll get along the way. Never paste secrets into chat, issues,
  or committed files.

All commands run in **PowerShell** from the repository root:

```powershell
cd C:\Users\mchow\dev\jarvis
```

(Use this folder, not the retired OneDrive copy, which has no staging
config.)

---

## 1. Prerequisites and Cloudflare login

These are already installed on this PC. A new machine needs them too.

| Tool | Check | Install |
| --- | --- | --- |
| Node.js ≥ 22 and npm | `node --version` | nodejs.org (LTS) |
| Rust stable + wasm target | `rustup target list --installed` shows `wasm32-unknown-unknown` | README.md › Prerequisites |
| `worker-build` 0.8.7 | `worker-build --version` | `cargo install worker-build --version 0.8.7 --locked` (on Windows, see README.md for the MinGW note) |
| Project packages | `node_modules` folder exists | 💻 `npm ci` |

💻 Put Rust's tools on `PATH` for this PowerShell window (the build needs
`worker-build`):

```powershell
$env:Path = "$HOME\.cargo\bin;$env:Path"
```

💻 Run the local gate first, so you deploy only what passes:

```powershell
npm run verify
```

This takes about 5 minutes. It should end without errors. The last lines
show a Worker dry-run upload size and the bindings.

🧑 **Log in to Cloudflare** (this opens your browser; approve access):

```powershell
npx wrangler login
npx wrangler whoami
```

`whoami` should print your account email and account ID. If it says "You
are not authenticated", run `npx wrangler login` again.

---

## 2. Manual dashboard actions (🧑, at dash.cloudflare.com)

1. **Plan:** confirm the account is on **Workers Free**. Don't buy Workers
   Paid; nothing here needs it.
2. **workers.dev subdomain:** open *Workers & Pages*. If asked, choose a
   subdomain; the staging URL will be
   `https://directory-staging.<subdomain>.workers.dev`. Write `<subdomain>`
   down. You can see it later on the *Workers & Pages* overview page
   (Subdomain).
3. **Activate R2:** open *R2 Object Storage* and follow the activation
   prompt. Cloudflare asks for a **payment method** even for the free
   allowance; this is the one billing-related step. Read §11 first.
   Activating doesn't charge anything by itself.

Don't create a custom domain, a public bucket URL, or Cloudflare Access
for staging. None are needed (§11 covers the optional Access layer).

---

## 3. Create the staging-only D1 database and R2 bucket

💻 **D1 database:**

```powershell
npx wrangler d1 create directory-db-staging
```

It prints a block that includes `database_id = "…"`. Copy that id. Don't
use `--update-config`, because that edits the top-level (production) entry
instead of the staging one.

🧑 Paste the id into `wrangler.toml`, in the **staging** block only:

```toml
[[env.staging.d1_databases]]
binding = "DB"
database_name = "directory-db-staging"
database_id = "<database-id from the command above>"
migrations_dir = "migrations"
```

The top-level `[[d1_databases]]` entry (production) keeps its placeholder
of zeros. The database id isn't a secret, so committing it is fine.

💻 **R2 bucket** (private by default; keep the default *Standard* storage
class, because *Infrequent Access* has retrieval fees):

```powershell
npx wrangler r2 bucket create directory-files-staging
npx wrangler r2 bucket dev-url get directory-files-staging
```

The second command must report that the public r2.dev URL is
**disabled**. Never run `r2 bucket dev-url enable`, and never attach a
custom domain to this bucket. All file access goes through the Worker's
authenticated `/api/v1/files/*` routes.

---

## 4. Configuration and secrets

### 4.1 Hostname variables (🧑 edit `wrangler.toml`)

Passkeys are bound to the exact hostname. In `[env.staging.vars]`, set:

```toml
[env.staging.vars]
DIRECTORY_RP_ID = "directory-staging.<subdomain>.workers.dev"
DIRECTORY_ORIGINS = "https://directory-staging.<subdomain>.workers.dev"
# Recommended on staging: cap file storage per owner at 100 MB (default 1 GiB).
DIRECTORY_QUOTA_BYTES = "104857600"
```

- `DIRECTORY_RP_ID` is the bare hostname: no `https://`, no path, no port.
- `DIRECTORY_ORIGINS` is the exact origin, with `https://` and **no
  trailing slash**.
- If either is empty, sign-in fails closed with `503 auth_not_configured`.
  That's safe, but you can't use the app.

### 4.2 Setup token (🧑 + 💻)

First-run setup (creating the owner and their first passkey) requires a
one-time setup token that only you know. 💻 Generate one:

```powershell
node -e "console.log(require('crypto').randomBytes(32).toString('hex'))"
```

Keep the 64-character value in your password manager. 💻 Store it as a
Worker secret:

```powershell
npx wrangler secret put DIRECTORY_SETUP_TOKEN --env staging
```

Paste the value when prompted. If wrangler says the Worker doesn't exist
yet and offers to create it, answer **yes**; the next deploy fills in the
code. The secret is stored encrypted at Cloudflare and never appears in
the repository.

### 4.3 Optional: AI intent parsing (skip for the first deploy)

JARVIS works fully without AI. To enable the free OpenRouter fallback
later, see docs/JARVIS_SHORTCUTS.md › AI:

```powershell
npx wrangler secret put OPENROUTER_API_KEY --env staging
```

`[env.staging.vars]` sets `JARVIS_AI_MODEL = "nvidia/nemotron-3.5-super-vl:free"`
(the owner's choice, 2026-10-29; OpenRouter's public model list shows one
free NVIDIA endpoint, structured outputs, tools, and optional reasoning;
alternative `google/gemma-4-31b-it:free`). Any id not ending in `:free` leaves
AI off. Every request also sends `provider.max_price` 0 with
`allow_fallbacks: false` and no fallback model list, so OpenRouter refuses
rather than bills (core `openrouter_body`, unit-tested). Reasoning is
turned off so it can't use up the reply's token limit. Privacy: free
endpoints may log or train on prompts under the provider's policy; JARVIS
sends only the request text and minimal handles (D-043, D-048). The model
is active once the Worker is redeployed with this config:

```powershell
npx wrangler d1 time-travel info directory-db-staging   # note the bookmark
npm run deploy:staging
```

To check the model live before (or after) deploying, run the eval on a
local isolated Worker with your key in this shell only:

```powershell
$env:JARVIS_EVAL_LIVE="1"; $env:OPENROUTER_API_KEY="<your key>"; $env:JARVIS_AI_MODEL="nvidia/nemotron-3.5-super-vl:free"; npm run eval:jarvis
``` Optional limits:
`JARVIS_AI_DAILY` (calls per owner per UTC day, default 40; a JARVIS 2.0
chat turn uses up to 3) and `JARVIS_AI_TIMEOUT_MS` (per call, default
12000). Without a model, Ask understands direct commands and says so
(DECISIONS D-048).

### 4.4 Check before going further (💻)

```powershell
npx wrangler deploy --dry-run --env staging --outdir .wrangler/dry-run-staging
```

The bindings table must show `directory-db-staging`,
`directory-files-staging`, and your two hostname variables. **Stop if it
shows `directory-db` or `directory-files`.** Those names mean you're
targeting production.

---

## 5. Migrations, safely

The database schema comes from the append-only files in `migrations/`
(`0001` … `0024`). The app expects **schema v24**.

💻 See what will be applied (for a brand-new database, all 23 are listed):

```powershell
npx wrangler d1 migrations list DB --remote --env staging
```

💻 Apply them (wrangler asks you to confirm; answer yes):

```powershell
npm run db:migrate:staging
```

This runs `wrangler d1 migrations apply DB --remote --env staging`.
💻 Confirm nothing is left pending:

```powershell
npx wrangler d1 migrations list DB --remote --env staging
```

Rules for later migrations:
- Always pass `--env staging`. Without it, wrangler targets the production
  database.
- **Before** applying new migrations to a staging database that holds data
  you care about, note a restore point:
  `npx wrangler d1 time-travel info directory-db-staging`. It prints a
  bookmark; see §12.
- Never edit an applied migration file. Fixes are new files.

---

## 6. Build and deploy (💻)

```powershell
npm run deploy:staging
```

This runs `npm run build` (the PWA into `apps/web/dist`), then
`wrangler deploy --env staging`, which compiles the Rust Worker with
`worker-build --release` and uploads it together with the static assets. At
the end it prints the URL:
`https://directory-staging.<subdomain>.workers.dev`.

Run `npm run verify` before each redeploy. To watch the live logs while you
test:

```powershell
npx wrangler tail --env staging
```

---

## 7. HTTPS URL and authentication checks

`$base` below is your staging URL. 💻 Set it once per PowerShell window:

```powershell
$base = "https://directory-staging.<subdomain>.workers.dev"
```

1. **Health** should be `status: ok`, with the database check saying
   `schema v24` and storage ok:
   ```powershell
   Invoke-RestMethod "$base/api/v1/health" | ConvertTo-Json -Depth 4
   ```
   `schema vN, expected v24` means migrations are missing (§5). The
   endpoint returns HTTP 503 while degraded.
2. **Data needs sign-in.** This should print `401`:
   ```powershell
   curl.exe -s -o NUL -w "%{http_code}`n" "$base/api/v1/tasks"
   ```
3. 🧑 **First-run setup, on the iPhone** (recommended; see §10):
   1. Open `$base` in Safari.
   2. Enter the setup token from §4.2 and your name.
   3. Save the passkey to iCloud Keychain.

   Setup closes after the first owner exists. If someone else reaches the
   page, they can't continue without the setup token.
4. 🧑 **Sign in on the PC:**
   1. Open `$base` in Edge or Chrome.
   2. Choose *Sign in*, then *Use a phone or tablet*.
   3. Scan the QR code with the iPhone and approve with Face ID.
   4. Optional: in Settings, add a passkey for this PC (Windows Hello).
5. 🧑 **Device tokens**, in Settings › Device tokens. Copy each secret when
   it's shown; it isn't shown again:
   - `Staging smoke`, **Full access** (for the automated checks).
   - `iPhone Shortcut`, **JARVIS only (Shortcuts)**.

---

## 8. Task persistence and private storage

### Automated (💻)

The remote smoke test creates a task, reads it back, deletes it, uploads
a file, and confirms the file content is refused without auth. It cleans up
after itself. Your tokens go in environment variables for this window only:

```powershell
$env:BASE_URL = $base
$env:DIRECTORY_API_TOKEN = "dir_…"      # the Full-access token
$env:DIRECTORY_JARVIS_TOKEN = "dir_…"   # the JARVIS-only token
npm run smoke
Remove-Item Env:DIRECTORY_API_TOKEN, Env:DIRECTORY_JARVIS_TOKEN, Env:BASE_URL
```

It should end with `all checks passed`. The remote checks are:
- health;
- every non-public route answers 401 anonymously;
- file round trip;
- task persists in D1;
- private storage needs auth;
- JARVIS ask with a JARVIS-only token;
- static app shell and security headers.

**Read every line, not just the exit code.** If a token variable is
missing, its checks print `skip …` and the run still ends with `all checks
passed` and exit code 0. A full run shows `ok` for all seven checks above
and no `skip` lines.

(The same remote checks also run locally against an isolated Worker:
`npm run smoke:remote-local`.)

### By hand (🧑)

1. In Tasks, add "Staging check", set an estimate, and reload the page:
   it's still there. Open the same URL on the other device: it's there too.
2. In Files, upload a small file and download it again.
3. Confirm the bucket is still private:
   `npx wrangler r2 bucket dev-url get directory-files-staging` reports
   disabled.
4. Delete the test task and file.

---

## 9. JARVIS endpoint smoke tests (💻)

Use the **JARVIS-only** token.

```powershell
$h = @{ Authorization = "Bearer dir_…" }
$today = Get-Date -Format yyyy-MM-dd
function Ask($body) {
  Invoke-RestMethod -Method Post -Uri "$base/api/v1/jarvis/ask" -Headers $h `
    -ContentType "application/json" -Body ($body | ConvertTo-Json)
}

Ask @{ text = "what should I do today"; today = $today }
```

Expect `status: executed` and a `speech` line ("Top … for …", or "Nothing
open. Nice.").

```powershell
$p = Ask @{ text = "add task test the shortcut due friday"; today = $today }
$p.speech            # "Create task “test the shortcut” due …. Say yes to confirm."
Ask @{ text = "yes"; confirm = $p.plan_id; today = $today }   # executed: "Created task …"
Ask @{ text = "yes"; confirm = $p.plan_id; today = $today }   # "That was already done."
```

The JARVIS-only token must be refused everywhere else. This should print
`403`:

```powershell
curl.exe -s -o NUL -w "%{http_code}`n" -H "Authorization: Bearer dir_…" "$base/api/v1/tasks"
```

Delete "test the shortcut" in the app afterwards. Then build the Shortcut
on the iPhone (docs/IPHONE_SHORTCUT.md).

---

## 10. iPhone prerequisites

- **iOS 16 or later**, with iCloud Keychain on (Settings › your name ›
  iCloud › Passwords and Keychain). Passkeys need it.
- **Safari**, at the HTTPS staging URL. Passkeys don't work over plain
  HTTP or on a raw IP.
- Doing first-run setup **on the iPhone** puts the owner passkey in iCloud
  Keychain, so the phone can also approve sign-ins on the PC by QR code. A
  passkey made with Windows Hello can't be used on the iPhone.
- Optional: in Safari, Share › **Add to Home Screen** installs the app.
  Check that it opens in Airplane Mode after one online visit.
- **Shortcuts app** for JARVIS. Lock Screen controls need iOS 18, and the
  Action button needs an iPhone 15 Pro or later.
- The Shortcut keeps the JARVIS-only token. If the phone is lost, revoke
  it in Settings › Device tokens.

---

## 11. Free tier and billing

| Service | Free allowance | What happens past it |
| --- | --- | --- |
| Workers (Free plan) | 100,000 requests/day; 10 ms CPU per request | Requests fail until the daily reset; no charge on the Free plan |
| Static assets | Free; they don't count as Worker requests (`run_worker_first` only for `/api/*`) | — |
| D1 | 5 GB storage; 5 M rows read, 100 k rows written per day | Queries fail until the reset; no charge on the Free plan |
| R2 (Standard) | 10 GB-month storage; 1 M Class A and 10 M Class B operations/month; free egress | **Billed to the payment method** (pay as you go) |

- **R2 is the one place a charge is possible.** Directory caps file
  storage per owner with `DIRECTORY_QUOTA_BYTES` (§4.1 sets 100 MB on
  staging), far below 10 GB.
- Check *R2 › Metrics* and *Workers & Pages › directory-staging › Metrics*
  now and then.
- Don't enable Workers Paid, Logpush, Workers AI, Queues, Durable Objects,
  custom domains with paid features, or Cloudflare Access without deciding
  first. None are used.
- Cloudflare Access (optional, owner decision): its free plan may also ask
  for a payment method. The app's own passkey auth already protects all
  data. Apple Shortcuts can't complete an Access login, so `/api/v1/jarvis/ask`
  would need a bypass rule.
- CPU: in Metrics, check CPU time for `/api/v1/auth/login/verify` against
  the 10 ms limit, and note it in PROGRESS.md.

---

## 12. Troubleshooting and rollback

| Symptom | Likely cause | Fix |
| --- | --- | --- |
| `worker-build: command not found` during deploy | Rust tools not on `PATH` | Run the `$env:Path = "$HOME\.cargo\bin;$env:Path"` line (§1) |
| Deploy fails about the D1 database / `database_id` | Placeholder zeros, or the id is in the wrong block | Paste the id from §3 into `[[env.staging.d1_databases]]` |
| Deploy or bucket creation fails mentioning R2 | R2 not activated on the account | §2 step 3 |
| Health 503: `migrations not applied` / `schema vN, expected v22` | Migrations missing | §5 |
| `503 auth_not_configured` | `DIRECTORY_RP_ID`/`DIRECTORY_ORIGINS` empty, or setup token not set | §4.1, §4.2, then redeploy |
| `401 setup_token_invalid` during setup | Wrong setup token | Re-enter it, or `secret put` a new one (§4.2) |
| Passkey prompt errors or "authentication failed" | RP id or origin doesn't match the URL exactly (http vs https, trailing slash, other hostname) | Fix §4.1 and redeploy. Passkeys made for another hostname won't work |
| `403 origin_not_allowed` on saves | `DIRECTORY_ORIGINS` doesn't exactly match the browser's origin | §4.1 |
| `429 rate_limited` while signing in | 30 sign-in requests per 10 minutes per IP | Wait for the `Retry-After` time |
| `403 token_scope` | A JARVIS-only token used outside `/jarvis/ask` | Expected; use a Full token for other routes |
| Old screens after a deploy | The service worker is still serving the previous version | Reload twice, or close and reopen the tab |

Logs: `npx wrangler tail --env staging`.

**Rolling back code** (💻; doesn't touch data):

```powershell
npx wrangler deployments list --env staging
npx wrangler rollback --env staging --message "back to previous"
```

**Rolling back data** (🧑 decision; **destructive**: it overwrites the
staging database with an earlier state):

```powershell
npx wrangler d1 time-travel info directory-db-staging        # current bookmark
npx wrangler d1 time-travel restore directory-db-staging --timestamp "2026-10-24T12:00:00Z"
```

Use a time from before the problem. Rolling back code doesn't undo a
migration, and migrations are never edited, so a bad schema change is
fixed with a new migration or a time-travel restore.

**Taking staging down** (🧑 decision; **destructive**, not undoable):

```powershell
npx wrangler delete --env staging
npx wrangler d1 delete directory-db-staging
npx wrangler r2 bucket delete directory-files-staging   # the bucket must be empty
```

---

## After staging passes

Record in PROGRESS.md:
- the remote smoke result;
- the iPhone Shortcut result;
- CPU time for sign-in;
- any phrase JARVIS misunderstood.

Production uses the top-level config and is a **separate decision**
(docs/ARCHITECTURE.md › Deployment).
