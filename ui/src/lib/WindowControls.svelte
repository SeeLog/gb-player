<script lang="ts">
  // Minimize / maximize / close for the custom title bar (the window has no system frame).
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import type { Translate } from "./format";

  let { t }: { t: Translate } = $props();

  const win = getCurrentWindow();
  let maximized = $state(false);

  onMount(() => {
    const update = () => win.isMaximized().then((m) => (maximized = m));
    update();
    const unlisten = win.onResized(update);
    return () => unlisten.then((f) => f());
  });
</script>

<div class="controls">
  <button
    onclick={() => win.minimize()}
    title={t("window_minimize")}
    aria-label={t("window_minimize")}
  >
    <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5.5h10" /></svg>
  </button>
  <button
    onclick={() => win.toggleMaximize()}
    title={maximized ? t("window_restore") : t("window_maximize")}
    aria-label={maximized ? t("window_restore") : t("window_maximize")}
  >
    <svg viewBox="0 0 10 10" aria-hidden="true">
      {#if maximized}
        <path d="M0.5 2.5h7v7h-7zM2.5 2.5v-2h7v7h-2" />
      {:else}
        <path d="M0.5 0.5h9v9h-9z" />
      {/if}
    </svg>
  </button>
  <button
    class="close"
    onclick={() => win.close()}
    title={t("window_close")}
    aria-label={t("window_close")}
  >
    <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0.5 0.5l9 9M9.5 0.5l-9 9" /></svg>
  </button>
</div>

<style>
  .controls {
    display: flex;
    align-self: stretch;
  }
  button {
    display: grid;
    place-items: center;
    width: 46px;
    color: var(--muted);
  }
  button:hover {
    background: var(--panel-2);
    color: var(--text);
  }
  button.close:hover {
    background: #c42b1c;
    color: #fff;
  }
  button:focus-visible {
    outline-offset: -2px;
  }
  svg {
    width: 10px;
    height: 10px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1;
    shape-rendering: crispEdges;
  }
</style>
