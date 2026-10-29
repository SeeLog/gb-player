// The notes the APU's channels play, as a piano roll drawn LCD-style on a canvas: pitch goes up,
// time to the left, the latest frame at the right edge. Everything is in full ink, which keeps
// the screen's own contrast, so the channels differ in shape: pulse 1 solid, pulse 2 outlined and
// the wave channel dotted. A bar is as thick as the channel is loud, and starts afresh where it is
// keyed on again. The noise has no pitch: it is a strip of hits along the bottom, solid for a low
// clock and dotted for a high one. Muted channels are drawn faded. Like the oscilloscope, it draws
// from animation frames with the latest tick and never re-renders React.
import { useEffect, useRef } from "react";
import { tickStore } from "./tick";
import type { Tick } from "./types";

/** Frames kept, enough for the widest canvas. */
const HISTORY = 512;
/** The fewest semitones shown; the range is whole octaves round the notes on screen. */
const MIN_SPAN = 24;
/** Notes outside this range (MIDI numbers) are not drawn: a frequency register near its top end
 * makes no tone anyone hears as a note. */
const LOWEST = 12;
const HIGHEST = 120;
const NOISE_H = 9;
/** The noise's clock (as a MIDI note) from which it is drawn dotted: above 16 kHz. */
const NOISE_HIGH = 132;
/** The ink a muted channel is drawn with. */
const MUTED = 0.45;

