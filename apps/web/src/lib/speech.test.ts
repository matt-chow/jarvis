import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  canSpeak,
  listen,
  listenErrorKind,
  recognitionSupport,
  speak,
  type ListenOutcome,
  type RecognitionLike,
  type SpeechWindow,
} from "./speech";

/** A scriptable SpeechRecognition. */
class FakeRecognition implements RecognitionLike {
  static last: FakeRecognition;
  lang = "";
  continuous = true;
  interimResults = false;
  maxAlternatives = 5;
  onresult: RecognitionLike["onresult"] = null;
  onerror: RecognitionLike["onerror"] = null;
  onend: RecognitionLike["onend"] = null;
  started = false;
  aborted = false;
  constructor() {
    FakeRecognition.last = this;
  }
  start() {
    this.started = true;
  }
  stop() {
    this.onend?.();
  }
  abort() {
    this.aborted = true;
    this.onerror?.({ error: "aborted" });
    this.onend?.();
  }
  hear(...parts: [string, boolean][]) {
    this.onresult?.({ results: parts.map(([transcript, isFinal]) => ({ isFinal, 0: { transcript } })) });
  }
  fail(error: string) {
    this.onerror?.({ error });
    this.onend?.();
  }
}

const allow = { allowsFeature: () => true };

describe("recognition support", () => {
  it("is unsupported without the API (Firefox, iOS Home Screen apps)", () => {
    expect(recognitionSupport({ isSecureContext: true })).toEqual({ ok: false, reason: "no_api" });
  });

  it("needs a secure context", () => {
    expect(recognitionSupport({ isSecureContext: false, webkitSpeechRecognition: FakeRecognition })).toEqual({
      ok: false,
      reason: "insecure",
    });
  });

  it("uses the prefixed constructor (Safari, Chrome)", () => {
    const s = recognitionSupport({ isSecureContext: true, webkitSpeechRecognition: FakeRecognition, document: {} });
    expect(s).toEqual({ ok: true, ctor: FakeRecognition });
  });

  it("is blocked when the Permissions-Policy denies the microphone", () => {
    const deny = { allowsFeature: (f: string) => f !== "microphone" };
    expect(recognitionSupport({ SpeechRecognition: FakeRecognition, document: { featurePolicy: deny } })).toEqual({
      ok: false,
      reason: "blocked",
    });
    expect(recognitionSupport({ SpeechRecognition: FakeRecognition, document: { permissionsPolicy: deny, featurePolicy: allow } })).toMatchObject({
      ok: false,
    });
    expect(recognitionSupport({ SpeechRecognition: FakeRecognition, document: { featurePolicy: allow } })).toMatchObject({ ok: true });
  });

  it("maps recognition errors", () => {
    expect(listenErrorKind("not-allowed")).toBe("permission");
    expect(listenErrorKind("service-not-allowed")).toBe("permission");
    expect(listenErrorKind("no-speech")).toBe("silence");
    expect(listenErrorKind("audio-capture")).toBe("no_microphone");
    expect(listenErrorKind("network")).toBe("network");
    expect(listenErrorKind("aborted")).toBe("cancelled");
    expect(listenErrorKind("something-new")).toBe("failed");
  });
});

describe("listen", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  const run = (opts: { silenceMs?: number } = {}) => {
    const texts: string[] = [];
    const outcomes: ListenOutcome[] = [];
    const session = listen(FakeRecognition, { lang: "en-US", onText: (t) => texts.push(t), onDone: (o) => outcomes.push(o), ...opts });
    return { session, texts, outcomes, rec: FakeRecognition.last };
  };

  it("configures one utterance with live results", () => {
    const { rec } = run();
    expect(rec.started).toBe(true);
    expect(rec).toMatchObject({ lang: "en-US", continuous: false, interimResults: true, maxAlternatives: 1 });
  });

  it("streams the transcript and finishes with the final text", () => {
    const { rec, texts, outcomes } = run();
    rec.hear(["add task", false]);
    rec.hear(["add task buy milk", true]);
    rec.stop();
    expect(texts).toEqual(["add task", "add task buy milk"]);
    expect(outcomes).toEqual([{ kind: "final", text: "add task buy milk" }]);
  });

  it("reports silence, from the engine or by timeout", () => {
    const a = run();
    a.rec.fail("no-speech");
    expect(a.outcomes).toEqual([{ kind: "error", error: "silence" }]);

    const b = run({ silenceMs: 5000 });
    vi.advanceTimersByTime(5000);
    expect(b.rec.aborted).toBe(true);
    expect(b.outcomes).toEqual([{ kind: "error", error: "silence" }]);
  });

  it("reports a denied permission", () => {
    const { rec, outcomes } = run();
    rec.fail("not-allowed");
    expect(outcomes).toEqual([{ kind: "error", error: "permission" }]);
  });

  it("cancel discards, finishes once, even if the engine never ends", () => {
    const { rec, session, outcomes } = run();
    rec.hear(["half a sen", false]);
    rec.abort = () => {}; // engine that never fires `end`
    session.cancel();
    session.cancel();
    expect(outcomes).toEqual([{ kind: "cancelled" }]);
  });

  it("a start() that throws is an error, not a crash", () => {
    class Broken extends FakeRecognition {
      start() {
        throw new Error("InvalidStateError");
      }
    }
    const outcomes: ListenOutcome[] = [];
    listen(Broken, { lang: "en", onText: () => {}, onDone: (o) => outcomes.push(o) });
    expect(outcomes).toEqual([{ kind: "error", error: "failed" }]);
  });
});

describe("speech output", () => {
  it("is optional and feature-detected", () => {
    expect(canSpeak({})).toBe(false);
    const spoken: { text: string; lang: string; volume: number }[] = [];
    const w: SpeechWindow = {
      speechSynthesis: { speak: (u) => spoken.push(u as (typeof spoken)[number]), cancel: vi.fn() },
      SpeechSynthesisUtterance: class {
        lang = "";
        volume = 1;
        constructor(readonly text: string) {}
      },
    };
    expect(canSpeak(w)).toBe(true);
    speak(w, "Nothing open. Nice.", "en-US");
    expect(w.speechSynthesis!.cancel).toHaveBeenCalled();
    expect(spoken[0]).toMatchObject({ text: "Nothing open. Nice.", lang: "en-US", volume: 1 });
    speak({}, "ignored", "en"); // no API: no throw
  });
});
