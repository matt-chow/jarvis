import { useMemo, useRef, useState, type FormEvent } from "react";
import type { ModuleProps } from "../App";
import { Icon } from "../components/icons";
import { SeekBar, Transport, usePlayer, usePlayerTime, type Track } from "../components/Player";
import { Turntable } from "../components/Turntable";
import { Button, Panel, inputClass } from "../components/ui";
import { useMusic, type MusicState } from "../hooks/useMusic";
import { reportIfUnauthorized } from "../hooks/useSession";
import { api } from "../lib/api";
import type { FileEntry } from "../lib/files";
import { useDevice } from "../lib/device";
import { formatBytes } from "../lib/format";
import { moveTrack, playlistTracks, trackTitle, type Playlist } from "../lib/music";

// The turntable screen (D-050): the record is tied to real playback; the
// library and playlists sit below it.

const small = "min-h-8 px-3 text-xs";
/** Where uploads from this screen go (an ordinary Files folder). */
const MUSIC_FOLDER = "Music";

export function Music({ online }: ModuleProps) {
  const music = useMusic(online);
  const player = usePlayer();
  const [selected, setSelected] = useState<string>();
  const offline = music.stale || !online;
  const lib = music.library;
  // Offline, only tracks saved on this device can play.
  const playable = (lib?.tracks ?? []).filter((t) => !offline || music.saved.has(t.id));
  const playlist = lib?.playlists.find((p) => p.id === selected);

  return (
    <div className="flex flex-col gap-5">
    <NowPlaying />
    <div className="grid items-start gap-5 lg:grid-cols-[minmax(0,1fr)_20rem]">
      <Panel
        title={`library${lib ? ` · ${lib.tracks.length}` : ""}`}
        actions={
          <Button className={small} variant="primary" disabled={playable.length === 0} onClick={() => player.play(playable)}>
            Play all
          </Button>
        }
      >
        {offline && lib && (
          <p className="mb-3 rounded-sm border border-line bg-sunken/60 px-3 py-2 text-sm text-muted">
            Offline: tracks saved on this device can play. Playlists and uploads need a connection.
          </p>
        )}
        {music.error && (
          <p role="alert" className="mb-3 text-sm text-bad">
            {music.error}
          </p>
        )}
        <Upload disabled={offline} onUploaded={music.reload} />
        {music.loading && !lib ? (
          <p className="mt-4 text-sm text-muted">Loading music…</p>
        ) : !lib ? (
          <div className="mt-4 text-sm">
            <p className="text-muted">The music library couldn't be loaded.</p>
            <Button className={`${small} mt-2`} onClick={music.reload}>
              Try again
            </Button>
          </div>
        ) : lib.tracks.length === 0 ? (
          <p className="mt-4 text-sm text-muted">No music yet. Upload audio files here or in Files (any folder).</p>
        ) : (
          <ul className="mt-4 divide-y divide-line">
            {lib.tracks.map((t) => (
              <TrackRow
                key={t.id}
                track={t}
                music={music}
                offline={offline}
                playing={player.current?.id === t.id}
                playlists={lib.playlists}
                onPlay={() => player.play(playable, Math.max(0, playable.findIndex((x) => x.id === t.id)))}
              />
            ))}
          </ul>
        )}
      </Panel>

      <div className="flex flex-col gap-5">
        <Playlists music={music} offline={offline} selected={selected} onSelect={setSelected} />
        {playlist && lib && <PlaylistDetail key={playlist.id} playlist={playlist} tracks={lib.tracks} music={music} offline={offline} />}
        <LocalFiles />
      </div>
    </div>
    </div>
  );
}

