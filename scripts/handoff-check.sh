#!/usr/bin/env bash
# Measure uncommitted work and warn when a handoff would carry too much of it.
#
# Usage: bash scripts/handoff-check.sh [--strict] [--quiet] [--hook-json]
#   --strict  exit 1 when a threshold is exceeded (default: always exit 0)
#   --quiet   print only warnings
#   --hook-json  for a Claude Code Stop hook: print {"systemMessage": …} only
#                when a threshold is exceeded, nothing otherwise; never blocks
# Thresholds (env): HANDOFF_MAX_LINES (default 300, warn when exceeded),
#                   HANDOFF_MAX_FILES (default 10, warn when reached).
#
# Counting: tracked changes are measured once against HEAD (staged and
# unstaged edits to the same file are not double-counted); untracked files
# (respecting .gitignore) add their line counts. Binary files count as files
# with 0 lines. Read-only: never modifies the working tree or the index.
set -u

MAX_LINES="${HANDOFF_MAX_LINES:-300}"
MAX_FILES="${HANDOFF_MAX_FILES:-10}"
STRICT=0
QUIET=0
HOOK=0
for arg in "$@"; do
  case "$arg" in
    --strict) STRICT=1 ;;
    --quiet) QUIET=1 ;;
    --hook-json) HOOK=1; QUIET=1 ;;
    -h | --help) sed -n '2,15p' "$0"; exit 0 ;;
    *) echo "handoff-check: unknown option $arg" >&2; exit 2 ;;
  esac
done

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "handoff-check: not inside a Git work tree" >&2
  exit 2
fi
cd "$(git rev-parse --show-toplevel)" || exit 2

# Compare against HEAD, or the empty tree in a repo with no commits yet.
if git rev-parse --verify -q HEAD >/dev/null; then
  BASE=HEAD
else
  BASE=$(git hash-object -t tree /dev/null)
fi

added=0
deleted=0
tracked_files=0
# numstat: "<added>\t<deleted>\t<path>", "-" for binary; renames as one entry.
while IFS=$'\t' read -r a d _path; do
  [ -z "${a:-}" ] && continue
  tracked_files=$((tracked_files + 1))
  [ "$a" != "-" ] && added=$((added + a))
  [ "$d" != "-" ] && deleted=$((deleted + d))
done < <(git diff "$BASE" --numstat -M 2>/dev/null)

untracked_files=0
untracked_lines=0
while IFS= read -r -d '' f; do
  untracked_files=$((untracked_files + 1))
  # Count lines of text files only (grep -I treats binaries as non-matching).
  if [ -f "$f" ] && grep -Iq . "$f" 2>/dev/null; then
    n=$(wc -l <"$f" | tr -d ' ')
    # A last line without a trailing newline still counts.
    [ -n "$(tail -c 1 "$f")" ] && n=$((n + 1))
    untracked_lines=$((untracked_lines + n))
  fi
done < <(git ls-files --others --exclude-standard -z)

staged=$(git diff --cached --name-only "$BASE" 2>/dev/null | wc -l | tr -d ' ')
unstaged=$(git diff --name-only 2>/dev/null | wc -l | tr -d ' ')

lines=$((added + deleted + untracked_lines))
files=$((tracked_files + untracked_files))

if [ "$QUIET" -eq 0 ]; then
  echo "Uncommitted work: $files file(s), $lines changed line(s)"
  echo "  tracked:   $tracked_files file(s), +$added/-$deleted (staged: $staged, unstaged: $unstaged; a file can be both)"
  echo "  untracked: $untracked_files file(s), $untracked_lines line(s)"
fi

exceeded=0
if [ "$HOOK" -eq 1 ]; then
  if [ "$lines" -gt "$MAX_LINES" ] || [ "$files" -ge "$MAX_FILES" ]; then
    printf '{"systemMessage": "Handoff check: %s uncommitted file(s), %s changed line(s) (thresholds: >%s lines or %s files). Commit the finished slice or update HANDOVER.md before handing off."}\n' \
      "$files" "$lines" "$MAX_LINES" "$MAX_FILES"
  fi
  exit 0
fi
if [ "$lines" -gt "$MAX_LINES" ]; then
  echo "WARNING: $lines changed lines exceeds the handoff threshold of $MAX_LINES." >&2
  exceeded=1
fi
if [ "$files" -ge "$MAX_FILES" ]; then
  echo "WARNING: $files changed files reaches the handoff threshold of $MAX_FILES." >&2
  exceeded=1
fi
if [ "$exceeded" -eq 1 ]; then
  echo "  Finish and commit the current slice (or record exactly what is unfinished in HANDOVER.md) before handing off." >&2
fi

if [ "$STRICT" -eq 1 ] && [ "$exceeded" -eq 1 ]; then
  exit 1
fi
exit 0
