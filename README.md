# Directory

A modular, cloud-hosted, offline-capable personal operating environment, with a
future JARVIS voice/AI assistant. One Cloudflare Worker serves a React PWA and a
Rust API backed by D1 (structured data) and a private R2 bucket (files).

> Status: **Sprint 00 — infrastructure foundation.** See [PROGRESS.md](PROGRESS.md).

## Repository layout

```
apps/web/                 React + TypeScript + Vite + Tailwind PWA
  src/modules/            One component per Directory module (Home, Scratchpad, Files, JARVIS)
  src/lib/                api client, IndexedDB wrapper, module registry
  sw/service-worker.js    Service worker template (build emits dist/sw.js)
crates/directory-core/    Portable Rust domain logic — no Cloudflare dependencies
crates/directory-worker/  Cloudflare Worker adapter (routing, D1, R2)
migrations/               Versioned D1 SQL migrations (append-only)
scripts/                  smoke test, offline browser check, icon generator
wrangler.toml             Worker, assets, D1 and R2 configuration
```

Docs: [VISION](docs/VISION.md) · [PRD](docs/PRD.md) · [ARCHITECTURE](docs/ARCHITECTURE.md) ·
[MODULES](docs/MODULES.md) · [ROADMAP](docs/ROADMAP.md) · [DECISIONS](docs/DECISIONS.md) ·
[AGENTS](AGENTS.md) · [PROGRESS](PROGRESS.md)

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
cp .dev.vars.example .dev.vars   # local API token (gitignored)
npm run db:migrate:local         # create local D1 schema
npm run build                    # build the PWA into apps/web/dist
npm run dev:worker               # Worker + assets + local D1/R2 on http://127.0.0.1:8787
```

For UI work with hot reload, run `npm run dev` in a second terminal
(http://localhost:5173, proxies `/api` to 8787). The service worker is disabled in
the Vite dev server; test offline behaviour against `dev:worker`.

`wrangler dev` does not pick up a rebuilt `apps/web/dist`. Restart it after
`npm run build`, or new hashed assets fall through to the SPA fallback.

## Verify

| Command | What it checks |
| --- | --- |
| `npm run verify` | fmt + clippy + TS typecheck, Rust and web unit tests, PWA build, Worker build (deploy dry run) |
| `npm test` | Rust core tests (`cargo test`) and web tests (Vitest) |
| `npm run smoke` | End-to-end API checks against a running Worker (`BASE_URL`, default `http://127.0.0.1:8787`) |
| `npm run test:offline` | Headless Chrome/Edge: SW install, offline reload, IndexedDB persistence (needs `npm run build`) |

## Deploy

See [docs/ARCHITECTURE.md › Deployment](docs/ARCHITECTURE.md#deployment) for the one-time
Cloudflare setup (D1 database, private R2 bucket, API secret). After that:

```sh
npm run db:migrate:remote
npm run deploy
BASE_URL=https://directory.<subdomain>.workers.dev DIRECTORY_API_TOKEN=... npm run smoke
```
