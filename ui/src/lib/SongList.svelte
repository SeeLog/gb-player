<script lang="ts">
  import { api } from "./api";
  import { describe, type Translate } from "./format";
  import type { Library, Tick } from "./types";
  import Icon from "./Icon.svelte";

  let {
    library,
    tick,
    t,
    search = $bindable(),
  }: {
    library: Library;
    tick: Tick | null;
    t: Translate;
    search: HTMLInputElement | null;
  } = $props();

  let filter = $state("");
  let list = $state<HTMLElement | null>(null);

  const current = $derived(tick?.current ?? null);
  const playing = $derived(tick?.state === "playing");
  const songs = $derived.by(() => {
    const f = filter.trim().toLowerCase();
    return f ? library.songs.filter((s) => s.title.toLowerCase().includes(f)) : library.songs;
  });

  // Keep the playing song in view when the playlist moves on.
  $effect(() => {
    if (current === null || !list) return;
    list.querySelector(`[data-index="${current}"]`)?.scrollIntoView({ block: "nearest" });
  });
</script>

<section class="songs">
  <div class="search">
    <Icon name="search" size={16} />
    <input bind:this={search} bind:value={filter} placeholder={t("search")} spellcheck="false" />
  </div>
  <div class="list" bind:this={list} role="listbox" aria-label={library.game}>
    {#each songs as song (song.index)}
      {@const active = song.index === current}
      <button
        class="row"
        class:active
        data-index={song.index}
        role="option"
        aria-selected={active}
        onclick={() => api.play(song.index)}
      >
        <span class="no num">
          {#if active && playing}
            <span class="eq" aria-hidden="true"><i></i><i></i><i></i></span>
          {:else}
            {song.index + 1}
          {/if}
        </span>
        <span class="title">{song.title}</span>
        <span class="len num" class:pending={!song.length}>{describe(t, song.length)}</span>
      </button>
    {:else}
      <p class="none">{t("no_results")}</p>
    {/each}
  </div>
</section>

<style>
  .songs {
    display: flex;
    flex-direction: column;
    min-height: 0;
    border-right: 1px solid var(--line);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 12px 16px 8px;
    padding: 0 12px;
    height: 36px;
    border-radius: 9px;
    background: var(--panel);
    border: 1px solid var(--line);
    color: var(--muted);
  }
  .search:focus-within {
    border-color: var(--accent-2);
  }
  .search input {
    flex: 1;
    background: transparent;
    border: 0;
    outline: none;
    color: var(--text);
  }
  .list {
    flex: 1;
    overflow-y: auto;
    padding: 0 8px 12px;
  }
  .row {
    width: 100%;
    display: grid;
    grid-template-columns: 44px minmax(0, 1fr) auto;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border-radius: 8px;
    text-align: left;
  }
  .row:hover {
    background: var(--panel);
  }
  .row.active {
    background: color-mix(in srgb, var(--accent) 14%, var(--panel));
  }
  .row.active .title {
    color: var(--accent);
    font-weight: 600;
  }
  .no {
    color: var(--faint);
    text-align: right;
    padding-right: 6px;
  }
  .title {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .len {
    color: var(--muted);
    font-size: 12px;
  }
  .len.pending {
    color: var(--faint);
    font-style: italic;
  }
  .none {
    color: var(--muted);
    text-align: center;
    margin-top: 32px;
  }
  .eq {
    display: inline-flex;
    align-items: flex-end;
    gap: 2px;
    height: 12px;
  }
  .eq i {
    width: 3px;
    background: var(--accent);
    animation: eq 0.9s ease-in-out infinite;
  }
  .eq i:nth-child(2) {
    animation-delay: -0.3s;
  }
  .eq i:nth-child(3) {
    animation-delay: -0.6s;
  }
  @keyframes eq {
    0%,
    100% {
      height: 3px;
    }
    50% {
      height: 12px;
    }
  }
</style>
