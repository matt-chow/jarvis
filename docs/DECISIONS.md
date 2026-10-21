# Decisions

Architecture decision log. Add new entries at the bottom; supersede entries
rather than editing them.

## D-001 — One Worker for UI and API (modular monolith)
**Status:** accepted (Sprint 00)
A single Worker uses Static Assets for the PWA, and `run_worker_first` routes only
`/api/*` to Rust. This means one deploy, one origin (no CORS, simpler auth
cookies later), and static traffic that doesn't count against Worker requests.
Pages and separate API Workers were rejected as unnecessary services.

## D-002 — Rust core crate separate from the Workers adapter
**Status:** accepted
`directory-core` has no Cloudflare dependency and exposes async traits
(`ObjectStore`, `SystemRepository`). `directory-worker` implements them with
`workers-rs` 0.8 (built by `worker-build`). Core is tested natively with plain
`cargo test` and is reusable by Windows and iOS clients. The traits don't require
`Send`, because Workers is single-threaded.

## D-003 — No PWA framework; hand-written service worker
**Status:** accepted
A ~20-line Vite plugin injects the precache list into `sw/service-worker.js`.
This avoids Workbox and `vite-plugin-pwa` dependencies for a small,
well-understood caching policy. Revisit if background sync or complex runtime
caching is needed.

## D-004 — Sprint 00 auth: shared bearer token (temporary)
**Status:** accepted as a stopgap; **must be replaced in Sprint 01**
Private routes require `Authorization: Bearer <DIRECTORY_API_TOKEN>` (a Worker
secret, at least 32 characters, constant-time compare, fails closed). The
decision logic sits in core (`auth.rs`) so it can be swapped. Options for
Sprint 01:
(a) **Cloudflare Access** (Zero Trust free tier, up to 50 users), with the Worker
validating the Access JWT. No auth code to maintain, but native clients need
service tokens or an Access login flow.
(b) **Passkey (WebAuthn) + session cookie** stored in D1. Fully self-contained
and good on iPhone, but more code.
Recommendation: (a) for the web deploy, plus per-device API tokens for native
clients. **Needs owner decision.**

## D-005 — npm workspaces, pinned versions
**Status:** accepted
pnpm wasn't installed, and npm workspaces are sufficient. Exact versions are
pinned. npm 12 blocks install scripts by default, so `workerd` and `esbuild` are
allow-listed in `package.json#allowScripts`.

## D-006 — Tailwind v4 via `@tailwindcss/vite`, CSS-variable theme
**Status:** accepted
Design tokens are CSS variables redefined for `prefers-color-scheme: dark`. The
system font stack plus a monospace stack (Cascadia / SF Mono) avoids font
downloads and works offline.

## D-007 — Release profile without `strip`
**Status:** accepted
`strip = true` removes the wasm `target_features` section, and `wasm-bindgen`
then fails with "externref table required for catch wrappers". The release
profile uses `opt-level = "z"`, LTO, and one codegen unit; `wasm-opt` runs via
`worker-build`.

## D-008 — Hash-based client routing
**Status:** accepted
`#/module` routes need no router dependency and behave identically offline and
in installed PWAs. Revisit if deep links or shareable URLs need clean paths.
