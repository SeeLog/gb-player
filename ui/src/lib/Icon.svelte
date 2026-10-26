<script lang="ts">
  // Line icons drawn on a 24×24 grid.
  let { name, size = 20 }: { name: string; size?: number } = $props();

  const paths: Record<string, string> = {
    play: "M8 5.5v13l10.5-6.5z",
    pause: "M7 5h3.5v14H7zM13.5 5H17v14h-3.5z",
    stop: "M6.5 6.5h11v11h-11z",
    prev: "M6 5h2.5v14H6zM19 5.5v13L9.5 12z",
    next: "M15.5 5H18v14h-2.5zM5 5.5v13L14.5 12z",
    open: "M3.5 6.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z",
    search: "M10.5 4a6.5 6.5 0 1 1 0 13 6.5 6.5 0 0 1 0-13zM15.5 15.5 20 20",
    volume: "M4 9.5h3.5L12 5.5v13l-4.5-4H4zM15.5 9a4 4 0 0 1 0 6M18 6.5a7.5 7.5 0 0 1 0 11",
    globe:
      "M12 3.5a8.5 8.5 0 1 1 0 17 8.5 8.5 0 0 1 0-17zM3.5 12h17M12 3.5c2.5 2.5 3.5 5.5 3.5 8.5s-1 6-3.5 8.5c-2.5-2.5-3.5-5.5-3.5-8.5s1-6 3.5-8.5z",
    repeat: "M5 11V9.5a3 3 0 0 1 3-3h10M15 3.5l3 3-3 3M19 13v1.5a3 3 0 0 1-3 3H6M9 20.5l-3-3 3-3",
  };
  const filled = new Set(["play", "pause", "stop", "prev", "next"]);
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  aria-hidden="true"
  fill={filled.has(name) ? "currentColor" : "none"}
  stroke={filled.has(name) ? "none" : "currentColor"}
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
>
  <path d={paths[name]} />
</svg>