function NowPlaying() {
  const player = usePlayer();
  const time = usePlayerTime();
  const [device, setDevice] = useDevice();
  const current = player.current;
  const title = current ? trackTitle(current.name) : "";
  const progress = Number.isFinite(time.length) && time.length > 0 ? time.at / time.length : 0;
  const upNext = player.queue.slice(player.index + 1, player.index + 6);
  const field = "flex flex-col gap-1.5 text-xs text-muted";

  return (
    <Panel title="now playing">
      <div className="grid items-center gap-6 md:grid-cols-[auto_minmax(0,1fr)]">
        <div className="mx-auto">
          <Turntable playing={player.playing} progress={progress} label={title || "idle"} size={260} />
        </div>
        <div className="flex min-w-0 flex-col gap-4">
          {current ? (
            <div className="min-w-0">
              <p className="truncate text-2xl">{title}</p>
              <p className="truncate text-sm text-muted">
                {current.local ? "From this device · this session only" : current.folder || "Files"} · track {player.index + 1} of {player.queue.length}
              </p>
            </div>
          ) : (
            <p className="text-sm text-muted">Nothing on the platter. Choose a track below, or play a file from this device.</p>
          )}
          {current && (
            <>
              <Transport player={player} size="lg" />
              <SeekBar player={player} />
            </>
          )}
          <div className="grid gap-4 sm:grid-cols-2">
            <label className={field}>
              <span className="flex items-center gap-1.5">
                <Icon name={device.volume === 0 ? "mute" : "volume"} size={14} /> Volume · {Math.round(device.volume * 100)}%
              </span>
              <input type="range" min={0} max={1} step={0.05} value={device.volume} onChange={(e) => setDevice({ volume: Number(e.target.value) })} className="accent-accent" />
            </label>
            <div className={field}>
              <label className="flex items-center gap-2 text-ink">
                <input type="checkbox" checked={device.crackle} onChange={(e) => setDevice({ crackle: e.target.checked })} /> Record crackle
              </label>
              <input
                type="range"
                min={0.05}
                max={1}
                step={0.05}
                value={device.crackleLevel}
                disabled={!device.crackle}
                aria-label="Crackle intensity"
                aria-valuetext={`${Math.round(device.crackleLevel * 100)}%`}
                onChange={(e) => setDevice({ crackleLevel: Number(e.target.value) })}
                className="accent-accent disabled:opacity-40"
              />
            </div>
          </div>
          <p className="text-xs text-muted">Volume and crackle are saved on this device. Crackle plays only while music does. On iPhone, use the side buttons for volume.</p>
          {upNext.length > 0 && (
            <div>
              <p className="mb-1.5 text-xs text-muted">Up next</p>
              <ol className="flex flex-col gap-0.5">
                {upNext.map((t, i) => (
                  <li key={`${t.id}-${i}`}>
                    <button type="button" className="w-full truncate rounded-sm px-2 py-1 text-left text-sm hover:bg-sunken/60" onClick={() => player.play(player.queue, player.index + 1 + i)}>
                      {trackTitle(t.name)}
                    </button>
                  </li>
                ))}
              </ol>
            </div>
          )}
        </div>
      </div>
    </Panel>
  );
}

/** Play audio files from this device without uploading them. Nothing is kept. */
function LocalFiles() {
  const player = usePlayer();
  const pick = (files: FileList | null) => {
    if (!files?.length) return;
    const now = Date.now();
    const tracks: Track[] = [...files].map((f, i) => ({ id: `local-${now}-${i}`, folder: "", name: f.name, size: f.size, content_type: f.type, created_at: now, updated_at: now, local: f }));
    player.play(tracks);
  };
  return (
    <Panel title="from this device">
      <p className="mb-3 text-sm text-muted">Play audio files without uploading them. They stay for this session only: reloading or closing the page forgets them. To keep a track, upload it to your library.</p>
      <input type="file" accept="audio/*" multiple aria-label="Play files from this device" onChange={(e) => pick(e.target.files)} className="max-w-full text-sm text-ink file:mr-3 file:rounded-full file:border file:border-line file:bg-panel file:px-3 file:py-1.5 file:text-[13px] file:text-ink" />
    </Panel>
  );
}

function Upload({ disabled, onUploaded }: { disabled: boolean; onUploaded: () => void }) {
  const input = useRef<HTMLInputElement>(null);
  const [status, setStatus] = useState<string>();
  const upload = async (files: FileList | null) => {
    if (!files?.length) return;
    for (const [i, f] of [...files].entries()) {
      setStatus(`Uploading ${i + 1} of ${files.length}…`);
      try {
        await api.uploadFile(MUSIC_FOLDER, f);
      } catch (e) {
        reportIfUnauthorized(e);
        setStatus(`Couldn't upload “${f.name}”.`);
        onUploaded();
        return;
      }
    }
    setStatus(undefined);
    if (input.current) input.current.value = "";
    onUploaded();
  };
  return (
    <div className="flex flex-wrap items-center gap-2">
      <label className={`inline-flex ${disabled ? "opacity-50" : "cursor-pointer"}`}>
        <span className="sr-only">Upload audio</span>
        <input ref={input} type="file" accept="audio/*" multiple disabled={disabled} aria-label="Upload audio" onChange={(e) => void upload(e.target.files)} className="text-xs" />
      </label>
      {status && (
        <span role="status" className="text-xs text-muted">
          {status}
        </span>
      )}
    </div>
  );
}

