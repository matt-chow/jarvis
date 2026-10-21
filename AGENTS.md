# Agent Guide

Instructions for AI coding agents (and humans) working in this repository.

## Ground rules

- Read README.md, docs/ARCHITECTURE.md, and docs/DECISIONS.md before changing structure.
- Keep `crates/directory-core` free of Cloudflare and `worker` imports. Add
  traits there and implement them in `crates/directory-worker`.
- Every route that touches personal data must call `require_auth`
  (`crates/directory-worker/src/http.rs`). Never make the R2 bucket public.
- Never commit secrets. `.dev.vars` is gitignored; use `.dev.vars.example`.
- Free tier only. Don't add paid Cloudflare products, bindings, or third-party
  services without explicit owner approval.
- Don't add dependencies unless the current task needs them; pin exact versions.
- Migrations are append-only. Never edit an applied file in `migrations/`.
- The service worker must never cache `/api/*`.
- Don't claim a deployment or integration works without running it (smoke or
  offline check).
- Don't start the next sprint unprompted. Update PROGRESS.md at the end of
  each sprint.

## Commands

```sh
npm run verify          # fmt, clippy, typecheck, all unit tests, builds, deploy dry run
npm run dev:worker      # local Worker with D1/R2 (restart after `npm run build`)
npm run smoke           # API end-to-end against :8787 (or BASE_URL)
npm run test:offline    # headless browser offline check (after `npm run build`)
```

On Windows with the GNU Rust host, prefix PATH with `~/.cargo/bin`. MinGW
(`~/tools/winlibs/mingw64/bin`) is only needed for `cargo install worker-build`.

## Style

- Rust: `cargo fmt`, clippy with `-D warnings`. Errors go to typed enums in core,
  mapped to HTTP in the adapter.
- TypeScript: strict mode, no `any`. Tests are colocated as `*.test.ts` (Vitest,
  node environment, `fake-indexeddb`).
- UI: use the tokens in `src/index.css` (`bg-panel`, `text-muted`,
  `text-accent`, …) and the `Panel`, `Button`, and `StatusChip` components.
