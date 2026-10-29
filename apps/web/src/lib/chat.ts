/**
 * JARVIS › Ask: a conversation over `POST /api/v1/jarvis/chat` (D-048).
 * The server keeps the conversation (what "it" and "the second one" mean,
 * the plan awaiting approval); this module keeps what the screen shows.
 * Pure state and request logic; the screen is modules/Ask.tsx.
 */
import { ApiError } from "./api";
import type { Plan, PlanStatus } from "./jarvis";
import { entityRoute } from "./links";
import { askErrorMessage } from "./talk";

/** An answer JARVIS offers to its own question. */
export interface Choice {
  kind: string;
  id: string;
  title: string;
}

/** What the owner is looking at: hints only, the server checks each id. */
export interface Screen {
  route?: string;
  task_id?: string;
  event_id?: string;
  block_id?: string;
  project_id?: string;
  note_id?: string;
  date?: string;
}

export interface ChatRequest {
  text: string;
  today: string;
  time?: string;
  conversation_id?: string;
  screen?: Screen;
  skip?: number[];
  /** Propose undoing this executed plan. */
  undo?: string;
}

/** One reply (core `jarvis::chat::ChatReply`). */
export interface ChatReply {
  conversation_id: string;
  /** `plan`, `ask`, `answer`, `cancelled`, `reset`, or an error status. */
  status: string;
  say: string;
  plan?: Plan;
  choices?: Choice[];
  /** Who understood the request. */
  via: "parser" | "model";
  /** Whether an AI model is configured. */
  ai: boolean;
  replaces?: string;
  examples?: string[];
}

export interface Api {
  chat: (req: ChatRequest) => Promise<ChatReply>;
  execute: (id: string, body: { skip: number[]; conversation_id?: string }) => Promise<Plan>;
  cancel: (id: string) => Promise<Plan>;
}

/** The user's local date and time, and what screen they came from. */
export interface Context {
  today: string;
  time: string;
  screen?: Screen;
}

/** What to do again after a failed request. */
export type Retry = { kind: "say"; text: string; opts?: SayOptions } | { kind: "decide"; yes: boolean };

export interface SayOptions {
  /** Propose undoing this executed plan. */
  undo?: string;
  /** What the conversation shows instead of `text` (a picked choice's title). */
  shown?: string;
}

export type Outcome = "confirmed" | "cancelled" | "replaced" | "dropped";

export type ChatTurn =
  | { id: number; who: "you"; text: string }
  | {
      id: number;
      who: "jarvis";
      reply: ChatReply;
      /** For a proposed plan: what became of it. */
      outcome?: Outcome;
      /** An undo of this turn's plan ran. */
      undone?: boolean;
    }
  | { id: number; who: "error"; text: string; retry?: Retry };

export interface Chat {
  turns: ChatTurn[];
  /** A request is in flight: every send and decision is refused until it ends. */
  busy: boolean;
  conversationId?: string;
  /** The plan awaiting Confirm or Cancel, and the steps left out of it. */
  pending?: { turnId: number; planId: string; skip: number[] };
  /** From the last reply: whether only direct commands work. */
  ai?: boolean;
  nextId: number;
}

export const EMPTY_CHAT: Chat = { turns: [], busy: false, nextId: 1 };

/** Tones for status labels (tokens in index.css). */
export function replyTone(r: ChatReply): "ok" | "accent" | "bad" | "idle" | "warn" {
  if (r.plan) return PLAN_TONE[r.plan.status];
  switch (r.status) {
    case "answer":
      return "ok";
    case "ask":
      return "accent";
    case "cancelled":
    case "reset":
    case "expired":
      return "idle";
    case "executed":
      return "ok";
    default:
      return "warn";
  }
}

const PLAN_TONE: Record<PlanStatus, "ok" | "accent" | "bad" | "idle"> = {
  pending: "accent",
  running: "accent",
  executed: "ok",
  failed: "bad",
  cancelled: "idle",
  expired: "idle",
};

