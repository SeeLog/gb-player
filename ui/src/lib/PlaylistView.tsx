// A playlist in the middle column. Songs from any ROM; a song whose ROM is missing is greyed
// out and asks for the file when clicked. Drag a song to move it, Delete removes it.
import { memo, useEffect, useRef, useState, type PointerEvent } from "react";
import { api } from "./api";
import type { Translate } from "./format";
import type { Library, PlaylistView as Playlist } from "./types";
import Icon from "./Icon";
import { Duration } from "./SongList";
import { useTick } from "./tick";

/** How far the pointer moves before a press on a song becomes a drag. */
const DRAG_START = 4;

interface Drag {
  from: number;
  startY: number;
  /** The gap the song would be dropped into: 0 is above the first song. */
  gap: number | null;
}

export default memo(function PlaylistView({
  library,
  list,
  t,
}: {
  library: Library;
  list: Playlist;
  t: Translate;
}) {
  const rows = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  // A drag ends in a click on the row it started from, which must not play it.
  const dragged = useRef(false);
  const position = useTick((tick) =>
    tick?.queue?.playlist === list.id ? tick.queue.position : null,
  );
  const playing = useTick((tick) => tick?.state === "playing");
  const firstPlayable = list.entries.findIndex((e) => !e.missing);

  useEffect(() => {
    if (position === null) return;
    rows.current
      ?.querySelector(`[data-entry="${position}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }, [position]);

  function activate(i: number) {
    if (list.entries[i].missing) api.relinkEntry(list.id, i);
    else api.playEntry(list.id, i);
  }

  /** The gap nearest to the pointer at `y`. */
  function gapAt(y: number): number {
    const els = rows.current?.querySelectorAll<HTMLElement>("[data-entry]") ?? [];
    let gap = 0;
    els.forEach((el, i) => {
      const r = el.getBoundingClientRect();
      if (y > r.top + r.height / 2) gap = i + 1;
    });
    return gap;
  }

  function onPointerDown(e: PointerEvent, i: number) {
    if (e.button !== 0) return;
    dragged.current = false;
    setDrag({ from: i, startY: e.clientY, gap: null });
  }

  function onPointerMove(e: PointerEvent) {
    if (!drag) return;
    if (drag.gap === null && Math.abs(e.clientY - drag.startY) < DRAG_START) return;
    if (drag.gap === null) (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    setDrag({ ...drag, gap: gapAt(e.clientY) });
  }

  function onPointerUp() {
    if (drag?.gap != null) {
      dragged.current = true;
      // Dropping into the gap below the song itself leaves it where it was.
      const to = drag.gap > drag.from ? drag.gap - 1 : drag.gap;
      if (to !== drag.from) api.moveInPlaylist(list.id, drag.from, to);
    }
    setDrag(null);
  }

  return (
    <section className="flex min-h-0 flex-col border-r border-line">
      <div className="mx-4 mt-3 mb-2 flex h-9 items-center gap-3">
        <h2 className="min-w-0 truncate text-base font-semibold">{list.name}</h2>
        <span className="whitespace-nowrap text-muted">
          {t("song_count", { n: list.entries.length })}
        </span>
        <div className="flex-1" />
        <button
          className="flex h-[30px] items-center gap-1.5 rounded-lg bg-accent px-3 font-semibold text-accent-ink hover:brightness-108 disabled:opacity-50"
          disabled={firstPlayable < 0}
          onClick={() => api.playEntry(list.id, firstPlayable)}
        >
          <Icon name="play" size={16} />
          <span>{t("playlist_play")}</span>
        </button>
      </div>
      <div
        ref={rows}
        className="flex-1 overflow-y-auto px-2 pb-3"
        role="listbox"
        aria-label={list.name}
        data-playlist={list.id}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={() => setDrag(null)}
      >
        {list.entries.length === 0 && (
          <p className="mx-6 mt-8 text-center text-muted">{t("playlist_empty")}</p>
        )}
        {list.entries.map((entry, i) => {
          const active = i === position;
          const gapLine =
            drag?.gap === i
              ? "shadow-[inset_0_2px_0_var(--accent)]"
              : drag?.gap === i + 1 && i === list.entries.length - 1
                ? "shadow-[inset_0_-2px_0_var(--accent)]"
                : "";
          return (
            <div
              key={i}
              data-entry={i}
              role="option"
              aria-selected={active}
              aria-disabled={entry.missing || undefined}
              tabIndex={0}
              title={entry.missing ? t("playlist_missing", { path: entry.path }) : entry.path}
              onPointerDown={(e) => onPointerDown(e, i)}
              onClick={() => {
                if (!dragged.current) activate(i);
              }}
              onKeyDown={(e) => {
                if (e.key === "Enter") activate(i);
                else if (e.key === "Delete" || e.key === "Backspace") {
                  e.stopPropagation();
                  api.removeFromPlaylist(list.id, [i]);
                }
              }}
              className={`grid w-full cursor-default grid-cols-[44px_minmax(0,1fr)_auto] items-center gap-2 rounded-lg px-2.5 py-[7px] text-left outline-none select-none focus-visible:ring-1 focus-visible:ring-accent-2 ${gapLine} ${
                drag?.gap != null && drag.from === i ? "opacity-50" : ""
              } ${
                active ? "bg-[color-mix(in_srgb,var(--accent)_14%,var(--panel))]" : "hover:bg-panel"
              }`}
            >
              <span className="num pr-1.5 text-right text-faint">
                {active && playing ? (
                  <span className="eq inline-flex h-3 items-end gap-0.5" aria-hidden="true">
                    <i />
                    <i />
                    <i />
                  </span>
                ) : (
                  i + 1
                )}
              </span>
              <span className={`flex min-w-0 flex-col ${entry.missing ? "text-faint" : ""}`}>
                <span className={`truncate ${active ? "font-semibold text-accent" : ""}`}>
                  {entry.title}
                </span>
                <span className="truncate text-xs text-muted">{entry.game}</span>
              </span>
              {entry.missing ? (
                <span className="rounded border border-line px-1.5 text-[11px] leading-5 text-muted">
                  {t("playlist_locate")}
                </span>
              ) : entry.length ? (
                <Duration t={t} length={entry.length} loopCount={library.loopCount} />
              ) : (
                <span className="num text-xs text-faint">—</span>
              )}
            </div>
          );
        })}
      </div>
    </section>
  );
});
