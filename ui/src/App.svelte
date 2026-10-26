<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { api, onLibrary, onTick, previous } from "./lib/api";
  import { translator } from "./lib/format";
  import type { Library, Tick } from "./lib/types";
  import Icon from "./lib/Icon.svelte";
  import SongList from "./lib/SongList.svelte";
  import NowPlaying from "./lib/NowPlaying.svelte";
  import ThemeEditor from "./lib/ThemeEditor.svelte";
  import WindowControls from "./lib/WindowControls.svelte";
  import { applyTheme, imageUrl, type Draft } from "./lib/theme";

  let library = $state<Library | null>(null);
  let tick = $state<Tick | null>(null);
  let dragging = $state(false);
  let search = $state<HTMLInputElement | null>(null);

  const t = $derived(translator(library?.text ?? {}));
  const hasRom = $derived(!!library && library.songs.length > 0);

  onMount(() => {
    const unlisten = [
      onLibrary((l) => (library = l)),
      onTick((v) => (tick = v)),
      getCurrentWebview().onDragDropEvent((e) => {
        const p = e.payload;
        dragging = p.type === "enter" || p.type === "over";
        if (p.type === "drop" && p.paths.length > 0) api.openRom(p.paths[0]);
      }),
    ];
    api.library().then((l) => (library = l));
    return () => unlisten.forEach((u) => u.then((f) => f()));
  });

  $effect(() => {
    if (library) document.documentElement.lang = library.language;
  });

  // A draft from the theme editor is shown instead of the chosen theme while it is open.
  let draft = $state<Draft | null>(null);
  const theme = $derived(library?.themes.find((th) => th.id === library!.theme));
  $effect(() => {
    if (draft) applyTheme(draft.base, draft.colors);
    else if (theme) applyTheme(theme.base, theme.colors);
  });

  // The background image of the shown theme, loaded as a data: URL.
  const background = $derived(draft ? draft.background : (theme?.background ?? null));
  let backgroundUrl = $state<string | null>(null);
  let backgroundError = $state<string | null>(null);
  $effect(() => {
    const image = background?.image;
    backgroundError = null;
    if (!image) {
      backgroundUrl = null;
      return;
    }
    let current = true;
    imageUrl(image).then(
      (url) => current && (backgroundUrl = url),
      (e) => current && ((backgroundUrl = null), (backgroundError = String(e))),
    );
    return () => (current = false);
  });
  const errorText = $derived(
    [library?.error, backgroundError].filter(Boolean).join("\n"),
  );

  function editTheme() {
    if (!theme) return;
    draft = {
      name: theme.custom ? theme.name : t("theme_new_name"),
      base: theme.base,
      colors: { ...theme.colors },
      background: theme.background && { ...theme.background },
    };
  }

  function onKey(e: KeyboardEvent) {
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement;
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "o") {
      e.preventDefault();
      api.pickRom();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
      e.preventDefault();
      search?.focus();
    } else if (typing) {
      if (e.key === "Escape") (e.target as HTMLElement).blur();
    } else if (e.key === " ") {
      e.preventDefault();
      api.toggle();
    } else if (e.key === "ArrowLeft") {
      previous(tick);
    } else if (e.key === "ArrowRight") {
      api.skip(1);
    }
  }
</script>

<svelte:window onkeydown={onKey} />