export function replyLabel(r: ChatReply): string {
  if (r.plan) {
    const p = r.plan;
    if (p.status === "executed" && p.results.some((x) => !x.ok)) return "partly done";
    return { pending: "needs confirm", running: "running", executed: "done", failed: "failed", cancelled: "cancelled", expired: "expired" }[p.status];
  }
  return (
    {
      answer: "answer",
      ask: "question",
      cancelled: "cancelled",
      reset: "new conversation",
      expired: "expired",
      executed: "already done",
      not_understood: "not understood",
      unresolved: "no match",
      invalid: "invalid",
      not_found: "not found",
    }[r.status] ?? r.status
  );
}

/** A plan's executed results as one sentence for the screen and voice. */
export function resultSay(plan: Plan): string {
  const ran = plan.results.filter((r) => !r.skipped);
  if (plan.status === "cancelled") return "Okay, I won't do that.";
  if (!ran.length) return "Nothing ran.";
  return ran.map((r) => r.message).join(" ");
}

/**
 * Steps left out after toggling step `i`. Leaving a step out also leaves out
 * the steps that need it; putting one back puts back what it needs.
 */
export function toggleStep(plan: Plan, skip: number[], i: number): number[] {
  const out = new Set(skip);
  if (out.has(i)) {
    const back = (j: number) => {
      out.delete(j);
      for (const k of plan.actions[j]?.after ?? []) if (out.has(k)) back(k);
    };
    back(i);
  } else {
    const drop = (j: number) => {
      out.add(j);
      plan.actions.forEach((a, k) => {
        if (!out.has(k) && a.after?.includes(j)) drop(k);
      });
    };
    drop(i);
  }
  return [...out].sort((a, b) => a - b);
}

/** The plan id an undo plan reverses (its `revert` steps name it). */
export function undoes(plan: Plan): string | undefined {
  const step = plan.actions.find((a) => a.command.type === "revert");
  const id = step?.command.plan_id;
  return typeof id === "string" ? id : undefined;
}

/** Whether a plan that ran can still be undone from the screen. */
export function canUndo(plan: Plan): boolean {
  return (plan.status === "executed" || plan.status === "failed") && !plan.reversed_by && !undoes(plan) && plan.results.some((r) => r.undo);
}

type NewTurn = ChatTurn extends infer T ? (T extends ChatTurn ? Omit<T, "id"> : never) : never;

function add(c: Chat, turn: NewTurn): Chat {
  return { ...c, turns: [...c.turns, { ...turn, id: c.nextId } as ChatTurn], nextId: c.nextId + 1 };
}

function settle(c: Chat, outcome: Outcome): Chat {
  const p = c.pending;
  if (!p) return c;
  return {
    ...c,
    pending: undefined,
    turns: c.turns.map((x) => (x.id === p.turnId && x.who === "jarvis" ? { ...x, outcome } : x)),
  };
}

/** Mark the turn whose plan an executed undo reversed. */
function markUndone(c: Chat, plan: Plan): Chat {
  const target = plan.status === "executed" ? undoes(plan) : undefined;
  if (!target) return c;
  return { ...c, turns: c.turns.map((x) => (x.who === "jarvis" && x.reply.plan?.id === target ? { ...x, undone: true } : x)) };
}

/** Fold a reply into the chat: a new pending plan, or what became of the old one. */
function replied(c: Chat, reply: ChatReply): Chat {
  let s: Chat = { ...c, busy: false, conversationId: reply.conversation_id, ai: reply.ai };
  const plan = reply.plan;
  const waiting = s.pending;
  if (waiting) {
    if (plan?.id === waiting.planId && plan.status !== "pending") {
      s = settle(s, plan.status === "cancelled" ? "cancelled" : plan.status === "expired" ? "dropped" : "confirmed");
    } else if (plan?.status === "pending" || reply.replaces === waiting.planId) {
      s = settle(s, "replaced");
    } else if (["reset", "expired", "executed", "cancelled", "not_found"].includes(reply.status)) {
      // The server no longer waits on it.
      s = settle(s, "dropped");
    }
  }
  if (plan) s = markUndone(s, plan);
  const next = add(s, { who: "jarvis", reply });
  return plan?.status === "pending" ? { ...next, pending: { turnId: s.nextId, planId: plan.id, skip: [] } } : next;
}

