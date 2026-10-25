/**
 * Browser speech for JARVIS › Ask (D-044). Everything is feature-detected:
 * recognition may be missing (Firefox, iOS Home Screen apps), blocked by
 * the site's Permissions-Policy, or refused by the user. Directory never
 * records or uploads audio; the browser's own speech service turns it into
 * text, and only that text is sent to `/api/v1/jarvis/ask`.
 */

/** The subset of `SpeechRecognition` used here (Chrome, Edge, Safari: prefixed). */
export interface RecognitionLike {
  lang: string;
  continuous: boolean;
  interimResults: boolean;
  maxAlternatives: number;
  onresult: ((e: RecognitionResultEvent) => void) | null;
  onerror: ((e: { error: string }) => void) | null;
  onend: (() => void) | null;
  start(): void;
  stop(): void;
  abort(): void;
}

export interface RecognitionResultEvent {
  results: ArrayLike<{ isFinal: boolean; 0: { transcript: string } }>;
}

export type RecognitionCtor = new () => RecognitionLike;

interface PolicyLike {
  allowsFeature(feature: string): boolean;
}

/** The parts of `window` this module reads (a fake in tests). */
export interface SpeechWindow {
  isSecureContext?: boolean;
  SpeechRecognition?: RecognitionCtor;
  webkitSpeechRecognition?: RecognitionCtor;
  document?: { featurePolicy?: PolicyLike; permissionsPolicy?: PolicyLike };
  speechSynthesis?: { speak(u: unknown): void; cancel(): void };
  SpeechSynthesisUtterance?: new (text: string) => { lang: string; volume: number };
}

export type Unsupported = "insecure" | "no_api" | "blocked";

export type Support = { ok: true; ctor: RecognitionCtor } | { ok: false; reason: Unsupported };

/** Can this page start speech recognition? Never assumes it can. */
export function recognitionSupport(w: SpeechWindow): Support {
  if (w.isSecureContext === false) return { ok: false, reason: "insecure" };
  const ctor = w.SpeechRecognition ?? w.webkitSpeechRecognition;
  if (typeof ctor !== "function") return { ok: false, reason: "no_api" };
  // Chrome and Edge expose the policy; Safari doesn't (a block then shows up
  // as a `not-allowed` error when listening starts).
  const policy = w.document?.permissionsPolicy ?? w.document?.featurePolicy;
  try {
    if (policy && !policy.allowsFeature("microphone")) return { ok: false, reason: "blocked" };
  } catch {
    /* unknown feature name: let start() decide */
  }
  return { ok: true, ctor };
}

const TYPE_HINT = "Type instead. On iPhone, the keyboard's mic key dictates into the box.";

export const UNSUPPORTED_MESSAGES: Record<Unsupported, string> = {
  insecure: `Voice input needs HTTPS. ${TYPE_HINT}`,
  no_api: `This browser has no speech recognition. ${TYPE_HINT}`,
  blocked: `Voice input is turned off for this site. ${TYPE_HINT}`,
};

export type ListenError = "permission" | "silence" | "no_microphone" | "network" | "language" | "failed";

export const LISTEN_MESSAGES: Record<ListenError, string> = {
  permission: `The microphone or speech recognition isn't allowed. Check the browser's microphone setting (on iPhone, Siri & Dictation must be on). ${TYPE_HINT}`,
  silence: "I didn't hear anything. Tap the mic and try again.",
  no_microphone: `No microphone was found. ${TYPE_HINT}`,
  network: "The browser's speech service couldn't be reached. Check the connection, or type.",
  language: `Speech recognition doesn't support this language here. ${TYPE_HINT}`,
  failed: "Listening stopped unexpectedly. Try again, or type.",
};

/** `SpeechRecognitionErrorEvent.error` → what to tell the user. */
export function listenErrorKind(code: string): ListenError | "cancelled" {
  switch (code) {
    case "aborted":
      return "cancelled";
    case "not-allowed":
    case "service-not-allowed":
      return "permission";
    case "no-speech":
      return "silence";
    case "audio-capture":
      return "no_microphone";
    case "network":
      return "network";
    case "language-not-supported":
      return "language";
    default:
      return "failed";
  }
}

export type ListenOutcome = { kind: "final"; text: string } | { kind: "cancelled" } | { kind: "error"; error: ListenError };

export interface Listening {
  /** Stop and keep what was heard. */
  stop(): void;
  /** Stop and discard. */
  cancel(): void;
}

export interface ListenOptions {
  lang: string;
  /** Live transcript while speaking. */
  onText(text: string): void;
  /** Called exactly once. */
  onDone(outcome: ListenOutcome): void;
  /** Give up when nothing is heard for this long (some engines never fire `no-speech`). */
  silenceMs?: number;
  /** Hard stop for one utterance. */
  maxMs?: number;
}

/** One utterance: start, stream the transcript, finish once. */
export function listen(ctor: RecognitionCtor, opts: ListenOptions): Listening {
  const rec = new ctor();
  rec.lang = opts.lang;
  rec.continuous = false;
  rec.interimResults = true;
  rec.maxAlternatives = 1;

  let text = "";
  let error: ListenError | undefined;
  let finished = false;
  const finish = (outcome: ListenOutcome) => {
    if (finished) return;
    finished = true;
    clearTimeout(silence);
    clearTimeout(cap);
    rec.onresult = rec.onerror = rec.onend = null;
    opts.onDone(outcome);
  };
  const silence = setTimeout(() => {
    if (!text) {
      error = "silence";
      rec.abort();
    }
  }, opts.silenceMs ?? 8000);
  const cap = setTimeout(() => rec.stop(), opts.maxMs ?? 30_000);

  rec.onresult = (e) => {
    let heard = "";
    for (let i = 0; i < e.results.length; i++) heard += e.results[i][0].transcript;
    text = heard.trim();
    opts.onText(text);
  };
  rec.onerror = (e) => {
    const kind = listenErrorKind(e.error);
    if (kind !== "cancelled") error ??= kind;
  };
  rec.onend = () => {
    if (text && (!error || error === "silence")) finish({ kind: "final", text });
    else finish({ kind: "error", error: error ?? "silence" });
  };

  try {
    rec.start();
  } catch {
    finish({ kind: "error", error: "failed" });
  }
  return {
    stop: () => rec.stop(),
    cancel: () => {
      // Finish now: some engines never fire `end` after `abort()`.
      finish({ kind: "cancelled" });
      try {
        rec.abort();
      } catch {
        /* already stopped */
      }
    },
  };
}

/** Spoken replies are available (Speech Synthesis; on-device voices). */
export function canSpeak(w: SpeechWindow): boolean {
  return typeof w.speechSynthesis?.speak === "function" && typeof w.SpeechSynthesisUtterance === "function";
}

/** Read `text` aloud, replacing anything still being spoken. */
export function speak(w: SpeechWindow, text: string, lang: string, volume = 1): void {
  if (!canSpeak(w)) return;
  w.speechSynthesis!.cancel();
  const u = new w.SpeechSynthesisUtterance!(text);
  u.lang = lang;
  u.volume = volume;
  w.speechSynthesis!.speak(u);
}

export function stopSpeaking(w: SpeechWindow): void {
  w.speechSynthesis?.cancel();
}
