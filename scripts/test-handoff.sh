#!/usr/bin/env bash
# Tests for the handoff scripts, run in throwaway Git repositories (the real
# working tree is never touched). Usage: bash scripts/test-handoff.sh
set -u

SRC="$(cd "$(dirname "$0")" && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
pass=0
fail=0

ok() { pass=$((pass + 1)); }
bad() { fail=$((fail + 1)); echo "  FAIL: $*"; }
check() { # check "<description>" <condition command…>
  local what="$1"; shift
  if "$@"; then ok; else bad "$what"; fi
}
has() { grep -qF -- "$2" <<<"$1"; }
lacks() { ! grep -qF -- "$2" <<<"$1"; }

# A fresh repository with the scripts and the shared protocol in place.
new_repo() {
  local dir="$TMP/$1"
  mkdir -p "$dir/scripts"
  cp "$SRC"/handoff-check.sh "$SRC"/session-start.sh "$SRC"/pre-compact.sh "$SRC"/handoff.sh "$dir/scripts/"
  (
    cd "$dir" || exit 1
    git init -q
    git config user.email test@example.com
    git config user.name Test
    git config core.autocrlf false
    printf '.handoff/\nignored.txt\n' >.gitignore
    printf 'rules\n<!-- handoff-protocol:start -->\nsame\n<!-- handoff-protocol:end -->\n' >AGENTS.md
    cp AGENTS.md CLAUDE.md
    : >HANDOVER.md
  )
  echo "$dir"
}
commit_all() { git -C "$1" add -A && git -C "$1" commit -qm "${2:-commit}"; }
lines() { for i in $(seq 1 "$2"); do echo "line $i"; done >"$1"; }
check_out() { (cd "$1" && bash scripts/handoff-check.sh "${@:2}" 2>&1); }

echo "handoff-check.sh"
OUT=$(cd "$TMP" && bash "$SRC/handoff-check.sh" 2>&1); code=$?
check "outside a repo exits 2" test "$code" -eq 2

R=$(new_repo nohead)
lines "$R/a.txt" 3
OUT=$(check_out "$R")
# 4 scripts, .gitignore, AGENTS.md, CLAUDE.md, HANDOVER.md, a.txt
check "repo without commits counts everything as untracked" has "$OUT" "untracked: 9 file(s)"

R=$(new_repo clean)
commit_all "$R" init
OUT=$(check_out "$R")
check "clean tree: zero" has "$OUT" "Uncommitted work: 0 file(s), 0 changed line(s)"
check "clean tree: no warning" lacks "$OUT" "WARNING"
(cd "$R" && bash scripts/handoff-check.sh --strict >/dev/null 2>&1); check "clean tree: --strict passes" test $? -eq 0

lines "$R/f.txt" 5 && git -C "$R" add f.txt
lines "$R/f.txt" 10
OUT=$(check_out "$R")
check "staged + unstaged edits to one file are not double-counted" has "$OUT" "tracked:   1 file(s), +10/-0 (staged: 1, unstaged: 1"
check "…and total lines are the net change" has "$OUT" "1 file(s), 10 changed line(s)"
commit_all "$R" f

printf 'no newline' >"$R/partial.txt"
printf '\x00\x01\x02binary' >"$R/blob.bin"
echo "secret" >"$R/ignored.txt"
OUT=$(check_out "$R")
check "untracked: text without final newline counts, binary is 0 lines, ignored is skipped" has "$OUT" "untracked: 2 file(s), 1 line(s)"
rm -f "$R/partial.txt" "$R/blob.bin" "$R/ignored.txt"

git -C "$R" mv f.txt g.txt
OUT=$(check_out "$R")
check "a rename is one file" has "$OUT" "tracked:   1 file(s), +0/-0"
commit_all "$R" rename

lines "$R/big.txt" 300
OUT=$(check_out "$R")
check "300 lines: no warning" lacks "$OUT" "WARNING"
lines "$R/big.txt" 301
OUT=$(check_out "$R")
check "301 lines: warning" has "$OUT" "301 changed lines exceeds"
(cd "$R" && bash scripts/handoff-check.sh --strict >/dev/null 2>&1); check "--strict exits 1 when exceeded" test $? -eq 1
JSON=$(cd "$R" && bash scripts/handoff-check.sh --hook-json)
check "--hook-json prints a systemMessage when exceeded" has "$JSON" '"systemMessage"'
check "--hook-json output is valid JSON" node -e "JSON.parse(process.argv[1])" "$JSON"
(cd "$R" && HANDOFF_MAX_LINES=1000 bash scripts/handoff-check.sh --strict >/dev/null 2>&1); check "threshold override" test $? -eq 0
rm -f "$R/big.txt"

for i in 1 2 3 4 5 6 7 8 9; do echo x >"$R/n$i.txt"; done
OUT=$(check_out "$R")
check "9 files: no warning" lacks "$OUT" "WARNING"
echo x >"$R/n10.txt"
OUT=$(check_out "$R")
check "10 files: warning" has "$OUT" "10 changed files reaches"
JSON=$(cd "$R" && rm -f n*.txt && bash scripts/handoff-check.sh --hook-json)
check "--hook-json is silent within thresholds" test -z "$JSON"

