import { describe, expect, it } from "vitest";
import { Api as Client, ApiError } from "./api";
import {
  EMPTY_CHAT,
  canUndo,
  decide,
  itemHref,
  replyLabel,
  say,
  toggle,
  toggleStep,
  undoes,
  type Api,
  type Chat,
  type ChatReply,
  type ChatRequest,
  type Context,
} from "./chat";
import { currentScreen, noteRoute, resetFocus, setShown, shownItem } from "./focus";
import type { Plan, PlannedAction } from "./jarvis";

const json = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

const CTX: Context = { today: "2026-10-29", time: "08:00" };
const CONV = "c".repeat(32);
const P1 = "a".repeat(32);
const P2 = "b".repeat(32);

const step = (summary: string, after?: number[]): PlannedAction => ({
  command: { type: "create_block" },
  summary,
  risk: "write",
  ...(after ? { after } : {}),
});

function plan(id: string, status: Plan["status"], actions: PlannedAction[], extra: Partial<Plan> = {}): Plan {
  return { id, input: "find four hours for my essay", actions, status, requires_approval: true, created_at: 1, expires_at: 2, results: [], ...extra };
}

const reply = (r: Partial<ChatReply>): ChatReply => ({ conversation_id: CONV, status: "plan", say: "", via: "model", ai: true, ...r });

/** A fake API that answers from a queue and records requests. */
function fake(replies: (ChatReply | Plan | Error)[]) {
  const sent: { kind: string; body: unknown }[] = [];
  const next = <T,>(): Promise<T> => {
    const r = replies.shift();
    if (!r) throw new Error("no reply queued");
    return r instanceof Error ? Promise.reject(r) : Promise.resolve(r as T);
  };
  const api: Api = {
    chat: (req: ChatRequest) => {
      sent.push({ kind: "chat", body: req });
      return next<ChatReply>();
    },
    execute: (id, body) => {
      sent.push({ kind: "execute", body: { id, ...body } });
      return next<Plan>();
    },
    cancel: (id) => {
      sent.push({ kind: "cancel", body: { id } });
      return next<Plan>();
    },
  };
  return { api, sent };
}

const noop = () => {};
const proposal = plan(P1, "pending", [step("Create task “Essay”"), step("Plan “Essay” Thu 14:00–16:00", [0]), step("Plan “Essay” Fri 09:00–11:00", [0])]);

describe("jarvisChat request", () => {
  it("posts the turn with the session cookie, and execute carries skipped steps", async () => {
    const calls: [string, RequestInit][] = [];
    const client = new Client(async (url, init) => {
      calls.push([String(url), init!]);
      return json(200, String(url).includes("execute") ? plan(P1, "executed", []) : reply({ status: "answer", say: "Hi." }));
    });
    await client.jarvisChat({ text: "hello", today: CTX.today, time: "08:00", screen: { route: "tasks", task_id: P2 } });
    await client.jarvisExecute(P1, { skip: [2], conversation_id: CONV });
    expect(calls[0][0]).toBe("/api/v1/jarvis/chat");
    expect(calls[0][1].credentials).toBe("same-origin");
    expect(new Headers(calls[0][1].headers).get("Authorization")).toBeNull();
    expect(JSON.parse(String(calls[0][1].body))).toEqual({ text: "hello", today: CTX.today, time: "08:00", screen: { route: "tasks", task_id: P2 } });
    expect(calls[1][0]).toMatch(new RegExp(`^/api/v1/jarvis/plans/${P1}/execute\\?today=`));
    expect(JSON.parse(String(calls[1][1].body))).toEqual({ skip: [2], conversation_id: CONV });
  });
});

