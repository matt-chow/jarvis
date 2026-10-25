/**
 * JARVIS › Ask: a conversation over `POST /api/v1/jarvis/ask` (D-044), the
 * same contract the iPhone Shortcut uses (docs/IPHONE_SHORTCUT.md). Pure
 * state and request logic; the screen is modules/Ask.tsx.
 */
import { ApiError, isNetworkError } from "./api";

export type AskStatus =
  | "executed"
  | "pending"
  | "failed"
  | "cancelled"
  | "expired"
  | "running"
  | "not_understood"
  | "unresolved"
  | "invalid"
  | "not_found";

/** One `ask` reply (core `jarvis::voice::Answer`). */
export interface AskAnswer {
  speech: string;
  status: AskStatus;
  needs_confirmation: boolean;
  plan_id?: string;
}

export type AskFn = (text: string, today: string, confirm?: string) => Promise<AskAnswer>;

/** Mirrors core `jarvis::MAX_INPUT`. */
export const MAX_ASK = 1000;

export type Turn =
  | { id: number; who: "you"; text: string }
  | {
      id: number;
      who: "jarvis";
      answer: AskAnswer;
      /** For a pending answer: what became of it. */
      outcome?: "confirmed" | "cancelled" | "dropped";
    }
  | { id: number; who: "error"; text: string };

export interface Talk {
  turns: Turn[];
  /** A request is in flight: every send and confirm is refused until it ends. */
  busy: boolean;
  /** The answer awaiting Confirm or Cancel. */
  pending?: { turnId: number; planId: string };
  nextId: number;
}

export const EMPTY_TALK: Talk = { turns: [], busy: false, nextId: 1 };

/** Tones for status labels (tokens in index.css). */
export const ASK_TONE: Record<AskStatus, "ok" | "accent" | "bad" | "idle" | "warn"> = {
  executed: "ok",
  pending: "accent",
  running: "accent",
  failed: "bad",
  cancelled: "idle",
  expired: "idle",
  not_understood: "warn",
  unresolved: "warn",
  invalid: "warn",
  not_found: "warn",
};

export const ASK_LABEL: Record<AskStatus, string> = {
  executed: "done",
  pending: "needs confirm",
  running: "running",
  failed: "failed",
  cancelled: "cancelled",
  expired: "expired",
  not_understood: "not understood",
  unresolved: "no match",
  invalid: "invalid",
  not_found: "not found",
};

/** An actionable sentence for a failed `ask` request (no `speech` came back). */
export function askErrorMessage(e: unknown): string {
  if (e instanceof ApiError) {
    switch (e.status) {
      case 401:
        return "Your session has ended. Sign in again, then repeat the request.";
      case 403:
        return e.code === "origin_not_allowed"
          ? "This address isn't allowed to make changes. Open Directory from its usual address."
          : `Not allowed (${e.code}).`;
      case 413:
        return "That's too long. Keep it under 1,000 characters.";
      case 502:
        return "Directory's database had a problem. Nothing changed; try again in a moment.";
      case 503:
        return "Sign-in isn't configured on this server yet.";
      default:
        return `JARVIS couldn't handle that (${e.code}).`;
    }
  }
  if (isNetworkError(e)) return "Can't reach Directory. Check the connection and try again.";
  return String(e);
}

type NewTurn = Turn extends infer T ? (T extends Turn ? Omit<T, "id"> : never) : never;

function add(t: Talk, turn: NewTurn): Talk {
  return { ...t, turns: [...t.turns, { ...turn, id: t.nextId } as Turn], nextId: t.nextId + 1 };
}

function settle(t: Talk, outcome: "confirmed" | "cancelled" | "dropped"): Talk {
  const p = t.pending;
  if (!p) return t;
  return {
    ...t,
    pending: undefined,
    turns: t.turns.map((x) => (x.id === p.turnId && x.who === "jarvis" ? { ...x, outcome } : x)),
  };
}

function answered(t: Talk, answer: AskAnswer): Talk {
  const next = add({ ...t, busy: false }, { who: "jarvis", answer });
  return answer.status === "pending" && answer.plan_id
    ? { ...next, pending: { turnId: t.nextId, planId: answer.plan_id } }
    : next;
}

function failed(t: Talk, e: unknown): Talk {
  return add({ ...t, busy: false }, { who: "error", text: askErrorMessage(e) });
}

/**
 * Send a request. A change still awaiting confirmation is dropped (it was
 * never run, and expires on the server). Returns the new state at each step.
 */
export async function send(t: Talk, text: string, today: string, ask: AskFn, update: (t: Talk) => void): Promise<Talk> {
  const said = text.trim();
  if (t.busy || !said) return t;
  let s = add({ ...settle(t, "dropped"), busy: true }, { who: "you", text: said });
  update(s);
  try {
    s = answered(s, await ask(said, today));
  } catch (e) {
    s = failed(s, e);
  }
  update(s);
  return s;
}

/**
 * Answer the pending change with "yes" or "no", using its `plan_id`. Ignored
 * while busy or when nothing is pending, so a double tap can't send twice.
 * The server also runs a plan at most once ("That was already done.").
 */
export async function decide(t: Talk, yes: boolean, today: string, ask: AskFn, update: (t: Talk) => void): Promise<Talk> {
  const p = t.pending;
  if (t.busy || !p) return t;
  let s = add({ ...t, busy: true }, { who: "you", text: yes ? "Confirm" : "Cancel" });
  update(s);
  try {
    const answer = await ask(yes ? "yes" : "no", today, p.planId);
    // "Please say yes or no." keeps it pending; can't happen for exact yes/no.
    s = answer.status === "pending" ? answered(s, answer) : answered(settle(s, yes ? "confirmed" : "cancelled"), answer);
  } catch (e) {
    // Unknown result (e.g. the connection dropped): keep it pending, so the
    // user can retry; a repeat is safe because a plan runs once.
    s = failed(s, e);
  }
  update(s);
  return s;
}
