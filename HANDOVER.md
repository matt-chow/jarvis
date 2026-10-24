# Handover

Base commit: 9b9d978
From: claude → To: (either; no handoff in progress)
Updated: 2026-10-24

## Repository location (read first)
The active repo is **`C:\Users\mchow\dev\jarvis`**. It was copied out of
OneDrive on 2026-10-24: git fsck is clean, the index and tree matched, and
verify, smoke, and browser e2e all passed there. The OneDrive copy
(`C:\Users\mchow\OneDrive\Documents\jarvis\jarvis`) is **retired at
9b9d978**. Don't edit it, and don't delete it without the owner's approval.

## Status
Goal (owner, 2026-10-24): a $0/month MVP centred on JARVIS on iPhone.
Order of work:
1. Repo migration ✅
2. Continuity ✅
3. Cloudflare staging: config and steps; the deploy needs the owner.
4. JARVIS Alpha: Shortcuts endpoint, task commands, optional OpenRouter
   free model.
5. Deterministic planner.
6. Mail: feasibility first (docs/MAIL_FEASIBILITY.md).

## In progress
See "Next steps"; this file is updated at each milestone.

## Next steps
1. Planner core (`crates/directory-core/src/planner.rs`), then JARVIS
   commands `priorities` / `suggest` / task updates, then the Shortcuts
   endpoint `/api/v1/jarvis/ask` with JARVIS-only device tokens.
2. Staging config (`[env.staging]`, separate D1/R2) and docs/STAGING.md.
3. Owner-gated items: see PROGRESS.md › Unresolved decisions.

## Working tree
Clean after each commit. No stashes.

## Notes
- A `wrangler dev --port 8799 --persist-to .wrangler/hdr` server (and its
  workerd), started from the OneDrive copy, has been running since
  2026-10-22 21:29. It's probably left over from the security-header review.
  Stop it if nobody needs it.
- Windows: prefix PATH with `~/.cargo/bin` for Rust.
- Scripted shell edits (sed, heredocs, `node -e`) drop backslashes in this
  environment. Edit text containing backslashes with an editor tool.
- Full gate: `npm run ci` (about 10 minutes). Run its pieces in the
  foreground if memory is tight.

## Done (earlier sessions)
- Offline library (D-030), offline search (D-031), read-only offline Files
  (D-032), quota race and orphaned blobs (D-033, migration 0020), offline
  pinning (D-034), journal delete (D-035), triage (D-036), links (D-037),
  Unicode search (D-038), stale list loads (D-039). Gate: verify, smoke,
  and browser e2e 31/31.
- Watch item: the intermittent "Offline edits" failure. Its likely cause
  is fixed in D-039; not yet proven.

## Handoff log
