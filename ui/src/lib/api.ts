// Calls into the Rust side (the Tauri commands in app/src/gui/).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Background, Library, Shuffle, Tick } from "./types";

export const api = {
  library: () => invoke<Library>("library"),
  /** Resolves to the id of the last ROM opened. */
  openRoms: (paths: string[]) => invoke<string | null>("open_roms", { paths }),
  closeRom: (id: string) => invoke<void>("close_rom", { id }),
  /** Resolves to the id of the last ROM opened, or `null` when none was picked. */
  pickRoms: () => invoke<string | null>("pick_roms"),
  play: (rom: string, index: number) => invoke<void>("play", { rom, index }),
  createPlaylist: (name: string, rom: string | null, songs: number[]) =>
    invoke<string>("create_playlist", { name, rom: rom ?? "", songs }),
  renamePlaylist: (id: string, name: string) => invoke<void>("rename_playlist", { id, name }),
  /** Asks first; resolves to whether it was deleted. */
  deletePlaylist: (id: string, name: string) => invoke<boolean>("delete_playlist", { id, name }),
  addToPlaylist: (id: string, rom: string, songs: number[]) =>
    invoke<void>("add_to_playlist", { id, rom, songs }),
  removeFromPlaylist: (id: string, positions: number[]) =>
    invoke<void>("remove_from_playlist", { id, positions }),
  moveInPlaylist: (id: string, from: number, to: number) =>
    invoke<void>("move_in_playlist", { id, from, to }),
  playEntry: (id: string, position: number) => invoke<boolean>("play_entry", { id, position }),
  /** The ROM whose songs are shown (or were, before the playlist), and the playlist shown. */
  setView: (rom: string | null, playlist: string | null) =>
    invoke<void>("set_view", { rom, playlist }),
  relinkEntry: (id: string, position: number) => invoke<void>("relink_entry", { id, position }),
  toggle: () => invoke<void>("toggle"),
  stop: () => invoke<void>("stop"),
  skip: (delta: number) => invoke<void>("skip", { delta }),
  seek: (secs: number) => invoke<void>("seek", { secs }),
  setLoopCount: (count: number | null) => invoke<void>("set_loop_count", { count }),
  setAutoNext: (autoNext: boolean) => invoke<void>("set_auto_next", { autoNext }),
  setShuffle: (shuffle: Shuffle) => invoke<void>("set_shuffle", { shuffle }),
  setVolume: (volume: number) => invoke<void>("set_volume", { volume }),
  setMuted: (channel: number, muted: boolean) => invoke<void>("set_muted", { channel, muted }),
  setTheme: (theme: string) => invoke<void>("set_theme", { theme }),
  saveTheme: (
    name: string,
    base: string,
    colors: Record<string, string>,
    background: Background | null,
  ) => invoke<string>("save_theme", { name, base, colors, background }),
  themeImage: (image: string) => invoke<string>("theme_image", { image }),
  pickThemeImage: () => invoke<string | null>("pick_theme_image"),
  setLanguage: (language: string) => invoke<void>("set_language", { language }),
};

export const onLibrary = (f: (l: Library) => void): Promise<UnlistenFn> =>
  listen<Library>("library", (e) => f(e.payload));

export const onTick = (f: (t: Tick) => void): Promise<UnlistenFn> =>
  listen<Tick>("tick", (e) => f(e.payload));

/** Seconds into a song after which Previous starts it again instead of going back a song. */
export const RESTART_AFTER = 3;

/** Previous, as in most music players: back to the start of the song once it has played a
 * little, otherwise to the previous song. */
export function previous(tick: Tick | null) {
  if (tick?.rom != null && tick.current != null && tick.position > RESTART_AFTER) {
    api.play(tick.rom, tick.current);
  }
  else api.skip(-1);
}
