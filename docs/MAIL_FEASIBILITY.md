# Directory Mail: access feasibility

Status: **research done; live authorization not yet tested.** Testing needs
the owner to create OAuth clients and sign in. No mail code exists yet, and
none will until access is confirmed on the real accounts.

Scope (owner, 2026-10-24): several personal Gmail accounts, plus one school
Outlook account. The client is read-only: it never sends, deletes, labels,
or marks mail read in the provider.

## Gmail (personal accounts)

| Question | Finding |
| --- | --- |
| API | Gmail API, `users.messages.list`/`get`, `users.labels.get` (unread counts). Free; no billing account. |
| Scope | `https://www.googleapis.com/auth/gmail.readonly`. It is a *restricted* scope, which needs verification and a paid security assessment (CASA), but only for public apps. |
| Personal use without verification | Possible. An unverified app can be authorized by up to **100 users over the project's lifetime**. Each user sees an "unverified app" warning and clicks through it. |
| Token lifetime | In **Testing** status, refresh tokens expire after **7 days**. Move the consent screen to **In production** (still unverified) and re-consent; the tokens then last until revoked, or until about 6 months unused. Tokens issued during Testing keep the 7-day limit, so re-authorize after publishing. |
| Several accounts | One OAuth client serves every account. Each account consents separately and gets its own refresh token, up to 50 per account per client. |
| Cost | $0: a Google Cloud project with no billing account. |

**Verdict: feasible at $0**, if the owner accepts the unverified-app warning
on each account.

## School Outlook (Microsoft 365)

| Question | Finding |
| --- | --- |
| API | Microsoft Graph `/me/messages`, `/me/mailFolders/inbox` (unread count). |
| Scope | Delegated `Mail.Read` + `offline_access` + `User.Read`. Microsoft doesn't require admin consent for delegated `Mail.Read`. |
| The catch | A school tenant can block user consent, and many do, or allow it only for verified publishers. The sign-in then shows "Need admin approval", and only the school's IT can approve the app. |
| App registration | A free Entra app registration (any Microsoft account) set to multi-tenant: "Accounts in any organizational directory and personal Microsoft accounts". |
| Cost | $0. |

**Verdict: unknown until tried.** It depends on the school's consent policy.
If consent is blocked, the options are to ask school IT to approve the app,
or to leave school mail out.

## Feasibility test (owner actions)

These steps are for the owner. Agents can't create accounts or consent on
the owner's behalf.

1. **Google:**
   1. Create a project at console.cloud.google.com (no billing).
   2. Enable the Gmail API.
   3. Configure the OAuth consent screen: External, add yourself as a test
      user, then publish it to "In production".
   4. Create an OAuth client of type "Web application". The redirect URI
      will be `https://<staging host>/api/v1/mail/oauth/google/callback`.
   5. Give the agent only the **client ID**. The client secret goes in
      `wrangler secret put` later; never paste it into chat.
2. **Microsoft:**
   1. Go to entra.microsoft.com, then App registrations → New registration,
      choosing the multi-tenant + personal accounts option.
   2. Add the redirect URI `https://<staging host>/api/v1/mail/oauth/microsoft/callback`.
   3. Add the API permissions `Mail.Read`, `offline_access`, and `User.Read`
      (delegated).
   4. Sign in once with the school account to see whether consent is
      allowed. If it says admin approval is needed, report that.
3. A staging deployment must exist first (docs/STAGING.md), because both
   providers need an HTTPS redirect URI. Gmail also accepts `http://localhost`
   for a local-only check.

## Design constraints, fixed now

These hold for any later implementation:

- **Read-only scopes only.** Request no write scopes. Nothing calls send,
  modify, trash, or delete endpoints.
- **Minimal storage.** D1 holds message ids, thread ids, the sender, the
  subject, the date, an unread flag, provider labels, and Directory's own
  category. Message bodies are fetched on demand and never stored, except in
  the client's in-memory view.
- **Refresh tokens** are encrypted at rest with a Worker secret key
  (AES-GCM via WebCrypto). They are never returned to the client and never
  exported.
- **Safe rendering.** HTML bodies are shown in a sandboxed `iframe` with
  `srcdoc` and no scripts. Remote images are blocked by default, and links
  open in new tabs with `noopener`.
- **Classification:** rules and metadata first (sender, list headers,
  labels). AI classification only with an explicit email-content opt-in, and
  never by default.
- **Suggested tasks** link to the source message id. They are created only
  when the owner accepts them.
- **Out of scope:** calendar, contacts, and Drive/OneDrive.

Sources:
- Google: "Unverified apps" and "OAuth app verification" (support.google.com/cloud);
  refresh-token expiry in Testing (developers.google.com/identity/protocols/oauth2).
- Microsoft: "Configure how users consent to applications"
  (learn.microsoft.com/entra/identity/enterprise-apps/configure-user-consent).
