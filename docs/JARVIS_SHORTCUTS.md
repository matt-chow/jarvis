# JARVIS on iPhone (Apple Shortcuts)

JARVIS Alpha on iPhone is an Apple Shortcut. It uses Apple's built-in
**Dictate Text** for speech input and **Speak Text** for replies, and sends
the transcript to Directory over HTTPS. Directory has no custom speech
recognition, no voice synthesis, no wake word, and no continuous listening.
Audio never reaches Directory, only the text Apple transcribed.

```
Lock Screen control / Siri / Action button
  → Dictate Text (Apple)
  → POST /api/v1/jarvis/ask  (Bearer device token, JSON)
  → Speak Text (Apple) of `speech`
  → if `needs_confirmation`: Dictate "yes"/"no" → POST again with `confirm`
```

## The endpoint

`POST /api/v1/jarvis/ask` with `Authorization: Bearer dir_…`:

```json
{ "text": "add task call the dentist due friday", "today": "2026-10-24" }
```

- `text`: what was said, up to 1,000 characters.
- `today`: the phone's local date (`YYYY-MM-DD`). It anchors "today",
  "tomorrow", and weekdays. If omitted, the server uses its UTC date.
- `confirm` (optional): the `plan_id` of a pending plan. `text` is then the
  answer: yes, yeah, confirm, do it, okay, sure, or go ahead, versus no,
  cancel, stop, or never mind.

The response is always JSON with a `speech` line to read aloud:

```json
{
  "speech": "Create task “call the dentist” due 2026-10-31. Say yes to confirm.",
  "status": "pending",
  "needs_confirmation": true,
  "plan_id": "…32 hex…"
}
```

| `status` | Meaning |
| --- | --- |
| `executed` | Done. `speech` is the result: the answer, or what changed. |
| `pending` | A change awaits confirmation (`needs_confirmation: true`). Nothing has changed yet. |
| `failed` | It ran, but a step failed; `speech` says which. |
| `cancelled` | You said no. |
| `not_understood` / `unresolved` / `invalid` | Nothing ran; `speech` explains and suggests a phrasing. |

Understanding problems come back as HTTP 200 with a `speech` line, so the
Shortcut always has something to say. Auth failures are 401 (bad or revoked
token) or 403 (wrong token type). HTTP 409 can't happen on `ask`: a repeated
confirmation answers "That was already done."

## Safety

- **Auth:** a device token, sent as a Bearer header, resolves the
  `Principal`. Every command runs as that owner, through the module
  services (never raw database access).
- **Use a JARVIS-only token.** In Settings → Device tokens, choose
  **JARVIS only (Shortcuts)**. That token works only for
  `/api/v1/jarvis/ask`: it can't list files, export data, or call any other
  route (403 `token_scope`). If the phone or Shortcut leaks, revoke it in
  Settings.
- **Confirmation:** anything that changes data (create or update tasks,
  complete, notes, journal, capture, link, check-in) is previewed and runs
  only after you say yes. Reading (priorities, suggestions, agenda, lists,
  find) runs at once.
- **No duplicates:** a confirmed plan runs exactly once. Repeating the
  confirmation, say after a network retry, returns "That was already done."
  Pending plans expire after 10 minutes.
- **AI is optional** (see below). Without it, JARVIS uses the
  deterministic parser, and everything here works.

## Build the Shortcut (about 5 minutes)

First, in Directory → Settings → Device tokens, create a **JARVIS only**
token named "iPhone Shortcut". Copy the `dir_…` secret; it's shown once.

In the Shortcuts app, create a new shortcut named **JARVIS**:

1. **Dictate Text**. Language: yours. Stop Listening: *After Pause*.
2. **Format Date**: Current Date, Custom format `yyyy-MM-dd`.
3. **Get Contents of URL**:
   - URL: `https://<your host>/api/v1/jarvis/ask`
   - Method: **POST**
   - Headers: `Authorization` = `Bearer dir_…` (your token), and
     `Content-Type` = `application/json`
   - Request Body: **JSON**, with `text` = *Dictated Text* and `today` =
     *Formatted Date*
4. **Get Dictionary Value**: `speech` from *Contents of URL*.
5. **Speak Text**: *Dictionary Value*.
6. **Get Dictionary Value**: `needs_confirmation` from *Contents of URL*.
7. **If** *Dictionary Value* **is** `1` (Shortcuts shows booleans as 1/0):
   1. **Dictate Text** (for the yes/no answer).
   2. **Get Dictionary Value**: `plan_id` from *Contents of URL*.
   3. **Get Contents of URL**: the same URL, method, and headers. JSON body:
      `text` = the new *Dictated Text*, `today` = *Formatted Date*,
      `confirm` = *plan_id*.
   4. **Get Dictionary Value**: `speech`, then **Speak Text**.
8. **End If**.

Keep the token out of shared shortcuts. If you share the shortcut, remove
the header value first.

## Activate it

- **Siri:** "Hey Siri, JARVIS" runs the shortcut by name.
- **Lock Screen:** long-press the Lock Screen → Customize → Lock Screen →
  add a control → **Shortcut** → JARVIS (iOS 18+). Shortcuts that use the
  network may ask you to unlock first; that is an iOS rule.
- **Action button** (iPhone 15 Pro and later): Settings → Action Button →
  Shortcut → JARVIS.
- **Control Center:** add the same Shortcut control there.

## Things to say

| Say | Does |
| --- | --- |
| "What should I do today?" / "What are my priorities?" | Top 5 open tasks by the planner, with reasons |
| "I have 45 minutes" / "What can I do in an hour?" | What fits, in priority order |
| "What should I do for project Garden?" / "I have 30 minutes in project Work" | The same, within one active project |
| "Add task file taxes high priority due Friday" | Creates the task after you confirm |
| "Add task stretch takes 10 minutes" | Task with an estimate |
| "Move file taxes to Monday" / "Clear the due date of taxes" | Re-dates after you confirm |
| "Set priority of taxes to high" / "Make taxes low priority" | Changes priority after you confirm |
| "Estimate taxes at 2 hours" | Sets the estimate after you confirm |
| "Done file taxes" / "What's on tomorrow?" / "List tasks" | Complete (after confirming), agenda, list |

Several steps can be chained with "then". For example: "add task buy milk
then what should I do today".

## AI intent parsing (optional, $0)

When the deterministic parser doesn't understand a sentence, JARVIS can ask
a **free** OpenRouter model to turn it into the same typed commands. The
model gets only the sentence, today's date, and the command schema: no
tasks, no ids, and no database access. Its answer is validated like typed
input, names are resolved by Directory, and anything that changes data
still needs your "yes".

It's off unless both are set (owner action; no billing):

```sh
npx wrangler secret put OPENROUTER_API_KEY --env staging   # key from openrouter.ai, $0 credit
```

and, in `wrangler.toml` `[env.staging.vars]`,
`JARVIS_AI_MODEL = "<model id ending in :free>"`.

- Only model ids ending in `:free` are accepted; anything else leaves AI
  off. There is no fallback model and no automatic upgrade to paid models.
  With $0 credit, OpenRouter refuses paid calls anyway.
- Free models are rate limited (about 50 requests a day without credit).
  Directory calls the model only when the parser fails, at most
  `JARVIS_AI_DAILY` times per day (default 40). Past that, you get the
  normal "didn't understand" reply.
- The sentence you said is sent to OpenRouter and the model's provider.
  Don't enable AI if that's not acceptable.
