// The notes the APU's channels play, as a piano roll drawn LCD-style on a canvas: pitch goes up,
// a note to a row. The notes come ahead of the music (see `NOTES_AHEAD` in gb-music's player).
// With `keyboard`, a keyboard runs down the left edge and the notes to come run in from the right,
// sounding as they reach the keys, which are pressed in half-tone ink while they sound. Without
// it, the notes played so far run off to the left, the one playing now at the right edge.
// Everything is in full ink, which keeps the screen's own contrast, so the channels differ in
// shape: pulse 1 solid, pulse 2 outlined and the wave channel dotted, each bar as tall as its
// key's row and starting afresh where the channel is keyed on again. The noise has no pitch: it is
// a strip of hits along the bottom, solid for a low clock and dotted for a high one. Muted
// channels are left out (their notes are still gathered, for when they are heard again). The rows, names and the noise strip grow with the canvas, for the big
// view. The notes are gathered from every tick into one history, whatever shows them, so a
// piano roll that comes up (a switch from the waveform, the big view) has the notes to come at
// once. Ticks come about 30 times a second with a few frames each, so the notes are not moved
// tick by tick: where the music is now is reckoned from the clock between ticks, and they move on
// every animation frame, at a speed set in pixels per frame whatever the width. Like the
// oscilloscope, it never re-renders React.
import { useEffect, useRef } from "react";
import { tickStore } from "./tick";

/** Frames kept, enough for a canvas as wide as a big screen. */
const HISTORY = 1536;
/** Frames of music a second (FRAME_RATE in gb-music). */
const FRAME_RATE = 4194304 / 70224;
/** The most frames ahead of the latest tick that the time between ticks moves the notes on. */
const MAX_GUESS = 8;
/** The fewest semitones shown; the range is whole octaves round the notes the song has played
 * so far, so it only ever grows during a song and the rows keep their height. */
const MIN_SPAN = 24;
/** Notes outside this range (MIDI numbers) are not drawn: a frequency register near its top end
 * makes no tone anyone hears as a note. */
const LOWEST = 12;
const HIGHEST = 120;
/** The tallest a semitone's row gets: a tall canvas shows more octaves instead, and is centred
 * once it shows them all. */
const MAX_ROW = 10;
/** The noise strip's height, and its share of a tall canvas. */
const NOISE_H = 9;
const NOISE_SHARE = 0.06;
/** The noise's clock (as a MIDI note) from which it is drawn dotted: above 16 kHz. */
const NOISE_HIGH = 132;
/** The space between the keyboard and the notes. */
const KEYS_GAP = 3;
/** The black keys' share of the keyboard's width. */
const BLACK_W = 0.6;
const BLACK = new Set([1, 3, 6, 8, 10]);
/** The ink of a pressed key: a shade between the bare white keys and the solid black ones. */
const PRESSED = 0.45;

/** The notes of the current song, gathered from every tick: a ring of frames holding, per
 * channel, its note, its volume (0 silent) and whether it is keyed on. */
const ring = {
  note: new Uint8Array(HISTORY * 4),
  volume: new Uint8Array(HISTORY * 4),
  keyed: new Uint8Array(HISTORY * 4),
  /** The frames gathered since the song started, the last `count` of them kept: frame `j` is in
   * slot `j % HISTORY`. */
  total: 0,
  count: 0,
  /** The frame the music was at when the last tick came, at `nowAt` (performance.now()), and
   * whether it is playing on from there. */
  now: 0,
  nowAt: 0,
  playing: false,
  /** The lowest and highest notes the song has played so far. */
  low: 255,
  high: 0,
  /** Counts the changes, for the piano rolls to tell when to draw again. */
  version: 0,
};
let song = "";