echo "session-start.sh"
R=$(new_repo start)
commit_all "$R" "first commit"
before=$(cd "$R" && git status --porcelain && git stash list && git rev-parse HEAD)
OUT=$(cd "$R" && bash scripts/session-start.sh)
check "clean: briefing names the commit" has "$OUT" "first commit"
check "clean: empty HANDOVER.md means no handoff" has "$OUT" "HANDOVER.md exists but is empty"
printf '# Handover\n\nBase commit: abc\n## Next steps\n1. Do the thing\n' >"$R/HANDOVER.md"
echo wip >"$R/wip.txt"
before=$(cd "$R" && git status --porcelain && md5sum HANDOVER.md wip.txt)
OUT=$(cd "$R" && bash scripts/session-start.sh)
after=$(cd "$R" && git status --porcelain && md5sum HANDOVER.md wip.txt)
check "dirty: shows HANDOVER.md" has "$OUT" "1. Do the thing"
check "dirty: lists changed paths" has "$OUT" "?? wip.txt"
check "session-start does not modify the tree" test "$before" = "$after"
(cd "$R" && git stash -q -u)
OUT=$(cd "$R" && bash scripts/session-start.sh)
check "stashes are reported, not applied" has "$OUT" "Stashes: 1"
check "…and the stashed file stays stashed" test ! -e "$R/wip.txt"

echo "pre-compact.sh"
R=$(new_repo compact)
commit_all "$R" init
echo wip >"$R/wip.txt"
head_before=$(git -C "$R" rev-parse HEAD)
OUT=$(cd "$R" && echo '{"session_id":"s","trigger":"auto","transcript_path":"/tmp/t.jsonl"}' | bash scripts/pre-compact.sh)
check "warns about uncommitted work" has "$OUT" "uncommitted work is in the tree"
snap=$(ls "$R"/.handoff/precompact-*.txt 2>/dev/null | head -1)
check "writes a snapshot" test -n "$snap"
check "snapshot records the trigger and transcript" grep -q "trigger: auto" "$snap"
check "snapshot lists the work" grep -q "?? wip.txt" "$snap"
check "never commits" test "$(git -C "$R" rev-parse HEAD)" = "$head_before"
check "never stashes" test -z "$(git -C "$R" stash list)"
check "work stays in place" test -f "$R/wip.txt"
check "the snapshot folder is ignored" test -z "$(git -C "$R" status --porcelain .handoff)"
rm "$R/wip.txt"
OUT=$(cd "$R" && bash scripts/pre-compact.sh </dev/null)
check "clean tree: says so" has "$OUT" "working tree clean"

echo "handoff.sh"
R=$(new_repo handoff)
commit_all "$R" init
(cd "$R" && bash scripts/handoff.sh >/dev/null 2>&1); check "--to is required" test $? -eq 2
OUT=$(cd "$R" && bash scripts/handoff.sh --to codex); code=$?
check "empty HANDOVER.md: not ready" test "$code" -eq 1
check "…and says why" has "$OUT" "HANDOVER.md is missing or empty"
head=$(git -C "$R" rev-parse --short HEAD)
handover() {
  printf '# Handover\n\nBase commit: %s\n\n## Status\nok\n\n## In progress\nnone\n\n## Next steps\n1. go\n\n## Working tree\nclean\n' "$1" >"$R/HANDOVER.md"
}
handover "$head"
OUT=$(cd "$R" && bash scripts/handoff.sh --to codex); code=$?
check "complete handover: ready" test "$code" -eq 0
check "…with Codex startup instructions" has "$OUT" "bash scripts/session-start.sh"
handover 0000000
(cd "$R" && bash scripts/handoff.sh --to claude >/dev/null); check "stale base commit: not ready" test $? -eq 1
handover "$head"
printf 'rules\n<!-- handoff-protocol:start -->\ndifferent\n<!-- handoff-protocol:end -->\n' >"$R/CLAUDE.md"
OUT=$(cd "$R" && bash scripts/handoff.sh --to claude); code=$?
check "differing protocols: not ready" test "$code" -eq 1
check "…and says why" has "$OUT" "differ"
cp "$R/AGENTS.md" "$R/CLAUDE.md"
for i in $(seq 1 12); do echo x >"$R/w$i.txt"; done
(cd "$R" && bash scripts/handoff.sh --to codex >/dev/null); check "large work: not ready" test $? -eq 1
(cd "$R" && bash scripts/handoff.sh --to codex --allow-large >/dev/null); check "--allow-large: ready" test $? -eq 0
rm -f "$R"/w*.txt
cp "$R/HANDOVER.md" "$TMP/before.md"
(cd "$R" && bash scripts/handoff.sh --to codex --record >/dev/null)
check "--record keeps the existing content" bash -c "head -c \$(wc -c <'$TMP/before.md') '$R/HANDOVER.md' | cmp -s - '$TMP/before.md'"
check "--record appends to the Handoff log" grep -q "^- .* → codex at $head" "$R/HANDOVER.md"
(cd "$R" && bash scripts/handoff.sh --to codex >/dev/null 2>&1)
check "without --record nothing is written" test "$(grep -c '→ codex' "$R/HANDOVER.md")" -eq 1
git -C "$R" checkout -q -b side
echo side >"$R/c.txt" && commit_all "$R" side
git -C "$R" checkout -q -
echo main >"$R/c.txt" && commit_all "$R" main
git -C "$R" merge -q side >/dev/null 2>&1
OUT=$(cd "$R" && bash scripts/handoff.sh --to codex); code=$?
check "merge conflict: not ready" test "$code" -eq 1
check "…and names it" has "$OUT" "Git operation in progress: MERGE"
OUT=$(cd "$R" && bash scripts/session-start.sh)
check "session-start flags the merge" has "$OUT" "ATTENTION"

echo
echo "handoff scripts: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
