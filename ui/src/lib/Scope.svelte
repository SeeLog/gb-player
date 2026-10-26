<script lang="ts">
  // Oscilloscope of the mixed output, drawn LCD-style.
  let { samples, volume, active }: { samples: number[]; volume: number; active: boolean } =
    $props();

  let canvas = $state<HTMLCanvasElement | null>(null);
  const POINTS = 512;

  $effect(() => {
    const c = canvas;
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    const w = c.clientWidth;
    const h = c.clientHeight;
    if (c.width !== Math.round(w * dpr) || c.height !== Math.round(h * dpr)) {
      c.width = Math.round(w * dpr);
      c.height = Math.round(h * dpr);
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

    // Start on a rising zero crossing so periodic waves stand still.
    let start = 0;
    if (active && samples.length > POINTS) {
      const limit = samples.length - POINTS;
      for (let i = 1; i < limit; i++) {
        if (samples[i - 1] < 0 && samples[i] >= 0) {
          start = i;
          break;
        }
      }
    }
    const n = Math.min(POINTS, samples.length - start);
    if (!active || n < 2) return;
    const gain = 1.6 / Math.max(volume, 0.05);
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
  });
</script>

<canvas bind:this={canvas}></canvas>

<style>
  canvas {
    width: 100%;
    height: 100%;
    display: block;
  }
</style>
