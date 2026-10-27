// The mini player: the whole window drawn as a handheld console, with the song, the oscilloscope
// and the progress on its screen. The cross pad goes back and forward (left, right) and sets the
// volume (up, down), A plays or pauses, B stops, SELECT changes the shuffle and START goes back to
// the full window. The colours of the body come from the theme (shell, shell-ink, shell-frame, key,
// key-ink). A see-through shell colour shows the insides, as clear plastic does: the board fills
// most of the body, and only around the rim does the desktop show through (the window is
// transparent). The color palette is drawn as the later colour model, the
// others as the first model, unless the theme says which. The body has the console's outline:
// the desktop shows around it.
// The Rust side shapes the window (gui/mini.rs). Anywhere but the controls drags the window.
import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { RESTART_AFTER, api, previous } from "./api";
import type { Translate } from "./format";
import type { Console, Library, Shuffle } from "./types";
import Icon from "./Icon";
import Marquee from "./Marquee";
import { Progress, useSong } from "./NowPlaying";
import Scope from "./Scope";
import { contrast, paletteColors, splitColor } from "./theme";
import Tip from "./Tip";
import { tickStore, useTick } from "./tick";

const SHUFFLES: Shuffle[] = ["off", "list", "all"];
/** How much a press on the cross pad changes the volume. */
const VOLUME_STEP = 0.1;
/** How long a message stays on the screen, in milliseconds. */
const FLASH_FOR = 1500;

// A colour made from the theme's: the cross pad and the START / SELECT keys.
const body = {
  "--pad": "color-mix(in srgb, var(--shell-ink) 15%, #202026)",
} as CSSProperties;

/** Which console the body is drawn as (the theme says): the first model, or the later colour
 * one. They differ in the frame and its logo, the power lamp, the tilt of the buttons and the
 * speaker. */
type Model = Console;

export default function MiniPlayer({ library, t }: { library: Library; t: Translate }) {
  const theme = library.themes.find((th) => th.id === library.theme);
  const model: Model = theme?.console ?? "dmg";
  // Whether the shell is see-through, so the insides show, and the colour of the logo on the
  // colour model's dark frame: the accent, or the second one when the accent is too dark to show.
  const { clear, logo } = useMemo(() => {
    if (!theme) return { clear: false, logo: "var(--accent)" };
    const palette = paletteColors(theme.base);
    const color = (token: string) => theme.colors[token] ?? palette[token];
    return {
      clear: splitColor(color("shell")).alpha < 1,
      logo:
        contrast(color("accent"), color("shell-frame")) >= 3 ? "var(--accent)" : "var(--accent-2)",
    };
  }, [theme]);
  // Kept here as in NowPlaying: the Rust side does not send the library back for the volume.
  const [volume, setVolume] = useState(library.volume);
  useEffect(() => setVolume(library.volume), [library.volume]);

  // A message shown on the screen for a moment in place of the play state.
  const [flash, setFlash] = useState<string | null>(null);
  const flashTimer = useRef(0);
  const say = (text: string) => {
    setFlash(text);
    clearTimeout(flashTimer.current);
    flashTimer.current = window.setTimeout(() => setFlash(null), FLASH_FOR);
  };
  useEffect(() => () => clearTimeout(flashTimer.current), []);

  const changeVolume = (delta: number) => {
    const v = Math.round(Math.min(1, Math.max(0, volume + delta)) * 100) / 100;
    setVolume(v);
    api.setVolume(v);
    say(`${t("volume")} ${Math.round(v * 100)}`);
  };
  const nextShuffle = () => {
    const mode = SHUFFLES[(SHUFFLES.indexOf(library.shuffle) + 1) % SHUFFLES.length];
    api.setShuffle(mode);
    say(`${t("shuffle")}: ${t(`shuffle_${mode}`)}`);
  };

  return (
    <div
      data-tauri-drag-region="deep"
      style={body}
      // The light halo keeps the print readable over whatever shows through a clear shell.
      className={`relative h-full overflow-hidden text-shell-ink [text-shadow:0_0_3px_rgba(255,255,255,0.7)] ${
        model === "dmg" ? "rounded-[14px_14px_56px_14px]" : "rounded-[18px_18px_44px_44px]"
      }`}
    >
      {clear && <Insides model={model} />}
      {/* The shell, over the insides, with a lit edge so a clear one still has an outline. */}
      <div
        className="absolute inset-0 rounded-[inherit] bg-[linear-gradient(165deg,color-mix(in_srgb,var(--shell)_88%,white),var(--shell)_35%,color-mix(in_srgb,var(--shell)_92%,black))] shadow-[inset_0_0_0_1px_rgba(255,255,255,0.5),inset_0_0_0_2px_rgba(0,0,0,0.15)]"
        aria-hidden="true"
      />
      <div className="relative flex h-full flex-col px-4 pt-1 pb-3">
        <TopEdge library={library} t={t} model={model} />
        <Screen
          library={library}
          t={t}
          volume={volume}
          flash={flash}
          model={model}
          logo={logo}
        />
        {model === "dmg" && (
          <div className="mt-2 pl-1 text-[17px] font-bold tracking-tight italic">GB Player</div>
        )}
        <div className="relative flex-1">
          <CrossPad
            t={t}
            model={model}
            clear={clear}
            onUp={() => changeVolume(VOLUME_STEP)}
            onDown={() => changeVolume(-VOLUME_STEP)}
          />
          <AB t={t} model={model} clear={clear} />
          <div className="absolute bottom-6 left-1/2 flex -translate-x-1/2 gap-3">
            <Pill
              label="SELECT"
              title={`${t("shuffle")}: ${t(`shuffle_${library.shuffle}`)}`}
              onClick={nextShuffle}
              model={model}
              clear={clear}
            />
            <Pill
              label="START"
              title={`${t("mini_exit")} (Ctrl+M)`}
              onClick={() => api.setMini(false)}
              model={model}
              clear={clear}
            />
          </div>
          <Speaker model={model} />
        </div>
      </div>
    </div>
  );
}

