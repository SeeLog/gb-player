// What the right-click menu holds: window items on the title bar, play, copy and add to a
// playlist on a song, Close on an open ROM, the playlist items on a playlist and its songs, and
// playback elsewhere. Text fields keep the WebView's Cut / Copy / Paste.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { RESTART_AFTER, api, previous } from "./api";
import { deletePlaylist, locateEntry } from "./actions";
import type { MenuItem } from "./ContextMenu";
import { notify } from "./Toasts";
import type { Translate } from "./format";
import type { Library } from "./types";
import { tickStore } from "./tick";

const NOT_TEXT = new Set(["checkbox", "radio", "range", "button", "submit", "reset", "color", "file"]);

/** Whether a right-click here should get the WebView's own menu (to cut, copy and paste). */
export function wantsNativeMenu(el: Element): boolean {
  if (el instanceof HTMLTextAreaElement) return true;
  if (el instanceof HTMLInputElement) return !NOT_TEXT.has(el.type);
  return el.closest('[contenteditable="true"]') !== null;
}

/** What the menus need from the app beyond the API. */
export interface MenuActions {
  showPlaylist: (id: string) => void;
  renamePlaylist: (id: string) => void;
  /** Open a ROM the user picks, and show its songs. */
  pickRom: () => void;
}

/** The items for a right-click on `target`, or `null` to show no menu. */
export async function menuFor(
  target: Element,
  library: Library | null,
  t: Translate,
  actions: MenuActions,
): Promise<MenuItem[] | null> {
  if (target.closest("header")) return windowItems(t);
  const row = target.closest<HTMLElement>("[data-index]");
  const romId = target.closest<HTMLElement>("[data-rom]")?.dataset.rom;
  const rom = library?.roms.find((r) => r.id === romId);
  const song = row && rom?.songs.find((s) => s.index === Number(row.dataset.index));
  if (library && rom && song) {
    return [
      ...songItems(rom.id, song.index, song.title, t),
      "separator",
      ...addItems(rom.id, song.index, library, t, actions),
    ];
  }
  const romItem = target.closest<HTMLElement>("[data-rom-item]")?.dataset.romItem;
  if (romItem && library?.roms.some((r) => r.id === romItem)) {
    return [{ label: t("rom_close"), onSelect: () => api.closeRom(romItem) }];
  }
  const entryRow = target.closest<HTMLElement>("[data-entry]");
  const listId = target.closest<HTMLElement>("[data-playlist]")?.dataset.playlist;
  const list = library?.playlists.find((l) => l.id === listId);
  if (entryRow && list) return entryItems(list.id, Number(entryRow.dataset.entry), library!, t);
  const itemId = target.closest<HTMLElement>("[data-playlist-item]")?.dataset.playlistItem;
  const item = library?.playlists.find((l) => l.id === itemId);
  if (item) {
    const first = item.entries.findIndex((e) => !e.missing);
    return playlistItems(item.id, item.name, first, t, actions);
  }
  return playbackItems(!!library && library.roms.length > 0, t, actions);
}

/** Add a song of an open ROM to a playlist (in a submenu, however many there are), or to a
 * new one. */
function addItems(
  rom: string,
  index: number,
  library: Library,
  t: Translate,
  actions: MenuActions,
): MenuItem[] {
  const newPlaylist: MenuItem = {
    label: t("playlist_add_new"),
    onSelect: async () => {
      const name = t("playlist_default_name", { n: library.playlists.length + 1 });
      const id = await api.createPlaylist(name, rom, [index]);
      actions.showPlaylist(id);
      actions.renamePlaylist(id);
    },
  };
  if (library.playlists.length === 0) return [newPlaylist];
  return [
    {
      label: t("playlist_add"),
      // The new playlist first, so it is in reach however many playlists follow.
      submenu: [
        newPlaylist,
        "separator",
        ...library.playlists.map((l) => ({
          label: l.name,
          onSelect: () => api.addToPlaylist(l.id, rom, [index]),
        })),
      ],
    },
  ];
}

