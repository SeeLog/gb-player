// The screen as big as the window, over everything under the title bar: the notes with a big
// keyboard, the waveform under them and the song's place, with the transport under the screen.
// Escape, F or its × put the window back.
import { useEffect } from "react";
import type { Translate } from "./format";
import type { Library } from "./types";
import Icon from "./Icon";
import Marquee from "./Marquee";
import Notes from "./Notes";
import { Progress, Transport, lcdScreen, useSong } from "./NowPlaying";
import Scope from "./Scope";
import Tip from "./Tip";
import { useTick } from "./tick";

export default function BigView({
  library,
  t,
  onClose,
}: {
  library: Library;
  t: Translate;
  onClose: () => void;
}) {
  const song = useSong(library);
  const romId = useTick((tick) => tick?.rom ?? null);
  const state = useTick((tick) => tick?.state ?? "none");
  const rom = library.roms.find((r) => r.id === romId);
  // Audio files have no channels: their waveform takes the screen.
  const channels = !rom?.folder;

  useEffect(() => {
    // Escape in an open menu or a question closes just that.
    const onKey = (e: KeyboardEvent) => {
      const inPopup =
        e.target instanceof Element &&
        e.target.closest('[data-base-ui-focusable], [role="menu"], [role="alertdialog"]');
      if (e.key === "Escape" && !inPopup) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    // Opaque, so nothing under it shows through the translucent theme colours.
    <div className="absolute inset-0 z-20 flex flex-col gap-4 p-5 [background:linear-gradient(var(--bg),var(--bg)),var(--backdrop)] animate-[lcd-in_180ms_ease-out] motion-reduce:animate-none">
      <div className={`${lcdScreen} flex min-h-0 min-w-0 flex-1 flex-col gap-2 px-5 pt-4 pb-4`}>
        <div className="flex items-center gap-3 text-sm font-bold">
          <span className="num opacity-80">
            {song ? `#${String(song.index + 1).padStart(2, "0")}` : "--"}
          </span>
          <span className="min-w-0 flex-1 truncate opacity-80">{rom?.game || rom?.title}</span>
          <span className="tracking-[0.08em] uppercase opacity-80">{t(`state_${state}`)}</span>
          <Tip text={`${t("big_view_close")} (Esc)`}>
            <button
              className="-my-1 -mr-1.5 grid size-8 place-items-center rounded-md opacity-80 hover:bg-lcd-ink/12 hover:opacity-100"
              onClick={onClose}
              aria-label={t("big_view_close")}
            >
              <Icon name="close" size={20} />
            </button>
          </Tip>
        </div>
        {/* A new element for each song, so a long name starts scrolling from its beginning. */}
        <div key={song ? `${romId}:${song.index}` : ""} className="text-3xl font-bold">
          <Marquee text={song?.title ?? t("not_playing")} />
        </div>
        {channels && (
          <div className="mt-1 min-h-0 flex-[4]">
            <Notes muted={library.muted} keyboard={36} frameWidth={4} />
          </div>
        )}
        <div className={channels ? "min-h-10 flex-1" : "mt-1 min-h-0 flex-1"}>
          <Scope volume={library.volume} />
        </div>
        <Progress library={library} t={t} lcd />
      </div>
      <Transport t={t} />
    </div>
  );
}
