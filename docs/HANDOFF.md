# Switching between Claude Code and Codex

Directory can be developed with either Claude Code or Codex CLI, in the same
working tree, one at a time. The rules both follow are the "Handoff protocol"
section in AGENTS.md and CLAUDE.md (identical copies). This page covers the
how-to. Nothing here switches CLIs automatically.

## The shared scripts

| Script | What it does | Changes files? |
| --- | --- | --- |
| `scripts/session-start.sh` | Resume briefing: branch, HEAD, upstream, in-progress Git operations, stashes, size of uncommitted work, changed paths, recent commits, HANDOVER.md, and PROGRESS.md › Next task | No |
| `scripts/handoff-check.sh` | Measures uncommitted work (staged, unstaged, and untracked, without double-counting) and warns past 300 lines or at 10 files. `--strict` exits 1 when exceeded; `--hook-json` is for the Claude Stop hook | No |
| `scripts/pre-compact.sh` | Before context compaction: warns about uncommitted work and saves a snapshot (status, diff stat, commits, stashes, HANDOVER.md) to `.handoff/precompact-*.txt`, keeping the 20 newest | Only `.handoff/` (gitignored) |
| `scripts/handoff.sh --to codex\|claude` | Validates the handoff: no merge/rebase/conflicts; work within thresholds (or `--allow-large`); HANDOVER.md complete and written for the current HEAD; AGENTS.md and CLAUDE.md protocols identical. `--record` appends one line to HANDOVER.md's Handoff log | Only with `--record` (append) |
| `scripts/test-handoff.sh` | Tests the scripts in throwaway repositories (clean and dirty trees) | No (temp dirs) |

Thresholds can be changed per run with `HANDOFF_MAX_LINES` and
`HANDOFF_MAX_FILES`.

Claude Code runs three of them as hooks (`.claude/settings.json`):
SessionStart → `session-start.sh` (its output becomes session context),
PreCompact → `pre-compact.sh`, Stop → `handoff-check.sh --hook-json` (shows
a warning when a turn ends with too much uncommitted work; it never blocks).
Codex has no hooks, so AGENTS.md tells it to run `session-start.sh` first.

## Handing off (either direction)

1. Let the current agent finish or pause at a safe point. Ideally the last
   verified slice is committed and the tree is clean or nearly clean.
2. Update HANDOVER.md (template below). Its `Base commit:` line must be the
   current short HEAD (`git rev-parse --short HEAD`). These HANDOVER.md edits
   may stay uncommitted.
3. Run `bash scripts/handoff.sh --to codex` (or `--to claude`). Fix whatever
   it reports until it says READY. Add `--record` to log the handoff.
4. Quit the current CLI, so only one tool edits the tree.
5. Start the other CLI in the repository root:
   - **Codex:** ask it to run `bash scripts/session-start.sh`, then read
     AGENTS.md and HANDOVER.md before doing anything else.
   - **Claude Code:** the SessionStart hook prints the briefing. You can also
     run `bash scripts/session-start.sh` yourself.
6. The receiving agent continues from HANDOVER.md › Next steps. When the
   handoff is complete, it moves finished items to a "Done" list rather than
   deleting them.

## Restoring work in progress safely

- Uncommitted changes stay in the working tree; they are the handoff. The next
  agent picks them up as they are. Nothing is stashed or reset for a handoff.
- If the owner stashed something by hand, the briefing lists it. Look before
  restoring: `git stash show -p stash@{0}`. Restore only with the owner's
  agreement (`git stash apply`, which keeps the stash until you've checked
  the result), and never over uncommitted changes you haven't reviewed.
- If a session was compacted, read the newest `.handoff/precompact-*.txt`
  together with HANDOVER.md.
- If the scripts report a merge or rebase in progress, or conflicts, the
  owner resolves them before either agent continues.
- Agents never push, deploy, reset, or commit just to hand off. Committing a
  finished, verified slice is normal work (AGENTS.md › Ground rules).

## HANDOVER.md template

```markdown
# Handover

Base commit: <short HEAD>
From: <claude|codex> → To: <codex|claude>
Updated: <YYYY-MM-DD HH:MM>

## Status
One or two sentences: what's done, and whether the gate (`npm run ci`) passed.

## In progress
The slice being worked on, its acceptance criteria, and what's left.

## Next steps
1. The very next action, specific enough to start without asking.

## Working tree
What's uncommitted and why (or "clean"). Mention any stashes.

## Notes
Decisions, gotchas, commands that failed and why.

## Handoff log
- (appended by `scripts/handoff.sh --record`)
```

## Testing the scripts

```sh
bash scripts/test-handoff.sh   # also part of npm run verify
```
