// Text that scrolls sideways, as on an LCD, when it is too long for its box: a second copy follows
// the first, so the loop has no seam, and it rests at the start of each lap. Text that fits stands
// still, and so does all text when the system asks for less motion (cut short with "…" instead).
import { useEffect, useLayoutEffect, useRef, useState } from "react";

/** How fast the text moves, in pixels a second. */
const SPEED = 30;
/** How long it rests at the start of each lap, in milliseconds. */
const REST = 1500;
/** The space between the end of the text and its copy, in pixels. */
const GAP = 40;

export default function Marquee({ text }: { text: string }) {
  const box = useRef<HTMLDivElement>(null);
  const row = useRef<HTMLDivElement>(null);
  const first = useRef<HTMLSpanElement>(null);
  // The width of the text when it does not fit, otherwise 0.
  const [overflow, setOverflow] = useState(0);
  const still = matchMedia("(prefers-reduced-motion: reduce)").matches;

  useLayoutEffect(() => {
    const measure = () => {
      const width = first.current?.offsetWidth ?? 0;
      setOverflow(width > (box.current?.clientWidth ?? 0) ? width : 0);
    };
    measure();
    const observer = new ResizeObserver(measure);
    if (box.current) observer.observe(box.current);
    if (first.current) observer.observe(first.current);
    return () => observer.disconnect();
  }, [text]);

  useEffect(() => {
    if (!overflow || still || !row.current) return;
    const distance = overflow + GAP;
    const duration = REST + (distance / SPEED) * 1000;
    const rest = REST / duration;
    const animation = row.current.animate(
      [
        { transform: "translateX(0)" },
        { transform: "translateX(0)", offset: rest },
        { transform: `translateX(-${distance}px)` },
      ],
      { duration, iterations: Infinity },
    );
    return () => animation.cancel();
  }, [overflow, still, text]);

  const moving = overflow > 0 && !still;
  return (
    <div ref={box} className="overflow-hidden whitespace-nowrap">
      <div ref={row} className={moving ? "flex w-max" : "truncate"}>
        <span ref={first}>{text}</span>
        {moving && (
          <span style={{ paddingLeft: GAP }} aria-hidden="true">
            {text}
          </span>
        )}
      </div>
    </div>
  );
}
