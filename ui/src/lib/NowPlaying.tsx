// The right-hand panel: LCD with the oscilloscope, progress, transport, loop settings, volume and
// channels. It is split into small parts so that a tick only re-renders what it changes: the
// progress (position), the LCD and transport (song and state), and nothing else; the meters and
// the oscilloscope update from animation frames.
import { memo, useEffect, useRef, useState } from "react";
import { RESTART_AFTER, api, previous } from "./api";
import { describe, fmtTime, type Translate } from "./format";
import type { Length, Library, SongView } from "./types";
import Icon from "./Icon";
import Scope from "./Scope";
import { tickStore, useTick } from "./tick";

const CHANNELS = ["channel_pulse1", "channel_pulse2", "channel_wave", "channel_noise"];
const LOOP_COUNTS: (number | null)[] = [null, 1, 2, 3];

const label = "text-[11px] font-bold tracking-[0.08em] uppercase text-faint";
const round =
  "grid size-[42px] place-items-center rounded-full border border-line bg-panel-2 text-text hover:border-faint";

function useSong(library: Library): SongView | undefined {
  const current = useTick((tick) => tick?.current ?? null);
  return current === null ? undefined : library.songs.find((s) => s.index === current);
}

export default memo(function NowPlaying({ library, t }: { library: Library; t: Translate }) {
  const [volume, setVolume] = useState(library.volume);
  useEffect(() => setVolume(library.volume), [library.volume]);

  return (
    <aside className="flex flex-col gap-3.5 overflow-y-auto bg-panel px-[18px] py-3.5">
      <Lcd library={library} t={t} volume={volume} />
      <Progress library={library} t={t} />
      <Failure />
      <Transport t={t} />

      <div className="flex flex-col gap-2">
        <div className={label}>{t("loop_label")}</div>
        <div
          className="grid grid-cols-4 rounded-[9px] border border-line bg-panel-2 p-[3px]"
          role="radiogroup"
          aria-label={t("loop_label")}
        >
          {LOOP_COUNTS.map((n) => {
            const on = library.loopCount === n;
            return (
              <button
                key={String(n)}
                role="radio"
                aria-checked={on}
                className={`h-[30px] rounded-md font-semibold ${
                  on ? "bg-accent text-accent-ink" : "text-muted hover:text-text"
                }`}
                onClick={() => api.setLoopCount(n)}
                title={n === null ? t("loop_forever") : t("loop_playlist", { n })}
              >
                {n === null ? "∞" : `×${n}`}
              </button>
            );
          })}
        </div>
        <label className="group mt-1 flex cursor-pointer items-center gap-2.5 text-[13px] text-muted hover:text-text">
          <input
            type="checkbox"
            role="switch"
            checked={library.autoNext}
            onChange={(e) => api.setAutoNext(e.currentTarget.checked)}
            className="peer pointer-events-none absolute opacity-0"
          />
          <span className="relative h-5 w-[34px] flex-none rounded-[10px] border border-line bg-panel-2 transition-colors peer-checked:border-accent peer-checked:bg-accent peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-accent peer-checked:[&>span]:translate-x-3.5 peer-checked:[&>span]:bg-accent-ink">
            <span className="absolute top-0.5 left-0.5 size-3.5 rounded-full bg-muted transition-[transform,background-color]" />
          </span>
          <span>{t("auto_next")}</span>
        </label>
      </div>

      <div className="flex items-center gap-2.5 text-muted">
        <Icon name="volume" size={18} />
        <input
          type="range"
          min="0"
          max="1"
          step="0.01"
          value={volume}
          onChange={(e) => {
            const v = e.currentTarget.valueAsNumber;
            setVolume(v);
            api.setVolume(v);
          }}
          aria-label={t("volume")}
          title={t("volume")}
          className="volume-slider flex-1"
          style={{ "--v": `${volume * 100}%` } as React.CSSProperties}
        />
        <span className="num w-7 text-right text-xs">{Math.round(volume * 100)}</span>
      </div>

      <div className="flex flex-col gap-2">
        <div className={label}>{t("channels")}</div>
        <Channels library={library} t={t} />
      </div>
    </aside>
  );
});