// The rubber under the keys, seen through a clear shell of the colour model: the shell's colour
// is laid over it, so a tinted shell tints it (faint under clear purple, mint under clear).
const RUBBER =
  "[background:linear-gradient(var(--shell),var(--shell)),#86d3c5] shadow-[inset_0_1px_3px_rgba(0,0,0,0.2)]";
const RUBBER_SMALL =
  "[background:linear-gradient(var(--shell),var(--shell)),#9fb6c4] shadow-[inset_0_1px_2px_rgba(0,0,0,0.25)]";

/** What is inside the body, drawn only under a see-through shell: the board and the frame that
 * hold it, which fill the body all but a rim about 10px wide (where the desktop shows through), a
 * ribbon cable, screw posts and the speaker's magnet. */
function Insides({ model }: { model: Model }) {
  return (
    <div className="pointer-events-none absolute inset-0" aria-hidden="true">
      {/* The same outline as the body, 10px in. */}
      <div
        className={`absolute inset-2.5 bg-[#d3d8da] ${
          model === "dmg" ? "rounded-[6px_6px_46px_6px]" : "rounded-[10px_10px_34px_34px]"
        }`}
      />
      <svg className="absolute inset-0 size-full" viewBox="0 0 288 468" preserveAspectRatio="none">
        <g fill="#b6b1c0">
          <rect x="120" y="402" width="5" height="52" />
          <rect x="129" y="402" width="5" height="52" />
          <rect x="150" y="402" width="5" height="52" />
          <rect x="159" y="402" width="5" height="52" />
        </g>
        <g fill="none" stroke="#a6a1b2" strokeWidth="3">
          <circle cx="24" cy="352" r="6" />
          <circle cx="264" cy="352" r="6" />
          <circle cx="144" cy="312" r="6" />
          <circle cx="40" cy="440" r="5" />
        </g>
        {/* Right under the speaker's grille (see Speaker: 60px across, 4px in from the right and
            at the bottom of the controls, which are 16px and 12px in from the edges). */}
        <circle cx="238" cy="426" r="24" fill="#cbb47f" />
        <circle cx="238" cy="426" r="11" fill="#a99360" />
      </svg>
    </div>
  );
}

/** The top of the body: the power switch's marks, and the window's own buttons. */
function TopEdge({ library, t, model }: { library: Library; t: Translate; model: Model }) {
  const onTop = library.miniOnTop;
  const button =
    "grid size-6 place-items-center rounded-md drop-shadow-[0_0_2px_rgba(255,255,255,0.8)] hover:bg-black/10";
  return (
    <div className="flex h-6 flex-none items-center">
      {model === "dmg" && (
        <span className="text-[8px] font-bold tracking-[0.12em] opacity-45">◁OFF · ON▷</span>
      )}
      <span className="flex-1" />
      <Tip text={t("always_on_top")}>
        <button
          className={`${button} ${onTop ? "" : "opacity-40"}`}
          onClick={() => api.setMiniOnTop(!onTop)}
          aria-label={t("always_on_top")}
          aria-pressed={onTop}
        >
          <Icon name="pin" size={14} />
        </button>
      </Tip>
      <Tip text={t("window_close")}>
        <button
          className={`${button} opacity-60 hover:opacity-100`}
          onClick={() => getCurrentWindow().close()}
          aria-label={t("window_close")}
        >
          <Icon name="close" size={14} />
        </button>
      </Tip>
    </div>
  );
}

