# Product Requirements

## User

A single owner who wants one private workspace across a Windows PC and an iPhone,
usable offline, with an AI assistant later.

## Functional requirements

| ID | Requirement | Status |
| --- | --- | --- |
| F1 | Responsive app shell with module navigation (sidebar on desktop, tab bar on phone) | Sprint 00 ✅ |
| F2 | Installable PWA; the shell opens offline after the first visit | Sprint 00 ✅ |
| F3 | On-device storage (IndexedDB) for offline module data | Sprint 00 ✅ (Scratchpad) |
| F4 | Health endpoint reporting API, database, and storage status | Sprint 00 ✅ |
| F5 | Private file storage: upload, list, download, delete | Sprint 00 PoC ✅ (shared token) |
| F6 | Real sign-in for the owner, protecting all personal data | Sprint 01 |
| F7 | Notes synced between devices through D1 | Planned |
| F8 | Additional modules (tasks, bookmarks, journal, …) | Planned |
| F9 | JARVIS: voice input and natural-language commands across modules | Planned |
| F10 | Windows and iPhone clients using the same API | Planned |

## Non-functional requirements

- **Security:** no secrets in the repo or the client bundle. R2 is never publicly
  readable. Private endpoints fail closed when auth isn't configured. User files
  are served as attachments with a sandbox CSP.
- **Cost:** Workers Free, D1 Free, R2 Free tier only. The Worker stays under
  3 MB compressed (currently about 175 KiB total upload, gzipped).
- **Performance:** static assets don't invoke the Worker. API handlers stay
  within the 10 ms CPU free-tier budget.
- **Compatibility:** a versioned JSON API (`/api/v1`) with bearer auth that
  native clients can use. Supports iOS Safari PWA install.
- **Quality:** unit tests for domain logic and client libraries, an end-to-end
  smoke test, and an offline browser check.
