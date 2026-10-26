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