tickStore.subscribe(() => {
  const tick = tickStore.get();
  const id = `${tick?.rom}:${tick?.current}`;
  if (id !== song || tick?.state === "stopped" || tick?.state === "none") {
    ring.total = 0;
    ring.count = 0;
    ring.now = 0;
    ring.low = 255;
    ring.high = 0;
  }
  song = id;
  for (const f of tick?.notes ?? []) {
    const at = (ring.total % HISTORY) * 4;
    for (let ch = 0; ch < 4; ch++) {
      const [k, v, on] = f[ch];
      ring.note[at + ch] = k;
      ring.volume[at + ch] = v;
      ring.keyed[at + ch] = on ? 1 : 0;
      if (ch < 3 && v > 0 && k >= LOWEST && k <= HIGHEST) {
        ring.low = Math.min(ring.low, k);
        ring.high = Math.max(ring.high, k);
      }
    }
    ring.total++;
    ring.count = Math.min(ring.count + 1, HISTORY);
  }
  if (tick?.notes.length) {
    ring.now = Math.max(ring.total - 1 - tick.notesAhead, 0);
    ring.nowAt = performance.now();
  }
  ring.playing = tick?.state === "playing";
  ring.version++;
});

export default function Notes({
  muted,
  frameWidth = 1.5,
  labels = true,
  keyboard = 0,
}: {
  muted: boolean[];
  /** The width of a frame, in CSS pixels: how fast the notes move. */
  frameWidth?: number;
  /** Whether each C is named at the left edge. */
  labels?: boolean;
  /** The width of a keyboard down the left edge, in CSS pixels; 0 for none. */
  keyboard?: number;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const mutedRef = useRef(muted);
  mutedRef.current = muted;

  useEffect(() => {
    const { note, volume, keyed } = ring;
    let drawn = -1;
    let drawnMuted = "";
    // The pitch range shown, moving towards whole octaves round the notes played so far; at
    // once on the first draw, so a piano roll comes up as it was left.
    let lo = 48;
    let hi = 84;
    let shown = false;
    // Where the music is, in frames, as drawn: it moves on with the clock and is drawn towards
    // where the ticks say it is.
    let pos = 0;
    let posAt = 0;
    let frame = 0;

    const draw = () => {
      frame = requestAnimationFrame(draw);
      const c = canvas.current;
      if (!c) return;
      const fresh = ring.version !== drawn;
      drawn = ring.version;
      const { total, count, low, high } = ring;

      const t = performance.now();
      const since = ring.playing ? ((t - ring.nowAt) / 1000) * FRAME_RATE : 0;
      const target = ring.now + Math.min(since, MAX_GUESS);
      const before = pos;
      if (!shown || Math.abs(target - pos) > FRAME_RATE / 2) {
        pos = target;
      } else {
        if (ring.playing) pos += ((t - posAt) / 1000) * FRAME_RATE;
        pos += (target - pos) * 0.1;
      }
      pos = Math.max(Math.min(pos, total - 1), 0);
      posAt = t;

      const w = c.clientWidth;
      const h = c.clientHeight;
      const dpr = window.devicePixelRatio || 1;
      const size = Math.round(w * dpr) + "x" + Math.round(h * dpr);
      // Where the notes start: right of the keyboard.
      const x0 = keyboard ? keyboard + KEYS_GAP : 0;
      const fw = frameWidth;
      // Where the frame playing now goes: at the keyboard, the notes to come right of it; without
      // one, at the right edge, the notes played left of it.
      const xNow = keyboard ? x0 : w - fw;
      const x = (j: number) => xNow + (j - pos) * fw;
      const now = Math.floor(pos);
      const from = Math.max(total - count, keyboard ? now : now - Math.ceil(w / fw) - 1);
      const to = keyboard ? Math.min(total - 1, now + Math.ceil((w - x0) / fw) + 1) : now;
      const at = (j: number) => (j % HISTORY) * 4;

      const top = 2;
      const noiseH = Math.max(NOISE_H, Math.round(h * NOISE_SHARE));
      const pitchH = h - noiseH - 4 - top;

      // Whole octaves round the notes played so far (or C3–C6 before any), and more octaves,
      // below and above in turn, where the canvas is too tall for them.
      let toLo = low <= high ? Math.floor(low / 12) * 12 : 48;
      let toHi = low <= high ? Math.ceil((high + 1) / 12) * 12 : 84;
      const span = Math.max(MIN_SPAN, Math.ceil(pitchH / MAX_ROW));
      const ceiling = Math.ceil((HIGHEST + 1) / 12) * 12;
      let down = true;
      while (toHi - toLo < span && (toLo > LOWEST || toHi < ceiling)) {
        if (toLo > LOWEST && (down || toHi >= ceiling)) toLo -= 12;
        else toHi += 12;
        down = !down;
      }
      const moving = Math.abs(toLo - lo) > 0.05 || Math.abs(toHi - hi) > 0.05;
      const mutedKey = mutedRef.current.join();
      const still = pos === before && !fresh && !moving;
      if (still && c.dataset.size === size && drawnMuted === mutedKey) return;
      drawnMuted = mutedKey;
      lo = moving && shown ? lo + (toLo - lo) * 0.2 : toLo;
      hi = moving && shown ? hi + (toHi - hi) * 0.2 : toHi;
      shown = true;

      if (c.dataset.size !== size) {
        c.width = Math.round(w * dpr);
        c.height = Math.round(h * dpr);
        c.dataset.size = size;
      }
      const g = c.getContext("2d")!;
      g.setTransform(dpr, 0, 0, dpr, 0, 0);
      g.clearRect(0, 0, w, h);
      // Read the palette on every draw so a palette change shows at once.
      const css = getComputedStyle(c);
      const ink = css.getPropertyValue("--lcd-ink");
      const row = Math.min(pitchH / (hi - lo), MAX_ROW);
      // Centred in the space for the notes when all of them fit with room to spare.
      const notesTop = top + (pitchH - row * (hi - lo)) / 2;
      const y = (k: number) => notesTop + (hi - k - 0.5) * row;

      if (keyboard) {
        // The keys sounding now, from the channels that are heard.
        const pressed = new Set<number>();
        for (let ch = 0; ch < 3 && now >= total - count && now < total; ch++) {
          const k = at(now) + ch;
          if (volume[k] > 0 && !mutedRef.current[ch]) pressed.add(note[k]);
        }
        drawKeys(g, ink, keyboard, lo, hi, notesTop, row * (hi - lo), row, y, pressed);
      }
      // The notes stay right of the keyboard.
      g.save();
      g.beginPath();
      g.rect(x0, 0, w - x0, h);
      g.clip();

      // A line under each C, named at the left edge.
      g.strokeStyle = css.getPropertyValue("--lcd-grid");
      g.lineWidth = 1;
      g.fillStyle = ink;
      g.font = `bold ${Math.min(Math.max(Math.floor(row), 8), 13)}px ui-monospace, monospace`;
      g.textBaseline = "bottom";
      for (let k = Math.ceil(lo / 12) * 12; k <= hi; k += 12) {
        const ly = Math.round(y(k) + row / 2) + 0.5;
        g.beginPath();
        g.moveTo(x0, ly);
        g.lineTo(w, ly);
        g.stroke();
        if (labels && ly > 10) g.fillText(`C${k / 12 - 1}`, x0 + 2, ly - 1);
      }
      g.beginPath();
      g.moveTo(x0, h - noiseH - 1.5);
      g.lineTo(w, h - noiseH - 1.5);
      g.stroke();

      // Frames start on whole device pixels, so that a held note has no seams.
      const left = (j: number) => Math.round(x(j) * dpr) / dpr;
      const dots = dotted(g, ink, dpr);
      g.fillStyle = ink;
      // Whether frame `j` of `ch` goes on from the one before, as the same note held.
      const held = (j: number, ch: number) => {
        if (j <= from || j > to) return false;
        const k = at(j) + ch;
        const p = at(j - 1) + ch;
        return !keyed[k] && volume[p] > 0 && volume[k] > 0 && note[p] === note[k];
      };
      // Wave first, as it is mostly under the others, and pulse 1 on top.
      for (const ch of [2, 1, 0]) {
        if (mutedRef.current[ch]) continue;
        g.fillStyle = ch === 2 ? dots : ink;
        for (let j = from; j <= to; j++) {
          const k = at(j) + ch;
          const v = volume[k];
          if (v === 0 || note[k] < LOWEST || note[k] > HIGHEST) continue;
          // As tall as the note's row, and its key.
          const by = Math.round(y(note[k]) - row / 2);
          const bh = Math.max(Math.round(y(note[k]) + row / 2) - by, 1);
          // A gap before a note that starts straight after another.
          const gap = j > from && !held(j, ch) && volume[at(j - 1) + ch] > 0 ? 1 : 0;
          const bx = left(j) + gap;
          const bw = left(j + 1) - bx;
          if (ch !== 1 || bh < 3) {
            g.fillRect(bx, by, bw, bh);
            continue;
          }
          // Outlined: the top and bottom edges, closed where the note starts and ends.
          g.fillRect(bx, by, bw, 1);
          g.fillRect(bx, by + bh - 1, bw, 1);
          if (!held(j, ch)) g.fillRect(bx, by, 1, bh);
          if (!held(j + 1, ch)) g.fillRect(bx + bw - 1, by, 1, bh);
        }
      }
      // The noise: as loud as its volume, with a full-height tick at each key-on.
      const base = h - 1;
      for (let j = from; j <= to && !mutedRef.current[3]; j++) {
        const k = at(j) + 3;
        const v = volume[k];
        if (v === 0) continue;
        g.fillStyle = note[k] >= NOISE_HIGH ? dots : ink;
        const bx = left(j);
        const bar = keyed[k] ? noiseH : Math.max(Math.round((noiseH * v) / 15), 1);
        g.fillRect(bx, base - bar, left(j + 1) - bx, bar);
      }
      g.globalAlpha = 1;
      g.restore();
    };
    frame = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(frame);
  }, [frameWidth, labels, keyboard]);

  return <canvas ref={canvas} className="block size-full" />;
}

/**
 * A keyboard down the left edge, a key to each row of the notes (`y` is a row's middle): the
 * black keys short and filled, the white keys parted where two of them meet and through the
 * middle of a black key, as on a piano. The keys in `pressed` are filled in half-tone ink, a
 * pressed black key keeping its solid edge.
 */
function drawKeys(
  g: CanvasRenderingContext2D,
  ink: string,
  width: number,
  lo: number,
  hi: number,
  top: number,
  height: number,
  row: number,
  y: (k: number) => number,
  pressed: Set<number>,
) {
  g.save();
  g.beginPath();
  g.rect(0, top, width, height);
  g.clip();
  g.fillStyle = ink;
  const blackW = Math.round(width * BLACK_W);
  for (let k = Math.floor(lo); k <= Math.ceil(hi); k++) {
    const black = BLACK.has(((k % 12) + 12) % 12);
    // The row's top and bottom edges, on whole pixels.
    const t = Math.round(y(k) - row / 2);
    const b = Math.round(y(k) + row / 2);
    if (black) {
      // The white keys meet under the middle of the black key.
      g.fillRect(blackW, Math.round(y(k)), width - blackW, 1);
      if (!pressed.has(k) || b - t <= 2) {
        g.fillRect(0, t, blackW, b - t);
        continue;
      }
      g.strokeStyle = ink;
      g.lineWidth = 1;
      g.strokeRect(0.5, t + 0.5, blackW - 1, b - t - 1);
      g.globalAlpha = PRESSED;
      g.fillRect(1, t + 1, blackW - 2, b - t - 2);
      g.globalAlpha = 1;
      continue;
    }
    if (pressed.has(k)) {
      g.globalAlpha = PRESSED;
      g.fillRect(0, t, width, b - t);
      g.globalAlpha = 1;
    }
    // Between two white keys next to each other (B–C and E–F).
    if (!BLACK.has((((k - 1) % 12) + 12) % 12)) g.fillRect(0, b - 1, width, 1);
  }
  // The keyboard's edge.
  g.fillRect(width - 1, top, 1, height);
  g.restore();
}

/** A fill of every other pixel in `ink`, in a checker, made again only when the ink changes. */
let dotCache: { ink: string; dpr: number; pattern: CanvasPattern | string } | null = null;
function dotted(g: CanvasRenderingContext2D, ink: string, dpr: number) {
  if (dotCache?.ink === ink && dotCache.dpr === dpr) return dotCache.pattern;
  const tile = document.createElement("canvas");
  const s = Math.max(1, Math.round(dpr));
  tile.width = tile.height = 2 * s;
  const t = tile.getContext("2d")!;
  t.fillStyle = ink;
  t.fillRect(0, 0, s, s);
  t.fillRect(s, s, s, s);
  const pattern = g.createPattern(tile, "repeat") ?? ink;
  // The pattern is laid out in CSS pixels by the transform; undo it so a dot is a device pixel.
  if (typeof pattern !== "string")
    pattern.setTransform(new DOMMatrix([1 / dpr, 0, 0, 1 / dpr, 0, 0]));
  dotCache = { ink, dpr, pattern };
  return pattern;
}
