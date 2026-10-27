// The left column: the open ROMs and the playlists, one of which the middle column shows. A
// playlist is renamed in place (double-click it, or Rename in its right-click menu); a ROM is
// closed with its × (or Close in its right-click menu).
import { memo, useEffect, useRef, useState } from "react";
import { api } from "./api";
import type { Translate } from "./format";
import type { Library } from "./types";
import Icon from "./Icon";
import { useTick } from "./tick";
import Tip from "./Tip";

/** What the middle column shows: the songs of the shown ROM, or a playlist by id. */
export type View = { kind: "rom" } | { kind: "playlist"; id: string };

const label = "px-2.5 text-[11px] font-bold tracking-[0.08em] uppercase text-faint";
const item = "flex w-full items-center gap-2 rounded-lg px-2.5 py-[7px] text-left";

export default memo(function Sidebar({
  library,
  t,
  view,
  setView,
  shownRom,
  showRom,
  pickRom,
  renaming,
  setRenaming,
}: {
  library: Library;
  t: Translate;
  view: View;
  setView: (v: View) => void;
  shownRom: string | null;
  showRom: (id: string) => void;
  pickRom: () => void;
  renaming: string | null;
  setRenaming: (id: string | null) => void;
}) {
  const playingList = useTick((tick) => tick?.queue?.playlist ?? null);
  const playingRom = useTick((tick) => (tick?.state === "playing" ? tick.rom : null));
  const on = "bg-[color-mix(in_srgb,var(--accent)_14%,var(--panel))] font-semibold text-accent";
  const selected = (v: View) =>
    view.kind === "playlist" && v.kind === "playlist" && v.id === view.id ? on : "hover:bg-panel";

  async function newPlaylist() {
    const id = await api.createPlaylist(
      t("playlist_default_name", { n: library.playlists.length + 1 }),
      null,
      [],
    );
    setView({ kind: "playlist", id });
    setRenaming(id);
  }

  return (
    <nav className="flex min-h-0 flex-col gap-1 overflow-y-auto border-r border-line px-2 py-3">
      <div className={`${label} flex items-center justify-between pr-1`}>
        <span>{t("now_open")}</span>
        <Tip text={`${t("open_rom")} (Ctrl+O)`}>
          <button
            className="grid size-6 place-items-center rounded-md text-muted hover:bg-panel hover:text-text"
            onClick={pickRom}
            aria-label={t("open_rom")}
          >
            <Icon name="plus" size={16} />
          </button>
        </Tip>
      </div>
      {library.roms.length === 0 && <div className={`${item} text-muted`}>—</div>}
      {library.roms.map((rom) => {
        const name = rom.game || rom.title;
        return (
          <div key={rom.id} data-rom-item={rom.id} className="group relative">
            <Tip text={rom.path}>
              <button
                className={`${item} pr-8 ${view.kind === "rom" && rom.id === shownRom ? on : "hover:bg-panel"}`}
                onClick={() => showRom(rom.id)}
              >
                <span className="flex-1 truncate">{name}</span>
                {playingRom === rom.id ? (
                  <span className="eq inline-flex h-3 items-end gap-0.5 group-hover:invisible group-focus-within:invisible" aria-hidden="true">
                    <i />
                    <i />
                    <i />
                  </span>
                ) : (
                  <span className="num text-xs font-normal text-faint group-hover:invisible group-focus-within:invisible">
                    {rom.songs.length}
                  </span>
                )}
              </button>
            </Tip>
            <Tip text={t("rom_close")}>
              <button
                className="absolute top-1/2 right-1.5 grid size-6 -translate-y-1/2 place-items-center rounded-md text-muted opacity-0 group-hover:opacity-100 focus-visible:opacity-100 hover:bg-panel-2 hover:text-text"
                onClick={() => api.closeRom(rom.id)}
                aria-label={`${t("rom_close")}: ${name}`}
              >
                <Icon name="close" size={14} />
              </button>
            </Tip>
          </div>
        );
      })}

      <div className={`${label} mt-3 flex items-center justify-between pr-1`}>
        <span>{t("playlists")}</span>
        <Tip text={t("playlist_new")}>
          <button
            className="grid size-6 place-items-center rounded-md text-muted hover:bg-panel hover:text-text"
            onClick={newPlaylist}
            aria-label={t("playlist_new")}
          >
            <Icon name="plus" size={16} />
          </button>
        </Tip>
      </div>
      {library.playlists.map((list) =>
        renaming === list.id ? (
          <RenameField
            key={list.id}
            name={list.name}
            onDone={(name) => {
              if (name !== null) api.renamePlaylist(list.id, name);
              setRenaming(null);
            }}
          />
        ) : (
          <button
            key={list.id}
            data-playlist-item={list.id}
            className={`${item} ${selected({ kind: "playlist", id: list.id })}`}
            onClick={() => setView({ kind: "playlist", id: list.id })}
            onDoubleClick={() => setRenaming(list.id)}
          >
            <span className="flex-1 truncate">{list.name}</span>
            {playingList === list.id ? (
              <span className="eq inline-flex h-3 items-end gap-0.5" aria-hidden="true">
                <i />
                <i />
                <i />
              </span>
            ) : (
              <span className="num text-xs font-normal text-faint">{list.entries.length}</span>
            )}
          </button>
        ),
      )}
    </nav>
  );
});

/** A text field over a playlist's name; `onDone` gets the new name, or `null` when cancelled. */
function RenameField({ name, onDone }: { name: string; onDone: (name: string | null) => void }) {
  const [value, setValue] = useState(name);
  const ref = useRef<HTMLInputElement>(null);
  const done = useRef(false);
  useEffect(() => {
    ref.current?.focus();
    ref.current?.select();
  }, []);
  const finish = (result: string | null) => {
    if (done.current) return;
    done.current = true;
    onDone(result && result.trim() ? result : null);
  };
  return (
    <input
      ref={ref}
      value={value}
      spellCheck={false}
      onChange={(e) => setValue(e.currentTarget.value)}
      onBlur={() => finish(value)}
      onKeyDown={(e) => {
        e.stopPropagation();
        // Enter while an IME is composing only confirms the composition.
        if (e.nativeEvent.isComposing) return;
        if (e.key === "Enter") finish(value);
        else if (e.key === "Escape") finish(null);
      }}
      className="w-full rounded-lg border border-accent-2 bg-panel px-2.5 py-[6px] text-text outline-none"
    />
  );
}
