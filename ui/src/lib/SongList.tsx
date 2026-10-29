import { memo, useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { api } from "./api";
import { describe, fmtDuration, playTime, type Translate } from "./format";
import type { Length, Library, RomView, SongView as Song } from "./types";
import Icon from "./Icon";
import { useTick } from "./tick";
import Tip from "./Tip";

type SortKey = "title" | "length";
type Sort = { key: SortKey; desc: boolean } | null;

/** The sort survives switching to a playlist or another ROM and back, but not a restart. `null`
 * is song order. */
let lastSort: Sort = null;

// Re-renders only when the library (including the loop setting), the language, the filter, the
// sort or the playing song change, not on every tick.
export default memo(function SongList({
  library,
  rom,
  t,
  search,
}: {
  library: Library;
  rom: RomView;
  t: Translate;
  search: RefObject<HTMLInputElement | null>;
}) {
  const [filter, setFilter] = useState("");
  const [sort, setSortState] = useState<Sort>(lastSort);
  const list = useRef<HTMLDivElement>(null);
  const current = useTick((tick) => (tick?.rom === rom.id ? tick.current : null));
  const playing = useTick((tick) => tick?.state === "playing");

  function setSort(next: Sort) {
    lastSort = next;
    setSortState(next);
  }

  /** Ascending, then descending, then back to song order. */
  function cycle(key: SortKey) {
    if (sort?.key !== key) setSort({ key, desc: false });
    else if (!sort.desc) setSort({ key, desc: true });
    else setSort(null);
  }

  const songs = useMemo(() => {
    const f = filter.trim().toLowerCase();
    const found = f ? rom.songs.filter((s) => s.title.toLowerCase().includes(f)) : rom.songs;
    if (!sort) return found;
    const dir = sort.desc ? -1 : 1;
    let compare: (a: Song, b: Song) => number;
    if (sort.key === "title") {
      const collator = new Intl.Collator(library.language, { numeric: true });
      compare = (a, b) => dir * collator.compare(a.title, b.title) || a.index - b.index;
    } else {
      // Songs still being analysed or with no known length go last either way.
      const secs = (s: Song) => (s.length ? playTime(s.length, library.loopCount) : null);
      compare = (a, b) => {
        const x = secs(a);
        const y = secs(b);
        if (x === null || y === null) {
          return (x === null ? 1 : 0) - (y === null ? 1 : 0) || a.index - b.index;
        }
        return dir * (x - y) || a.index - b.index;
      };
    }
    return [...found].sort(compare);
  }, [rom.songs, library.language, library.loopCount, filter, sort]);

  // Keep the playing song in view when the playlist moves on.
  useEffect(() => {
    if (current === null) return;
    list.current?.querySelector(`[data-index="${current}"]`)?.scrollIntoView({ block: "nearest" });
  }, [current]);

  /** A column title that sorts by `key`, or returns to song order when it is null. */
  function header(label: string, key: SortKey | null, className = "") {
    const sorted = key ? sort?.key === key : sort === null;
    return (
      <span
        role="columnheader"
        aria-sort={sorted ? (sort?.desc ? "descending" : "ascending") : "none"}
        className={className}
      >
        <button
          className={`inline-flex items-center gap-1 hover:text-text ${sorted && sort ? "text-text" : ""}`}
          onClick={() => (key ? cycle(key) : setSort(null))}
        >
          {label}
          {sorted && sort && (
            <span aria-hidden="true" className="text-[9px]">
              {sort.desc ? "▼" : "▲"}
            </span>
          )}
        </button>
      </span>
    );
  }

  return (
    <section data-rom={rom.id} className="flex min-h-0 flex-col border-r border-line">
      <div className="mx-4 mt-3 mb-2 flex h-9 items-center gap-2 rounded-[9px] border border-line bg-panel px-3 text-muted focus-within:border-accent-2">
        <Icon name="search" size={16} />
        <input
          ref={search}
          value={filter}
          onChange={(e) => setFilter(e.currentTarget.value)}
          placeholder={t("search")}
          onKeyDown={(e) => {
            // The first Escape clears the search, the next leaves the field (in App).
            if (e.key === "Escape" && filter) {
              e.stopPropagation();
              setFilter("");
            }
          }}
          spellCheck={false}
          className="min-w-0 flex-1 bg-transparent text-text outline-none"
        />
        {filter && (
          <Tip text={t("search_clear")}>
            <button
              className="-mr-1.5 grid size-6 place-items-center rounded-md text-muted hover:bg-panel-2 hover:text-text"
              aria-label={t("search_clear")}
              // Keep the focus in the field.
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => {
                setFilter("");
                search.current?.focus();
              }}
            >
              <Icon name="close" size={14} />
            </button>
          </Tip>
        )}
      </div>
      <div
        role="row"
        className="grid grid-cols-[44px_minmax(0,1fr)_auto_24px] gap-2 overflow-hidden border-b border-line px-[18px] pb-1 text-xs text-muted [scrollbar-gutter:stable]"
      >
        {header("#", null, "pr-1.5 text-right")}
        {header(t("col_title"), "title")}
        {header(t("col_length"), "length", "text-right")}
        <span />
      </div>
      <div
        ref={list}
        className="flex-1 overflow-y-auto px-2 pt-1 pb-3 [scrollbar-gutter:stable]"
        role="listbox"
        aria-label={rom.game || rom.title}
      >
        {songs.length === 0 && <p className="mt-8 text-center text-muted">{t("no_results")}</p>}
        {songs.map((song) => {
          const active = song.index === current;
          // The star sits over the row's last column, as a button cannot hold another.
          return (
            <div key={song.index} data-index={song.index} className="group relative">
              <button
                className={`grid w-full grid-cols-[44px_minmax(0,1fr)_auto_24px] items-center gap-2 rounded-lg px-2.5 py-[7px] text-left ${
                  active ? "bg-playing" : "group-hover:bg-hover"
                }`}
                role="option"
                aria-selected={active}
                onClick={() => api.play(rom.id, song.index)}
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
                  className={`truncate ${active ? "font-semibold text-[color:var(--row-text,var(--accent))]" : ""}`}
                >
                  {song.title}
                </span>
                <Duration t={t} length={song.length} loopCount={library.loopCount} />
              </button>
              <span className="absolute top-1/2 right-2.5 -translate-y-1/2">
                <Star
                  t={t}
                  on={song.favorite}
                  onToggle={() => api.setFavorite(rom.id, song.index, !song.favorite)}
                />
              </span>
            </div>
          );
        })}
      </div>
    </section>
  );
});

