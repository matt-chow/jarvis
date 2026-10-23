# Handover

Base commit: 08c3174
From: claude → To: (either; no handoff in progress)
Updated: 2026-10-23

## Status
Everything unblocked is committed, including the offline library (D-030)
offline search (D-031), read-only offline Files (D-032), and
the upload quota race / orphaned-blob retry (D-033, migration 0020), and offline
note pinning (D-034) journal delete (D-035), inbox
triage (D-036), and links (D-037). Gate: verify, smoke, and browser e2e 28/28.

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
   - An in-app PDF reader: pdf.js, or serving PDFs inline (CSP change).
     ePubs already read in-app (D-029).
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
