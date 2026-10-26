// Mirrors of the views sent by app/src/gui/mod.rs.

export type Length =
  | { kind: "loops"; intro: number; period: number }
  | { kind: "ends"; at: number }
  | { kind: "unknown" };

export interface SongView {
  index: number;
  title: string;
  /** `null` while the song is still being analysed. */
  length: Length | null;
}

export interface Library {
  language: string;
  languages: { code: string; name: string }[];
  text: Record<string, string>;
  game: string;
  title: string;
  romPath: string | null;
  songs: SongView[];
  loopCount: number | null;
  volume: number;
  muted: [boolean, boolean, boolean, boolean];
  error: string | null;
}

export type PlayState = "none" | "stopped" | "playing" | "paused" | "finished" | "failed";

export interface Tick {
  current: number | null;
  state: PlayState;
  failure: string | null;
  /** Seconds since the song started. */
  position: number;
  length: Length;
  levels: [number, number, number, number];
  scope: number[];
}
