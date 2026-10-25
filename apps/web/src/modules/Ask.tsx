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

// Provisional screen: awaiting design integration (see docs/MODULES.md › Design status).

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
    return (
      <li className="flex justify-end">
        <p className="max-w-[85%] rounded-md rounded-br-sm bg-sunken px-3 py-2 text-[15px] whitespace-pre-wrap">{turn.text}</p>
      </li>
    );
  }
  if (turn.who === "error") {
    return (
      <li role="alert" className="max-w-[90%] rounded-md border border-bad/40 bg-panel px-3 py-2 text-sm text-bad">
        {turn.text}
      </li>
    );
  }
  const { answer, outcome } = turn;
  return (
    <li className="max-w-[90%] rounded-md rounded-bl-sm border border-line bg-panel px-3 py-2.5">
      <p className="text-[15px] leading-snug">{answer.speech}</p>
      <div className="mt-2 flex flex-wrap items-center gap-2">
        <StatusChip
          tone={outcome ? (outcome === "confirmed" ? "ok" : "idle") : ASK_TONE[answer.status]}
          label="jarvis"
          value={outcome ? { confirmed: "confirmed", cancelled: "cancelled", dropped: "not applied" }[outcome] : ASK_LABEL[answer.status]}
        />
      </div>
      {pending && (
        <div className="mt-3 grid grid-cols-2 gap-2">
          <Button variant="primary" className="min-h-12 text-sm" disabled={busy} onClick={() => onDecide(true)}>
            confirm
          </Button>
          <Button className="min-h-12 text-sm" disabled={busy} onClick={() => onDecide(false)}>
            cancel
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
    <div className="fixed inset-0 z-30 flex flex-col bg-canvas md:relative md:inset-auto md:z-auto md:mx-auto md:h-[calc(100dvh-10rem)] md:max-w-2xl md:overflow-hidden md:rounded-md md:border md:border-line md:bg-panel">
      <header className="pt-safe border-b border-line bg-panel/95 backdrop-blur">
        <div className="flex items-center gap-2 px-3 py-2">
          <a href="#/home" aria-label="Close JARVIS" className="flex min-h-11 min-w-11 items-center justify-center font-mono text-lg text-muted md:hidden">
            ←
          </a>
          <h2 className="flex-1 font-mono text-sm font-semibold tracking-[0.2em]">
            <span className="text-accent">◎</span> JARVIS
          </h2>
          {speakable && (
            <button
              type="button"
              aria-pressed={voice}
              onClick={toggleVoice}
              className={`min-h-11 rounded-sm border px-3 font-mono text-[11px] uppercase ${voice ? "border-accent text-accent" : "border-line text-muted"}`}
            >
              voice reply {voice ? "on" : "off"}
            </button>
          )}
          <a href="#/jarvis" className="flex min-h-11 items-center px-2 font-mono text-[11px] text-muted underline-offset-4 hover:underline">
            history
          </a>
        </div>
      </header>

      <ol ref={log} role="log" aria-label="Conversation" aria-live="polite" className="flex flex-1 flex-col gap-3 overflow-y-auto px-3 py-4">
        {talk.turns.length === 0 ? (
          <li className="my-auto text-center">
            <p className="text-sm text-muted">Ask about your day, or tell JARVIS what to change. Changes wait for your confirm.</p>
            <ul aria-label="Examples" className="mt-4 flex flex-wrap justify-center gap-2">
              {EXAMPLES.map((ex) => (
                <li key={ex}>
                  <button
                    type="button"
                    onClick={() => {
                      setDraft(ex);
                      field.current?.focus();
                    }}
                    className="min-h-10 rounded-sm border border-line bg-panel px-3 text-sm text-muted hover:border-accent hover:text-ink"
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
        <div className="flex flex-col gap-2 px-3 pt-3 pb-3">
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
              placeholder={listening ? "Listening…" : "Type, or tap the mic"}
              className={`${inputClass} min-h-12 flex-1 resize-none py-2 text-base`}
            />
            <Button type="submit" variant="primary" className="min-h-12" disabled={disabled || !draft.trim()}>
              send
            </Button>
          </div>
          <div className="flex flex-col items-center gap-1 pt-1">
            <button
              type="button"
              onClick={toggleMic}
              disabled={!online}
              aria-pressed={!!listening}
              aria-label={listening ? "Stop listening" : support.ok ? "Speak to JARVIS" : "Dictate with the keyboard"}
              className={`flex size-20 items-center justify-center rounded-full border-2 text-3xl transition disabled:opacity-50 ${
                listening ? "animate-pulse border-accent motion-reduce:animate-none bg-accent text-accent-ink" : "border-accent bg-panel text-accent hover:bg-sunken"
              }`}
            >
              <svg aria-hidden viewBox="0 0 24 24" className="size-8" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
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
            {listening ? (
              <button type="button" onClick={() => listening.cancel()} className="min-h-9 font-mono text-[11px] text-muted underline underline-offset-4">
                discard
              </button>
            ) : (
              <p className="label text-center">
                {support.ok ? "your browser transcribes; Directory gets text only" : "keyboard dictation"}
              </p>
            )}
          </div>
        </div>
      </form>
    </div>
  );
}
