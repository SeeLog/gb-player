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

export interface Theme {
  id: string;
  name: string;
  /** The built-in palette the colours are laid over. */
  base: string;
  /** Token (CSS custom property without `--`) to colour. */
  colors: Record<string, string>;
  background: Background | null;
  custom: boolean;
}

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

export interface Library {
  language: string;
  languages: { code: string; name: string }[];
  text: Record<string, string>;
  game: string;
  title: string;
  romPath: string | null;
  songs: SongView[];
  loopCount: number | null;
  autoNext: boolean;
  volume: number;
  muted: [boolean, boolean, boolean, boolean];
  /** The id of the chosen theme. */
  theme: string;
  themes: Theme[];
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