function Screen({
  library,
  t,
  volume,
  flash,
  model,
  logo,
}: {
  library: Library;
  t: Translate;
  volume: number;
  flash: string | null;
  model: Model;
  /** The colour of "Player" in the colour model's logo. */
  logo: string;
}) {
  const song = useSong(library);
  const rom = useTick((tick) => tick?.rom ?? null);
  const state = useTick((tick) => tick?.state ?? "none");
  const game = library.roms.find((r) => r.id === rom)?.game;
  const playing = state === "playing";
  const dmg = model === "dmg";
  return (
    // A thick frame as on the console: the screen takes two thirds of its width. On the first
    // model the margin below is the deepest, under the big rounded corner; on the colour one the
    // frame is rounded at both bottom corners, with the logo on it.
    <div
      className={`flex-none bg-shell-frame shadow-[inset_0_1px_3px_rgba(0,0,0,0.35)] [text-shadow:none] ${
        dmg ? "rounded-[10px_10px_44px_10px] pt-3 pb-9" : "rounded-[10px_10px_30px_30px] pt-5 pb-2"
      }`}
    >
      {dmg && (
        <div className="mb-3.5 flex items-center gap-1.5 px-3 text-[7px] font-bold tracking-[0.16em] text-shell-ink/75">
          <span className="flex flex-1 flex-col gap-0.5">
            <span className="h-0.5 bg-key" />
            <span className="h-0.5 bg-shell-ink/70" />
          </span>
          <span>GB PLAYER</span>
          <span className="flex w-6 flex-col gap-0.5">
            <span className="h-0.5 bg-key" />
            <span className="h-0.5 bg-shell-ink/70" />
          </span>
        </div>
      )}
      {/* The screen in the middle of the frame, with the same margin on both sides. */}
      <div className="relative flex px-[42px]">
        {/* Lit while a song plays: in the middle of the left margin, near the top of the
            screen, as on the console. */}
        <div
          className={`absolute top-[18%] left-0 flex w-[42px] flex-col items-center gap-1 text-[6px] tracking-[0.08em] ${
            dmg ? "text-shell-ink/75" : "text-white/55"
          }`}
          aria-hidden="true"
        >
          <span className="flex items-center gap-0.5">
            <span
              className={`size-2 rounded-full transition-colors ${
                playing ? "bg-[#ff3b30] shadow-[0_0_6px_#ff3b30]" : "bg-black/45"
              }`}
            />
            {/* The colour model's lamp has waves beside it. */}
            {!dmg && (
              <svg
                viewBox="0 0 14 8"
                className="h-2 w-3.5 fill-none stroke-current"
                strokeWidth="1"
                strokeLinecap="round"
              >
                <path d="M2 1.5a3.5 3.5 0 0 1 0 5M6 1a4.5 4.5 0 0 1 0 6M10 .5a5.5 5.5 0 0 1 0 7" />
              </svg>
            )}
          </span>
          <span>POWER</span>
        </div>
        <div className="flex h-[156px] min-w-0 flex-1 flex-col gap-1 [text-shadow:none] bg-[linear-gradient(160deg,var(--lcd-hi),var(--lcd)_60%,var(--lcd-lo))] px-2 pt-1 pb-1.5 text-lcd-ink shadow-[inset_0_0_0_1px_rgba(0,0,0,0.3),inset_0_2px_6px_rgba(0,0,0,0.25)]">
          <div className="flex justify-between gap-2 text-[10px] font-bold opacity-80">
            <span className="num">{song ? `#${String(song.index + 1).padStart(2, "0")}` : "--"}</span>
            <span className="truncate tracking-[0.06em] uppercase">
              {flash ?? t(`state_${state}`)}
            </span>
          </div>
          {/* A new element for each song, so the name comes in afresh, and a long one starts
              scrolling from its beginning. */}
          <Tip
            key={song ? `${rom}:${song.index}` : ""}
            text={song && [song.title, game].filter(Boolean).join("\n")}
          >
            <div className="animate-[lcd-in_180ms_ease-out] text-[13px] font-bold motion-reduce:animate-none">
              <Marquee text={song?.title ?? t("not_playing")} />
            </div>
          </Tip>
          <div className="min-h-0 flex-1">
            <Scope volume={volume} />
          </div>
          {/* Clicks on the bar seek rather than drag the window. */}
          <div data-tauri-drag-region="false">
            <Progress library={library} t={t} lcd />
          </div>
        </div>
      </div>
      {!dmg && (
        <div className="mt-2.5 text-center text-[16px] font-bold tracking-tight">
          <span className="text-white/75">GB </span>
          {/* In the sheen where the palette has one (Clear). */}
          <span
            className="[background-image:var(--accent-sheen,none)] bg-clip-text [-webkit-text-fill-color:var(--sheen-text,currentColor)]"
            style={{ color: logo }}
          >
            Player
          </span>
        </div>
      )}
    </div>
  );
}

