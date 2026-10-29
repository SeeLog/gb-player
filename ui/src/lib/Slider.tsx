// A slider in the app's colours, for the volumes and the theme editor's amounts.
import type { HTMLAttributes, Ref } from "react";
import { Slider as Base } from "@base-ui/react/slider";

/** What a <Tip> around it hands down, besides the props below. */
type Passed = Omit<HTMLAttributes<HTMLDivElement>, "onChange" | "defaultValue"> & {
  ref?: Ref<HTMLDivElement>;
};

export default function Slider({
  value,
  onChange,
  label,
  min = 0,
  max = 1,
  step = 0.01,
  small = false,
  track,
  className = "",
  ...passed
}: Passed & {
  value: number;
  onChange: (value: number) => void;
  /** Its name, for screen readers. */
  label: string;
  min?: number;
  max?: number;
  step?: number;
  /** A thinner track and thumb, for a channel or a colour. */
  small?: boolean;
  /** A background for the whole track in place of the fill, such as the range of a colour. */
  track?: string;
  className?: string;
}) {
  return (
    <Base.Root
      value={value}
      min={min}
      max={max}
      step={step}
      onValueChange={(v) => onChange(v as number)}
      className={className}
      {...passed}
    >
      {/* The control is taller than the track, so it is easy to grab. */}
      <Base.Control className="group flex cursor-pointer touch-none items-center py-1.5 select-none">
        <Base.Track
          className={`w-full rounded-full bg-panel-2 shadow-[inset_0_0_0_1px_var(--line)] ${
            track ? "h-2" : small ? "h-1" : "h-1.5"
          }`}
          style={track ? { background: track } : undefined}
        >
          {!track && <Base.Indicator className="rounded-full bg-accent" />}
          <Base.Thumb
            aria-label={label}
            className={`rounded-full bg-knob shadow-[0_1px_3px_rgba(0,0,0,0.3)] transition-transform group-hover:scale-115 has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-accent motion-reduce:transition-none ${
              small ? "size-3" : "size-4"
            }`}
          />
        </Base.Track>
      </Base.Control>
    </Base.Root>
  );
}
