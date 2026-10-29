# Directory

A modular, cloud-hosted, offline-capable personal operating environment, with a
future JARVIS voice/AI assistant. One Cloudflare Worker serves a React PWA and a
Rust API backed by D1 (structured data) and a private R2 bucket (files).

> Status: **MVP on staging.** The modules below work end to end, online and
> offline. Current work and next steps: [PROGRESS.md](PROGRESS.md).

## What it does

| Area | Modules |
| --- | --- |
| Plan | **Calendar** (unified planner: tasks, events, and work sessions in month, week, and day views; repeats; suggested times; [PLANNER](docs/PLANNER.md)) · **Tasks** · **Projects** · **Habits** |
| Capture and write | **Inbox** · **Notes** (wiki links and backlinks) · **Journal** |
| Files and media | **Files** · **Library** (PDF and ePub reader) · **Music** |
| Assistant | **JARVIS** (typed and spoken commands; changes need confirmation) · **Ask** (mobile conversation, the iPhone Home Screen app's default start page; [JARVIS_ASK](docs/JARVIS_ASK.md)) · **Automations** |
| Everywhere | **Home** dashboard with widgets · **Search** · quick task capture · **Settings** (passkeys, device tokens, export and restore, diagnostics) |

Sign-in is by passkey. All data lives in your own Cloudflare account on the
free tier, and edits made offline queue and sync when the connection returns.

## Repository layout

```
apps/web/                 React + TypeScript + Vite + Tailwind PWA
  src/modules/            One screen per module (Calendar, Tasks, Notes, JARVIS, …)
  src/components/         Shared UI (Panel, Button, StatusChip) and the planner grid
  src/hooks/              Data hooks: cached lists, optimistic edits, offline outbox
  src/lib/                API client, IndexedDB, sync, planner engine, time zones
  sw/service-worker.js    Service worker template (build emits dist/sw.js)
crates/directory-core/    Portable Rust domain logic — no Cloudflare dependencies
crates/directory-worker/  Cloudflare Worker adapter (routing, D1, R2)
migrations/               Versioned D1 SQL migrations (append-only)
scripts/                  smoke test, browser e2e, handoff checks, icon generator
wrangler.toml             Worker, assets, D1 and R2 configuration
```

Docs: [VISION](docs/VISION.md) · [PRD](docs/PRD.md) · [ARCHITECTURE](docs/ARCHITECTURE.md) ·
[MODULES](docs/MODULES.md) · [PLANNER](docs/PLANNER.md) · [JARVIS_ASK](docs/JARVIS_ASK.md) ·
[SECURITY](docs/SECURITY.md) · [STAGING](docs/STAGING.md) · [ROADMAP](docs/ROADMAP.md) ·
[DECISIONS](docs/DECISIONS.md) · [AGENTS](AGENTS.md) · [HANDOFF](docs/HANDOFF.md) · [PROGRESS](PROGRESS.md)

## Prerequisites

- Node.js ≥ 22 (tested on 24) and npm (tested on 12)
- Rust stable with the wasm target: `rustup target add wasm32-unknown-unknown`
- `worker-build`: `cargo install worker-build --version 0.8.7 --locked`

**Windows note.** Without Visual Studio Build Tools, use the GNU Rust host
(`rustup-init --default-host x86_64-pc-windows-gnu`). Installing `worker-build`
then needs a full MinGW-w64 on `PATH` (rustup's bundled one lacks `as.exe`), e.g.
[WinLibs](https://winlibs.com/) extracted to `~/tools/winlibs`:

```sh
PATH="$HOME/tools/winlibs/mingw64/bin:$PATH" cargo install worker-build --version 0.8.7 --locked
```

MinGW is only needed for that install; building the Worker itself does not need it.

## Run locally

```sh
npm install                      # also installs wrangler (pinned)
cp .dev.vars.example .dev.vars   # local setup token + passkey origin (gitignored)
npm run db:migrate:local         # create local D1 schema
npm run build                    # build the PWA into apps/web/dist
npm run dev:worker               # applies local D1 migrations, then Worker + assets + local D1/R2
```

Open **http://localhost:8787**, not 127.0.0.1: passkeys need a hostname. On
first run, enter the `DIRECTORY_SETUP_TOKEN` from `.dev.vars` and create a
passkey (Windows Hello, Touch ID, Face ID, or a security key).

For UI work with hot reload, run `npm run dev` in a second terminal
(http://localhost:5173, proxies `/api` to 8787; that origin is already in
`DIRECTORY_ORIGINS`). The service worker is disabled in the Vite dev server;
test offline behaviour against `dev:worker`.

`wrangler dev` does not pick up a rebuilt `apps/web/dist`. Restart it after
`npm run build`, or new hashed assets fall through to the SPA fallback.

## Verify

| Command | What it checks |
| --- | --- |
| `npm run ci` | Everything below in order: the full pre-deploy gate |
| `npm run verify` | fmt + clippy + TS typecheck, Rust and web unit tests, PWA build, Worker build (deploy dry run) |
| `npm test` | Rust core tests (`cargo test`) and web tests (Vitest) |
| `npm run smoke` | Starts an isolated `wrangler dev` (fresh D1/R2) and runs the API end-to-end: setup, passkey login (software authenticator), sessions, CSRF, device tokens, every module's API, files and quota |
| `npm run test:browser` | Headless Chrome/Edge with a virtual passkey: every module's key flow (including the planner), offline edits, accessibility, error recovery, offline relaunch (run `npm run build` first) |

## Deploy

Never deploy without the owner's approval. Migrations are append-only; apply
them before deploying the Worker that needs them.

**Staging** (one-time setup and checks: [docs/STAGING.md](docs/STAGING.md)):

```sh
npx wrangler d1 time-travel info <staging-db>   # note a restore point first
npx wrangler d1 migrations list DB --remote --env staging
npm run db:migrate:staging
npm run deploy:staging
```

Then open `/api/v1/health` on the staging host: it should report the schema
version the code expects (see docs/ARCHITECTURE.md).

**Production:** one-time Cloudflare setup in
[docs/ARCHITECTURE.md › Deployment](docs/ARCHITECTURE.md#deployment), then
`npm run db:migrate:remote` and `npm run deploy`. To run the smoke test
against a deployed Worker, create a device token in Settings and pass it in the
environment (never commit it):

```sh
BASE_URL=https://<your-worker-host> DIRECTORY_API_TOKEN=<token> npm run smoke
```