function entryItems(id: string, position: number, library: Library, t: Translate): MenuItem[] {
  const list = library.playlists.find((l) => l.id === id);
  const entry = list?.entries[position];
  if (!entry) return [];
  const tick = tickStore.get();
  const current = tick?.queue?.playlist === id && tick.queue.position === position;
  return [
    entry.missing
      ? { label: t("playlist_locate"), onSelect: () => locateEntry(id, position, t) }
      : current && tick?.state === "playing"
        ? { label: t("pause"), onSelect: () => api.toggle() }
        : {
            label: t("play"),
            onSelect: () =>
              current && tick?.state === "paused" ? api.toggle() : api.playEntry(id, position),
          },
    { label: t("copy_title"), onSelect: () => copyText(entry.title, t) },
    "separator",
    {
      label: t("playlist_remove"),
      shortcut: "Del",
      onSelect: () => api.removeFromPlaylist(id, [position]),
    },
  ];
}

function playlistItems(
  id: string,
  name: string,
  /** The first song whose ROM is there, or -1. */
  first: number,
  t: Translate,
  actions: MenuActions,
): MenuItem[] {
  return [
    {
      label: t("playlist_play"),
      disabled: first < 0,
      onSelect: () => {
        actions.showPlaylist(id);
        api.playEntry(id, first);
      },
    },
    "separator",
    { label: t("playlist_rename"), onSelect: () => actions.renamePlaylist(id) },
    { label: t("playlist_delete"), onSelect: () => deletePlaylist(id, name, t) },
  ];
}

async function windowItems(t: Translate): Promise<MenuItem[]> {
  const win = getCurrentWindow();
  const [onTop, maximized] = await Promise.all([win.isAlwaysOnTop(), win.isMaximized()]);
  return [
    { label: t("always_on_top"), checked: onTop, onSelect: () => win.setAlwaysOnTop(!onTop) },
    "separator",
    { label: t("window_minimize"), onSelect: () => win.minimize() },
    {
      label: maximized ? t("window_restore") : t("window_maximize"),
      onSelect: () => win.toggleMaximize(),
    },
    "separator",
    { label: t("window_close"), onSelect: () => win.close() },
  ];
}

function songItems(rom: string, index: number, title: string, t: Translate): MenuItem[] {
  const tick = tickStore.get();
  const current = tick?.rom === rom && tick.current === index;
  const playing = current && tick?.state === "playing";
  return [
    playing
      ? { label: t("pause"), onSelect: () => api.toggle() }
      : {
          label: t("play"),
          // A paused song carries on from where it was; any other starts from the top.
          onSelect: () =>
            current && tick?.state === "paused" ? api.toggle() : api.play(rom, index),
        },
    { label: t("copy_title"), onSelect: () => copyText(title, t) },
  ];
}

function playbackItems(hasRom: boolean, t: Translate, actions: MenuActions): MenuItem[] {
  const tick = tickStore.get();
  const playing = tick?.state === "playing";
  const restarts = tick?.current != null && tick.position > RESTART_AFTER;
  return [
    {
      label: playing ? t("pause") : t("play"),
      shortcut: "Space",
      disabled: !hasRom,
      onSelect: () => api.toggle(),
    },
    {
      label: restarts ? t("prev_restart") : t("prev"),
      shortcut: "←",
      disabled: !hasRom,
      onSelect: () => previous(tickStore.get()),
    },
    { label: t("next"), shortcut: "→", disabled: !hasRom, onSelect: () => api.skip(1) },
    "separator",
    { label: t("open_rom"), shortcut: "Ctrl+O", onSelect: actions.pickRom },
  ];
}

/** Copy a song's title, and say so. */
async function copyText(text: string, t: Translate) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    // Fall back to the old way where the async clipboard is not allowed.
    const area = document.createElement("textarea");
    area.value = text;
    document.body.append(area);
    area.select();
    document.execCommand("copy");
    area.remove();
  }
  notify(t("title_copied"));
}
