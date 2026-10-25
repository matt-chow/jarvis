# JARVIS on iPhone: build the Shortcut

This guide is a step-by-step walkthrough for building the **JARVIS**
Shortcut on your iPhone. It takes about 10 minutes. It uses only Apple's
built-in actions: **Dictate Text** to listen, **Get Contents of URL** to
ask Directory, and **Speak Text** to answer. Directory only ever receives
the text that Apple transcribed. No audio is sent.

Background and design: docs/JARVIS_SHORTCUTS.md and DECISIONS D-042. The
API contract below is taken from the code
(`crates/directory-worker/src/http/jarvis.rs` › `ask`,
`crates/directory-core/src/jarvis/voice.rs`).

Staging URL used throughout:
`https://directory-staging.mchowcl.workers.dev`

---

## Before you start

You need:

1. An iPhone on **iOS 16 or later**, with the **Shortcuts** app installed
   (it's built in; reinstall it from the App Store if you deleted it).
2. The **JARVIS-only** device token you created in Directory › Settings ›
   Device tokens (named "iPhone Shortcut"). It starts with `dir_`. Keep it
   in your password manager.
   - If you didn't save it, the secret can't be shown again. Revoke the
     old token in Settings, then create a new one and choose **JARVIS only
     (Shortcuts)**.
   - **Don't use the Full-access token.** It would work on this endpoint,
     but if the phone or the shortcut leaked, it would open every other
     API route as well. A JARVIS-only token is refused everywhere except
     `/api/v1/jarvis/ask` (403 `token_scope`).

### Keeping the token safe

- Paste the token into the Shortcut **only**, straight from your password
  manager. Don't put it in Notes, Messages, email, screenshots, or a chat
  with an AI assistant.
- The token is stored inside the shortcut on your iPhone. If iCloud sync
  for Shortcuts is on, it is synced to your other Apple devices too.
- **Never share this shortcut.** If you want to share the layout, first
  duplicate it and delete the token from the copy's *Text* action
  (step 4 below).
- If the phone is lost, or you think the token leaked, go to Directory ›
  Settings › Device tokens and **revoke** it. The Shortcut then gets HTTP
  401 and says so (step 9). Create a new JARVIS-only token and paste it
  into the *Text* action.
- Tokens don't expire on their own. Revoking is the only way to turn one
  off.

---

## Build it

The Shortcuts app shows the actions you've added as a vertical list. To add
an action, tap the **search field at the bottom** ("Search for apps and
actions"), type the action's name, and tap it. Tap a blue placeholder
(such as *Text* or *URL*) inside an action to fill it in. To insert the
result of an earlier action (a **variable**), tap the field, then pick the
variable from the bar above the keyboard, or tap **Select Variable**.

1. Open **Shortcuts** and tap **+** (top right) to create a new shortcut.
2. Tap the name at the top, then **Rename**, and call it **JARVIS**.
   (Siri runs it by this name.)

### Listen and get today's date

3. Add **Dictate Text**.
   - Tap the arrow (›) on the action. Set **Language** to the language you
     speak, and **Stop Listening** to **After Pause**.
4. Add **Text**. Into its box, type `Bearer ` (with one space after it),
   then paste your `dir_…` token right after the space. The box now holds
   `Bearer dir_…`, all on one line.
   - Tap the action's icon and **Rename** it to `Token`. This makes it
     easy to pick in steps 6 and 15.
5. Add **Format Date**.
   - Tap *Date* and choose **Current Date**.
   - Set **Date Format** to **Custom**, and **Format String** to
     `yyyy-MM-dd`. Type it exactly: lowercase `yyyy` and `dd`, capital
     `MM`. A capital `YYYY` gives the wrong year in the last days of
     December.
   - This sends the phone's own date, so "today", "tomorrow", and "Friday"
     mean your days, not the server's UTC date.

### Ask Directory

6. Add **Get Contents of URL**.
   - URL: `https://directory-staging.mchowcl.workers.dev/api/v1/jarvis/ask`
   - Tap **Show More**.
   - **Method:** **POST**.
   - **Headers:** tap **Add new header**.
     - Key `Authorization`, Text: the **Token** variable (from step 4).
     - Add a second header: Key `Content-Type`, Text `application/json`.
   - **Request Body:** **JSON**. Tap **Add new field** › **Text** twice:
     - Key `text`, value: the **Dictated Text** variable.
     - Key `today`, value: the **Formatted Date** variable.

### Read and speak the answer

7. Add **Get Dictionary Value**. Set it to get **Value** for key `speech`
   in **Contents of URL**.
8. Add **If**. Input: the **Dictionary Value** from step 7. Condition:
   **has any value**.
9. Inside the *If*, add **Speak Text**, with the **Dictionary Value** from
   step 7.
10. Under **Otherwise**, add **Speak Text** with the text:
    `JARVIS couldn't reach Directory. Check the token and try again.`
    - This runs for HTTP 401 (bad or revoked token), 502/503 (server
      problems), and network failures. Those replies have no `speech`
      field (see "Errors" below).
11. Leave the **End If** that Shortcuts added.

### Confirm changes with "yes"

Anything that changes your data (adding, moving, or completing a task, and
so on) is only **previewed** first. Nothing changes until you say yes.

12. Add **Get Dictionary Value**: **Value** for key `status` in **Contents
    of URL** (the one from step 6).
13. Add **If**. Input: that *Dictionary Value*. Condition: **is**, text
    `pending`.
    - Use `status`, not `needs_confirmation`. It's plain text, so this
      comparison is reliable.
14. Inside this *If*, add:
    1. **Dictate Text** (same settings as step 3). You'll say "yes" or
       "no".
    2. **Get Dictionary Value**: **Value** for key `plan_id` in **Contents
       of URL** (from step 6).
15. Still inside the *If*, add another **Get Contents of URL**. Set it up
    like step 6: same URL, **POST**, the same two headers (`Authorization`
    = **Token**), and a **JSON** body with three **Text** fields:
    - `text`: the **Dictated Text** from step 14.1 (the yes/no answer,
      *not* the first one).
    - `today`: **Formatted Date**.
    - `confirm`: the **Dictionary Value** from step 14.2 (the `plan_id`).
16. Add **Get Dictionary Value**: **Value** for key `speech` in the new
    **Contents of URL** (step 15). Then add **Speak Text** with that value.
17. Leave the final **End If**. Tap **Done**.

If you say something JARVIS can't classify as yes or no ("maybe"), it
replies "Please say yes or no." and the shortcut ends. Nothing has changed.
Run JARVIS again and repeat the request.

### First run

18. Tap the shortcut to run it. Say: **"What should I do today?"**
    - The first time, iOS asks whether JARVIS may connect to
      `directory-staging.mchowcl.workers.dev`. Choose **Allow** (or
      **Always Allow**).
    - You should hear your top tasks, or "Nothing open. Nice."
19. Test a change: say **"Add task test the shortcut due Friday."** You
    should hear `Create task “test the shortcut” due 2026-…. Say yes to
    confirm.` Say **"yes"**. You should hear `Created task …`.
20. Check in the app (Tasks) that "test the shortcut" exists, then delete
    it.
21. Test cancel: say "Add task should not exist". When asked, say **"no"**.
    You should hear "Okay, cancelled." Check that no task was created.

## Run it hands-free

- **Siri:** "Hey Siri, JARVIS".
- **Action button** (iPhone 15 Pro and later): Settings › Action Button ›
  **Shortcut** › JARVIS.
- **Lock Screen / Control Center** (iOS 18 and later): add a
  **Shortcut** control and choose JARVIS. iOS may ask you to unlock
  first before the shortcut can go online.

---

## API contract (reference)

What the shortcut talks to, exactly as implemented.

### Request

```
POST /api/v1/jarvis/ask
Authorization: Bearer dir_…        (a device token; JARVIS-only recommended)
Content-Type: application/json
```

```json
{ "text": "add task call the dentist due friday", "today": "2026-10-24" }
```

| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `text` | string | yes | What was said. Leading and trailing spaces are trimmed. At most 1,000 characters. Capital letters and Dictation's trailing `.`/`?` are fine. |
| `today` | string `YYYY-MM-DD` | no | The phone's date. If it's missing, the server's UTC date is used. |
| `confirm` | string | no | The `plan_id` of a pending plan. `text` is then the answer to "Say yes to confirm". |

- The whole JSON body may be at most 64 KiB.
- **Yes** answers: yes, yeah, yep, yup, sure, ok, okay, confirm,
  confirmed, do it, go ahead, please do, approve, approved, correct,
  affirmative, yes please, sounds good.
- **No** answers: no, nope, nah, cancel, stop, don't, do not, never mind,
  nevermind, no thanks, no thank you, abort, forget it.
- The answer must be four words or fewer and must not mix yes and no.
  "Yes, do it." counts. "Yes but change the date" doesn't.

### Response (HTTP 200)

Every understood or not-understood request returns HTTP 200 with:

```json
{
  "speech": "Create task “call the dentist” due 2026-10-30. Say yes to confirm.",
  "status": "pending",
  "needs_confirmation": true,
  "plan_id": "…32 hex characters…"
}
```

| Field | Type | Notes |
| --- | --- | --- |
| `speech` | string | One short sentence to read aloud. Always present on 200. |
| `status` | string | See the table below. |
| `needs_confirmation` | boolean | `true` only when `status` is `pending`. |
| `plan_id` | string | Present only when `status` is `pending`. Send it back as `confirm`. |

| `status` | Meaning | Example `speech` |
| --- | --- | --- |
| `executed` | Done (a read, or a confirmed change). Also returned when you confirm a plan that already ran. | "Created task …", "That was already done." |
| `pending` | A change is waiting for yes or no. Nothing has changed yet. | "… Say yes to confirm.", "Please say yes or no." |
| `failed` | It ran, but a step failed. | The step's error |
| `cancelled` | You said no, or the plan was already cancelled. | "Okay, cancelled." |
| `expired` | The plan was older than 10 minutes. Nothing changed. | "That request expired, so nothing changed. Please ask again." |
| `running` | The same plan is executing right now. | "Still working on that." |
| `not_understood` | The parser didn't recognize a step. | "Sorry, I don't know how to “…” yet. Try: what should I do today." |
| `unresolved` | A name didn't match (task, habit, project). | "No active project matches “…”." |
| `invalid` | A value was out of range: too-long text, a bad `today`. | "Invalid input: too long." |
| `not_found` | The `confirm` id doesn't exist (or isn't yours). | "I couldn't find that request. Please ask again." |

A confirmed plan runs **exactly once**. Retrying the same `confirm` (for
example after a dropped connection) returns `executed` with "That was
already done." Your automations (Settings › Automations) fire when a
confirmed change completes a task or checks in a habit, using `today`.

### Errors (not HTTP 200; no `speech` field)

| HTTP | Body | Cause |
| --- | --- | --- |
| 400 | `{"error":"invalid_json","message":"…"}` | The body isn't JSON, or `text` is missing |
| 401 | `{"error":"unauthorized"}` | No token, a wrong token, or a revoked token |
| 413 | `{"error":"too_large"}` | The body is over 64 KiB |
| 502 | `{"error":"backend_unavailable"}` | The database had an error (logged) |
| 503 | `{"error":"auth_not_configured"}` | Staging hostname vars aren't set (not the case on the current deploy) |

The shortcut's *Otherwise* branch (step 10) covers all of these.

### What you can say

These are the commands the deterministic parser supports, which work
without AI. Join up to five with "then" or ";".

| Say | Does | Needs "yes" |
| --- | --- | --- |
| "What should I do today / tomorrow?", "What are my priorities?" | Top open tasks by the planner, with reasons | no |
| "I have 45 minutes", "What can I do in an hour?" | What fits, in priority order | no |
| "… for project Garden" (with either of the above) | The same, within one active project | no |
| "What's on tomorrow?", "Agenda Friday" | Events, tasks due, and open habits for that day | no |
| "List tasks", "What's open?" | Open tasks | no |
| "List projects" | Projects | no |
| "Find passport", "Search for taxes" | Search tasks, notes, projects, and more | no |
| "Add task file taxes high priority due Friday", "… takes 20 minutes", "… in project Home" | Create a task | yes |
| "Move taxes to Monday", "Clear the due date of taxes" | Change a due date | yes |
| "Set priority of taxes to high", "Make taxes low priority" | Change priority | yes |
| "Estimate taxes at 2 hours" | Set an estimate | yes |
| "Done taxes", "Complete taxes" | Complete a task | yes |
| "New project Garden" | Create a project | yes |
| "Check in stretch", "Check off stretch yesterday" | Habit check-in | yes |
| "Note gate code is 4512" | Create a note | yes |
| "Journal long walk by the river" | Add to today's journal | yes |
| "Capture look into solar panels", "Remember …" | Add to Inbox | yes |
| "Link note trip ideas to task book flights" | Link two items | yes |

Dates: today, tomorrow, a weekday (the next one), or `YYYY-MM-DD`.

**Not supported by voice** (no backend command exists): deleting
anything, creating or changing calendar events, creating habits, reading
note or file contents, mail, and reminders or notifications. Use the app
for those.
