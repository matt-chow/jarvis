# Security

How Directory protects a single owner's personal data, what is checked
automatically, and what is still open. Changing anything here needs owner
approval (AGENTS.md).

## Model

- **One owner per deployment.** Setup is open only until the first passkey
  is registered, and needs `DIRECTORY_SETUP_TOKEN`. After that it answers
  `409 setup_unavailable`.
- **Every personal-data route resolves a `Principal`** (`http::auth::principal`).
  Core services scope every query by `principal.user_id`. Public routes are
  only health, the sign-in and setup ceremony, the session probe, and
  sign-out.
- **Passkeys only** (WebAuthn, ES256/RS256, user verification required). No
  passwords exist to phish or reuse.
- **Sessions:** a random token whose SHA-256 hash is stored. The cookie is
  `__Host-dir_session` (HttpOnly, Secure, SameSite=Strict, Path=/). It slides
  to 30 days, with a hard 90-day maximum. Sign-out revokes it server-side.
- **CSRF:** on top of SameSite=Strict, cookie-authenticated writes must send
  an `Origin` in `DIRECTORY_ORIGINS`, or get `403 origin_not_allowed`.
- **Device tokens** (`dir_…` bearer) are for native clients and scripts.
  They're hashed at rest and shown once. They can read and write data but
  can't manage credentials (`403 session_required`), so a leaked token can't
  mint more access.
- **Rate limiting:** sign-in and setup endpoints allow 30 requests per 10
  minutes per client IP (`429` + `Retry-After`).
- **Files** live in a private R2 bucket (no public URL). Content is served
  only by an authenticated route, as an attachment with
  `Content-Security-Policy: sandbox` and `nosniff`, so uploaded HTML or SVG
  can't run on the app origin.
- **JARVIS** runs typed, parsed commands through module services as the
  caller (no raw database access). Writes need per-plan approval, and a plan
  runs at most once. **Automations** may run only additive commands,
  pre-approved when the rule is saved, and are injection-safe (D-024).
- **Input limits:** JSON bodies are capped at 64 KiB by default (notes
  512 KiB, restore 4 MiB, error reports 32 KiB). Field lengths and formats
  are validated in core, and lists are capped (D-022).
- **Headers:**
  - App shell: CSP `default-src 'self'`, with blob: allowed only for images
    and media; `frame-ancestors 'none'`, `X-Frame-Options: DENY`, and
    `nosniff`.
  - `Referrer-Policy: strict-origin-when-cross-origin`, and a
    Permissions-Policy that denies camera, microphone, and geolocation.
  - API responses: `no-store`.
- **Offline data** (IndexedDB) stays in the owner's browser profile. The
  service worker never caches `/api/*`. Plain sign-out keeps the copy for
  offline use; "sign out and forget this device" deletes it (D-028).
- **Secrets:** only in Wrangler secrets or `.dev.vars` (gitignored). Exports
  never contain credential material.

## Checked automatically (`npm run ci`)

- **Security sweep (smoke):** reads every route from the Worker's router. It
  asserts that each non-public route answers `401` to anonymous callers, and
  that each cookie-authenticated write from a foreign Origin gets `403`. New
  routes are covered without editing the test.
- Credential routes refuse device tokens; setup closes after first use;
  sign-in is rate limited; export holds no secrets; R2 content is sandboxed;
  JARVIS plans run once; automations can't be injected.
- The browser e2e plays a saved track from a `blob:` URL, which catches CSP
  regressions in media playback.

## Review log

- **2026-10-22.** Walked all routes and the auth layer; no unauthenticated or
  unscoped data route was found. Fixed: the app-shell CSP blocked `blob:`
  media, so tracks saved offline wouldn't play. `media-src 'self' blob:` was
  added, and a browser test covers it.

## Open items

- `rsa` 0.9 advisory RUSTSEC-2023-0071 (timing, private-key operations
  only; Directory only verifies). Revisit when `rsa` 0.10 is stable.
- On a shared computer, use "sign out and forget this device" (D-028);
  plain sign-out leaves the offline copy in that browser.
- No rate limit on authenticated routes (single owner; D-022 caps bound
  storage). Revisit if device tokens are given to third-party scripts.
- Signature-verification CPU time hasn't been measured against the Workers
  Free limit (see PROGRESS.md).
- After the first production deploy, run the remote smoke
  (`BASE_URL=https://… npm run smoke`). It checks that every non-public route
  answers 401 anonymously and that the deployed shell sends the security
  headers. The cross-origin half of the sweep needs a session cookie, so it
  runs only locally.
