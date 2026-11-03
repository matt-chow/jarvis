import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api } from "../lib/api";
import { primeCrackle, startCrackle, stopCrackle } from "../lib/crackle";
import { localDb } from "../lib/db";
import { useDevice } from "../lib/device";
import type { FileEntry } from "../lib/files";
import { formatDuration, trackTitle } from "../lib/music";
import { Icon } from "./icons";

/** A track to play: a file in Files, or (`local`) a file picked on this device for this session only. */
export type Track = FileEntry & { local?: Blob };

export interface PlayerApi {
  queue: Track[];
  index: number;
  current?: Track;
  playing: boolean;
  /** Play `queue` starting at `index`. */
  play: (queue: Track[], index?: number) => void;
  toggle: () => void;
  next: () => void;
  prev: () => void;
  stop: () => void;
  seek: (seconds: number) => void;
}

export interface PlayerTime {
  at: number;
  /** NaN until the duration is known. */
  length: number;
}

const PlayerContext = createContext<PlayerApi | undefined>(undefined);
// Separate, so the many-times-a-second position only re-renders what shows it.
const TimeContext = createContext<PlayerTime>({ at: 0, length: Number.NaN });

export function usePlayer(): PlayerApi {
  const p = useContext(PlayerContext);
  if (!p) throw new Error("usePlayer outside PlayerProvider");
  return p;
}

export const usePlayerTime = () => useContext(TimeContext);

/** One app-wide audio element, so playback continues across modules. */
export function PlayerProvider({ children }: { children: ReactNode }) {
  const audio = useRef<HTMLAudioElement>(null);
  const [queue, setQueue] = useState<Track[]>([]);
  const [index, setIndex] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [time, setTime] = useState<PlayerTime>({ at: 0, length: Number.NaN });
  const [device] = useDevice();
  const objectUrl = useRef<string | undefined>(undefined);
  const crackleOn = useRef(device.crackle);
  crackleOn.current = device.crackle;
  const current = queue[index];

  // A file picked this session plays from memory; otherwise prefer a copy
  // saved on this device; otherwise stream (Range requests let it seek).
  useEffect(() => {
    const el = audio.current;
    if (!el || !current) return;
    let cancelled = false;
    void (async () => {
      const blob =
        current.local ??
        (await localDb()
          .then((db) => db.getMedia(current.id))
          .then((m) => m?.blob)
          .catch(() => undefined));
      if (cancelled) return;
      if (objectUrl.current) URL.revokeObjectURL(objectUrl.current);
      objectUrl.current = blob ? URL.createObjectURL(blob) : undefined;
      el.src = objectUrl.current ?? api.fileContentUrl(current.id);
      el.play().catch(() => setPlaying(false));
    })();
    return () => {
      cancelled = true;
    };
  }, [current]);

  // Volume is per device. (iPhone Safari ignores it; the hardware buttons rule there.)
  useEffect(() => {
    if (audio.current) audio.current.volume = device.volume;
  }, [device.volume]);

  // Surface noise follows real playback: only while audio plays, never before the owner pressed play.
  useEffect(() => {
    if (playing && device.crackle) startCrackle(device.crackleLevel);
    else stopCrackle();
  }, [playing, device.crackle, device.crackleLevel]);
  useEffect(() => stopCrackle, []);

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
        if (crackleOn.current) primeCrackle();
        setQueue(q);
        setIndex(i);
      },
      toggle: () => {
        const el = audio.current;
        if (!el) return;
        if (el.paused) {
          if (crackleOn.current) primeCrackle();
          void el.play().catch(() => {});
        } else el.pause();
      },
      next,
      prev,
      stop: () => {
        audio.current?.pause();
        setQueue([]);
        setIndex(0);
        setTime({ at: 0, length: Number.NaN });
      },
      seek: (s) => {
        const el = audio.current;
        if (el && Number.isFinite(s)) el.currentTime = Math.max(0, Math.min(s, Number.isFinite(el.duration) ? el.duration : s));
      },
    }),
    [queue, index, current, playing, next, prev],
  );

  // Lock-screen and hardware media keys, where supported.
  useEffect(() => {
    if (!("mediaSession" in navigator) || !current) return;
    navigator.mediaSession.metadata = new MediaMetadata({
      title: trackTitle(current.name),
      album: current.folder || "Directory",
    });
    navigator.mediaSession.setActionHandler("nexttrack", next);
    navigator.mediaSession.setActionHandler("previoustrack", prev);
  }, [current, next, prev]);

  const tick = () => {
    const el = audio.current;
    if (el) setTime({ at: el.currentTime, length: el.duration });
  };

  return (
    <PlayerContext.Provider value={api_}>
      <TimeContext.Provider value={time}>
        {children}
        <audio ref={audio} onPlay={() => setPlaying(true)} onPause={() => setPlaying(false)} onEnded={next} onTimeUpdate={tick} onDurationChange={tick} hidden />
        {current && <PlayerBar player={api_} />}
      </TimeContext.Provider>
    </PlayerContext.Provider>
  );
}