function Lcd({ library, t, volume }: { library: Library; t: Translate; volume: number }) {
  const song = useSong(library);
  const state = useTick((tick) => tick?.state ?? "none");
  return (
    <div className="rounded-xl bg-[linear-gradient(160deg,var(--lcd-hi),var(--lcd)_60%,var(--lcd-lo))] px-3.5 pt-3 pb-2.5 text-lcd-ink shadow-[inset_0_0_0_1px_rgba(0,0,0,0.25),inset_0_2px_10px_rgba(0,0,0,0.25),0_0_0_6px_var(--bezel)]">
      <div className="flex justify-between text-xs font-bold opacity-80">
        <span className="num">{song ? `#${String(song.index + 1).padStart(2, "0")}` : "--"}</span>
        <span className="tracking-[0.08em] uppercase">{t(`state_${state}`)}</span>
      </div>
      <div className="mt-0.5 truncate text-lg font-bold" title={song?.title}>
        {song?.title ?? t("not_playing")}
      </div>
      <div className="mt-1.5 h-[72px]">
        <Scope volume={volume} />
      </div>
    </div>
  );
}

/** Where the playhead is on a bar that covers the intro and one pass of the loop. */
function progressOf(length: Length | null, position: number) {
  if (!length) return null;
  if (length.kind === "loops") {
    const total = length.intro + length.period;
    const inLoop = position >= length.intro;
    const p = inLoop ? length.intro + ((position - length.intro) % length.period) : position;
    return {
      frac: p / total,
      loopStart: length.intro / total,
      lap: inLoop ? Math.floor((position - length.intro) / length.period) + 1 : 0,
    };
  }
  if (length.kind === "ends") {
    return { frac: Math.min(position / length.at, 1), loopStart: null, lap: 0 };
  }
  return null;
}

/** The position (seconds) a point `frac` along the bar stands for: in the loop, the same lap as
 * now, so that seeking within a later lap keeps the loop count going. */
function positionAt(length: Length, frac: number, lap: number) {
  if (length.kind === "loops") {
    const p = frac * (length.intro + length.period);
    return p < length.intro || lap === 0 ? p : p + (lap - 1) * length.period;
  }
  return length.kind === "ends" ? frac * length.at : 0;
}

/** Seconds that the arrow keys move the playhead by. */
const SEEK_STEP = 5;

