#!/usr/bin/env bash
# Print a concise resume briefing for an agent (Claude or Codex) or a human
# starting a session. Read-only: inspects Git and HANDOVER.md, changes nothing.
#
# Usage: bash scripts/session-start.sh
# Claude Code runs it as a SessionStart hook (.claude/settings.json); its output
# becomes session context. Codex has no hooks: run it first (see AGENTS.md).
set -u

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "session-start: not inside a Git work tree"
  exit 0
fi
ROOT=$(git rev-parse --show-toplevel)
cd "$ROOT" || exit 0
SCRIPTS="$ROOT/scripts"

echo "=== Directory session briefing ==="
branch=$(git symbolic-ref --short -q HEAD || echo "(detached)")
if git rev-parse --verify -q HEAD >/dev/null; then
  echo "Branch: $branch @ $(git log -1 --format='%h %s (%cr)')"
else
  echo "Branch: $branch (no commits yet)"
fi
if upstream=$(git rev-parse --abbrev-ref --symbolic-full-name '@{u}' 2>/dev/null); then
  read -r behind ahead < <(git rev-list --left-right --count "$upstream"...HEAD 2>/dev/null || echo "0 0")
  echo "Upstream: $upstream (ahead $ahead, behind $behind)"
fi

gitdir=$(git rev-parse --git-dir)
for state in MERGE_HEAD REBASE_HEAD CHERRY_PICK_HEAD REVERT_HEAD; do
  [ -e "$gitdir/$state" ] && echo "ATTENTION: a ${state%_HEAD} is in progress. Resolve it before other work."
done
[ -d "$gitdir/rebase-merge" ] || [ -d "$gitdir/rebase-apply" ] && echo "ATTENTION: a rebase is in progress."
conflicts=$(git diff --name-only --diff-filter=U 2>/dev/null)
[ -n "$conflicts" ] && echo "ATTENTION: unmerged files: $(echo "$conflicts" | tr '\n' ' ')"

stashes=$(git stash list 2>/dev/null | wc -l | tr -d ' ')
[ "$stashes" -gt 0 ] && echo "Stashes: $stashes (not applied automatically; inspect with: git stash list)"

echo
bash "$SCRIPTS/handoff-check.sh" 2>&1
if [ -n "$(git status --porcelain)" ]; then
  echo "Changed paths (git status --short, first 20):"
  git status --short | head -20 | sed 's/^/  /'
fi

echo
echo "Recent commits:"
git log -5 --format='  %h %s' 2>/dev/null || true

echo
if [ -s HANDOVER.md ]; then
  echo "HANDOVER.md (last updated $(git log -1 --format='%cr' -- HANDOVER.md 2>/dev/null || echo 'never committed'); first 40 lines):"
  sed -n '1,40p' HANDOVER.md | sed 's/^/  /'
  [ "$(wc -l <HANDOVER.md)" -gt 40 ] && echo "  … (read the whole file before continuing)"
elif [ -e HANDOVER.md ]; then
  echo "HANDOVER.md exists but is empty: no handoff in progress."
else
  echo "No HANDOVER.md: no handoff in progress."
fi

if [ -f PROGRESS.md ]; then
  echo
  echo "PROGRESS.md › Next task:"
  awk '/^### Next task/{f=1; next} f && /^(#|---)/{exit} f' PROGRESS.md | sed '/^$/d; s/^/  /'
fi

echo
echo "Before editing: read AGENTS.md (and HANDOVER.md if present). Do not restore stashes or reset anything without the owner."
exit 0
