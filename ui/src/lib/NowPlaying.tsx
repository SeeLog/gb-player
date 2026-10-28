// The right-hand panel: LCD with the oscilloscope, progress, transport, loop, shuffle and
// output settings, volume and channels. It is split into small parts so that a tick only re-renders what it changes: the
// progress (position), the LCD and transport (song and state), and nothing else; the meters and
// the oscilloscope update from animation frames.
import { memo, useEffect, useRef, useState, type ReactNode } from "react";
import { Radio } from "@base-ui/react/radio";
import { RadioGroup } from "@base-ui/react/radio-group";
import { Slider } from "@base-ui/react/slider";
import { Switch } from "@base-ui/react/switch";
import { RESTART_AFTER, api, previous } from "./api";
import { describe, fmtTime, type Translate } from "./format";
import type { Length, Library, Profile, Shuffle, SongView } from "./types";
import Icon from "./Icon";
import Marquee from "./Marquee";
import Scope from "./Scope";
import VolumeSlider from "./Slider";
import Tip from "./Tip";
import { tickStore, useTick } from "./tick";

const CHANNELS = ["channel_pulse1", "channel_pulse2", "channel_wave", "channel_noise"];
const LOOP_COUNTS: (number | null)[] = [null, 1, 2, 3];
const SHUFFLES: Shuffle[] = ["off", "list", "all"];
const PROFILES: Profile[] = ["line", "gbcSpeaker"];

const round =
  "grid size-[42px] place-items-center rounded-full border border-line bg-panel-2 text-text hover:border-faint";

export function useSong(library: Library): SongView | undefined {
  const rom = useTick((tick) => tick?.rom ?? null);
  const current = useTick((tick) => tick?.current ?? null);
  if (current === null) return undefined;
  return library.roms.find((r) => r.id === rom)?.songs.find((s) => s.index === current);
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

      {/* Each setting is a row led by its icon, with what goes with it right under it. */}
      <div className="flex flex-col gap-2.5">
        <Row icon="repeat" label={t("loop_label")}>
          <Segmented
            label={t("loop_label")}
            options={LOOP_COUNTS.map((n) => ({
              value: n,
              text: n === null ? "∞" : `×${n}`,
              title: n === null ? t("loop_forever") : t("loop_playlist", { n }),
            }))}
            value={library.loopCount}
            onChange={(n) => api.setLoopCount(n)}
          />
          <Toggle checked={library.autoNext} onChange={(on) => api.setAutoNext(on)}>
            {t("auto_next")}
          </Toggle>
        </Row>

        <Row icon="shuffle" label={t("shuffle")}>
          <Segmented
            label={t("shuffle")}
            options={SHUFFLES.map((mode) => ({
              value: mode,
              text: t(`shuffle_${mode}`),
              title: t(`shuffle_${mode}_hint`),
            }))}
            value={library.shuffle}
            onChange={(mode) => api.setShuffle(mode)}
            small
          />
        </Row>

        <Row icon="speaker" label={t("output")}>
          <Segmented
            label={t("output")}
            options={PROFILES.map((profile) => ({
              value: profile,
              text: t(`output_${profile}`),
              title: t(`output_${profile}_hint`),
            }))}
            value={library.profile}
            onChange={(profile) => api.setProfile(profile, library.speakerNoise)}
            small
          />
          {library.profile !== "line" && (
            <Toggle
              checked={library.speakerNoise}
              onChange={(on) => api.setProfile(library.profile, on)}
            >
              {t("speaker_noise")}
            </Toggle>
          )}
        </Row>

        <Row icon="volume" label={t("volume")} center>
          <div className="flex items-center gap-2.5">
            <VolumeSlider
              value={volume}
              onChange={(v) => {
                setVolume(v);
                api.setVolume(v);
              }}
              label={t("volume")}
              className="flex-1"
            />
            <span className="num w-7 text-right text-xs text-muted">
              {Math.round(volume * 100)}
            </span>
          </div>
        </Row>
      </div>

      <GameChannels library={library} t={t} />
    </aside>
  );
});

