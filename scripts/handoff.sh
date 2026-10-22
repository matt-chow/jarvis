#!/usr/bin/env bash
# Validate that the repository is safe to hand to the other CLI (Claude Code ↔
# Codex) and print what the receiving agent should do first.
#
# Usage: bash scripts/handoff.sh --to codex|claude [--record] [--allow-large]
#   --record       append one line to HANDOVER.md's "Handoff log" (append-only;
#                  existing content is never rewritten). Without it, read-only.
#   --allow-large  hand off even though the uncommitted work exceeds the
#                  thresholds in scripts/handoff-check.sh (say why in HANDOVER.md)
# Exit status: 0 = ready, 1 = not ready (reasons printed), 2 = usage error.
# Never commits, stashes, resets, pushes, or switches CLIs.
set -u

TO=""
RECORD=0
ALLOW_LARGE=0
while [ $# -gt 0 ]; do
  case "$1" in
    --to) TO="${2:-}"; shift ;;
    --to=*) TO="${1#--to=}" ;;
    --record) RECORD=1 ;;
    --allow-large) ALLOW_LARGE=1 ;;
    -h | --help) sed -n '2,12p' "$0"; exit 0 ;;
    *) echo "handoff: unknown option $1" >&2; exit 2 ;;
  esac
  shift
done
case "$TO" in
  codex | claude) ;;
  *) echo "handoff: pass --to codex or --to claude" >&2; exit 2 ;;
esac

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "handoff: not inside a Git work tree" >&2
  exit 2
fi
ROOT=$(git rev-parse --show-toplevel)
cd "$ROOT" || exit 2

problems=0
fail() { echo "  ✗ $*"; problems=$((problems + 1)); }
pass() { echo "  ✓ $*"; }
warn() { echo "  ! $*"; }

echo "Handoff check → $TO"

# 1. Git state: no operation half-done, no conflicts.
gitdir=$(git rev-parse --git-dir)
busy=""
for state in MERGE_HEAD CHERRY_PICK_HEAD REVERT_HEAD; do
  [ -e "$gitdir/$state" ] && busy="$busy ${state%_HEAD}"
done
{ [ -d "$gitdir/rebase-merge" ] || [ -d "$gitdir/rebase-apply" ]; } && busy="$busy REBASE"
if [ -n "$busy" ]; then fail "Git operation in progress:$busy"; else pass "no merge/rebase/cherry-pick in progress"; fi
if [ -n "$(git diff --name-only --diff-filter=U)" ]; then fail "unmerged (conflicted) files"; else pass "no conflicts"; fi

if git rev-parse --verify -q HEAD >/dev/null; then
  head=$(git rev-parse --short HEAD)
  pass "HEAD is $head ($(git symbolic-ref --short -q HEAD || echo detached))"
else
  head=""
  fail "no commits yet"
fi

# 2. Size of uncommitted work.
if bash scripts/handoff-check.sh --strict --quiet 2>/dev/null; then
  pass "uncommitted work within thresholds"
elif [ "$ALLOW_LARGE" -eq 1 ]; then
  warn "uncommitted work exceeds thresholds (allowed with --allow-large):"
  bash scripts/handoff-check.sh --quiet 2>&1 | sed 's/^/      /'
else
  fail "uncommitted work exceeds thresholds:"
  bash scripts/handoff-check.sh --quiet 2>&1 | sed 's/^/      /'
fi
stashes=$(git stash list | wc -l | tr -d ' ')
[ "$stashes" -gt 0 ] && warn "$stashes stash(es) exist; the next agent won't apply them. Mention them in HANDOVER.md."

# 3. HANDOVER.md: present, structured, and written for this HEAD.
if [ ! -s HANDOVER.md ]; then
  fail "HANDOVER.md is missing or empty (fill it from the template in docs/HANDOFF.md)"
else
  missing=""
  for section in "## Status" "## In progress" "## Next steps" "## Working tree"; do
    grep -q "^$section" HANDOVER.md || missing="$missing \"$section\""
  done
  if [ -n "$missing" ]; then fail "HANDOVER.md lacks section(s):$missing"; else pass "HANDOVER.md has the required sections"; fi
  if [ -n "$head" ] && grep -Eq "^Base commit: *$head" HANDOVER.md; then
    pass "HANDOVER.md is written for HEAD $head"
  else
    fail "HANDOVER.md must contain the line \"Base commit: $head\" (it may describe an older state)"
  fi
fi

# 4. Both CLIs read the same rules.
extract() { sed -n '/<!-- handoff-protocol:start -->/,/<!-- handoff-protocol:end -->/p' "$1" 2>/dev/null; }
a=$(extract AGENTS.md)
c=$(extract CLAUDE.md)
if [ -z "$a" ] || [ -z "$c" ]; then
  fail "AGENTS.md and CLAUDE.md must both contain the shared handoff-protocol section"
elif [ "$a" != "$c" ]; then
  fail "the handoff-protocol sections in AGENTS.md and CLAUDE.md differ"
else
  pass "AGENTS.md and CLAUDE.md share the same handoff protocol"
fi

echo
if [ "$problems" -gt 0 ]; then
  echo "NOT READY: $problems problem(s). Fix them, then run this again."
  exit 1
fi

if [ "$RECORD" -eq 1 ]; then
  grep -q "^## Handoff log" HANDOVER.md || printf '\n## Handoff log\n' >>HANDOVER.md
  printf -- '- %s → %s at %s (uncommitted: %s)\n' "$(date '+%Y-%m-%d %H:%M %z')" "$TO" "$head" \
    "$(bash scripts/handoff-check.sh 2>/dev/null | head -1 | sed 's/^Uncommitted work: //')" >>HANDOVER.md
  echo "Recorded the handoff in HANDOVER.md (appended to the Handoff log)."
fi

echo "READY to hand off to $TO."
if [ "$TO" = codex ]; then
  echo "Next: start Codex in $ROOT and have it run: bash scripts/session-start.sh (then read AGENTS.md and HANDOVER.md)."
else
  echo "Next: start Claude Code in $ROOT; its SessionStart hook prints the briefing (or run: bash scripts/session-start.sh)."
fi
echo "Only one CLI should edit the tree at a time: stop this one before starting the other."
exit 0
