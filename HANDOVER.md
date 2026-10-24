# Handover

Base commit: 815dc45
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
3. Cloudflare staging: config ✅ (005250e). The deploy needs the owner
   (docs/STAGING.md).
4. JARVIS Alpha:
   - Shortcuts endpoint + JARVIS-only tokens ✅ (60dafec, D-042).
   - Task commands ✅ (29adfc2).
   - Optional OpenRouter free model ✅ (22714ab, D-043). Off until the
     owner sets a key; the live call is unverified.
   - Voice automations use the phone's date (64197f2).
   - Settings shows the Shortcut URL (a44d687).
5. Deterministic planner ✅ (29adfc2, D-041), with a project constraint
   (08c9b12).
6. Mail: feasibility research ✅ (docs/MAIL_FEASIBILITY.md). The OAuth
   test is **blocked on the owner** (Google/Microsoft clients and consent,
   plus a staging host for the redirect URI). No mail code until access is
   confirmed (owner rule: "verify access first").

Gate at 08c9b12: verify ✅ (core 133, web 96, handoff 50), smoke ✅, browser
e2e 32/32 ✅.

## In progress
See "Next steps"; this file is updated at each milestone.

## Next steps
0. **Staging prep is done** (2026-10-24). docs/STAGING.md is the
   step-by-step guide. Its first owner action is §1: `npx wrangler login`,
   run from `C:\Users\mchow\dev\jarvis`. Agents stop before
   creating any Cloudflare resource, activating billing, migrating, or
   deploying.
1. Owner: deploy staging (docs/STAGING.md), then build the iPhone Shortcut
   (docs/JARVIS_SHORTCUTS.md) and run the on-device checks. Misheard or
   unsupported phrases become parser tests.
2. Owner: optionally set `OPENROUTER_API_KEY` and a `:free`
   `JARVIS_AI_MODEL` on staging. Then check one unknown phrase end to end.
3. Owner: mail OAuth clients and consent test. Then implement the
   read-only inbox per the constraints in docs/MAIL_FEASIBILITY.md.
4. Owner-gated items: see PROGRESS.md › Unresolved decisions (staging
   deploy and on-iPhone check, OpenRouter key, mail OAuth clients, deleting
   the OneDrive copy).

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
- 2026-10-24, staging prep:
  - docs/STAGING.md rewritten in full.
  - Remote smoke covers tasks, private storage, and JARVIS ask; also
    `npm run smoke:remote-local`.
  - 75 e2e export files moved from Downloads to
    `C:\Users\mchow\Documents\Directory\Backups\e2e-test-exports\`
    (hash-verified).
  - e2e downloads now stay in the temp profile.
- Offline library (D-030), offline search (D-031), read-only offline Files
  (D-032), quota race and orphaned blobs (D-033, migration 0020), offline
  pinning (D-034), journal delete (D-035), triage (D-036), links (D-037),
  Unicode search (D-038), stale list loads (D-039). Gate: verify, smoke,
  and browser e2e 31/31.
- Watch item: the intermittent "Offline edits" failure. Its likely cause
  is fixed in D-039; not yet proven.

## Handoff log
