# Handover

Base commit: 69a2461
From: claude → To: (either; no handoff in progress)
Updated: 2026-10-22 23:30

## Status
Everything unblocked on the roadmap is done and committed. `npm run ci` is
green: core 113, web 75, handoff 50, smoke (including a route-wide security
sweep), browser e2e 27/27.

## In progress
Nothing uncommitted.

## Next steps
1. Owner decisions block the remaining work (see PROGRESS.md › Unresolved
   decisions):
   - First Cloudflare deploy (`wrangler login`, R2 activation, hostname
     vars). Afterwards: remote smoke (`BASE_URL=… npm run smoke`), Worker CPU
     measurement, and optionally `[observability]` logs (D-027).
   - Voice input via the browser's speech service (sends audio to the
     vendor).
   - An AI intent provider and its budget.
   - An in-app PDF/ePub reader dependency (pdf.js/epub.js).
   - iOS verification on a device.
2. Watch item: one intermittent "Offline edits" browser failure, not seen
   in the last five full runs.

## Working tree
Clean except this file. No stashes.

## Notes
- Windows: prefix PATH with `~/.cargo/bin` for Rust.
- Scripted shell edits (sed, heredocs, `node -e`) drop backslashes in this
  environment. Edit text containing backslashes with an editor tool.
- Full gate: `npm run ci` (about 10 minutes).

## Handoff log
