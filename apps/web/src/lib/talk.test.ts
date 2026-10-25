import { describe, expect, it, vi } from "vitest";
import { Api, ApiError } from "./api";
import { EMPTY_TALK, askErrorMessage, decide, send, type AskAnswer, type AskFn, type Talk } from "./talk";

const json = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

const TODAY = "2026-10-24";
const PLAN = "a".repeat(32);

const pending: AskAnswer = {
  speech: "Create task “call the dentist” due 2026-10-30. Say yes to confirm.",
  status: "pending",
  needs_confirmation: true,
  plan_id: PLAN,
};
const done = (speech: string): AskAnswer => ({ speech, status: "executed", needs_confirmation: false });

describe("jarvisAsk request", () => {
  it("posts text and the local date with the session cookie, no token", async () => {
    const calls: [string, RequestInit][] = [];
    const api = new Api(async (url, init) => {
      calls.push([String(url), init!]);
      return json(200, done("Nothing open. Nice."));
    });
    await expect(api.jarvisAsk("what should I do today", TODAY)).resolves.toMatchObject({ status: "executed" });
    const [url, init] = calls[0];
    expect(url).toBe("/api/v1/jarvis/ask");
    expect(init.method).toBe("POST");
    expect(init.credentials).toBe("same-origin");
    const headers = new Headers(init.headers);
    expect(headers.get("Authorization")).toBeNull();
    expect(headers.get("Content-Type")).toBe("application/json");
    expect(JSON.parse(String(init.body))).toEqual({ text: "what should I do today", today: TODAY });
  });

  it("sends the plan id as confirm", async () => {
    const bodies: unknown[] = [];
    const api = new Api(async (_url, init) => {
      bodies.push(JSON.parse(String(init!.body)));
      return json(200, done("Created task “call the dentist”."));
    });
    await api.jarvisAsk("yes", TODAY, PLAN);
    expect(bodies[0]).toEqual({ text: "yes", today: TODAY, confirm: PLAN });
  });

  it("surfaces 401 as an ApiError", async () => {
    const api = new Api(async () => json(401, { error: "unauthorized" }));
    await expect(api.jarvisAsk("list tasks", TODAY)).rejects.toMatchObject({ status: 401, code: "unauthorized" });
  });
});

/** A fake `ask` answering from a queue, recording each call. */
function fakeAsk(...answers: (AskAnswer | Error)[]) {
  const calls: [string, string, string | undefined][] = [];
  const ask: AskFn = vi.fn(async (text, today, confirm) => {
    calls.push([text, today, confirm]);
    const next = answers.shift();
    if (!next) throw new Error("unexpected ask");
    if (next instanceof Error) throw next;
    return next;
  });
  return { ask, calls };
}

describe("conversation", () => {
  it("shows the question, then the answer", async () => {
    const { ask, calls } = fakeAsk(done("Top 1 for 2026-10-24: Taxes."));
    const seen: Talk[] = [];
    const t = await send(EMPTY_TALK, "  what should I do today  ", TODAY, ask, (s) => seen.push(s));
    expect(calls).toEqual([["what should I do today", TODAY, undefined]]);
    expect(seen[0].busy).toBe(true);
    expect(seen[0].turns).toEqual([{ id: 1, who: "you", text: "what should I do today" }]);
    expect(t.busy).toBe(false);
    expect(t.turns[1]).toMatchObject({ who: "jarvis", answer: { speech: "Top 1 for 2026-10-24: Taxes." } });
    expect(t.pending).toBeUndefined();
  });

  it("ignores empty input and sends while busy", async () => {
    const { ask } = fakeAsk();
    expect(await send(EMPTY_TALK, "   ", TODAY, ask, () => {})).toBe(EMPTY_TALK);
    const busy = { ...EMPTY_TALK, busy: true };
    expect(await send(busy, "list tasks", TODAY, ask, () => {})).toBe(busy);
    expect(ask).not.toHaveBeenCalled();
  });

  it("confirm sends yes with the original plan id, once", async () => {
    const { ask, calls } = fakeAsk(pending, done("Created task “call the dentist”."));
    let t = await send(EMPTY_TALK, "add task call the dentist due friday", TODAY, ask, () => {});
    expect(t.pending).toEqual({ turnId: 2, planId: PLAN });

    // A second tap while the first is in flight is refused.
    let during: Talk | undefined;
    const first = decide(t, true, TODAY, ask, (s) => (during ??= s));
    await Promise.resolve();
    expect(during?.busy).toBe(true);
    expect(await decide(during!, true, TODAY, ask, () => {})).toBe(during);
    t = await first;

    expect(calls).toEqual([
      ["add task call the dentist due friday", TODAY, undefined],
      ["yes", TODAY, PLAN],
    ]);
    expect(t.pending).toBeUndefined();
    expect(t.turns.find((x) => x.id === 2)).toMatchObject({ outcome: "confirmed" });
    expect(t.turns.at(-1)).toMatchObject({ who: "jarvis", answer: { status: "executed" } });
    // Nothing pending any more: another confirm does nothing.
    expect(await decide(t, true, TODAY, ask, () => {})).toBe(t);
  });

  it("cancel sends no with the plan id", async () => {
    const { ask, calls } = fakeAsk(pending, { speech: "Okay, cancelled.", status: "cancelled", needs_confirmation: false });
    let t = await send(EMPTY_TALK, "add task x", TODAY, ask, () => {});
    t = await decide(t, false, TODAY, ask, () => {});
    expect(calls[1]).toEqual(["no", TODAY, PLAN]);
    expect(t.turns.find((x) => x.id === 2)).toMatchObject({ outcome: "cancelled" });
    expect(t.turns.at(-1)).toMatchObject({ answer: { speech: "Okay, cancelled." } });
  });

  it("a new request drops the unconfirmed change", async () => {
    const { ask } = fakeAsk(pending, done("2 open."));
    let t = await send(EMPTY_TALK, "add task x", TODAY, ask, () => {});
    t = await send(t, "list tasks", TODAY, ask, () => {});
    expect(t.pending).toBeUndefined();
    expect(t.turns.find((x) => x.id === 2)).toMatchObject({ outcome: "dropped" });
  });

  it("a failed confirm stays pending so it can be retried", async () => {
    const { ask } = fakeAsk(pending, new TypeError("Failed to fetch"));
    let t = await send(EMPTY_TALK, "add task x", TODAY, ask, () => {});
    t = await decide(t, true, TODAY, ask, () => {});
    expect(t.busy).toBe(false);
    expect(t.pending).toEqual({ turnId: 2, planId: PLAN });
    expect(t.turns.at(-1)).toMatchObject({ who: "error", text: "Can't reach Directory. Check the connection and try again." });
  });

  it("shows actionable errors", async () => {
    const { ask } = fakeAsk(new ApiError(401, "unauthorized"));
    const t = await send(EMPTY_TALK, "list tasks", TODAY, ask, () => {});
    expect(t.turns.at(-1)).toMatchObject({ who: "error", text: expect.stringContaining("Sign in again") });
    expect(askErrorMessage(new ApiError(502, "backend_unavailable"))).toContain("Nothing changed");
    expect(askErrorMessage(new ApiError(403, "origin_not_allowed"))).toContain("usual address");
    expect(askErrorMessage(new ApiError(413, "too_large"))).toContain("1,000");
  });
});