{#if background && backgroundUrl}
  <div class="backdrop" aria-hidden="true">
    <div
      class="image"
      style:background-image="url({backgroundUrl})"
      style:background-size={background.fit === "tile" ? "auto" : background.fit}
      style:background-repeat={background.fit === "tile" ? "repeat" : "no-repeat"}
      style:filter={background.blur > 0 ? `blur(${background.blur}px)` : null}
      style:inset="{-background.blur * 2}px"
    ></div>
    <div class="dim" style:opacity={background.dim}></div>
  </div>
{/if}

<div class="shell">
  <!-- The window has no system frame: this bar is the title bar (drag it, double-click it). -->
  <header data-tauri-drag-region>
    <div class="brand" data-tauri-drag-region>
      <span class="logo" aria-hidden="true"></span>
      <span>GB Player</span>
    </div>
    {#if library?.game}
      <div class="game" title={library.romPath ?? ""} data-tauri-drag-region>
        <span class="game-name">{library.game}</span>
        <span class="badge">{library.title}</span>
        <span class="count">{t("song_count", { n: library.songs.length })}</span>
      </div>
    {/if}
    <div class="spacer" data-tauri-drag-region></div>
    <button class="open" onclick={() => api.pickRom()} title="Ctrl+O">
      <Icon name="open" size={18} />
      <span>{t("open_rom")}</span>
    </button>
    {#if library}
      <label class="lang" title={t("theme")}>
        <Icon name="palette" size={18} />
        <select
          value={library.theme}
          onchange={(e) => api.setTheme(e.currentTarget.value)}
          disabled={!!draft}
          aria-label={t("theme")}
        >
          {#each library.themes as th (th.id)}
            <option value={th.id}>{th.name}</option>
          {/each}
        </select>
      </label>
      <button
        class="open square"
        onclick={editTheme}
        title={t("edit_theme")}
        aria-label={t("edit_theme")}
      >
        <Icon name="edit" size={18} />
      </button>
      <label class="lang" title={t("language")}>
        <Icon name="globe" size={18} />
        <select value={library.language} onchange={(e) => api.setLanguage(e.currentTarget.value)}>
          {#each library.languages as l (l.code)}
            <option value={l.code}>{l.name}</option>
          {/each}
        </select>
      </label>
    {/if}
    <WindowControls {t} />
  </header>

  {#if errorText}
    <div class="error" role="alert">{errorText}</div>
  {/if}

  {#if library && hasRom}
    <main>
      <SongList {library} {tick} {t} bind:search />
      <NowPlaying {library} {tick} {t} />
    </main>
  {:else}
    <div class="empty">
      <div class="cart" aria-hidden="true"></div>
      <p>{t("drop_hint")}</p>
      <button class="primary" onclick={() => api.pickRom()}>
        <Icon name="open" size={18} />
        <span>{t("open_rom")}</span>
      </button>
    </div>
  {/if}

  {#if library && draft}
    <ThemeEditor
      {library}
      {t}
      bind:draft={() => draft!, (d) => (draft = d)}
      onclose={() => (draft = null)}
    />
  {/if}

  {#if dragging}
    <div class="drop">
      <div>{t("drop_here")}</div>
    </div>
  {/if}
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    overflow: hidden;
  }
  .backdrop .image {
    position: absolute;
    background-position: center;
  }
  .backdrop .dim {
    position: absolute;
    inset: 0;
    background: #000;
  }
  .shell {
    position: relative;
    height: 100%;
    display: flex;
    flex-direction: column;
    background: var(--bg);
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 40px;
    flex: none;
    padding-left: 12px;
    border-bottom: 1px solid var(--line);
    background: var(--panel);
  }
  /* Only the bar itself drags the window, so let clicks on its text fall through to it. */
  .brand > *,
  .game > * {
    pointer-events: none;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 700;
    letter-spacing: 0.02em;
  }
  .logo {
    width: 22px;
    height: 22px;
    border-radius: 6px;
    background:
      radial-gradient(circle at 50% 50%, var(--accent) 0 5px, transparent 5.5px),
      var(--accent-ink);
    box-shadow: inset 0 0 0 2px var(--accent);
  }
  .game {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
    padding-left: 14px;
    border-left: 1px solid var(--line);
  }
  .game-name {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .badge {
    font-size: 11px;
    padding: 1px 6px;
    border-radius: 4px;
    background: var(--panel-2);
    color: var(--muted);
    border: 1px solid var(--line);
  }
  .count {
    color: var(--muted);
    white-space: nowrap;
  }
  .spacer {
    flex: 1;
  }
  .open,
  .lang {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 10px;
    border-radius: 8px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    color: var(--text);
  }
  .open:hover,
  .lang:hover {
    border-color: var(--faint);
  }
  .square {
    width: 30px;
    padding: 0;
    justify-content: center;
  }
  .lang select {
    background: transparent;
    border: 0;
    outline: none;
    cursor: pointer;
  }
  .lang option {
    /* Menus are drawn on their own, so give them a solid colour. */
    background: var(--backdrop);
  }
  .error {
    padding: 8px 16px;
    background: color-mix(in srgb, var(--danger) 16%, transparent);
    color: var(--danger);
    border-bottom: 1px solid color-mix(in srgb, var(--danger) 35%, transparent);
    white-space: pre-wrap;
  }
  main {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 380px;
  }
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 18px;
    color: var(--muted);
  }
  .cart {
    width: 120px;
    height: 136px;
    border-radius: 10px 10px 18px 10px;
    background:
      linear-gradient(var(--lcd), var(--lcd)) 50% 30% / 76px 56px no-repeat,
      repeating-linear-gradient(90deg, var(--line) 0 6px, transparent 6px 10px) 50% 6px / 70px 10px
        no-repeat,
      var(--panel-2);
    border: 1px solid var(--line);
    opacity: 0.9;
  }
  .primary {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 18px;
    border-radius: 10px;
    background: var(--accent);
    color: var(--accent-ink);
    font-weight: 600;
  }
  .primary:hover {
    filter: brightness(1.08);
  }
  .drop {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--backdrop) 75%, transparent);
    pointer-events: none;
  }
  .drop div {
    padding: 28px 40px;
    border: 2px dashed var(--accent);
    border-radius: 16px;
    color: var(--accent);
    font-size: 18px;
    font-weight: 600;
  }
</style>
