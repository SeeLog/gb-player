// Line icons drawn on a 24×24 grid.

const paths: Record<string, string> = {
  play: "M8 5.5v13l10.5-6.5z",
  pause: "M7 5h3.5v14H7zM13.5 5H17v14h-3.5z",
  stop: "M6.5 6.5h11v11h-11z",
  prev: "M6 5h2.5v14H6zM19 5.5v13L9.5 12z",
  next: "M15.5 5H18v14h-2.5zM5 5.5v13L14.5 12z",
  open: "M3.5 6.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z",
  search: "M10.5 4a6.5 6.5 0 1 1 0 13 6.5 6.5 0 0 1 0-13zM15.5 15.5 20 20",
  volume: "M4 9.5h3.5L12 5.5v13l-4.5-4H4zM15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11",
  speaker:
    "M7 3.5h10a1.5 1.5 0 0 1 1.5 1.5v14a1.5 1.5 0 0 1-1.5 1.5H7A1.5 1.5 0 0 1 5.5 19V5A1.5 1.5 0 0 1 7 3.5zM12 10.5a3.5 3.5 0 1 1 0 7 3.5 3.5 0 0 1 0-7zM12 7h.01",
  globe:
    "M12 3.5a8.5 8.5 0 1 1 0 17 8.5 8.5 0 0 1 0-17zM3.5 12h17M12 3.5c2.5 2.5 3.5 5.5 3.5 8.5s-1 6-3.5 8.5c-2.5-2.5-3.5-5.5-3.5-8.5s1-6 3.5-8.5z",
  palette:
    "M12 3.5a8.5 8.5 0 0 0 0 17c1.2 0 1.8-.8 1.8-1.7 0-1.3-1.1-1.6-1.1-2.7 0-.9.7-1.6 1.6-1.6h2.2a4 4 0 0 0 4-4c0-3.9-3.8-7-8.5-7zM7.5 12.5h.01M9 8.5h.01M13.5 7.5h.01M17 10h.01",
  edit: "M4 20h4L19 9l-4-4L4 16zM13.5 6.5l4 4",
  close: "M6 6l12 12M18 6 6 18",
  chevron: "M7 10l5 5 5-5",
  plus: "M12 5v14M5 12h14",
  repeat: "M5 11V9.5a3 3 0 0 1 3-3h10M15 3.5l3 3-3 3M19 13v1.5a3 3 0 0 1-3 3H6M9 20.5l-3-3 3-3",
  pin: "M9 4h6M10 4v6l-3 4h10l-3-4V4M12 14v6",
  expand: "M14 4h6v6M20 4l-7 7M10 20H4v-6M4 20l7-7",
  shuffle: "M4 7h3.5c4.5 0 4.5 10 9 10H20M4 17h3.5c1.6 0 2.6-1.3 3.3-3M13.2 10c.7-1.7 1.7-3 3.3-3H20M17.5 4.5 20 7l-2.5 2.5M17.5 14.5 20 17l-2.5 2.5",
};
const filled = new Set(["play", "pause", "stop", "prev", "next"]);

export default function Icon({ name, size = 20 }: { name: string; size?: number }) {
  const fill = filled.has(name);
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      aria-hidden="true"
      className="shrink-0"
      fill={fill ? "currentColor" : "none"}
      stroke={fill ? "none" : "currentColor"}
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d={paths[name]} />
    </svg>
  );
}
