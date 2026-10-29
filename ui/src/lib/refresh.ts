// The refresh rate of the monitor the main window is on, measured from its animation frames and
// told to the Rust side, which sends the playback state that often when the settings say to
// follow the monitor. It is measured again every few seconds, so moving the window to another
// monitor is followed; a hidden or minimised window, whose frames stop or slow, tells nothing.

import { api } from "./api";

/** How often a measurement is taken, and the fewest frames it needs. */
const EVERY_MS = 2000;
const MIN_FRAMES = 30;
/** Gaps longer than this are a stall, not a frame. */
const LONGEST_MS = 100;

/** Start measuring; returns the function that stops it. */
export function watchRefreshRate(): () => void {
  let frame = 0;
  let last = 0;
  let since = performance.now();
  let gaps: number[] = [];
  let told = 0;

  const step = (t: number) => {
    frame = requestAnimationFrame(step);
    if (last && t - last < LONGEST_MS) gaps.push(t - last);
    last = t;
    if (t - since < EVERY_MS) return;
    since = t;
    if (gaps.length >= MIN_FRAMES && document.visibilityState === "visible") {
      gaps.sort((a, b) => a - b);
      const hz = Math.round(1000 / gaps[gaps.length >> 1]);
      // Small wobbles of the measurement are not a new rate.
      if (Math.abs(hz - told) >= 2) {
        told = hz;
        api.setDisplayRate(hz);
      }
    }
    gaps = [];
  };
  frame = requestAnimationFrame(step);
  return () => cancelAnimationFrame(frame);
}
