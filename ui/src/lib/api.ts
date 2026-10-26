// Calls into the Rust side (the commands in app/src/gui/mod.rs).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Background, Library, Tick } from "./types";

export const api = {
  library: () => invoke<Library>("library"),
  openRom: (path: string) => invoke<void>("open_rom", { path }),
  pickRom: () => invoke<void>("pick_rom"),
  play: (index: number) => invoke<void>("play", { index }),
  toggle: () => invoke<void>("toggle"),
  stop: () => invoke<void>("stop"),
  skip: (delta: number) => invoke<void>("skip", { delta }),
  setLoopCount: (count: number | null) => invoke<void>("set_loop_count", { count }),
  setAutoNext: (autoNext: boolean) => invoke<void>("set_auto_next", { autoNext }),
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
