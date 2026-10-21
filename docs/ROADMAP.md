# Roadmap

| Sprint | Theme | Outcomes |
| --- | --- | --- |
| **00** | Infrastructure foundation | Monorepo, app shell, Rust Worker + health, D1 migrations, R2 PoC, PWA/offline, tests, deploy config. ✅ |
| 01 | Identity & first deploy | Owner sign-in (Cloudflare Access or passkey session; see DECISIONS D-004). Replace the shared token. First production deploy with smoke test. CI running `npm run verify`. |
| 02 | Synced notes | Notes schema in D1, a sync protocol (IndexedDB outbox, server timestamps, conflict policy), Scratchpad sync. |
| 03 | Files module | Real file browser: folders, metadata in D1, streaming and multipart uploads for large files, quota display. |
| 04 | More modules | Tasks and bookmarks/links on the same module pattern; search across modules. |
| 05+ | JARVIS | Command palette first, then voice input (Web Speech API) and an AI provider (free-tier or explicitly approved). |
| Later | Native clients | Windows (e.g. Tauri reusing `directory-core`) and iPhone (PWA first, then native), on the `/api/v1` contract. |
