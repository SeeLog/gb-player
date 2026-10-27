// Applying themes to the page; the palettes themselves are in app.css and app/src/themes.rs.

import { api } from "./api";
import type { Background, Console } from "./types";

/** The colour tokens, the CSS custom properties without `--` (mirrors `TOKENS` in themes.rs). */
export const TOKENS = [
  "backdrop",
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
  "shell",
  "shell-ink",
  "shell-frame",
  "key",
  "key-ink",
] as const;

/** Ready-made colours for the mini player's body (shell, shell-ink, shell-frame, key, key-ink),
 * picked in the theme editor; the name is a key of the UI text. */
export const BODY_COLORS: { name: string; colors: Record<string, string> }[] = [
  {
    name: "body_gray",
    colors: {
      shell: "#c4c2bb",
      "shell-ink": "#2f3672",
      "shell-frame": "#8e8d9b",
      key: "#982a5b",
      "key-ink": "#f1e4ea",
    },
  },
  {
    name: "body_silver",
    colors: {
      shell: "#b8bbc0",
      "shell-ink": "#2b2e35",
      "shell-frame": "#7a7d86",
      key: "#4b4f58",
      "key-ink": "#e2e4e8",
    },
  },
  {
    name: "body_white",
    colors: {
      shell: "#eceae4",
      "shell-ink": "#3a4070",
      "shell-frame": "#a9a8b4",
      key: "#982a5b",
      "key-ink": "#f6ecf0",
    },
  },
  {
    name: "body_purple",
    colors: {
      shell: "#6a4bb0",
      "shell-ink": "#efe9ff",
      "shell-frame": "#1f1d26",
      key: "#34313c",
      "key-ink": "#d7d3e0",
    },
  },
  {
    name: "body_clear",
    colors: {
      shell: "#ebebf080",
      "shell-ink": "#25252d",
      "shell-frame": "#1f1d26",
      key: "#34313c",
      "key-ink": "#d7d3e0",
    },
  },
  {
    name: "body_clear_purple",
    colors: {
      shell: "#6b4fb88f",
      "shell-ink": "#2c2244",
      "shell-frame": "#1f1d26",
      key: "#34313c",
      "key-ink": "#d7d3e0",
    },
  },
];

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

const hex2 = (n: number) =>
  Math.round(Math.min(255, Math.max(0, n || 0)))
    .toString(16)
    .padStart(2, "0");

/** Any CSS colour as `#rrggbb` (for the colour picker) and its alpha from 0 to 1. */
export function splitColor(color: string): { hex: string; alpha: number } {
  canvas ??= document.createElement("canvas").getContext("2d");
  if (!canvas) return { hex: "#000000", alpha: 1 };
  canvas.fillStyle = "#000000";
  canvas.fillStyle = color;
  const v = canvas.fillStyle;
  if (v.startsWith("#")) return { hex: v, alpha: 1 };
  const [r, g, b, a] = (v.match(/[\d.]+/g) ?? []).map(Number);
  return { hex: "#" + [r, g, b].map(hex2).join(""), alpha: a ?? 1 };
}

/** `#rrggbb`, or `#rrggbbaa` when `alpha` is below 1. */
export function joinColor(hex: string, alpha: number): string {
  return alpha >= 1 ? hex : hex + hex2(alpha * 255);
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
  background: Background | null;
  console: Console;
}

const images = new Map<string, Promise<string>>();

/** A background image of the themes folder as a `data:` URL, loaded once. */
export function imageUrl(name: string): Promise<string> {
  let url = images.get(name);
  if (!url) {
    url = api.themeImage(name);
    url.catch(() => images.delete(name));
    images.set(name, url);
  }
  return url;
}

/** Forget a loaded image, after a new file was copied in under its name. */
export function forgetImage(name: string) {
  images.delete(name);
}
