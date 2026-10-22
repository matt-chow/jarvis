/** Music types (mirrors directory-core `music`). Tracks are audio files. */
import type { FileEntry } from "./files";

export interface Playlist {
  id: string;
  name: string;
  /** File ids in play order. */
  tracks: string[];
  created_at: number;
  updated_at: number;
}

export interface MusicLibrary {
  tracks: FileEntry[];
  playlists: Playlist[];
}

/** "02 - Song.mp3" → "02 - Song". */
export const trackTitle = (name: string) => name.replace(/\.[a-z0-9]{2,5}$/i, "");

/** The playlist's tracks that still exist, in order. */
export function playlistTracks(p: Playlist, tracks: FileEntry[]): FileEntry[] {
  const byId = new Map(tracks.map((t) => [t.id, t]));
  return p.tracks.flatMap((id) => byId.get(id) ?? []);
}

/** Move the track at `index` up (-1) or down (+1). */
export function moveTrack(ids: string[], index: number, by: -1 | 1): string[] {
  const j = index + by;
  if (index < 0 || j < 0 || j >= ids.length) return ids;
  const next = [...ids];
  [next[index], next[j]] = [next[j], next[index]];
  return next;
}

export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "–:––";
  const s = Math.floor(seconds);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}
