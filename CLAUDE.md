# CLAUDE.md

Claude Code reads this file; Codex reads AGENTS.md. The project rules live in
AGENTS.md and apply to Claude Code unchanged:

@AGENTS.md

Claude-specific setup: `.claude/settings.json` runs the shared handoff scripts
as hooks (SessionStart → `scripts/session-start.sh`, PreCompact →
`scripts/pre-compact.sh`, Stop → `scripts/handoff-check.sh --hook-json`).

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
