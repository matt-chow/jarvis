import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api } from "../lib/api";
import { localDb } from "../lib/db";
import type { FileEntry } from "../lib/files";
import { formatDuration, trackTitle } from "../lib/music";
import { Button } from "./ui";

// Provisional component: awaiting design integration (see docs/MODULES.md › Design status).

export interface PlayerApi {
  queue: FileEntry[];
  index: number;
  current?: FileEntry;
  playing: boolean;
  /** Play `queue` starting at `index`. */
  play: (queue: FileEntry[], index?: number) => void;
  toggle: () => void;
  next: () => void;
  prev: () => void;
  stop: () => void;
}

const PlayerContext = createContext<PlayerApi | undefined>(undefined);

export function usePlayer(): PlayerApi {
  const p = useContext(PlayerContext);
  if (!p) throw new Error("usePlayer outside PlayerProvider");
  return p;
}

/** One app-wide audio element, so playback continues across modules. */
export function PlayerProvider({ children }: { children: ReactNode }) {
  const audio = useRef<HTMLAudioElement>(null);
  const [queue, setQueue] = useState<FileEntry[]>([]);
  const [index, setIndex] = useState(0);
  const [playing, setPlaying] = useState(false);
  const objectUrl = useRef<string | undefined>(undefined);
  const current = queue[index];

  // Prefer a copy saved on this device; otherwise stream (Range requests let it seek).
  useEffect(() => {
    const el = audio.current;
    if (!el || !current) return;
    let cancelled = false;
    void (async () => {
      const saved = await localDb()
        .then((db) => db.getMedia(current.id))
        .catch(() => undefined);
      if (cancelled) return;
      if (objectUrl.current) URL.revokeObjectURL(objectUrl.current);
      objectUrl.current = saved ? URL.createObjectURL(saved.blob) : undefined;
      el.src = objectUrl.current ?? api.fileContentUrl(current.id);
      el.play().catch(() => setPlaying(false));
    })();
    return () => {
      cancelled = true;
    };
  }, [current]);

  const next = useCallback(() => {
    setIndex((i) => {
      if (i + 1 < queue.length) return i + 1;
      setPlaying(false);
      return i;
    });
  }, [queue.length]);
  const prev = useCallback(() => {
    const el = audio.current;
    // Like most players: restart the track unless it just began.
    if (el && el.currentTime > 3) el.currentTime = 0;
    else setIndex((i) => Math.max(0, i - 1));
  }, []);

  const api_: PlayerApi = useMemo(
    () => ({
      queue,
      index,
      current,
      playing,
      play: (q, i = 0) => {
        setQueue(q);
        setIndex(i);
      },
      toggle: () => {
        const el = audio.current;
        if (!el) return;
        if (el.paused) void el.play().catch(() => {});
        else el.pause();
      },
      next,
      prev,
      stop: () => {
        audio.current?.pause();
        setQueue([]);
        setIndex(0);
      },
    }),
    [queue, index, current, playing, next, prev],
  );

  // Lock-screen and hardware media keys, where supported.
  useEffect(() => {
    if (!("mediaSession" in navigator) || !current) return;
    navigator.mediaSession.metadata = new MediaMetadata({ title: trackTitle(current.name), album: current.folder || "Directory" });
    navigator.mediaSession.setActionHandler("nexttrack", next);
    navigator.mediaSession.setActionHandler("previoustrack", prev);
  }, [current, next, prev]);

  return (
    <PlayerContext.Provider value={api_}>
      {children}
      <audio ref={audio} onPlay={() => setPlaying(true)} onPause={() => setPlaying(false)} onEnded={next} hidden />
      {current && <PlayerBar player={api_} audio={audio} />}
    </PlayerContext.Provider>
  );
}

function PlayerBar({ player, audio }: { player: PlayerApi; audio: React.RefObject<HTMLAudioElement | null> }) {
  const [time, setTime] = useState({ at: 0, length: Number.NaN });
  useEffect(() => {
    const el = audio.current;
    if (!el) return;
    const tick = () => setTime({ at: el.currentTime, length: el.duration });
    el.addEventListener("timeupdate", tick);
    el.addEventListener("durationchange", tick);
    return () => {
      el.removeEventListener("timeupdate", tick);
      el.removeEventListener("durationchange", tick);
    };
  }, [audio]);
  const small = "min-h-8 px-2 text-xs";
  const title = player.current ? trackTitle(player.current.name) : "";

  return (
    <div role="region" aria-label="Player" className="fixed inset-x-0 bottom-[3.9rem] z-20 border-t md:bottom-0 border-line bg-panel/95 px-4 py-2 backdrop-blur">
      <div className="mx-auto flex max-w-5xl flex-wrap items-center gap-3">
        <div className="flex gap-1">
          <Button className={small} aria-label="Previous track" onClick={player.prev}>
            ⏮
          </Button>
          <Button className={small} variant="primary" aria-label={player.playing ? "Pause" : "Play"} onClick={player.toggle}>
            {player.playing ? "⏸" : "▶"}
          </Button>
          <Button className={small} aria-label="Next track" disabled={player.index + 1 >= player.queue.length} onClick={player.next}>
            ⏭
          </Button>
        </div>
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm" aria-live="polite">
            {title}
          </p>
          <input
            type="range"
            min={0}
            max={Number.isFinite(time.length) ? time.length : 0}
            step={1}
            value={time.at}
            aria-label="Seek"
            onChange={(e) => {
              if (audio.current) audio.current.currentTime = Number(e.target.value);
            }}
            className="w-full accent-accent"
          />
        </div>
        <span className="font-mono text-[11px] text-muted">
          {formatDuration(time.at)} / {formatDuration(time.length)} · {player.index + 1}/{player.queue.length}
        </span>
        <Button className={small} aria-label="Close player" onClick={player.stop}>
          ✕
        </Button>
      </div>
    </div>
  );
}