export default function Notes({
  muted,
  frameWidth = 1.5,
  labels = true,
}: {
  muted: boolean[];
  /** The width of a frame, in CSS pixels. */
  frameWidth?: number;
  /** Whether each C is named at the left edge. */
  labels?: boolean;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const mutedRef = useRef(muted);
  mutedRef.current = muted;

  useEffect(() => {
    // A ring of frames: per channel, its note, its volume (0 silent) and whether it is keyed on.
    const note = new Uint8Array(HISTORY * 4);
    const volume = new Uint8Array(HISTORY * 4);
    const keyed = new Uint8Array(HISTORY * 4);
    let head = 0;
    let count = 0;
    let song = "";
    let seen: Tick | null | undefined = undefined;
    let drawnMuted = "";
    // The pitch range shown, moving towards the one the notes on screen need.
    let lo = 48;
    let hi = 84;
    let frame = 0;

    const draw = () => {
      frame = requestAnimationFrame(draw);
      const c = canvas.current;
      if (!c) return;
      const tick = tickStore.get();
      const fresh = tick !== seen;
      if (fresh) {
        seen = tick;
        const id = `${tick?.rom}:${tick?.current}`;
        if (id !== song || tick?.state === "stopped" || tick?.state === "none") count = 0;
        song = id;
        for (const f of tick?.notes ?? []) {
          for (let ch = 0; ch < 4; ch++) {
            note[head * 4 + ch] = f[ch][0];
            volume[head * 4 + ch] = f[ch][1];
            keyed[head * 4 + ch] = f[ch][2] ? 1 : 0;
          }
          head = (head + 1) % HISTORY;
          count = Math.min(count + 1, HISTORY);
        }
      }
      const w = c.clientWidth;
      const h = c.clientHeight;
      const dpr = window.devicePixelRatio || 1;
      const size = Math.round(w * dpr) + "x" + Math.round(h * dpr);
      const n = Math.min(count, Math.ceil(w / frameWidth) + 1);
      const at = (i: number) => ((head - n + i + HISTORY) % HISTORY) * 4;

      // Whole octaves round the notes on screen.
      let low = 255;
      let high = 0;
      for (let i = 0; i < n; i++) {
        for (let ch = 0; ch < 3; ch++) {
          const k = at(i) + ch;
          if (volume[k] === 0 || note[k] < LOWEST || note[k] > HIGHEST) continue;
          low = Math.min(low, note[k]);
          high = Math.max(high, note[k]);
        }
      }
      let toLo = lo;
      let toHi = hi;
      if (low <= high) {
        toLo = Math.floor(low / 12) * 12;
        toHi = Math.ceil((high + 1) / 12) * 12;
        while (toHi - toLo < MIN_SPAN) {
          if (toLo > LOWEST) toLo -= 12;
          else toHi += 12;
        }
      }
      const moving = Math.abs(toLo - lo) > 0.05 || Math.abs(toHi - hi) > 0.05;
      const mutedKey = mutedRef.current.join();
      if (!fresh && !moving && c.dataset.size === size && drawnMuted === mutedKey) return;
      drawnMuted = mutedKey;
      lo = moving ? lo + (toLo - lo) * 0.2 : toLo;
      hi = moving ? hi + (toHi - hi) * 0.2 : toHi;

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
      const top = 2;
      const pitchH = h - NOISE_H - 4 - top;
      const row = pitchH / (hi - lo);
      const y = (k: number) => top + (hi - k - 0.5) * row;

      // A line under each C, named at the left edge.
      g.strokeStyle = css.getPropertyValue("--lcd-grid");
      g.lineWidth = 1;
      g.fillStyle = ink;
      g.font = "bold 8px ui-monospace, monospace";
      g.textBaseline = "bottom";
      for (let k = Math.ceil(lo / 12) * 12; k <= hi; k += 12) {
        const ly = Math.round(y(k) + row / 2) + 0.5;
        g.beginPath();
        g.moveTo(0, ly);
        g.lineTo(w, ly);
        g.stroke();
        if (labels && ly > 10) g.fillText(`C${k / 12 - 1}`, 1, ly - 1);
      }
      g.beginPath();
      g.moveTo(0, h - NOISE_H - 1.5);
      g.lineTo(w, h - NOISE_H - 1.5);
      g.stroke();

      // Frames start on whole device pixels, so that a held note has no seams.
      const left = (i: number) => Math.round((w - (n - i) * frameWidth) * dpr) / dpr;
      const thick = Math.min(Math.max(row * 1.6, 4), 7);
      const dots = dotted(g, ink, dpr);
      g.fillStyle = ink;
      // Whether frame `i` of `ch` goes on from the one before, as the same note held.
      const held = (i: number, ch: number) => {
        if (i <= 0 || i >= n) return false;
        const k = at(i) + ch;
        const p = at(i - 1) + ch;
        return !keyed[k] && volume[p] > 0 && volume[k] > 0 && note[p] === note[k];
      };
      // Wave first, as it is mostly under the others, and pulse 1 on top.
      for (const ch of [2, 1, 0]) {
        g.globalAlpha = mutedRef.current[ch] ? MUTED : 1;
        g.fillStyle = ch === 2 ? dots : ink;
        for (let i = 0; i < n; i++) {
          const k = at(i) + ch;
          const v = volume[k];
          if (v === 0 || note[k] < LOWEST || note[k] > HIGHEST) continue;
          const t = Math.max(Math.round(thick * (0.4 + (0.6 * v) / 15)), ch === 1 ? 3 : 2);
          const by = Math.round(y(note[k]) - t / 2);
          // A gap before a note that starts straight after another.
          const gap = i > 0 && !held(i, ch) && volume[at(i - 1) + ch] > 0 ? 1 : 0;
          const x = left(i) + gap;
          const fw = left(i + 1) - x;
          if (ch !== 1 || t < 3) {
            g.fillRect(x, by, fw, t);
            continue;
          }
          // Outlined: the top and bottom edges, closed where the note starts and ends.
          g.fillRect(x, by, fw, 1);
          g.fillRect(x, by + t - 1, fw, 1);
          if (!held(i, ch)) g.fillRect(x, by, 1, t);
          if (!held(i + 1, ch)) g.fillRect(x + fw - 1, by, 1, t);
        }
      }
      // The noise: as loud as its volume, with a full-height tick at each key-on.
      const base = h - 1;
      for (let i = 0; i < n; i++) {
        const k = at(i) + 3;
        const v = volume[k];
        if (v === 0) continue;
        g.globalAlpha = mutedRef.current[3] ? MUTED : 1;
        g.fillStyle = note[k] >= NOISE_HIGH ? dots : ink;
        const x = left(i);
        const bar = keyed[k] ? NOISE_H : Math.max(Math.round((NOISE_H * v) / 15), 1);
        g.fillRect(x, base - bar, left(i + 1) - x, bar);
      }
      g.globalAlpha = 1;
    };
    frame = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(frame);
  }, [frameWidth, labels]);

  return <canvas ref={canvas} className="block size-full" />;
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
