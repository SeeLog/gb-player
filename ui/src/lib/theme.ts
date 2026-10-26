// Applying themes to the page; the palettes themselves are in app.css and app/src/themes.rs.

/** The colour tokens, the CSS custom properties without `--` (mirrors `TOKENS` in themes.rs). */
export const TOKENS = [
  "bg",
  "panel",
  "panel-2",
  "line",
  "text",
  "muted",
  "faint",
  "accent",
  "accent-2",
  "accent-ink",
  "lcd-hi",
  "lcd",
  "lcd-lo",
  "lcd-ink",
  "lcd-grid",
  "bezel",
  "overlay",
  "danger",
] as const;

/** Show `base` with `colors` laid over it, dropping the colours set before. */
export function applyTheme(base: string, colors: Record<string, string>) {
  const root = document.documentElement;
  root.dataset.palette = base;
  for (const token of TOKENS) {
    const value = colors[token];
    if (value) root.style.setProperty(`--${token}`, value);
    else root.style.removeProperty(`--${token}`);
  }
}

/** The colours of a built-in palette, read from a hidden element that uses it. */
export function paletteColors(base: string): Record<string, string> {
  const probe = document.createElement("div");
  probe.dataset.palette = base;
  probe.hidden = true;
  document.body.append(probe);
  const style = getComputedStyle(probe);
  const colors = Object.fromEntries(
    TOKENS.map((token) => [token, style.getPropertyValue(`--${token}`).trim()]),
  );
  probe.remove();
  return colors;
}

let canvas: CanvasRenderingContext2D | null = null;

/** `#rrggbb` for any CSS colour (alpha dropped), for `<input type="color">`. */
export function toHex(color: string): string {
  canvas ??= document.createElement("canvas").getContext("2d");
  if (!canvas) return "#000000";
  canvas.fillStyle = "#000000";
  canvas.fillStyle = color;
  const v = canvas.fillStyle;
  if (v.startsWith("#")) return v;
  const [r, g, b] = (v.match(/[\d.]+/g) ?? []).map(Number);
  return "#" + [r, g, b].map((n) => Math.round(n || 0).toString(16).padStart(2, "0")).join("");
}

/** Whether `value` is a colour themes.rs accepts (hex, or rgb()/hsl() with numbers). */
export function isColor(value: string): boolean {
  const v = value.trim();
  return (
    /^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(v) ||
    /^(rgba?|hsla?)\((?=[^)]*\d)[\d\s.,%/deg-]*\)$/i.test(v)
  );
}

/** A theme being edited, shown live until it is saved or dropped. */
export interface Draft {
  name: string;
  base: string;
  colors: Record<string, string>;
}
