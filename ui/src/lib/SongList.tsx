import { memo, useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { api } from "./api";
import { describe, type Translate } from "./format";
import type { Library } from "./types";
import Icon from "./Icon";
import { useTick } from "./tick";

// Re-renders only when the library, the language, the filter or the playing song change, not on
// every tick.
export default memo(function SongList({
  library,
  t,
  search,
}: {
  library: Library;
  t: Translate;
  search: RefObject<HTMLInputElement | null>;
}) {
  const [filter, setFilter] = useState("");
  const list = useRef<HTMLDivElement>(null);
  const current = useTick((tick) => tick?.current ?? null);
  const playing = useTick((tick) => tick?.state === "playing");

  const songs = useMemo(() => {
    const f = filter.trim().toLowerCase();
    return f ? library.songs.filter((s) => s.title.toLowerCase().includes(f)) : library.songs;
  }, [library.songs, filter]);

  // Keep the playing song in view when the playlist moves on.
  useEffect(() => {
    if (current === null) return;
    list.current
      ?.querySelector(`[data-index="${current}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [current]);

  return (
    <section className="flex min-h-0 flex-col border-r border-line">
      <div className="mx-4 mt-3 mb-2 flex h-9 items-center gap-2 rounded-[9px] border border-line bg-panel px-3 text-muted focus-within:border-accent-2">
        <Icon name="search" size={16} />
        <input
          ref={search}
          value={filter}
          onChange={(e) => setFilter(e.currentTarget.value)}
          placeholder={t("search")}
          spellCheck={false}
          className="flex-1 bg-transparent text-text outline-none"
        />
      </div>
      <div
        ref={list}
        className="flex-1 overflow-y-auto px-2 pb-3"
        role="listbox"
        aria-label={library.game}
      >
        {songs.length === 0 && <p className="mt-8 text-center text-muted">{t("no_results")}</p>}
        {songs.map((song) => {
          const active = song.index === current;
          return (
            <button
              key={song.index}
              className={`grid w-full grid-cols-[44px_minmax(0,1fr)_auto] items-center gap-2 rounded-lg px-2.5 py-[7px] text-left ${
                active ? "bg-[color-mix(in_srgb,var(--accent)_14%,var(--panel))]" : "hover:bg-panel"
              }`}
              data-index={song.index}
              role="option"
              aria-selected={active}
              onClick={() => api.play(song.index)}
            >
              <span className="num pr-1.5 text-right text-faint">
                {active && playing ? (
                  <span className="eq inline-flex h-3 items-end gap-0.5" aria-hidden="true">
                    <i />
                    <i />
                    <i />
                  </span>
                ) : (
                  song.index + 1
                )}
              </span>
              <span
                className={`truncate ${active ? "font-semibold text-accent" : ""}`}
              >
                {song.title}
              </span>
              <span className={`num text-xs ${song.length ? "text-muted" : "text-faint italic"}`}>
                {describe(t, song.length)}
              </span>
            </button>
          );
        })}
      </div>
    </section>
  );
});
