import { useEffect, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import type { ModuleProps } from "../App";
import { Button, StatusChip, inputClass } from "../components/ui";
import { reportIfUnauthorized } from "../hooks/useSession";
import { api, ApiError } from "../lib/api";
import {
  LISTEN_MESSAGES,
  UNSUPPORTED_MESSAGES,
  canSpeak,
  listen,
  recognitionSupport,
  speak,
  stopSpeaking,
  type Listening,
  type SpeechWindow,
} from "../lib/speech";
import { ASK_LABEL, ASK_TONE, EMPTY_TALK, MAX_ASK, decide, send, type AskAnswer, type Talk, type Turn } from "../lib/talk";
import { localDate } from "../lib/tasks";

// Paper "Mobile · JARVIS · Voice" (docs/DESIGN_HANDOFF.md): tidal orb, quoted turns, Proposed card.

const EXAMPLES = ["What should I do today?", "I have 30 minutes", "Add task call the dentist due Friday", "What's on tomorrow?"];
const VOICE_KEY = "jarvis:voice-reply";

const win = () => window as unknown as SpeechWindow;
const lang = () => navigator.language || "en-US";

function readVoicePref(): boolean {
  try {
    return localStorage.getItem(VOICE_KEY) === "on";
  } catch {
    return false;
  }
}

function writeVoicePref(on: boolean) {
  try {
    localStorage.setItem(VOICE_KEY, on ? "on" : "off");
  } catch {
    /* private mode: the toggle still works for this visit */
  }
}

/** The 401 path also signs the app out (the auth screen takes over). */
const ask = async (text: string, today: string, confirm?: string): Promise<AskAnswer> => {
  try {
    return await api.jarvisAsk(text, today, confirm);
  } catch (e) {
    if (e instanceof ApiError && e.status === 401) setTimeout(() => reportIfUnauthorized(e), 1500);
    throw e;
  }
};

function Bubble({ turn, pending, busy, onDecide }: { turn: Turn; pending: boolean; busy: boolean; onDecide: (yes: boolean) => void }) {
  if (turn.who === "you") {
    // Paper "Mobile · JARVIS · Voice": what you said, set as a quote.
    return (
      <li className="flex justify-end">
        <p className="max-w-[85%] rounded-md rounded-br-sm bg-sunken px-4 py-2.5 text-[15px] whitespace-pre-wrap">“{turn.text}”</p>
      </li>
    );
  }
  if (turn.who === "error") {
    return (
      <li role="alert" className="max-w-[90%] rounded-md border border-bad/40 bg-panel px-4 py-3 text-sm text-bad">
        {turn.text}
      </li>
    );
  }
  const { answer, outcome } = turn;
  return (
    // A pending change is the Paper "Proposed" card; other answers are plain sheets.
    <li className={`max-w-[92%] border bg-panel ${pending ? "rounded-lg border-accent/40 px-5 py-4" : "rounded-md rounded-bl-sm border-line px-4 py-3"}`}>
      {pending && <p className="label mb-2">proposed</p>}
      <p className="text-[15px] leading-snug">{answer.speech}</p>
      <div className="mt-2 flex flex-wrap items-center gap-2">
        <StatusChip
          tone={outcome ? (outcome === "confirmed" ? "ok" : "idle") : ASK_TONE[answer.status]}
          label="jarvis"
          value={outcome ? { confirmed: "confirmed", cancelled: "cancelled", dropped: "not applied" }[outcome] : ASK_LABEL[answer.status]}
        />
      </div>
      {pending && (
        <div className="mt-4 flex justify-end gap-2">
          <Button className="min-h-11 min-w-24" disabled={busy} onClick={() => onDecide(false)}>
            cancel
          </Button>
          <Button variant="primary" className="min-h-11 min-w-28" disabled={busy} onClick={() => onDecide(true)}>
            confirm
          </Button>
        </div>
      )}
    </li>
  );
}

/** JARVIS conversation: talk or type, hear the answer, confirm changes. iPhone-first. */
export function Ask({ online }: ModuleProps) {
  const [talk, setTalk] = useState<Talk>(EMPTY_TALK);
  const talkRef = useRef(talk);
  talkRef.current = talk;
  const [draft, setDraft] = useState("");
  const [listening, setListening] = useState<Listening>();
  const [hint, setHint] = useState<string>();
  const [voice, setVoice] = useState(readVoicePref);
  const field = useRef<HTMLTextAreaElement>(null);
  const log = useRef<HTMLOListElement>(null);
  const support = useRef(recognitionSupport(win())).current;
  const speakable = useRef(canSpeak(win())).current;

  const update = (t: Talk) => {
    talkRef.current = t;
    setTalk(t);
  };

  useEffect(() => {
    log.current?.lastElementChild?.scrollIntoView({ block: "end", behavior: "smooth" });
  }, [talk.turns.length]);

  // Stop listening and speaking when leaving the screen.
  const listeningRef = useRef(listening);
  listeningRef.current = listening;
  useEffect(
    () => () => {
      listeningRef.current?.cancel();
      stopSpeaking(win());
    },
    [],
  );

  const reply = (t: Talk) => {
    const last = t.turns[t.turns.length - 1];
    if (voice && speakable && last?.who === "jarvis") speak(win(), last.answer.speech, lang());
  };

  const submit = async (e?: FormEvent) => {
    e?.preventDefault();
    if (!draft.trim() || talkRef.current.busy) return;
    listening?.stop();
    const text = draft;
    setDraft("");
    setHint(undefined);
    reply(await send(talkRef.current, text, localDate(), ask, update));
  };

  const onKey = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
      e.preventDefault();
      void submit();
    }
  };

  const onDecide = async (yes: boolean) => {
    stopSpeaking(win());
    reply(await decide(talkRef.current, yes, localDate(), ask, update));
  };

  const toggleMic = () => {
    if (listening) {
      listening.stop();
      return;
    }
    if (!support.ok) {
      // Fallback: open the keyboard, whose mic key uses the system's dictation.
      setHint(UNSUPPORTED_MESSAGES[support.reason]);
      field.current?.focus();
      return;
    }
    stopSpeaking(win());
    setHint(undefined);
    const before = draft.trim();
    const session = listen(support.ctor, {
      lang: lang(),
      onText: (heard) => setDraft(before ? `${before} ${heard}` : heard),
      onDone: (outcome) => {
        setListening(undefined);
        if (outcome.kind === "error") setHint(LISTEN_MESSAGES[outcome.error]);
        if (outcome.kind === "cancelled") setDraft(before);
        if (outcome.kind === "final") field.current?.focus();
      },
    });
    setListening(session);
  };

  const toggleVoice = () => {
    const on = !voice;
    setVoice(on);
    writeVoicePref(on);
    // iOS only speaks after a tap: a silent utterance now unlocks later replies.
    if (on) speak(win(), " ", lang(), 0);
    else stopSpeaking(win());
  };

  const disabled = !online || talk.busy;

  return (
    <div className="fixed inset-0 z-30 flex flex-col bg-canvas md:relative md:inset-auto md:z-auto md:mx-auto md:h-[calc(100dvh-9rem)] md:max-w-2xl md:overflow-hidden md:rounded-lg md:border md:border-line">
      <header className="pt-safe">
        <div className="flex items-center gap-1 px-3 py-2 md:px-5">
          <a href="#/home" aria-label="Close JARVIS" className="flex min-h-11 min-w-11 items-center justify-center text-lg text-muted md:hidden">
            ←
          </a>
          <h2 className="flex-1 text-[17px] font-medium tracking-wide">JARVIS</h2>
          {speakable && (
            <button
              type="button"
              aria-pressed={voice}
              onClick={toggleVoice}
              className={`min-h-11 rounded-full px-3 font-mono text-[11px] ${voice ? "text-accent" : "text-muted"}`}
            >
              voice reply · {voice ? "on" : "off"}
            </button>
          )}
          <a href="#/jarvis" className="flex min-h-11 items-center px-2 font-mono text-[11px] text-muted underline-offset-4 hover:underline">
            history
          </a>
        </div>
      </header>

      <ol ref={log} role="log" aria-label="Conversation" aria-live="polite" className="flex flex-1 flex-col gap-3 overflow-y-auto px-4 py-4 md:px-6">
        {talk.turns.length === 0 ? (
          <li className="my-auto flex flex-col items-center text-center">
            {/* Paper "Mobile · JARVIS · Voice": the tidal orb, haloed; it pulses while listening. */}
            <span aria-hidden className="grid size-52 place-items-center rounded-full bg-accent/5 md:size-44">
              <span className="grid size-40 place-items-center rounded-full bg-accent/10 md:size-36">
                <span
                  className={`bg-tidal grid size-32 place-items-center rounded-full shadow-[0_18px_40px_-16px_rgb(59_63_168/70%)] md:size-28 ${listening ? "animate-pulse motion-reduce:animate-none" : ""}`}
                >
                  <svg viewBox="0 0 24 24" className="size-10" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
                    <path d="M3 10v4M7 7v10M11 4v16M15 7v10M19 10v4" />
                  </svg>
                </span>
              </span>
            </span>
            <p className="label mt-4">{listening ? "listening" : "ready"}</p>
            <p className="mt-3 max-w-sm text-[15px] text-muted">Ask about your day, or tell JARVIS what to change. Changes wait for your confirm.</p>
            <ul aria-label="Examples" className="mt-4 flex flex-wrap justify-center gap-2">
              {EXAMPLES.map((ex) => (
                <li key={ex}>
                  <button
                    type="button"
                    onClick={() => {
                      setDraft(ex);
                      field.current?.focus();
                    }}
                    className="min-h-10 rounded-full border border-line bg-panel px-4 text-sm text-muted transition hover:border-accent hover:text-ink"
                  >
                    {ex}
                  </button>
                </li>
              ))}
            </ul>
          </li>
        ) : (
          talk.turns.map((t) => (
            <Bubble key={t.id} turn={t} pending={talk.pending?.turnId === t.id} busy={talk.busy} onDecide={(yes) => void onDecide(yes)} />
          ))
        )}
        {talk.busy && (
          <li aria-label="JARVIS is thinking" className="cursor font-mono text-sm text-muted">
            thinking
          </li>
        )}
      </ol>

      <form onSubmit={(e) => void submit(e)} className="pb-safe border-t border-line bg-panel/95 backdrop-blur">
        <div className="flex flex-col gap-2 px-3 pt-3 pb-3 md:px-5">
          {!online && <p className="text-sm text-warn">JARVIS needs a connection.</p>}
          {hint && (
            <p role="status" className="text-sm text-muted">
              {hint}
            </p>
          )}
          <div className="flex items-end gap-2">
            <textarea
              ref={field}
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={onKey}
              rows={2}
              maxLength={MAX_ASK}
              enterKeyHint="send"
              aria-label="Message to JARVIS"
              placeholder={listening ? "Listening…" : "Or type a command"}
              className={`${inputClass} min-h-12 flex-1 resize-none rounded-md border-transparent bg-sunken py-3 text-base leading-snug`}
            />
            <Button type="submit" variant="primary" className="min-h-12" disabled={disabled || !draft.trim()}>
              send
            </Button>
            <button
              type="button"
              onClick={toggleMic}
              disabled={!online}
              aria-pressed={!!listening}
              aria-label={listening ? "Stop listening" : support.ok ? "Speak to JARVIS" : "Dictate with the keyboard"}
              className={`bg-tidal grid size-14 shrink-0 place-items-center rounded-full shadow-[0_10px_24px_-10px_rgb(59_63_168/70%)] transition active:scale-95 disabled:opacity-50 ${
                listening ? "animate-pulse ring-4 ring-accent/30 motion-reduce:animate-none" : ""
              }`}
            >
              <svg aria-hidden viewBox="0 0 24 24" className="size-6" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
                {listening ? (
                  <rect x="7" y="7" width="10" height="10" rx="1.5" fill="currentColor" stroke="none" />
                ) : (
                  <>
                    <rect x="9" y="3" width="6" height="11" rx="3" />
                    <path d="M5 11a7 7 0 0 0 14 0M12 18v3" />
                  </>
                )}
              </svg>
            </button>
          </div>
          {listening ? (
            <button type="button" onClick={() => listening.cancel()} className="min-h-9 self-end font-mono text-[11px] text-muted underline underline-offset-4">
              discard
            </button>
          ) : (
            <p className="label text-right">{support.ok ? "your browser transcribes; Directory gets text only" : "mic · keyboard dictation"}</p>
          )}
        </div>
      </form>
    </div>
  );
}