/** Play/pause, previous, next; shared by the bar, the turntable screen, and the Home widget. */
export function Transport({ player, size = "sm" }: { player: PlayerApi; size?: "sm" | "lg" }) {
  const btn = size === "lg" ? "size-11" : "size-9";
  const base = `press grid ${btn} place-items-center rounded-full border border-line text-ink hover:bg-sunken disabled:opacity-40`;
  return (
    <div className="flex items-center gap-1.5">
      <button type="button" className={base} aria-label="Previous track" onClick={player.prev}>
        <Icon name="prev" size={size === "lg" ? 20 : 16} />
      </button>
      <button
        type="button"
        className={`press grid ${size === "lg" ? "size-14" : "size-10"} place-items-center rounded-full bg-accent text-accent-ink hover:brightness-105`}
        aria-label={player.playing ? "Pause" : "Play"}
        onClick={player.toggle}
      >
        <Icon name={player.playing ? "pause" : "play"} size={size === "lg" ? 24 : 18} />
      </button>
      <button type="button" className={base} aria-label="Next track" disabled={player.index + 1 >= player.queue.length} onClick={player.next}>
        <Icon name="next" size={size === "lg" ? 20 : 16} />
      </button>
    </div>
  );
}

/** The position slider with elapsed and total time. */
export function SeekBar({ player, compact = false }: { player: PlayerApi; compact?: boolean }) {
  const time = usePlayerTime();
  const length = Number.isFinite(time.length) ? time.length : 0;
  return (
    <div className={`flex min-w-0 items-center gap-2 ${compact ? "" : "w-full"}`}>
      <span className="w-10 shrink-0 text-right font-mono text-[11px] text-muted">{formatDuration(time.at)}</span>
      <input
        type="range"
        min={0}
        max={length}
        step={1}
        value={Math.min(time.at, length)}
        disabled={!length}
        aria-label="Seek"
        aria-valuetext={`${formatDuration(time.at)} of ${formatDuration(time.length)}`}
        onChange={(e) => player.seek(Number(e.target.value))}
        className="min-w-0 flex-1 accent-accent"
      />
      <span className="w-10 shrink-0 font-mono text-[11px] text-muted">{formatDuration(time.length)}</span>
    </div>
  );
}

function PlayerBar({ player }: { player: PlayerApi }) {
  const title = player.current ? trackTitle(player.current.name) : "";
  return (
    <div role="region" aria-label="Player" className="fixed inset-x-0 bottom-[3.9rem] z-20 border-t border-line bg-panel/95 px-4 py-2 backdrop-blur md:bottom-0">
      <div className="mx-auto flex max-w-5xl items-center gap-3">
        <Transport player={player} />
        <div className="flex min-w-0 flex-1 flex-col">
          <a href="#/music" className="truncate text-sm hover:underline" aria-live="polite">
            {title}
            {player.current?.local && <span className="text-muted"> · this session only</span>}
          </a>
          <SeekBar player={player} compact />
        </div>
        <span className="hidden font-mono text-[11px] text-muted sm:inline">
          {player.index + 1}/{player.queue.length}
        </span>
        <button type="button" className="press grid size-9 place-items-center rounded-full text-muted hover:bg-sunken hover:text-ink" aria-label="Close player" onClick={player.stop}>
          <Icon name="close" size={16} />
        </button>
      </div>
    </div>
  );
}