// Re-renders on every tick, so it is kept small.
function Progress({ library, t }: { library: Library; t: Translate }) {
  const song = useSong(library);
  const tickPosition = useTick((tick) => tick?.position ?? 0);
  const bar = useRef<HTMLDivElement>(null);
  // Where the playhead is being dragged to, and then sought to until a tick gets there.
  const [held, setHeld] = useState<number | null>(null);
  const dragging = useRef(false);
  const release = useRef(0);
  useEffect(() => {
    if (held !== null && !dragging.current && Math.abs(tickPosition - held) < 0.5) setHeld(null);
  }, [tickPosition, held]);
  useEffect(() => setHeld(null), [song?.index]);
  useEffect(() => () => clearTimeout(release.current), []);

  const position = held ?? tickPosition;
  const length = song?.length ?? null;
  const progress = song ? progressOf(length, position) : null;
  // The lap the song is in, not the one the playhead is dragged into.
  const lap = progressOf(length, tickPosition)?.lap ?? 0;

  const seek = (secs: number) => {
    setHeld(Math.max(secs, 0));
    api.seek(Math.max(secs, 0));
    // Let go of the playhead even if the song never gets there (it ended, or failed).
    clearTimeout(release.current);
    release.current = window.setTimeout(() => setHeld(null), 1000);
  };
  const pointAt = (e: React.PointerEvent) => {
    const rect = bar.current!.getBoundingClientRect();
    const frac = Math.min(Math.max((e.clientX - rect.left) / rect.width, 0), 1);
    return positionAt(length!, frac, lap);
  };

  return (
    <div>
      <div
        ref={bar}
        role={progress ? "slider" : undefined}
        tabIndex={progress ? 0 : undefined}
        aria-label={t("seek")}
        aria-valuemin={0}
        aria-valuenow={progress ? Math.round(position) : undefined}
        aria-valuetext={progress ? fmtTime(position) : undefined}
        className={`group relative h-2 rounded shadow-[inset_0_0_0_1px_var(--line)] outline-offset-4 focus-visible:outline-2 focus-visible:outline-accent ${
          song && !progress
            ? "bg-[repeating-linear-gradient(-45deg,var(--panel-2)_0_6px,var(--line)_6px_12px)]"
            : "bg-panel-2"
        } ${progress ? "cursor-pointer touch-none before:absolute before:-inset-y-2 before:inset-x-0" : ""}`}
        onPointerDown={(e) => {
          if (!progress || e.button !== 0) return;
          e.currentTarget.setPointerCapture(e.pointerId);
          dragging.current = true;
          clearTimeout(release.current);
          setHeld(pointAt(e));
        }}
        onPointerMove={(e) => {
          if (dragging.current) setHeld(pointAt(e));
        }}
        onPointerUp={(e) => {
          if (!dragging.current) return;
          dragging.current = false;
          seek(pointAt(e));
        }}
        onPointerCancel={() => {
          dragging.current = false;
          setHeld(null);
        }}
        onKeyDown={(e) => {
          if (!progress) return;
          const step = { ArrowLeft: -SEEK_STEP, ArrowRight: SEEK_STEP }[e.key];
          if (step === undefined && e.key !== "Home") return;
          // Not the previous / next song of the window's arrow keys.
          e.preventDefault();
          e.stopPropagation();
          seek(e.key === "Home" ? 0 : position + step!);
        }}
      >
        {progress && (
          <>
            {progress.loopStart !== null && (
              <>
                <div
                  className="absolute inset-y-0 left-0 rounded-l bg-overlay"
                  style={{ width: `${progress.loopStart * 100}%` }}
                />
                <div
                  className="absolute -top-1 -bottom-1 -ml-px w-0.5 rounded-[1px] bg-text opacity-60"
                  style={{ left: `${progress.loopStart * 100}%` }}
                  title={t("loop_point")}
                />
              </>
            )}
            <div
              className="absolute inset-y-0 left-0 rounded bg-accent"
              style={{ width: `${progress.frac * 100}%` }}
            />
            <div
              className="absolute top-1/2 -mt-[7px] -ml-[7px] size-3.5 rounded-full bg-text shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent)_40%,transparent)] transition-transform group-hover:scale-115"
              style={{ left: `${progress.frac * 100}%` }}
            />
          </>
        )}
      </div>
      <div className="mt-2 flex items-center gap-2 text-xs">
        <span className="num">{fmtTime(position)}</span>
        {progress && progress.lap > 0 ? (
          <span className="inline-flex items-center gap-1 rounded-[10px] bg-accent/18 px-[7px] py-px text-accent">
            <Icon name="repeat" size={13} />
            {t("loop_lap", { n: progress.lap })}
          </span>
        ) : (
          progress &&
          progress.loopStart !== null && (
            <span className="inline-flex items-center gap-1 rounded-[10px] bg-panel-2 px-[7px] py-px text-muted">
              {t("intro")}
            </span>
          )
        )}
        <span className="flex-1" />
        <span className="num text-muted">{song ? describe(t, length) : ""}</span>
      </div>
    </div>
  );
}

function Failure() {
  const failure = useTick((tick) => tick?.failure ?? null);
  return failure ? <div className="text-xs text-danger">{failure}</div> : null;
}

