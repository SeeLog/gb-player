// The left column: the open ROM's song list and the playlists, one of which the middle column
// shows. A playlist is renamed in place (double-click it, or Rename in its right-click menu).
import { memo, useEffect, useRef, useState } from "react";
import { api } from "./api";
import type { Translate } from "./format";
import type { Library } from "./types";
import Icon from "./Icon";
import { useTick } from "./tick";

/** What the middle column shows: the open ROM's songs, or a playlist by id. */
export type View = { kind: "rom" } | { kind: "playlist"; id: string };

const label = "px-2.5 text-[11px] font-bold tracking-[0.08em] uppercase text-faint";
const item = "flex w-full items-center gap-2 rounded-lg px-2.5 py-[7px] text-left";

export default memo(function Sidebar({
  library,
  t,
  view,
  setView,
  renaming,
  setRenaming,
}: {
  library: Library;
  t: Translate;
  view: View;
  setView: (v: View) => void;
  renaming: string | null;
  setRenaming: (id: string | null) => void;
}) {
  const playingList = useTick((tick) => tick?.queue?.playlist ?? null);
  const hasRom = library.songs.length > 0;
  const selected = (v: View) =>
    v.kind === view.kind && (v.kind === "rom" || (view.kind === "playlist" && v.id === view.id))
      ? "bg-[color-mix(in_srgb,var(--accent)_14%,var(--panel))] font-semibold text-accent"
      : "hover:bg-panel";

  async function newPlaylist() {
    const id = await api.createPlaylist(
      t("playlist_default_name", { n: library.playlists.length + 1 }),
      [],
    );
    setView({ kind: "playlist", id });
    setRenaming(id);
  }

  return (
    <nav className="flex min-h-0 flex-col gap-1 overflow-y-auto border-r border-line px-2 py-3">
      <div className={label}>{t("now_open")}</div>
      <button
        className={`${item} ${selected({ kind: "rom" })}`}
        onClick={() => setView({ kind: "rom" })}
        title={library.romPath ?? ""}
      >
        <span className="truncate">{hasRom ? library.game || library.title : "—"}</span>
      </button>

      <div className={`${label} mt-3 flex items-center justify-between pr-1`}>
        <span>{t("playlists")}</span>
        <button
          className="grid size-6 place-items-center rounded-md text-muted hover:bg-panel hover:text-text"
          onClick={newPlaylist}
          title={t("playlist_new")}
          aria-label={t("playlist_new")}
        >
          <Icon name="plus" size={16} />
        </button>
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
