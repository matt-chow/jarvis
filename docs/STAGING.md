# Staging

Staging is a protected copy of Directory used for testing on real devices
(iPhone Safari, Apple Shortcuts) before production. It has its own Worker
(`directory-staging`), its own D1 database (`directory-db-staging`), and its
own **private** R2 bucket (`directory-files-staging`), configured in
`wrangler.toml` under `[env.staging]`. Staging never touches production
data.

**Status:** the configuration is ready, and `wrangler deploy --dry-run --env staging`
passes with the staging bindings. **Not yet deployed:** this machine isn't
logged in to Cloudflare, and creating the resources is the owner's step.

## Protection

- Personal data needs server-side auth, as in production. Every data route
  resolves a `Principal` (passkey session or device token), and auth fails
  closed (503) until `DIRECTORY_RP_ID`/`DIRECTORY_ORIGINS` and the setup
  token are set.
- First-run setup is gated by `DIRECTORY_SETUP_TOKEN`, a secret only the
  owner holds. After setup, no new owner can be created.
- The R2 bucket stays private: no r2.dev URL and no custom domain. File
  bytes are served only through authenticated `/api/v1/files/*` routes.
- `preview_urls = false`, so no extra hostnames expose the app.
- Optional extra layer: Cloudflare Access in front of the staging hostname.
  This is free for up to 50 users, but the Zero Trust sign-up may ask for a
  payment method, so it needs the owner's approval. Apple Shortcuts can't
  complete an Access login, so `/api/v1/jarvis/ask` would need an Access
  bypass rule or a service token. It's not required: the app's own auth
  already protects all data.

## Free-tier budget

| Resource | Free allowance | Staging use |
| --- | --- | --- |
| Workers requests | 100,000/day, 10 ms CPU per request | Manual testing: well under 1% |
| Static assets | Free and unmetered (`run_worker_first` only for `/api/*`) | — |
| D1 | 5 GB storage; 5 M rows read and 100 k written per day | Test data only |
| R2 | 10 GB; 1 M Class A and 10 M Class B ops/month | A few test files |

Check usage in the dashboard: Workers & Pages → `directory-staging` →
Metrics, and R2 → Metrics. If it nears a limit, stop testing; nothing on
the free plan bills automatically.

## One-time setup (owner)

1. `npx wrangler login` (opens a browser).
2. `npx wrangler d1 create directory-db-staging`, then copy the printed
   `database_id` into `[[env.staging.d1_databases]]`.
3. Activate R2 if it isn't already. It's free within limits, but the
   dashboard asks for a payment method, so this is an owner decision.
   Then run `npx wrangler r2 bucket create directory-files-staging`. Don't
   enable public access.
4. Find your workers.dev subdomain (dashboard → Workers & Pages →
   Subdomain). In `[env.staging.vars]` set
   `DIRECTORY_RP_ID = "directory-staging.<subdomain>.workers.dev"` and
   `DIRECTORY_ORIGINS = "https://directory-staging.<subdomain>.workers.dev"`.
5. `npx wrangler secret put DIRECTORY_SETUP_TOKEN --env staging`, and
   paste a random value from
   `node -e "console.log(require('crypto').randomBytes(32).toString('hex'))"`.
6. Optional: set the JARVIS AI secrets (docs/JARVIS_SHORTCUTS.md › AI).
7. `npm run db:migrate:staging`, then `npm run deploy:staging`.

## Verify

1. **Health:** `curl https://directory-staging.<subdomain>.workers.dev/api/v1/health`
   should return 200 with the expected schema version.
2. **Auth:** open the site on a laptop and complete setup with the setup
   token (this creates the owner passkey). Without a session,
   `curl …/api/v1/tasks` must return 401.
3. **Smoke:** in Settings, create a device token, then run
   `BASE_URL=https://directory-staging.<subdomain>.workers.dev DIRECTORY_API_TOKEN=dir_… npm run smoke`.
4. **Storage:** upload a file in Files and download it again. The bucket's
   public URL setting must still read "Disallowed".
5. **Migrations:** `npx wrangler d1 migrations list DB --remote --env staging`
   should show nothing pending.
6. **iPhone:** in Safari, open the staging URL and sign in. Either add a
   passkey from the laptop session (Settings → Add passkey) or use the
   cross-device QR sign-in. Add the app to the Home Screen, then check
   offline launch (Airplane Mode). Then set up the JARVIS Shortcut
   (docs/JARVIS_SHORTCUTS.md).
7. **CPU:** in Metrics, check CPU time, especially for
   `/api/v1/auth/login/verify`, against the 10 ms limit.

Record the results in PROGRESS.md. Production deploys stay separate
(ARCHITECTURE.md › Deployment) and need the owner's approval.
