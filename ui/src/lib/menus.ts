// What the right-click menu holds: window items on the title bar, play and copy on a song, and
// playback elsewhere. Text fields keep the WebView's Cut / Copy / Paste.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { RESTART_AFTER, api, previous } from "./api";
import type { MenuItem } from "./ContextMenu";
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

/** The items for a right-click on `target`, or `null` to show no menu. */
export async function menuFor(
  target: Element,
  library: Library | null,
  t: Translate,
): Promise<MenuItem[] | null> {
  if (target.closest("header")) return windowItems(t);
  const row = target.closest<HTMLElement>("[data-index]");
  const song = row && library?.songs.find((s) => s.index === Number(row.dataset.index));
  if (song) return songItems(song.index, song.title, t);
  return playbackItems(!!library && library.songs.length > 0, t);
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

function songItems(index: number, title: string, t: Translate): MenuItem[] {
  const tick = tickStore.get();
  const current = tick?.current === index;
  const playing = current && tick?.state === "playing";
  return [
    playing
      ? { label: t("pause"), onSelect: () => api.toggle() }
      : {
          label: t("play"),
          // A paused song carries on from where it was; any other starts from the top.
          onSelect: () => (current && tick?.state === "paused" ? api.toggle() : api.play(index)),
        },
    { label: t("copy_title"), onSelect: () => copyText(title) },
  ];
}

function playbackItems(hasRom: boolean, t: Translate): MenuItem[] {
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
    { label: t("open_rom"), shortcut: "Ctrl+O", onSelect: () => api.pickRom() },
  ];
}

async function copyText(text: string) {
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
}