function Transport({ t }: { t: Translate }) {
  const playing = useTick((tick) => tick?.state === "playing");
  // Previous restarts the song once it has played a little (see `previous`).
  const restarts = useTick(
    (tick) => tick?.current != null && tick.position > RESTART_AFTER,
  );
  const prev = restarts ? t("prev_restart") : t("prev");
  return (
    <div className="flex items-center justify-center gap-3.5">
      <button className={round} onClick={() => previous(tickStore.get())} title={prev} aria-label={prev}>
        <Icon name="prev" />
      </button>
      <button className={round} onClick={() => api.stop()} title={t("stop")} aria-label={t("stop")}>
        <Icon name="stop" />
      </button>
      <button
        className="grid size-[62px] place-items-center rounded-full bg-accent text-accent-ink shadow-[0_6px_18px_color-mix(in_srgb,var(--accent)_25%,transparent)] hover:brightness-108"
        onClick={() => api.toggle()}
        title={`${playing ? t("pause") : t("play")} (Space)`}
        aria-label={playing ? t("pause") : t("play")}
      >
        <Icon name={playing ? "pause" : "play"} size={28} />
      </button>
      <button className={round} onClick={() => api.skip(1)} title={t("next")} aria-label={t("next")}>
        <Icon name="next" />
      </button>
    </div>
  );
}

// The meters are moved straight from animation frames (scaleX / translateX), with a slowly
// falling peak marker, so the channel buttons themselves render only when mute changes.
function Channels({ library, t }: { library: Library; t: Translate }) {
  const levels = useRef<(HTMLSpanElement | null)[]>([]);
  const peaks = useRef<(HTMLSpanElement | null)[]>([]);

  useEffect(() => {
    let frame = 0;
    let seen: unknown = undefined;
    const held = [0, 0, 0, 0];
    const update = () => {
      frame = requestAnimationFrame(update);
      const tick = tickStore.get();
      if (tick === seen) return;
      seen = tick;
      const now = tick?.levels ?? [0, 0, 0, 0];
      for (let ch = 0; ch < 4; ch++) {
        const level = Math.min(now[ch] ?? 0, 1);
        held[ch] = Math.max(level, held[ch] - 0.012);
        const bar = levels.current[ch];
        const peak = peaks.current[ch];
        if (bar) bar.style.transform = `scaleX(${level})`;
        if (peak) peak.style.transform = `translateX(${Math.min(held[ch], 1) * 100}%)`;
      }
    };
    frame = requestAnimationFrame(update);
    return () => cancelAnimationFrame(frame);
  }, []);

  return (
    <div className="grid grid-cols-2 gap-2">
      {CHANNELS.map((key, ch) => {
        const muted = library.muted[ch];
        return (
          <button
            key={key}
            className={`flex flex-col gap-2 rounded-[9px] border border-line bg-panel-2 px-2.5 py-[9px] text-left hover:border-faint ${
              muted ? "opacity-45" : ""
            }`}
            onClick={() => api.setMuted(ch, !muted)}
            title={t("toggle_mute")}
            aria-pressed={!muted}
          >
            <span className={`flex items-center gap-[7px] text-xs font-semibold ${muted ? "line-through" : ""}`}>
              <span
                className={`size-2 rounded-full ${
                  muted ? "bg-faint" : "bg-accent shadow-[0_0_6px_var(--accent)]"
                }`}
              />
              {t(key)}
            </span>
            {/* bg as it looks over the backdrop, whatever is under the meter. */}
            <span className="relative h-1.5 overflow-hidden rounded-[3px] [background:linear-gradient(var(--bg),var(--bg)),var(--backdrop)]">
              <span
                ref={(el) => {
                  levels.current[ch] = el;
                }}
                className="absolute inset-0 origin-left bg-[linear-gradient(90deg,var(--accent-2),var(--accent))] transition-transform duration-[60ms] ease-linear"
                style={{ transform: "scaleX(0)" }}
              />
              <span
                ref={(el) => {
                  peaks.current[ch] = el;
                }}
                className="absolute inset-0 before:absolute before:inset-y-0 before:left-0 before:w-0.5 before:bg-text before:opacity-70"
              />
            </span>
          </button>
        );
      })}
    </div>
  );
}
