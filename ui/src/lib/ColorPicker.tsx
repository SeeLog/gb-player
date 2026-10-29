// A colour swatch that opens a picker beside it, in place of the WebView's colour input: the
// colour square and hue strip, and under them hue, saturation and lightness one at a time, so
// one can change while the others stay.
import { useEffect, useState } from "react";
import { Popover } from "@base-ui/react/popover";
import { HexColorPicker } from "react-colorful";
import { popupClass } from "./ContextMenu";
import type { Translate } from "./format";
import Slider from "./Slider";
import { fromHsl, toHsl, type Hsl } from "./theme";

export default function ColorPicker({
  hex,
  onChange,
  label,
  t,
}: {
  /** The colour as `#rrggbb`. */
  hex: string;
  onChange: (hex: string) => void;
  /** Its name, for screen readers. */
  label: string;
  t: Translate;
}) {
  return (
    <Popover.Root>
      <Popover.Trigger
        aria-label={label}
        className="h-[26px] w-[30px] rounded-md border border-line bg-panel-2 p-[3px] hover:border-faint data-popup-open:border-accent"
      >
        <span className="block size-full rounded-[3px]" style={{ background: hex }} />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Positioner side="left" align="start" sideOffset={8} collisionPadding={4} className="z-50">
          <Popover.Popup className={`${popupClass} flex min-w-0 flex-col gap-1.5 p-2`}>
            <HexColorPicker color={hex} onChange={onChange} className="color-picker" />
            <HslSliders hex={hex} onChange={onChange} t={t} />
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  );
}

function HslSliders({
  hex,
  onChange,
  t,
}: {
  hex: string;
  onChange: (hex: string) => void;
  t: Translate;
}) {
  // Kept here rather than read back from the hex, which rounds them and has no hue for a grey:
  // moving one leaves the others where they were.
  const [hsl, setHsl] = useState(() => toHsl(hex));
  useEffect(() => {
    if (fromHsl(hsl) !== hex.toLowerCase()) setHsl(toHsl(hex));
  }, [hex]);

  const set = (part: Partial<Hsl>) => {
    const next = { ...hsl, ...part };
    setHsl(next);
    onChange(fromHsl(next));
  };
  const { h, s, l } = hsl;
  const at = (hue: number, sat: number, light: number) => `hsl(${hue} ${sat}% ${light}%)`;
  const rows: { key: keyof Hsl; label: string; max: number; track: string }[] = [
    {
      key: "h",
      label: t("color_hue"),
      max: 360,
      track: `linear-gradient(90deg, ${[0, 60, 120, 180, 240, 300, 360].map((x) => at(x, s, l)).join(", ")})`,
    },
    {
      key: "s",
      label: t("color_saturation"),
      max: 100,
      track: `linear-gradient(90deg, ${at(h, 0, l)}, ${at(h, 100, l)})`,
    },
    {
      key: "l",
      label: t("color_lightness"),
      max: 100,
      track: `linear-gradient(90deg, ${at(h, s, 0)}, ${at(h, s, 50)}, ${at(h, s, 100)})`,
    },
  ];
  return (
    <div className="grid w-[176px] grid-cols-[auto_1fr_26px] items-center gap-x-2 text-xs text-muted">
      {rows.map((row) => (
        <div key={row.key} className="contents">
          <span>{row.label}</span>
          <Slider
            value={hsl[row.key]}
            max={row.max}
            step={1}
            onChange={(v) => set({ [row.key]: v })}
            label={row.label}
            track={row.track}
          />
          <span className="num text-right">{Math.round(hsl[row.key])}</span>
        </div>
      ))}
    </div>
  );
}
