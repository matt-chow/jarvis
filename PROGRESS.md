# Progress

## Sprint 00 — Infrastructure Foundation (complete, 2026-10-21)

### Delivered
- Monorepo: npm workspace (`apps/web`) plus a Cargo workspace
  (`directory-core`, `directory-worker`).
- Responsive app shell (sidebar / phone tab bar, light and dark) with the Home,
  Scratchpad, Files, and JARVIS (placeholder) modules.
- Rust Worker (workers-rs 0.8.7) with `GET /api/v1/health`, which checks the D1
  schema version and the R2 binding.
- D1: `migrations/0001_initial.sql` (`system_meta`), applied locally with Wrangler.
- R2 proof of concept: authenticated PUT/GET/LIST/DELETE through the core
  `ObjectStore` trait, against a private bucket.
- PWA: manifest, icons (including the iOS touch icon), generated service worker
  with precache and update prompt, IndexedDB notes, and a cached health report.
- Deployment config (`wrangler.toml`), setup steps in docs/ARCHITECTURE.md, and the
  full doc set.

### Verification (all run locally on Windows 10, 2026-10-21)
| Check | Result |
| --- | --- |
| `cargo test -p directory-core` | 15 passed |
| Vitest (`apps/web`) | 10 passed (3 files) |
| clippy (core native, worker wasm32) `-D warnings`, `cargo fmt --check`, `tsc` | clean |
| `npm run verify` (incl. `wrangler deploy --dry-run`) | pass. Upload 508 KiB, 175 KiB gzipped |
| `npm run smoke` vs `wrangler dev` (local D1 + R2) | 10/10 pass |
| Unmigrated D1 → health `503` "migrations not applied" | verified manually |
| Weak or missing secret → storage `503 auth_not_configured` | verified manually |
| `npm run test:offline` (headless Edge 155) | 6/6 pass: SW controls page, shell renders with server down, cached health shown, IndexedDB survives reload |

### Not done / not verified
- **No deployment to Cloudflare was performed.** It needs `wrangler login`, D1
  and R2 creation (R2 activation asks for a payment method), and owner approval.
  Remote D1, remote R2, and production headers are untested.
- iOS Safari install and offline behaviour have not been tested on a device.
- No CI pipeline yet.

### Known limitations
- Shared bearer token is not real identity (DECISIONS D-004). Don't store
  personal data in a deployed instance yet.
- Files PoC buffers whole bodies in memory (10 MB cap). No multipart, folders,
  or pagination cursor.
- Scratchpad notes are device-local; no sync.
- `wrangler dev` must be restarted after rebuilding `apps/web/dist`.
- The repo lives under OneDrive. Syncing `node_modules`/`target` is slow and can
  lock files; moving it outside OneDrive is recommended.