/** Left and right go back and forward, up and down set the volume. */
function CrossPad({
  t,
  model,
  clear,
  onUp,
  onDown,
}: {
  t: Translate;
  model: Model;
  clear: boolean;
  onUp: () => void;
  onDown: () => void;
}) {
  const restarts = useTick((tick) => tick?.current != null && tick.position > RESTART_AFTER);
  const prev = restarts ? t("prev_restart") : t("prev");
  const arm =
    "absolute grid size-[25px] place-items-center text-white/35 hover:text-white/70 active:bg-black/25";
  return (
    // On the colour model the pad's middle is a quarter of the way across, level with A and B.
    <div
      className={`absolute size-[75px] ${model === "dmg" ? "top-3 left-1" : "top-6 left-4"}`}
    >
      {/* On the colour model the pad sits in a dark round part a little wider than the cross,
          with a cross-shaped opening for it, and that part sits in a round hollow on rubber that
          a clear shell shows (about 1.4 times the cross). */}
      {model === "gbc" && (
        <>
          <div
            className={`absolute -inset-[15px] rounded-full ${
              clear ? RUBBER : "bg-black/10 shadow-[inset_0_1px_3px_rgba(0,0,0,0.3)]"
            }`}
          />
          <div className="absolute -inset-[5px] rounded-full bg-[color-mix(in_srgb,var(--pad)_75%,#6a7090)] shadow-[inset_0_1px_2px_rgba(255,255,255,0.15),0_1px_2px_rgba(0,0,0,0.35)]" />
          <div className="absolute -inset-x-px top-6 h-[27px] rounded-[5px] bg-black/45" />
          <div className="absolute -inset-y-px left-6 w-[27px] rounded-[5px] bg-black/45" />
        </>
      )}
      <div className="absolute inset-x-0 top-[25px] h-[25px] rounded-[4px] bg-(--pad) shadow-[0_2px_3px_rgba(0,0,0,0.35)]" />
      <div className="absolute inset-y-0 left-[25px] w-[25px] rounded-[4px] bg-(--pad) shadow-[0_2px_3px_rgba(0,0,0,0.35)]" />
      <span className="absolute top-[31px] left-[31px] size-[13px] rounded-full bg-black/25" />
      <Arrow
        className={`${arm} top-0 left-[25px] rounded-t-[4px]`}
        label={t("volume_up")}
        onClick={onUp}
        turn={0}
      />
      <Arrow
        className={`${arm} top-[25px] right-0 rounded-r-[4px]`}
        label={t("next")}
        onClick={() => api.skip(1)}
        turn={90}
      />
      <Arrow
        className={`${arm} bottom-0 left-[25px] rounded-b-[4px]`}
        label={t("volume_down")}
        onClick={onDown}
        turn={180}
      />
      <Arrow
        className={`${arm} top-[25px] left-0 rounded-l-[4px]`}
        label={prev}
        onClick={() => previous(tickStore.get())}
        turn={270}
      />
    </div>
  );
}

function Arrow({
  className,
  label,
  onClick,
  turn,
}: {
  className: string;
  label: string;
  onClick: () => void;
  turn: number;
}) {
  return (
    <Tip text={label}>
      <button className={className} onClick={onClick} aria-label={label}>
        <svg
          viewBox="0 0 10 10"
          className="size-2 fill-current"
          style={{ rotate: `${turn}deg` }}
          aria-hidden="true"
        >
          <path d="M5 1.5 9 8H1z" />
        </svg>
      </button>
    </Tip>
  );
}

