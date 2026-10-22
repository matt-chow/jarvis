#!/usr/bin/env bash
# Before context compaction: warn about uncommitted work and save a diagnostic
# snapshot to .handoff/ (gitignored) so the next context can recover state.
# Never commits, stashes, resets, or edits tracked files.
#
# Usage: bash scripts/pre-compact.sh
# Claude Code runs it as a PreCompact hook and passes JSON on stdin
# ({"trigger": "manual"|"auto", "transcript_path": ...}); it is optional.
set -u

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "pre-compact: not inside a Git work tree" >&2
  exit 0
fi
ROOT=$(git rev-parse --show-toplevel)
cd "$ROOT" || exit 0

input=""
if [ ! -t 0 ]; then
  input=$(cat 2>/dev/null || true)
fi
json_field() {
  printf '%s' "$input" | sed -n "s/.*\"$1\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p" | head -1
}
trigger=$(json_field trigger)
transcript=$(json_field transcript_path)

mkdir -p .handoff
stamp=$(date +%Y%m%d-%H%M%S)
out=".handoff/precompact-$stamp.txt"
{
  echo "# Pre-compaction snapshot $(date '+%Y-%m-%d %H:%M:%S %z')"
  echo "trigger: ${trigger:-unknown}"
  [ -n "$transcript" ] && echo "transcript: $transcript"
  echo
  echo "## HEAD"
  git log -1 --format='%H %s' 2>/dev/null || echo "(no commits)"
  echo "branch: $(git symbolic-ref --short -q HEAD || echo detached)"
  echo
  echo "## Size"
  bash scripts/handoff-check.sh 2>&1
  echo
  echo "## git status --short"
  git status --short
  echo
  echo "## git diff HEAD --stat"
  git diff HEAD --stat 2>/dev/null
  echo
  echo "## Recent commits"
  git log -10 --format='%h %s' 2>/dev/null
  echo
  echo "## Stashes"
  git stash list 2>/dev/null
  if [ -s HANDOVER.md ]; then
    echo
    echo "## HANDOVER.md"
    cat HANDOVER.md
  fi
} >"$out" 2>&1

# Keep the 20 newest snapshots.
ls -1t .handoff/precompact-*.txt 2>/dev/null | tail -n +21 | while IFS= read -r old; do rm -f -- "$old"; done

if [ -n "$(git status --porcelain)" ]; then
  echo "PRE-COMPACT: uncommitted work is in the tree (nothing was committed or stashed)."
  bash scripts/handoff-check.sh --quiet
  echo "After compaction: re-read $out and HANDOVER.md, then run bash scripts/session-start.sh."
else
  echo "PRE-COMPACT: working tree clean. Snapshot: $out"
fi
exit 0