describe("chat state", () => {
  it("keeps the conversation id and screen, and a pending plan waits for confirm", async () => {
    const { api, sent } = fake([reply({ plan: proposal, say: "Here's a plan." })]);
    const states: Chat[] = [];
    const c = await say(EMPTY_CHAT, "  find four hours  ", { ...CTX, screen: { route: "tasks" } }, api, (s) => states.push(s));
    expect(states[0].busy).toBe(true);
    expect(states[0].turns[0]).toMatchObject({ who: "you", text: "find four hours" });
    expect(sent[0].body).toMatchObject({ text: "find four hours", screen: { route: "tasks" } });
    expect(c.busy).toBe(false);
    expect(c.conversationId).toBe(CONV);
    expect(c.pending).toEqual({ turnId: 2, planId: P1, skip: [] });

    const { api: api2, sent: sent2 } = fake([reply({ status: "answer", say: "Sure." })]);
    const c2 = await say(c, "what about friday", CTX, api2, noop);
    expect(sent2[0].body).toMatchObject({ conversation_id: CONV });
    expect(c2.pending?.planId).toBe(P1);
  });

  it("leaving a step out leaves out what needs it; putting it back restores prerequisites", () => {
    expect(toggleStep(proposal, [], 0)).toEqual([0, 1, 2]);
    expect(toggleStep(proposal, [0, 1, 2], 1)).toEqual([2]);
    expect(toggleStep(proposal, [], 2)).toEqual([2]);
    expect(toggleStep(proposal, [2], 2)).toEqual([]);
  });

  it("confirms exactly the previewed plan with its skipped steps, once", async () => {
    const { api } = fake([reply({ plan: proposal })]);
    let c = await say(EMPTY_CHAT, "find four hours", CTX, api, noop);
    c = toggle(c, 2);
    const ran = plan(P1, "executed", proposal.actions, {
      results: [
        { ok: true, message: "Created task “Essay”.", entity: { kind: "task", id: P2 } },
        { ok: true, message: "Planned “Essay” Thu 14:00–16:00.", item: { kind: "block", id: P2, date: "2026-10-29" }, undo: { op: "delete_block" } },
        { ok: true, message: "Left out.", skipped: true },
      ],
    });
    const { api: api2, sent } = fake([ran]);
    const first = decide(c, true, api2, noop);
    // A double tap while the first is in flight sends nothing.
    const busy = { ...c, busy: true };
    expect(await decide(busy, true, api2, noop)).toBe(busy);
    c = await first;
    expect(sent).toEqual([{ kind: "execute", body: { id: P1, skip: [2], conversation_id: CONV } }]);
    expect(c.pending).toBeUndefined();
    expect(c.turns.find((t) => t.id === 2)).toMatchObject({ outcome: "confirmed" });
    const last = c.turns.at(-1)!;
    expect(last.who === "jarvis" && last.reply.say).toBe("Created task “Essay”. Planned “Essay” Thu 14:00–16:00.");
    expect(c.turns.find((t) => t.who === "you" && t.text.startsWith("Confirm, without 1 step"))).toBeTruthy();
  });

  it("a newer plan replaces the pending one; an expired one is dropped", async () => {
    const revised = plan(P2, "pending", [step("Plan “Essay” Thu 13:00–15:00")], { revises: P1 });
    const { api } = fake([reply({ plan: proposal }), reply({ plan: revised, replaces: P1 }), reply({ status: "expired", say: "That request expired." })]);
    let c = await say(EMPTY_CHAT, "find four hours", CTX, api, noop);
    c = await say(c, "move it an hour earlier", CTX, api, noop);
    expect(c.turns.find((t) => t.id === 2)).toMatchObject({ outcome: "replaced" });
    expect(c.pending?.planId).toBe(P2);
    c = await say(c, "yes", CTX, api, noop);
    expect(c.pending).toBeUndefined();
    expect(c.turns.find((t) => t.id === 4)).toMatchObject({ outcome: "dropped" });
  });

  it("a typed yes that runs the plan settles it", async () => {
    const { api } = fake([reply({ plan: proposal }), reply({ plan: plan(P1, "executed", proposal.actions, { results: [] }) })]);
    let c = await say(EMPTY_CHAT, "find four hours", CTX, api, noop);
    c = await say(c, "yes", CTX, api, noop);
    expect(c.pending).toBeUndefined();
    expect(c.turns.find((t) => t.id === 2)).toMatchObject({ outcome: "confirmed" });
  });

  it("network failures can be retried; a decided plan is not retried", async () => {
    const { api } = fake([new TypeError("Failed to fetch")]);
    const c = await say(EMPTY_CHAT, "what's next", CTX, api, noop);
    expect(c.turns.at(-1)).toMatchObject({ who: "error", retry: { kind: "say", text: "what's next" } });

    const { api: api2 } = fake([reply({ plan: proposal }), new TypeError("Failed to fetch"), new ApiError(409, "not_pending")]);
    let d = await say(EMPTY_CHAT, "find four hours", CTX, api2, noop);
    d = await decide(d, true, api2, noop);
    expect(d.pending?.planId).toBe(P1);
    expect(d.turns.at(-1)).toMatchObject({ who: "error", retry: { kind: "decide", yes: true } });
    d = await decide(d, true, api2, noop);
    expect(d.pending).toBeUndefined();
    expect(d.turns.at(-1)).toMatchObject({ who: "error", text: expect.stringContaining("already decided") });
    expect(d.turns.at(-1)).toMatchObject({ retry: undefined });
  });

  it("choices send their number but show the title; undo names the plan", async () => {
    const { api, sent } = fake([reply({ status: "answer", say: "Done." }), reply({ status: "answer", say: "Hm." })]);
    let c = await say(EMPTY_CHAT, "2", CTX, api, noop, { shown: "Essay draft" });
    expect(c.turns[0]).toMatchObject({ who: "you", text: "Essay draft" });
    expect(sent[0].body).toMatchObject({ text: "2" });
    c = await say(c, "Undo “add task x”", CTX, api, noop, { undo: P1 });
    expect(sent[1].body).toMatchObject({ undo: P1 });
  });

  it("an executed undo marks the plan it reversed", async () => {
    const done = plan(P1, "executed", [step("Plan")], { results: [{ ok: true, message: "Planned.", undo: { op: "delete_block" } }] });
    expect(canUndo(done)).toBe(true);
    const undo = plan(P2, "executed", [{ command: { type: "revert", plan_id: P1 }, summary: "Remove", risk: "write" }], {
      results: [{ ok: true, message: "Removed." }],
    });
    expect(undoes(undo)).toBe(P1);
    expect(canUndo(undo)).toBe(false);
    expect(canUndo({ ...done, reversed_by: P2 })).toBe(false);
    const { api } = fake([reply({ plan: done }), reply({ plan: undo })]);
    let c = await say(EMPTY_CHAT, "x", CTX, api, noop);
    c = await say(c, "undo", CTX, api, noop);
    expect(c.turns.find((t) => t.id === 2)).toMatchObject({ undone: true });
  });

  it("labels partial results and links what changed", () => {
    const partly = plan(P1, "executed", [step("a"), step("b")], { results: [{ ok: true, message: "a" }, { ok: false, message: "b failed" }] });
    expect(replyLabel(reply({ plan: partly }))).toBe("partly done");
    expect(replyLabel(reply({ status: "ask" }))).toBe("question");
    expect(itemHref({ ok: true, message: "", item: { kind: "block", id: P1, date: "2026-10-30" } })).toBe("#/calendar?date=2026-10-30&view=day");
    expect(itemHref({ ok: true, message: "", entity: { kind: "task", id: P1 } })).toBe(`#/tasks?task=${P1}`);
    expect(itemHref({ ok: true, message: "" })).toBeUndefined();
  });
});

describe("screen focus", () => {
  it("reports the screen Ask was opened from, without empty fields", () => {
    resetFocus();
    expect(currentScreen()).toBeUndefined();
    setShown("tasks", { task_id: P1 });
    setShown("calendar", { date: "2026-10-30", ...shownItem(`b:${P2}:2026-10-30`) });
    noteRoute("tasks");
    noteRoute("ask");
    expect(currentScreen()).toEqual({ route: "tasks", task_id: P1 });
    noteRoute("calendar");
    expect(currentScreen()).toEqual({ route: "calendar", date: "2026-10-30", block_id: P2 });
    setShown("calendar", { date: "2026-10-30", ...shownItem(undefined) });
    expect(currentScreen()).toEqual({ route: "calendar", date: "2026-10-30" });
    expect(shownItem(`e:${P1}:2026-10-01:2026-10-02`)).toEqual({ event_id: P1 });
    expect(shownItem(`d:${P1}`)).toEqual({ task_id: P1 });
  });
});
