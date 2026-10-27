import { memo, useEffect, useMemo, useRef, useState, type RefObject } from "react";
import { api } from "./api";
import { fmtDuration, playTime, playTimeDetail, type Translate } from "./format";
import type { Length, Library, SongView as Song } from "./types";
import Icon from "./Icon";
import { useTick } from "./tick";

type SortKey = "title" | "length";
type Sort = { key: SortKey; desc: boolean } | null;

/** The sort survives switching to a playlist and back, but not a restart. `null` is song order. */
let lastSort: Sort = null;

// Re-renders only when the library (including the loop setting), the language, the filter, the
// sort or the playing song change, not on every tick.
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
  const [sort, setSortState] = useState<Sort>(lastSort);
  const list = useRef<HTMLDivElement>(null);
  const current = useTick((tick) => tick?.current ?? null);
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
    const found = f
      ? library.songs.filter((s) => s.title.toLowerCase().includes(f))
      : library.songs;
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
  }, [library.songs, library.language, library.loopCount, filter, sort]);

  // Keep the playing song in view when the playlist moves on.
  useEffect(() => {
    if (current === null) return;
    list.current
      ?.querySelector(`[data-index="${current}"]`)
      ?.scrollIntoView({ block: "nearest" });
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
    <section className="flex min-h-0 flex-col border-r border-line">
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
          <button
            className="-mr-1.5 grid size-6 place-items-center rounded-md text-muted hover:bg-panel-2 hover:text-text"
            title={t("search_clear")}
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
        )}
      </div>
      <div
        role="row"
        className="grid grid-cols-[44px_minmax(0,1fr)_auto] gap-2 overflow-hidden border-b border-line px-[18px] pb-1 text-xs text-muted [scrollbar-gutter:stable]"
      >
        {header("#", null, "pr-1.5 text-right")}
        {header(t("col_title"), "title")}
        {header(t("col_length"), "length", "text-right")}
      </div>
      <div
        ref={list}
        className="flex-1 overflow-y-auto px-2 pt-1 pb-3 [scrollbar-gutter:stable]"
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
              <Duration t={t} length={song.length} loopCount={library.loopCount} />
            </button>
          );
        })}
      </div>
    </section>
  );
});

/** How long the song plays with the current loop setting, with the details on hover. */
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
    <span className="flex items-center gap-2" title={playTimeDetail(t, length, loopCount)}>
      {length.kind === "ends" && (
        <span className="rounded border border-line px-1.5 text-[10px] leading-4 text-muted">
          {t("no_loop")}
        </span>
      )}
      <span className="num text-xs text-muted">{secs === null ? "—" : fmtDuration(secs)}</span>
    </span>
  );
}