function Lcd({ library, t, volume }: { library: Library; t: Translate; volume: number }) {
  const song = useSong(library);
  const rom = useTick((tick) => tick?.rom ?? null);
  const state = useTick((tick) => tick?.state ?? "none");
  return (
    <div className="rounded-xl bg-[linear-gradient(160deg,var(--lcd-hi),var(--lcd)_60%,var(--lcd-lo))] px-3.5 pt-3 pb-2.5 text-lcd-ink shadow-[inset_0_0_0_1px_var(--lcd-shadow),inset_0_2px_10px_var(--lcd-shadow),0_0_0_6px_var(--bezel)]">
      <div className="flex justify-between text-xs font-bold opacity-80">
        <span className="num">{song ? `#${String(song.index + 1).padStart(2, "0")}` : "--"}</span>
        <span className="tracking-[0.08em] uppercase">{t(`state_${state}`)}</span>
      </div>
      {/* A new element for each song, so the name comes in afresh even mid-way through the last,
          and a long one starts scrolling from its beginning. */}
      <div
        key={song ? `${rom}:${song.index}` : ""}
        className="mt-0.5 animate-[lcd-in_180ms_ease-out] text-lg font-bold motion-reduce:animate-none"
      >
        <Marquee text={song?.title ?? t("not_playing")} />
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
/** How long the playhead waits for a seek to get there, in milliseconds. */
const SEEK_GIVE_UP = 10_000;

/** The bar's look: on the panel, or drawn in LCD ink on the mini player's screen. */
const BAR = {
  panel: {
    track: "h-2 rounded bg-panel-2 shadow-[inset_0_0_0_1px_var(--line)]",
    intro: "rounded-l bg-overlay",
    mark: "-top-1 -bottom-1 rounded-[1px] bg-text opacity-60",
    fill: "rounded bg-accent",
    thumb:
      "size-3.5 rounded-full bg-knob shadow-[0_0_0_3px_color-mix(in_srgb,var(--accent-edge,var(--accent))_40%,transparent),0_1px_3px_rgba(0,0,0,0.3)]",
    empty: "bg-panel-2",
    analysing: "bg-[repeating-linear-gradient(-45deg,var(--panel-2)_0_6px,var(--line)_6px_12px)]",
    lap: "gap-1 rounded-[10px] bg-accent/18 px-[7px] py-px text-accent",
  },
  lcd: {
    track: "h-1.5 bg-lcd-ink/15",
    intro: "bg-lcd-ink/10",
    mark: "-top-0.5 -bottom-0.5 bg-lcd-ink",
    fill: "bg-lcd-ink",
    thumb: "size-2.5 bg-lcd-ink",
    empty: "bg-lcd-ink/15",
    analysing:
      "bg-[repeating-linear-gradient(-45deg,transparent_0_4px,color-mix(in_srgb,var(--lcd-ink)_20%,transparent)_4px_8px)]",
    lap: "gap-0.5 font-bold",
  },
};

// Re-renders on every tick, so it is kept small. `lcd` draws it in one line on the mini
// player's screen, the time beside the bar.
export function Progress({
  library,
  t,
  lcd = false,
}: {
  library: Library;
  t: Translate;
  lcd?: boolean;
}) {
  const song = useSong(library);
  const tickPosition = useTick((tick) => tick?.position ?? 0);
  const ended = useTick((tick) => tick?.state !== "playing" && tick?.state !== "paused");
  // Where the playhead is being dragged to, and then sought to until a tick gets there. A seek
  // far into a song takes a while, and the song keeps playing from where it was meanwhile.
  const [held, setHeld] = useState<number | null>(null);
  const dragging = useRef(false);
  const release = useRef(0);
  useEffect(() => {
    if (held !== null && !dragging.current && Math.abs(tickPosition - held) < 0.5) setHeld(null);
  }, [tickPosition, held]);
  useEffect(() => setHeld(null), [song?.index]);
  // A song that ends or fails never gets there.
  useEffect(() => {
    if (ended && !dragging.current) setHeld(null);
  }, [ended]);
  useEffect(() => () => clearTimeout(release.current), []);

  const position = held ?? tickPosition;
  const length = song?.length ?? null;
  const progress = song ? progressOf(length, position) : null;
  // The lap the song is in, not the one the playhead is dragged into.
  const lap = progressOf(length, tickPosition)?.lap ?? 0;

  const seek = (secs: number) => {
    setHeld(Math.max(secs, 0));
    api.seek(Math.max(secs, 0));
    // Let go of the playhead in the end, whatever happens.
    clearTimeout(release.current);
    release.current = window.setTimeout(() => setHeld(null), SEEK_GIVE_UP);
  };
  const at = (frac: number) => positionAt(length!, frac, lap);

  const bar = lcd ? BAR.lcd : BAR.panel;
  // The bar's shading shows the intro, so only the laps after it are counted.
  const lapBadge = progress && progress.lap > 0 && (
    <span className={`inline-flex items-center whitespace-nowrap ${bar.lap}`}>
      <Icon name="repeat" size={lcd ? 11 : 13} />
      {t("loop_lap", { n: progress.lap })}
    </span>
  );
  // What the end of the bar stands for, with the intro and loop lengths on hover.
  const end =
    length?.kind === "loops"
      ? fmtTime(length.intro + length.period)
      : length?.kind === "ends"
        ? fmtTime(length.at)
        : null;

  return (
    <div className={lcd ? "flex items-center gap-2 text-[10px]" : undefined}>
      {lcd && <span className="num flex-none font-bold">{fmtTime(position)}</span>}
      {progress ? (
        <Slider.Root
          className={lcd ? "min-w-0 flex-1" : undefined}
          value={progress.frac}
          min={0}
          max={1}
          step={0.0001}
          onValueChange={(frac) => {
            dragging.current = true;
            clearTimeout(release.current);
            setHeld(at(frac as number));
          }}
          onValueCommitted={(frac) => {
            dragging.current = false;
            seek(at(frac as number));
          }}
        >
          {/* The control is taller than the bar, so it is easy to grab. */}
          <Slider.Control className="group -my-2 flex cursor-pointer touch-none items-center py-2 select-none">
            <Slider.Track className={`w-full ${bar.track}`}>
              {progress.loopStart !== null && (
                <>
                  <div
                    className={`absolute inset-y-0 left-0 ${bar.intro}`}
                    style={{ width: `${progress.loopStart * 100}%` }}
                  />
                  <Tip text={t("loop_point")}>
                    <div
                      className={`absolute -ml-px w-0.5 ${bar.mark}`}
                      style={{ left: `${progress.loopStart * 100}%` }}
                    />
                  </Tip>
                </>
              )}
              <Slider.Indicator className={bar.fill} />
              <Slider.Thumb
                aria-label={t("seek")}
                getAriaValueText={() => fmtTime(position)}
                // Before the slider's own keys: seconds at a time, not the bar's own steps.
                onKeyDownCapture={(e) => {
                  const step = { ArrowLeft: -SEEK_STEP, ArrowRight: SEEK_STEP }[e.key];
                  if (step === undefined && e.key !== "Home") return;
                  e.stopPropagation();
                  e.preventDefault();
                  seek(e.key === "Home" ? 0 : position + step!);
                }}
                className={`${bar.thumb} transition-transform group-hover:scale-115 has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-accent motion-reduce:transition-none`}
              />
            </Slider.Track>
          </Slider.Control>
        </Slider.Root>
      ) : (
        // No song, or one whose length is not known yet (striped): nothing to seek in.
        <div className={`${bar.track} ${lcd ? "flex-1" : ""} ${song ? bar.analysing : bar.empty}`} />
      )}
      {lcd ? (
        lapBadge
      ) : (
        <div className="mt-2 flex items-center gap-2 text-xs">
          <span className="num">{fmtTime(position)}</span>
          {lapBadge}
          <span className="flex-1" />
          {song && end ? (
            <Tip text={length?.kind === "loops" ? describe(t, length) : undefined}>
              <span className="num text-muted">{end}</span>
            </Tip>
          ) : (
            <span className="text-muted">{song ? describe(t, length) : ""}</span>
          )}
        </div>
      )}
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
      <Tip text={prev}>
        <button className={round} onClick={() => previous(tickStore.get())} aria-label={prev}>
          <Icon name="prev" />
        </button>
      </Tip>
      <Tip text={t("stop")}>
        <button className={round} onClick={() => api.stop()} aria-label={t("stop")}>
          <Icon name="stop" />
        </button>
      </Tip>
      <Tip text={`${playing ? t("pause") : t("play")} (Space)`}>
        <button
          className="grid size-[62px] place-items-center rounded-full bg-accent text-accent-ink shadow-[0_6px_18px_color-mix(in_srgb,var(--accent)_25%,transparent)] hover:brightness-108"
          onClick={() => api.toggle()}
          aria-label={playing ? t("pause") : t("play")}
        >
          <Icon name={playing ? "pause" : "play"} size={28} />
        </button>
      </Tip>
      <Tip text={t("next")}>
        <button className={round} onClick={() => api.skip(1)} aria-label={t("next")}>
          <Icon name="next" />
        </button>
      </Tip>
    </div>
  );
}

/** The APU's channels, while a game is in the output: audio files have none. */
function GameChannels({ library, t }: { library: Library; t: Translate }) {
  const rom = useTick((tick) => tick?.rom ?? null);
  if (library.roms.find((r) => r.id === rom)?.folder) return null;
  return <Channels library={library} t={t} />;
}

// The meters run from accent-2 to accent, or between --meter-start and --meter-end where a
// palette or theme sets them (clear: pale mint to emerald, its accent being for text). The tip shows the
// end colour, so that is the stronger one. They are moved straight from animation frames (scaleX / translateX), with a slowly
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

  // Kept here while a slider is dragged, as the whole volume is (the Rust side does not send
  // the library back for it).
  const [volumes, setVolumes] = useState(library.channelVolumes);
  useEffect(() => setVolumes(library.channelVolumes), [library.channelVolumes]);

  return (
    <div className="grid grid-cols-2 gap-2">
      {CHANNELS.map((key, ch) => {
        const muted = library.muted[ch];
        return (
          <div
            key={key}
            className={`flex flex-col gap-1 rounded-[9px] border border-line bg-panel-2 px-2.5 pt-[9px] pb-1 hover:border-faint ${
              muted ? "opacity-45" : ""
            }`}
          >
            <Tip text={muted ? t("unmute") : t("mute")}>
              <button
                className="flex flex-col gap-2 text-left"
                onClick={() => api.setMuted(ch, !muted)}
                aria-pressed={!muted}
              >
                <span className="flex items-center gap-[7px] text-xs font-semibold">
                  <span
                    className={`size-2 rounded-full ${
                      muted ? "bg-faint" : "bg-accent shadow-[0_0_6px_var(--accent-edge,var(--accent))]"
                    }`}
                  />
                  <span className={`flex-1 ${muted ? "line-through" : ""}`}>{t(key)}</span>
                </span>
                {/* bg as it looks over the backdrop, whatever is under the meter. */}
                <span className="relative h-1.5 overflow-hidden rounded-[3px] [background:linear-gradient(var(--bg),var(--bg)),var(--backdrop)]">
                  <span
                    ref={(el) => {
                      levels.current[ch] = el;
                    }}
                    className="absolute inset-0 origin-left bg-[linear-gradient(90deg,var(--meter-start,var(--accent-2)),var(--meter-end,var(--accent)))] transition-transform duration-[60ms] ease-linear"
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
            </Tip>
            <VolumeSlider
              value={volumes[ch]}
              onChange={(v) => {
                setVolumes((all) => all.map((old, i) => (i === ch ? v : old)) as typeof all);
                api.setChannelVolume(ch, v);
              }}
              label={t("channel_volume", { name: t(key) })}
              small
            />
          </div>
        );
      })}
    </div>
  );
}

// A setting's icon (named by its tip), and the setting beside it. The icon is level with the first
// control, a segmented one unless `center`.
function Row({
  icon,
  label,
  center = false,
  children,
}: {
  icon: string;
  label: string;
  center?: boolean;
  children: ReactNode;
}) {
  return (
    <div className={`flex gap-2.5 ${center ? "items-center" : "items-start"}`}>
      <Tip text={label}>
        <span className={`grid w-[18px] flex-none place-items-center text-muted ${center ? "" : "h-9"}`}>
          <Icon name={icon} size={18} />
        </span>
      </Tip>
      <div className="flex min-w-0 flex-1 flex-col gap-2">{children}</div>
    </div>
  );
}

// A switch with its label beside it.
function Toggle({
  checked,
  onChange,
  children,
}: {
  checked: boolean;
  onChange: (on: boolean) => void;
  children: ReactNode;
}) {
  return (
    <label className="mt-1 flex cursor-pointer items-center gap-2.5 text-[13px] text-muted hover:text-text">
      <Switch.Root
        checked={checked}
        onCheckedChange={onChange}
        className="flex h-5 w-[34px] flex-none rounded-[10px] border border-line bg-panel-2 p-[2px] transition-colors data-checked:border-[var(--accent-edge,var(--accent))] data-checked:bg-accent focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent motion-reduce:transition-none"
      >
        <Switch.Thumb className="size-3.5 rounded-full bg-muted transition-[translate,background-color] data-checked:translate-x-3.5 data-checked:bg-[var(--knob-on,var(--accent-ink))] data-checked:shadow-[0_1px_2px_rgba(0,0,0,0.3)] motion-reduce:transition-none" />
      </Switch.Root>
      <span>{children}</span>
    </label>
  );
}

// A row of choices with one picked, whose highlight slides over to the new one: the edge on the
// side it moves to goes first and the other follows, so it stretches on the way and settles.
function Segmented<T>({
  label,
  options,
  value,
  onChange,
  small = false,
}: {
  label: string;
  options: { value: T; text: string; title: string }[];
  value: T;
  onChange: (value: T) => void;
  small?: boolean;
}) {
  const n = options.length;
  const index = options.findIndex((o) => o.value === value);
  // Which way the highlight last moved: 1 to the right, -1 to the left.
  const [last, setLast] = useState(index);
  const [direction, setDirection] = useState(1);
  if (index !== last) {
    setLast(index);
    if (index >= 0 && last >= 0) setDirection(Math.sign(index - last));
  }
  const lead = "150ms cubic-bezier(0.3, 1.25, 0.5, 1)";
  const follow = "230ms cubic-bezier(0.5, 0, 0.2, 1) 20ms";

  return (
    // The options go by their place in the row, as a value may be null.
    <RadioGroup
      value={index}
      onValueChange={(i) => onChange(options[i as number].value)}
      className="relative grid rounded-[9px] border border-line bg-panel-2 p-[3px]"
      style={{ gridTemplateColumns: `repeat(${n}, minmax(0, 1fr))` }}
      aria-label={label}
    >
      {index >= 0 && (
        <div className="pointer-events-none absolute inset-[3px]" aria-hidden="true">
          <div
            className="absolute inset-y-0 rounded-md bg-accent motion-reduce:transition-none!"
            style={{
              left: `${(index / n) * 100}%`,
              right: `${((n - 1 - index) / n) * 100}%`,
              transition:
                direction > 0 ? `right ${lead}, left ${follow}` : `left ${lead}, right ${follow}`,
            }}
          />
        </div>
      )}
      {options.map((o, i) => (
        <Tip key={String(o.value)} text={o.title}>
          <Radio.Root
            value={i}
            className={`relative grid h-[30px] cursor-pointer place-items-center rounded-md font-semibold transition-colors duration-150 outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent ${
              small ? "text-[13px]" : ""
            } ${i === index ? "text-accent-ink" : "text-muted hover:text-text"}`}
          >
            {o.text}
          </Radio.Root>
        </Tip>
      ))}
    </RadioGroup>
  );
}
