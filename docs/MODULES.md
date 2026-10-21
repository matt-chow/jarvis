# Modules

A module is a self-contained capability with a stable id. Module ids live in
`apps/web/src/lib/modules.ts` and must stay stable, because future clients and
JARVIS commands will refer to them.

| Id | Name | Stage | Offline | Backend | Notes |
| --- | --- | --- | --- | --- | --- |
| `home` | Home | ready | yes | `GET /api/v1/health` | Status dashboard; health cached in IndexedDB. |
| `notes` | Scratchpad | preview | yes | none yet | Device-local notes in IndexedDB. D1 sync planned. |
| `files` | Files | preview | no | `/api/v1/storage/*` | R2 proof of concept behind bearer token. |
| `jarvis` | JARVIS | planned | — | — | Placeholder screen. |

## Adding a module

1. Add metadata to `MODULES` in `apps/web/src/lib/modules.ts`.
2. Create `apps/web/src/modules/<Name>.tsx` (it receives `ModuleProps`) and
   register it in `VIEWS` in `App.tsx`.
3. Put domain rules in a new `crates/directory-core/src/<module>.rs`, with traits
   for any persistence.
4. Implement those traits in `crates/directory-worker`, and add routes under
   `/api/v1/<module>/…` that require auth.
5. Add a D1 migration if the module needs tables. Bump `EXPECTED_SCHEMA_VERSION`
   when the migration updates `system_meta`.
6. Add tests, and update this file and PROGRESS.md.
