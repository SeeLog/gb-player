// Oscilloscope of the mixed output, drawn LCD-style on a canvas. It draws from animation frames
// with the latest tick, so it never re-renders React.
import { useEffect, useRef } from "react";
import { tickStore } from "./tick";

const POINTS = 512;

export default function Scope({ volume }: { volume: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const gainVolume = useRef(volume);
  gainVolume.current = volume;

  useEffect(() => {
    let frame = 0;
    let drawn: unknown = undefined;
    const draw = () => {
      frame = requestAnimationFrame(draw);
      const c = canvas.current;
      const tick = tickStore.get();
      const w = c?.clientWidth ?? 0;
      const h = c?.clientHeight ?? 0;
      const dpr = window.devicePixelRatio || 1;
      const size = Math.round(w * dpr) + "x" + Math.round(h * dpr);
      // Only draw when there is a new tick or the canvas changed size.
      if (!c || (tick === drawn && c.dataset.size === size)) return;
      drawn = tick;
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

      g.strokeStyle = css.getPropertyValue("--lcd-grid");
      g.lineWidth = 1;
      for (let x = 0.5; x < w; x += 16) {
        g.beginPath();
        g.moveTo(x, 0);
        g.lineTo(x, h);
        g.stroke();
      }
      g.beginPath();
      g.moveTo(0, h / 2 + 0.5);
      g.lineTo(w, h / 2 + 0.5);
      g.stroke();

      const samples = tick?.scope ?? [];
      if (tick?.state !== "playing") return;
      // Start on a rising zero crossing so periodic waves stand still.
      let start = 0;
      if (samples.length > POINTS) {
        const limit = samples.length - POINTS;
        for (let i = 1; i < limit; i++) {
          if (samples[i - 1] < 0 && samples[i] >= 0) {
            start = i;
            break;
          }
        }
      }
      const n = Math.min(POINTS, samples.length - start);
      if (n < 2) return;
      const gain = 1.6 / Math.max(gainVolume.current, 0.05);
      g.strokeStyle = css.getPropertyValue("--lcd-ink");
      g.lineWidth = 2;
      g.lineJoin = "round";
      g.beginPath();
      for (let i = 0; i < n; i++) {
        const x = (i / (n - 1)) * w;
        const v = Math.max(-1, Math.min(1, samples[start + i] * gain));
        const y = h / 2 - v * (h / 2 - 4);
        if (i === 0) g.moveTo(x, y);
        else g.lineTo(x, y);
      }
      g.stroke();
    };
    frame = requestAnimationFrame(draw);
    return () => cancelAnimationFrame(frame);
  }, []);

  return <canvas ref={canvas} className="block size-full" />;
}