function TrackRow(props: { track: FileEntry; music: MusicState; offline: boolean; playing: boolean; playlists: Playlist[]; onPlay: () => void }) {
  const { track, music } = props;
  const saved = music.saved.has(track.id);
  const [busy, setBusy] = useState(false);
  const title = trackTitle(track.name);
  return (
    <li className="flex flex-wrap items-center gap-2 py-2">
      <Button className={small} aria-label={`Play “${title}”`} disabled={props.offline && !saved} onClick={props.onPlay}>
        <Icon name={props.playing ? "music" : "play"} size={14} />
      </Button>
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm">{title}</span>
        <span className="font-mono text-[11px] text-muted">
          {track.folder || "/"} · {formatBytes(track.size)}
          {saved && " · saved offline"}
        </span>
      </span>
      {props.playlists.length > 0 && !props.offline && (
        <select
          aria-label={`Add “${title}” to a playlist`}
          value=""
          onChange={(e) => {
            const pl = props.playlists.find((p) => p.id === e.target.value);
            if (pl) void music.updatePlaylist(pl.id, { tracks: [...pl.tracks, track.id] });
          }}
          className={`${inputClass} min-h-7 w-auto py-0 text-xs`}
        >
          <option value="">Add to playlist</option>
          {props.playlists.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
      )}
      <Button
        className={small}
        disabled={busy || (props.offline && !saved)}
        aria-label={saved ? `Remove “${title}” from this device` : `Save “${title}” for offline`}
        onClick={async () => {
          setBusy(true);
          await (saved ? music.unsave(track.id) : music.save(track.id));
          setBusy(false);
        }}
      >
        {busy ? "Saving…" : saved ? "Remove download" : "Save offline"}
      </Button>
    </li>
  );
}

function Playlists({ music, offline, selected, onSelect }: { music: MusicState; offline: boolean; selected?: string; onSelect: (id?: string) => void }) {
  const [name, setName] = useState("");
  const playlists = music.library?.playlists ?? [];
  const create = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) return;
    const p = await music.createPlaylist(name);
    if (p) {
      setName("");
      onSelect(p.id);
    }
  };
  return (
    <Panel title={`playlists · ${playlists.length}`}>
      <form onSubmit={(e) => void create(e)} className="flex gap-2">
        <input value={name} onChange={(e) => setName(e.target.value)} maxLength={120} placeholder="New playlist…" aria-label="New playlist" disabled={offline} className={`${inputClass} min-w-0 flex-1`} />
        <Button type="submit" variant="primary" className={small} disabled={offline || !name.trim()}>
          Create
        </Button>
      </form>
      {playlists.length === 0 ? (
        <p className="mt-3 text-sm text-muted">No playlists yet.</p>
      ) : (
        <ul className="-mx-2 mt-3 flex flex-col gap-0.5">
          {playlists.map((p) => (
            <li key={p.id}>
              <button
                type="button"
                aria-expanded={p.id === selected}
                onClick={() => onSelect(p.id === selected ? undefined : p.id)}
                className={`flex w-full items-center justify-between rounded-sm px-2 py-1.5 text-left text-sm ${p.id === selected ? "bg-sunken" : "hover:bg-sunken/60"}`}
              >
                <span className="truncate">{p.name}</span>
                <span className="font-mono text-[11px] text-muted">{p.tracks.length}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </Panel>
  );
}

function PlaylistDetail({ playlist, tracks, music, offline }: { playlist: Playlist; tracks: FileEntry[]; music: MusicState; offline: boolean }) {
  const player = usePlayer();
  const [confirm, setConfirm] = useState(false);
  const list = useMemo(() => playlistTracks(playlist, tracks), [playlist, tracks]);
  const playable = list.filter((t) => !offline || music.saved.has(t.id));
  const setOrder = (ids: string[]) => void music.updatePlaylist(playlist.id, { tracks: ids });

  return (
    <Panel
      title={playlist.name}
      actions={
        <Button className={small} variant="primary" disabled={playable.length === 0} onClick={() => player.play(playable)}>
          Play
        </Button>
      }
    >
      {list.length === 0 ? (
        <p className="text-sm text-muted">Empty. Add tracks with “Add to playlist” in the library.</p>
      ) : (
        <ol className="flex flex-col gap-1">
          {list.map((t, i) => (
            <li key={`${t.id}-${i}`} className="flex items-center gap-1.5 text-sm">
              <span className="w-5 font-mono text-[11px] text-muted">{i + 1}</span>
              <span className="min-w-0 flex-1 truncate">{trackTitle(t.name)}</span>
              <Button className={small} aria-label={`Move “${trackTitle(t.name)}” up`} disabled={offline || i === 0} onClick={() => setOrder(moveTrack(playlist.tracks, i, -1))}>
                ↑
              </Button>
              <Button className={small} aria-label={`Move “${trackTitle(t.name)}” down`} disabled={offline || i === list.length - 1} onClick={() => setOrder(moveTrack(playlist.tracks, i, 1))}>
                ↓
              </Button>
              <Button className={small} aria-label={`Remove “${trackTitle(t.name)}” from the playlist`} disabled={offline} onClick={() => setOrder(playlist.tracks.filter((_, j) => j !== i))}>
                ✕
              </Button>
            </li>
          ))}
        </ol>
      )}
      <div className="mt-3 flex justify-end">
        {confirm ? (
          <span className="flex gap-1.5">
            <Button className={small} onClick={() => setConfirm(false)}>
              Keep
            </Button>
            <Button variant="danger" className={small} onClick={() => void music.deletePlaylist(playlist.id)}>
              Delete it
            </Button>
          </span>
        ) : (
          <Button variant="danger" className={small} disabled={offline} onClick={() => setConfirm(true)}>
            Delete playlist
          </Button>
        )}
      </div>
    </Panel>
  );
}