/** A plays or pauses, B stops; tilted as on the console (less on the colour model). */
function AB({ t, model, clear }: { t: Translate; model: Model; clear: boolean }) {
  const playing = useTick((tick) => tick?.state === "playing");
  const tilt = model === "dmg" ? 25 : 12;
  return (
    <div
      className={`absolute flex gap-3 ${
        model === "dmg" ? "top-4 right-0" : "top-[45px] right-2.5"
      }`}
      style={{ rotate: `-${tilt}deg` }}
    >
      {/* Both keys on one piece of rubber, which a clear shell of the colour model shows. */}
      {model === "gbc" && clear && (
        <div className={`absolute -inset-x-1.5 -inset-y-1.5 -z-10 rounded-full ${RUBBER}`} />
      )}
      <Key letter="B" label={t("stop")} onClick={() => api.stop()} tilt={tilt} model={model}>
        <Icon name="stop" size={13} />
      </Key>
      <Key
        letter="A"
        label={`${playing ? t("pause") : t("play")} (Space)`}
        onClick={() => api.toggle()}
        tilt={tilt}
        model={model}
      >
        <Icon name={playing ? "pause" : "play"} size={15} />
      </Key>
    </div>
  );
}

/** A round key. The first model has its letter printed below and, here, a symbol on the key;
 * the colour model has the letter moulded on the key and nothing below. */
function Key({
  letter,
  label,
  onClick,
  tilt,
  model,
  children,
}: {
  letter: string;
  label: string;
  onClick: () => void;
  tilt: number;
  model: Model;
  children: ReactNode;
}) {
  const dmg = model === "dmg";
  return (
    <div className="flex flex-col items-center gap-1">
      <Tip text={label}>
        <button
          className="grid size-[34px] place-items-center rounded-full bg-key text-key-ink shadow-[0_2px_3px_rgba(0,0,0,0.35),inset_0_1px_1px_rgba(255,255,255,0.25)] hover:brightness-110 active:translate-y-px active:shadow-[0_1px_1px_rgba(0,0,0,0.35)]"
          onClick={onClick}
          aria-label={label}
        >
          {/* Upright, whatever the tilt of the buttons. */}
          <span
            className={dmg ? "opacity-75" : "text-[15px] font-bold opacity-55 [text-shadow:none]"}
            style={{ rotate: `${tilt}deg` }}
          >
            {dmg ? children : letter}
          </span>
        </button>
      </Tip>
      {dmg && <span className="text-[10px] font-bold tracking-[0.1em]">{letter}</span>}
    </div>
  );
}

function Pill({
  label,
  title,
  onClick,
  model,
  clear,
}: {
  label: string;
  title: string;
  onClick: () => void;
  model: Model;
  clear: boolean;
}) {
  const key = (
    <Tip text={title}>
      <button
        className={`h-[9px] w-[34px] rounded-full bg-(--pad) shadow-[0_1px_2px_rgba(0,0,0,0.35)] hover:brightness-125 active:translate-y-px ${
          model === "dmg" ? "-rotate-[25deg]" : ""
        }`}
        onClick={onClick}
        aria-label={title}
      />
    </Tip>
  );
  return (
    <div className="flex flex-col items-center gap-1">
      {model === "dmg" ? (
        key
      ) : (
        // Level, in a hollow, on the colour model.
        <div
          className={`flex rounded-full p-[3px] ${
            clear ? RUBBER_SMALL : "bg-black/10 shadow-[inset_0_1px_2px_rgba(0,0,0,0.3)]"
          }`}
        >
          {key}
        </div>
      )}
      <span className="text-[7px] font-bold tracking-[0.12em] opacity-70">{label}</span>
    </div>
  );
}

/** The speaker: tilted slots on the first model, a round grille of holes on the colour one. */
function Speaker({ model }: { model: Model }) {
  if (model === "gbc") {
    return (
      <div
        className="pointer-events-none absolute right-1 bottom-0 size-[60px] rounded-full bg-[radial-gradient(circle,rgba(0,0,0,0.45)_1.3px,transparent_1.8px)] bg-size-[6px_6px] bg-center shadow-[inset_0_0_0_2px_rgba(0,0,0,0.12)]"
        aria-hidden="true"
      />
    );
  }
  return (
    // Clear of the big rounded corner.
    <div className="pointer-events-none absolute right-4 bottom-1 flex gap-[5px]" aria-hidden="true">
      {Array.from({ length: 6 }, (_, i) => (
        <span
          key={i}
          className="h-[38px] w-[4px] -rotate-[25deg] rounded-full bg-black/15 shadow-[inset_0_1px_2px_rgba(0,0,0,0.3)]"
        />
      ))}
    </div>
  );
}
