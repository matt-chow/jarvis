# Agent Guide

Instructions for AI coding agents (and humans) working in this repository.

## Ground rules

- Read README.md, docs/ARCHITECTURE.md, and docs/DECISIONS.md before changing structure.
- Keep `crates/directory-core` free of Cloudflare and `worker` imports. Add
  traits there and implement them in `crates/directory-worker`.
- Every route that touches personal data must resolve a `Principal` with
  `http::auth::authenticate` and scope queries by `principal.user_id`.
  Never make the R2 bucket public. Changing auth or security policy needs owner
  approval.
- Never commit secrets. `.dev.vars` is gitignored; use `.dev.vars.example`.
- Free tier only. Don't add paid Cloudflare products, bindings, or third-party
  services without explicit owner approval.
- Don't add dependencies unless the current task needs them; pin exact versions.
- Migrations are append-only. Never edit an applied file in `migrations/`.
- The service worker must never cache `/api/*`.
- Don't claim a deployment or integration works without running it (smoke or
  offline check).
- Work in vertical slices (see docs/ROADMAP.md milestones). Commit each
  verified slice on `main` (owner-approved policy); never push or deploy
  without approval. Update PROGRESS.md after every slice.
- Screens without an approved Paper design are provisional (DECISIONS D-013):
  reuse existing components and don't invent a new aesthetic.

## Definition of Done

Where applicable, a slice must have:
- Working UI wired to the API, with persistence.
- Validation in core and authorization by `Principal`.
- Loading, empty, error, and success states.
- Offline behaviour where required.
- Core unit tests, web tests, smoke checks, and a browser e2e step for key flows.
- Updated docs.

No inert controls, and no mock-only features presented as done.

## Commands

```sh
npm run ci              # full gate: verify + smoke + browser e2e (run before committing a slice)
npm run verify          # fmt, clippy, typecheck, all unit tests, builds, deploy dry run
npm run dev:worker      # local Worker with D1/R2 (restart after `npm run build`)
npm run smoke           # API end-to-end on an isolated wrangler dev (or BASE_URL)
npm run test:browser    # headless browser e2e with virtual passkey (after `npm run build`)
npm run smoke:remote-local  # the staging (BASE_URL) smoke checks, against an isolated local Worker
bash scripts/session-start.sh             # resume briefing (run first; read-only)
bash scripts/handoff-check.sh             # size of uncommitted work vs handoff thresholds
bash scripts/handoff.sh --to codex|claude # validate a handoff (see Handoff protocol)
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

<!-- handoff-protocol:start -->
## Handoff protocol (Claude Code ↔ Codex)

This section is identical in AGENTS.md and CLAUDE.md; `scripts/handoff.sh`
checks that. Change both copies together. Details: docs/HANDOFF.md.

- **One editor at a time.** Only one CLI works in the tree. Stop one before
  starting the other.
- **Start of every session:** run `bash scripts/session-start.sh` (Claude Code
  runs it automatically as a SessionStart hook). If HANDOVER.md has content,
  read all of it before editing, and continue from its "Next steps".
- **Keep handoffs small.** `bash scripts/handoff-check.sh` warns past 300
  changed lines or at 10 changed files (staged, unstaged, and untracked
  together). Commit each verified slice (AGENTS.md › Ground rules) instead of
  handing over large uncommitted work.
- **Before handing off:** update HANDOVER.md: `Base commit: <short HEAD>`, and
  the sections Status, In progress, Next steps, and Working tree (what is
  uncommitted and why). Then run `bash scripts/handoff.sh --to codex|claude`
  until it reports READY (add `--record` to append to the handoff log).
- **Never automatic:** agents do not push, deploy, reset, commit to hand off,
  or create, apply, or drop stashes as part of a handoff. Uncommitted work
  stays in the tree for the next agent; restoring a stash is the owner's call.
- **HANDOVER.md is append-or-edit, never wipe.** Keep earlier entries (move
  finished items to a "Done" list, or clear the file only once the owner
  confirms the handoff is complete).
- Before compaction, `scripts/pre-compact.sh` saves a snapshot to `.handoff/`
  (gitignored). After compaction, re-read it and HANDOVER.md.
<!-- handoff-protocol:end -->
