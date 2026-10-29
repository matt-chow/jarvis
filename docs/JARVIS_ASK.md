# JARVIS Ask on iPhone (web)

**Ask** (`#/ask`) is the JARVIS conversation inside Directory. It's the
primary way to use JARVIS on an iPhone (DECISIONS D-044). You type, or
dictate, a request. JARVIS answers, and any change waits until you tap
**Confirm**. The Apple Shortcut (docs/IPHONE_SHORTCUT.md) still works and
stays optional.

## How it works

```
Ask screen (signed-in session, no device token)
  → POST /api/v1/jarvis/ask  { text, today: phone's yyyy-MM-dd }
  ← { speech, status, needs_confirmation, plan_id? }
  pending → Confirm / Cancel → POST { text: "yes" | "no", confirm: plan_id, today }
```

- **The same contract as the Shortcut** (docs/IPHONE_SHORTCUT.md › API
  contract). There's no new endpoint or backend code; the deterministic
  parser, the planner, and approval are unchanged.
- **Auth:** your passkey session cookie. The page never stores or sends a
  device token. If the session has ended, Ask says so, then the app shows
  sign-in.
- **No duplicates:** Confirm and Cancel disable while the request runs. A
  repeated confirmation can't run twice (the server answers "That was
  already done."). A new request drops an unconfirmed change; it's never
  run.
- **Errors:** a network failure, 401, 403, 413, 502, or 503 shows a
  sentence saying what to do. If confirming fails mid-way, the change
  stays pending so you can tap Confirm again safely.

## Voice

| | What Ask does | Where the audio goes |
| --- | --- | --- |
| **Keyboard dictation** (works today) | The big mic button focuses the text box; tap the keyboard's 🎤 key and speak. Edit the text, then **Send**. | Apple's dictation (iOS). Directory gets text only. |
| **In-page recognition** (off today) | Feature-detected `webkitSpeechRecognition`: live transcript into the editable box, then **Send**. Handles denied permission, silence (8 s), no microphone, network errors, and **discard**. | The browser's speech service (Apple in Safari, Google in Chrome). Directory gets text only. |
| **Voice reply** (toggle, off by default) | Reads each answer aloud with the phone's built-in voices. | On-device speech synthesis. Nothing is sent. |

In-page recognition is **off** because the site sends
`Permissions-Policy: microphone=()` (docs/SECURITY.md). Ask detects this
(or the missing API) and falls back to keyboard dictation, with a message.
Turning it on is an owner decision; see "Proposed changes".

## Home Screen launch

The installed iPhone app opens on **Ask**; browser tabs still open the
dashboard (DECISIONS D-045). iOS opens a Home Screen app at the URL it saved
when you installed it (`/`, the dashboard) and ignores the manifest
`shortcuts`, so the route is chosen in the page, before the app renders
(`lib/launch.ts`, called from `main.tsx`):

- **Only installed apps:** `navigator.standalone === true` (iOS) or
  `display-mode: standalone`. A Safari or desktop browser tab is never
  redirected.
- **Only an unrouted launch:** the URL has no route (`""`, `#`, `#/`) or
  `#/home`, which is what an install made from the dashboard saves. Any
  other route, like `#/tasks` or `#/notes?note=…`, is kept.
- **Once per app session, never on reload:** a reload or back/forward
  isn't redirected (navigation type), and a `sessionStorage` flag marks
  the launch as decided. "Reload to update" keeps you where you are, and
  there's no loop.
- **`history.replaceState`:** no extra history entry, so Back doesn't go
  to the dashboard.
- **Per device:** Settings › *this device* › "Home Screen app opens on",
  Ask (JARVIS) or Home. It's stored in `localStorage` (`launch:module`).
  The default is Ask on iPhone and iPad and Home elsewhere (for example an
  installed desktop Chrome app). On iOS the Home Screen app has its own
  storage, separate from Safari, so set it inside the app.
- **Sign-in is unchanged:** the route is set before the session check. If
  you're signed out, you sign in on the auth screen and land on Ask. The
  saved session (offline) works the same way.
- **Manifest unchanged** (`start_url: "/"`, `id: "/"`), as are the service
  worker and its precache. **An existing Home Screen install doesn't need
  to be reinstalled.** After the deploy, open it once while online so the
  service worker picks up the new version (tap "Reload to update" if it
  shows). The next launch from the icon opens Ask.

Tested: `lib/launch.test.ts` (the redirect rules) and a browser e2e step
that emulates `navigator.standalone` (launch → Ask, reload keeps Tasks, a
launch with `#/settings` keeps it, choosing Home turns the redirect off,
and a browser tab stays on Home).

## iPhone test steps (staging)

These steps need a deploy first. Run the gate (`npm run verify`), then
`npm run deploy:staging` (docs/STAGING.md §6). Deploying needs owner
approval.

1. In Safari on the iPhone, open
   `https://directory-staging.mchowcl.workers.dev` and sign in with your
   passkey.
2. Tap **Ask** in the bottom tab bar (second from the left). The screen
   fills the phone: a ← back arrow, the JARVIS title, *voice reply*, and
   *history* at the top, with the text box and a large mic button at the
   bottom.
3. Tap the **mic button**. You should see "Voice input is turned off for
   this site…" and the keyboard opens. Tap the keyboard's 🎤 key, say
   "What should I do today", and tap **Send** (or the keyboard's *send*
   key). You should get your top tasks, or "Nothing open. Nice.", tagged
   *done*.
