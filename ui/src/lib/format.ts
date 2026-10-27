import type { Length } from "./types";

export type Translate = (key: string, args?: Record<string, string | number>) => string;

/** A translator for `table`; `{name}` placeholders are replaced by `args`. */
export function translator(table: Record<string, string>): Translate {
  return (key, args = {}) =>
    (table[key] ?? key).replace(/\{(\w+)\}/g, (m, name: string) =>
      name in args ? String(args[name]) : m,
    );
}

export function fmtTime(secs: number): string {
  const s = Math.max(0, secs);
  const m = Math.floor(s / 60);
  return `${m}:${(s - m * 60).toFixed(1).padStart(4, "0")}`;
}

/** `m:ss`, for whole-song lengths in the list. */
export function fmtDuration(secs: number): string {
  const s = Math.max(0, Math.round(secs));
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
}

/** The player's fade-out (`Player::fade_secs` in gb-music), which the GUI leaves at its default. */
const FADE_SECS = 8;

/**
 * How long the song plays with the given loop setting: one pass through the loop when it loops
 * forever, or until the fade-out after the last loop has finished.
 */
export function playTime(length: Length, loopCount: number | null): number | null {
  switch (length.kind) {
    case "loops":
      return loopCount === null
        ? length.intro + length.period
        : length.intro + length.period * Math.max(loopCount, 1) + FADE_SECS;
    case "ends":
      return length.at;
    case "unknown":
      return null;
  }
}

/** The tooltip for `playTime`: the intro and loop lengths and what the total covers. */
export function playTimeDetail(t: Translate, length: Length, loopCount: number | null): string {
  const detail = describe(t, length);
  if (length.kind !== "loops") return detail;
  const total =
    loopCount === null
      ? t("duration_one_pass")
      : t("duration_with_fade", { n: Math.max(loopCount, 1), fade: FADE_SECS });
  return `${detail}\n${total}`;
}

export function describe(t: Translate, length: Length | null): string {
  if (!length) return t("analyzing");
  switch (length.kind) {
    case "loops":
      return t("length_loops", { intro: fmtTime(length.intro), loop: fmtTime(length.period) });
    case "ends":
      return t("length_ends", { at: fmtTime(length.at) });
    case "unknown":
      return t("length_unknown");
  }
}
