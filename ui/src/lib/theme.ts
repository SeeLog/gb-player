// Applying themes to the page; the palettes themselves are in app.css and app/src/themes.rs.

import { api } from "./api";
import type { Background, Console, Sheen } from "./types";

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
  "meter-start",
  "meter-end",
  "lcd-hi",
  "lcd",
  "lcd-lo",
  "lcd-ink",
  "lcd-grid",
  "lcd-shadow",
  "bezel",
  "overlay",
  "danger",
  "shell",
  "shell-ink",
  "shell-frame",
  "key",
  "key-ink",
  "knob",
] as const;

/** Tokens a palette may leave out, and the token each then follows (as in the meters'
 * `var(--meter-start, var(--accent-2))`). */
export const FALLBACKS: Record<string, string> = {
  "meter-start": "accent-2",
  "meter-end": "accent",
};

/** The sheens of the built-in palettes that have one (the others have none). */
export const BASE_SHEENS: Record<string, Sheen> = {
  // Pearly and iridescent: mint, aqua, lavender, pink and a little cream.
  prism: { colors: ["#bdf3e4", "#b2e4f7", "#d2c8fb", "#f8cbe4", "#fbe7c6"], angle: 115 },
  // A sunset: violet, magenta, orange and gold.
  nebula: { colors: ["#a58bff", "#e36cc4", "#ff9a6b", "#ffd27a"], angle: 115 },
  // Barely a sheen: light lime, so the fills are not the dark olive of the text.
  daylight: { colors: ["#b4da4e", "#9cc93a"], angle: 115 },
};

/** The sheen a theme shows: its own, or else its base's. */
export function sheenOf(base: string, sheen: Sheen | null): Sheen | null {
  const shown = sheen ?? BASE_SHEENS[base] ?? null;
  return shown && shown.colors.length > 0 ? shown : null;
}

/** A sheen as a CSS gradient. */
export function sheenGradient(sheen: Sheen): string {
  const colors = sheen.colors.length === 1 ? [sheen.colors[0], sheen.colors[0]] : sheen.colors;
  return `linear-gradient(${sheen.angle}deg, ${colors.join(", ")})`;
}

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
export function applyTheme(base: string, colors: Record<string, string>, sheen: Sheen | null) {
  const root = document.documentElement;
  root.dataset.palette = base;
  for (const token of TOKENS) {
    const value = colors[token];
    if (value) root.style.setProperty(`--${token}`, value);
    else root.style.removeProperty(`--${token}`);
  }
  // With a sheen, the accent's fills take it (see app.css), the switch's knob is the knob colour
  // rather than the dark accent-ink, and the mini player's logo is drawn in it. The accent then
  // only writes text: what else it marks (edges, the app's mark, rows under the pointer and
  // playing) is in accent-2, lighter washes for the rows (about 1.17:1 and 1.31:1 in Prism), and
  // the name on a playing or chosen row is in the plain text colour, which goes with the green.
  const shown = sheenOf(base, sheen);
  const withSheen: Record<string, string> = {
    "--accent-sheen": shown ? sheenGradient(shown) : "",
    "--sheen-text": "transparent",
    "--knob-on": "var(--knob)",
    "--accent-edge": "var(--accent-2)",
    "--row-hover": "color-mix(in srgb, var(--accent-2) 17%, transparent)",
    "--row-playing": "color-mix(in srgb, var(--accent-2) 29%, transparent)",
    "--row-text": "var(--text)",
  };
  for (const [name, value] of Object.entries(withSheen)) {
    if (shown) root.style.setProperty(name, value);
    else root.style.removeProperty(name);
  }
}

/** The colours of a built-in palette, read from a hidden element that uses it. */
export function paletteColors(base: string): Record<string, string> {
  // Inside a wrapper that clears the tokens a palette may leave out, which the probe would
  // otherwise take from the page (its palette and the theme's colours).
  const wrapper = document.createElement("div");
  for (const token of Object.keys(FALLBACKS)) wrapper.style.setProperty(`--${token}`, "initial");
  wrapper.hidden = true;
  const probe = document.createElement("div");
  probe.dataset.palette = base;
  wrapper.append(probe);
  document.body.append(wrapper);
  const style = getComputedStyle(probe);
  const colors = Object.fromEntries(
    TOKENS.map((token) => [token, style.getPropertyValue(`--${token}`).trim()]),
  );
  wrapper.remove();
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

/** The contrast ratio of two colours (their opacity left out), from 1 to 21. */
export function contrast(a: string, b: string): number {
  const luminance = (color: string) => {
    const hex = splitColor(color).hex;
    const [r, g, b] = [1, 3, 5].map((i) => {
      const v = parseInt(hex.slice(i, i + 2), 16) / 255;
      return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
    });
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  };
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

/** `#rrggbb`, or `#rrggbbaa` when `alpha` is below 1. */
export function joinColor(hex: string, alpha: number): string {
  return alpha >= 1 ? hex : hex + hex2(alpha * 255);
}

/** A colour's hue (0 to 360), saturation and lightness (0 to 100). */
export interface Hsl {
  h: number;
  s: number;
  l: number;
}

/** `#rrggbb` as hue, saturation and lightness. */
export function toHsl(hex: string): Hsl {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  const d = max - min;
  if (d === 0) return { h: 0, s: 0, l: l * 100 };
  const s = d / (1 - Math.abs(2 * l - 1));
  const h =
    max === r ? ((g - b) / d + 6) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return { h: h * 60, s: s * 100, l: l * 100 };
}

/** Hue, saturation and lightness as `#rrggbb`. */
export function fromHsl({ h, s, l }: Hsl): string {
  const sat = s / 100;
  const light = l / 100;
  const f = (n: number) => {
    const k = (n + h / 30) % 12;
    return light - sat * Math.min(light, 1 - light) * Math.max(-1, Math.min(k - 3, 9 - k, 1));
  };
  return "#" + [f(0), f(8), f(4)].map((v) => hex2(v * 255)).join("");
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
  sheen: Sheen | null;
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