4. Tap **voice reply off** to turn it on, then ask "What's on tomorrow?".
   The answer should be spoken. If it's silent, check the ringer and
   silent switch, then report it (see "Unverified").
5. Type "Add task test Ask due Friday" and send it. You should see
   "Create task “test Ask” due … Say yes to confirm." with **Confirm**
   and **Cancel**. Open Tasks in another tab: the task doesn't exist yet.
6. Tap **Confirm** (try a quick double tap). You should see one
   "Created task “test Ask”." The original card turns *confirmed*. Tasks
   shows exactly one "test Ask".
7. Type "Add task should not exist" and tap **Cancel**. You should see
   "Okay, cancelled.", and no task is created.
8. Install it: Share › **Add to Home Screen**. Open Directory from the
   Home Screen, sign in if asked, and repeat step 3. Note whether the
   keyboard dictation key is present.
9. Turn on Airplane Mode in Ask: it shows "JARVIS needs a connection.",
   and Send and the mic are disabled.
10. Delete "test Ask" in Tasks. Report anything that differs, and any
    phrase JARVIS misunderstood; it becomes a parser test.

## Implemented, tested, unverified

**Implemented:** the `ask` module (`modules/Ask.tsx`); `lib/talk.ts`
(conversation state, confirm and cancel, errors); `lib/speech.ts`
(detection, recognition lifecycle, synthesis); `Api.jarvisAsk`; the
manifest shortcut; docs.

**Tested automatically** (`npm run ci`):
- Unit (`lib/talk.test.ts`, `lib/speech.test.ts`):
  - the request body (`text`, `today`, `confirm`), with no Authorization
    header and same-origin credentials;
  - confirm sends `yes` with the original `plan_id`, once (a second tap
    while busy is ignored);
  - cancel sends `no`;
  - a new request drops a pending change;
  - a failed confirm stays pending;
  - 401, 403, 413, and 502 messages;
  - no API, insecure context, and a policy block;
  - error mapping (`not-allowed`, `no-speech`, …);
  - silence timeout, discard when the engine never ends, a throwing
    `start()`;
  - optional synthesis.
- Browser e2e (headless Chrome at 390×844, phone emulation):
  - full screen (the composer, not the tab bar, at the bottom) and no
    horizontal scroll;
  - the policy block shows the fallback and focuses the box;
  - ask, then confirm with a double tap (exactly one task, created only
    after confirm);
  - cancel (no task);
  - anonymous `ask` → 401;
  - accessible names on the screen.

**Unverified on a real iPhone** (needs steps 1–10):
- Safari layout with the notch and home indicator (safe areas), and the
  keyboard covering the composer.
- Keyboard dictation into the box, in Safari and in the Home Screen app.
- Voice reply: iOS needs a tap before speech, so the toggle speaks a
  silent utterance to unlock later replies. Whether that's enough on
  current iOS hasn't been checked.
- In-page recognition: it can't be tested until the header changes.
  Apple has said it's unavailable in Home Screen web apps; Safari tabs
  have supported it since iOS 14.5 (needs Siri & Dictation on).
- Home Screen launch (D-045), on the existing install after the deploy:
  1. Open the app once while online, and reload if it says an update is
     ready.
  2. Close it from the app switcher and open it from the icon. It should
     open on Ask.
  3. Go to Tasks and switch away briefly. Coming back stays on Tasks:
     iOS resumes the app without a reload. If iOS has evicted it, it
     relaunches on Ask, which is expected.
  4. In Settings › *this device*, choose Home and relaunch. It should open
     the dashboard. Then set it back to Ask.
  5. Open Safari and the staging URL. It still shows the dashboard.

  Unchecked: whether this install's saved URL is `/` or `/#/home` (both
  are handled), and whether iOS gives each cold launch a fresh
  `sessionStorage` (if it didn't, only the first launch per session would
  open on Ask). If step 2 opens the dashboard, delete the icon and add it
  again from Safari while on Ask (`#/ask`). That saves Ask as the launch
  URL, whatever happens in the page.

## Proposed changes (need owner approval)

1. **Allow in-page speech recognition** (security policy). In
   `apps/web/public/_headers`, change
   `microphone=()` → `microphone=(self)`, and update docs/SECURITY.md.
   - Effect: only Directory's own pages may *ask* for the microphone. The
     browser still prompts, and iOS also asks for speech recognition.
   - Privacy: the browser vendor's speech service hears the audio (Apple
     on iPhone). Directory still receives only text.
   - Keyboard dictation already sends audio to Apple, so on iPhone the
     difference is small; in Chrome it would be Google.
   - No code change is needed: Ask detects the new policy and shows live
     transcription.
2. **Add to Home Screen.** Directory is already installable: the manifest
   uses `display: standalone`, and there are `apple-mobile-web-app-*`
   metas, an apple-touch-icon, an offline app shell, and safe-area
   padding. The per-device "open Ask at launch" setting is done (see
   "Home Screen launch"). Still optional, no dependencies:
   - a one-time hint on iPhone Safari (not standalone) explaining
     Share › Add to Home Screen.

   The manifest `shortcuts` entry added here works on Android and desktop
   only; iOS ignores it.
