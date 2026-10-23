# Handover

Base commit: 080c106
From: claude → To: (either; no handoff in progress)
Updated: 2026-10-23

## Status
Everything unblocked is committed, including the offline library (D-030).

## In progress
Nothing uncommitted. The D-030 offline library slice is committed; its
gate ran as verify + smoke + browser e2e separately (the combined
`npm run ci` was stopped once for low memory).

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
