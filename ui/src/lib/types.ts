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

/** An open ROM. */
export interface RomView {
  /** The hash of the file, the same between runs. */
  id: string;
  game: string;
  title: string;
  path: string;
  songs: SongView[];
}

/** What Next plays: the next song, a random one of the ROM (or the playlist being played), or a
 * random one of any open ROM (or of the playlist being played). */
export type Shuffle = "off" | "list" | "all";

export interface Theme {
  id: string;
  name: string;
  /** The built-in palette the colours are laid over. */
  base: string;
  /** Token (CSS custom property without `--`) to colour. */
  colors: Record<string, string>;
  background: Background | null;
  /** The console the mini player is drawn as. */
  console: Console;
  custom: boolean;
}

/** The first model, or the colour one. */
export type Console = "dmg" | "gbc";

/** An image drawn behind the window, under the `bg` colour. */
export interface Background {
  /** A path inside the themes folder, such as `images/sky.png`. */
  image: string;
  fit: "cover" | "contain" | "tile";
  /** Blur radius in pixels, 0 to 40. */
  blur: number;
  /** How much to darken the image, 0 to 1. */
  dim: number;
}

/** A song of a playlist, from any ROM. */
export interface EntryView {
  game: string;
  title: string;
  length: Length | null;
  /** The ROM file. */
  path: string;
  /** Whether the ROM file is not there any more. */
  missing: boolean;
}

export interface PlaylistView {
  id: string;
  name: string;
  entries: EntryView[];
}

export interface Library {
  language: string;
  languages: { code: string; name: string }[];
  text: Record<string, string>;
  /** In the order they were opened. */
  roms: RomView[];
  /** The ROM whose songs were last shown. */
  shownRom: string | null;
  loopCount: number | null;
  autoNext: boolean;
  shuffle: Shuffle;
  volume: number;
  /** The volume of each channel, from 0 to 1. */
  channelVolumes: [number, number, number, number];
  muted: [boolean, boolean, boolean, boolean];
  /** The id of the chosen theme. */
  theme: string;
  themes: Theme[];
  playlists: PlaylistView[];
  /** The playlist shown when the app was last closed. */
  openPlaylist: string | null;
  /** Whether the window is the mini player. */
  mini: boolean;
  /** Whether the mini player stays above other windows. */
  miniOnTop: boolean;
  error: string | null;
}

export type PlayState = "none" | "stopped" | "playing" | "paused" | "finished" | "failed";

export interface Tick {
  /** The ROM the current song is from. */
  rom: string | null;
  current: number | null;
  /** The playlist being played and the position in it. */
  queue: { playlist: string; position: number } | null;
  state: PlayState;
  failure: string | null;
  /** Seconds since the song started. */
  position: number;
  length: Length;
  levels: [number, number, number, number];
  scope: number[];
}

/** How pointing a playlist entry at a file went (see app/src/gui/commands.rs). */
export type Relink =
  | { kind: "done" }
  | { kind: "cancelled" }
  | { kind: "failed"; message: string }
  /** A different ROM than the entry's: taken only once the user agrees. */
  | { kind: "mismatch"; message: string; path: string };