/** The star that adds a song to the favourites or takes it out: shown while the song is one,
 * otherwise on hovering its row (a `group`). */
export function Star({ t, on, onToggle }: { t: Translate; on: boolean; onToggle: () => void }) {
  const label = on ? t("favorite_remove") : t("favorite_add");
  return (
    <Tip text={label}>
      <button
        className={`grid size-6 place-items-center rounded-md hover:bg-panel-2 ${
          on
            ? "text-accent"
            : "text-faint opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 hover:text-text"
        }`}
        aria-label={label}
        aria-pressed={on}
        // Not a press, a drag or a key of the row.
        onPointerDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.stopPropagation()}
        onClick={(e) => {
          e.stopPropagation();
          onToggle();
        }}
      >
        <Icon name={on ? "starFilled" : "star"} size={16} />
      </button>
    </Tip>
  );
}

/** How long the song plays with the current loop setting, with the intro and loop on hover. */
export function Duration({
  t,
  length,
  loopCount,
}: {
  t: Translate;
  length: Length | null;
  loopCount: number | null;
}) {
  if (!length) return <span className="text-xs text-faint italic">{t("analyzing")}</span>;
  const secs = playTime(length, loopCount);
  return (
    <Tip text={length.kind === "ends" ? undefined : describe(t, length)}>
      <span className="num text-xs text-muted">{secs === null ? "—" : fmtDuration(secs)}</span>
    </Tip>
  );
}
