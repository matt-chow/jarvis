import { describe, expect, it } from "vitest";
import type { FileEntry } from "./files";
import { formatDuration, moveTrack, playlistTracks, trackTitle, type Playlist } from "./music";

const file = (id: string, name: string): FileEntry => ({ id, folder: "Music", name, size: 1, content_type: "audio/mpeg", created_at: 0, updated_at: 0 });

describe("music helpers", () => {
  it("titles, orders, and formats", () => {
    expect(trackTitle("02 - Song.mp3")).toBe("02 - Song");
    expect(trackTitle("Live.at.home.FLAC")).toBe("Live.at.home");
    const pl: Playlist = { id: "p", name: "Mix", tracks: ["b", "gone", "a", "b"], created_at: 0, updated_at: 0 };
    expect(playlistTracks(pl, [file("a", "A"), file("b", "B")]).map((t) => t.id)).toEqual(["b", "a", "b"]);
    expect(moveTrack(["a", "b", "c"], 2, -1)).toEqual(["a", "c", "b"]);
    expect(moveTrack(["a", "b"], 0, -1)).toEqual(["a", "b"]);
    expect(formatDuration(125.7)).toBe("2:05");
    expect(formatDuration(Number.NaN)).toBe("–:––");
  });
});