function failed(c: Chat, e: unknown, retry?: Retry): Chat {
  return add({ ...c, busy: false }, { who: "error", text: chatErrorMessage(e), retry });
}

export function chatErrorMessage(e: unknown): string {
  if (e instanceof ApiError && e.status === 409) return "That change was already decided or has expired. Nothing new ran.";
  if (e instanceof ApiError && e.status === 404) return "I couldn't find that change any more. Please ask again.";
  return askErrorMessage(e);
}

/**
 * Send a message (or, with `opts.undo`, ask to undo that plan). A plan still
 * awaiting approval stays pending on the server until the reply replaces it.
 */
export async function say(
  c: Chat,
  text: string,
  ctx: Context,
  api: Api,
  update: (c: Chat) => void,
  opts: SayOptions = {},
): Promise<Chat> {
  const said = text.trim();
  if (c.busy || !said) return c;
  let s = add({ ...c, busy: true }, { who: "you", text: opts.shown ?? said });
  update(s);
  try {
    const req: ChatRequest = { text: said, today: ctx.today, time: ctx.time, conversation_id: c.conversationId, screen: ctx.screen };
    if (opts.undo) req.undo = opts.undo;
    s = replied(s, await api.chat(req));
  } catch (e) {
    s = failed(s, e, { kind: "say", text: said, opts });
  }
  update(s);
  return s;
}

/** Leave step `i` of the pending plan out (or put it back). */
export function toggle(c: Chat, i: number): Chat {
  const p = c.pending;
  if (c.busy || !p) return c;
  const turn = c.turns.find((x) => x.id === p.turnId);
  const plan = turn?.who === "jarvis" ? turn.reply.plan : undefined;
  if (!plan) return c;
  return { ...c, pending: { ...p, skip: toggleStep(plan, p.skip, i) } };
}

/**
 * Confirm or cancel the pending plan by its id, so exactly the previewed
 * plan runs. Ignored while busy, so a double tap sends once; the server also
 * runs a plan at most once.
 */
export async function decide(c: Chat, yes: boolean, api: Api, update: (c: Chat) => void): Promise<Chat> {
  const p = c.pending;
  if (c.busy || !p) return c;
  const skipped = p.skip.length;
  let s = add({ ...c, busy: true }, { who: "you", text: yes ? (skipped ? `Confirm, without ${skipped} step${skipped > 1 ? "s" : ""}` : "Confirm") : "Cancel" });
  update(s);
  try {
    const plan = yes ? await api.execute(p.planId, { skip: p.skip, conversation_id: c.conversationId }) : await api.cancel(p.planId);
    const reply: ChatReply = {
      conversation_id: c.conversationId ?? "",
      status: plan.status === "cancelled" ? "cancelled" : "plan",
      say: resultSay(plan),
      plan,
      via: "parser",
      ai: c.ai ?? false,
    };
    s = replied(s, reply);
  } catch (e) {
    // 409/404: it was decided elsewhere or expired, so nothing waits any more.
    // Otherwise the result is unknown (e.g. the connection dropped): keep it
    // pending so a retry can run it; a plan runs once.
    if (e instanceof ApiError && (e.status === 409 || e.status === 404)) s = failed(settle(s, "dropped"), e);
    else s = failed(s, e, { kind: "decide", yes });
  }
  update(s);
  return s;
}

/** A link that opens what a step created or changed. */
export function itemHref(r: Plan["results"][number]): string | undefined {
  if (r.item) return `#/calendar?date=${r.item.date}&view=day`;
  return r.entity ? entityRoute(r.entity) : undefined;
}
