<script lang="ts">
  import { api } from "./api";
  import { describe, fmtTime, type Translate } from "./format";
  import type { Library, Tick } from "./types";
  import Icon from "./Icon.svelte";
  import Scope from "./Scope.svelte";

  let { library, tick, t }: { library: Library; tick: Tick | null; t: Translate } = $props();

  const CHANNELS = ["channel_pulse1", "channel_pulse2", "channel_wave", "channel_noise"];
  const LOOP_COUNTS: (number | null)[] = [null, 1, 2, 3];

  const song = $derived(
    tick?.current != null ? library.songs.find((s) => s.index === tick!.current) : undefined,
  );
  const playState = $derived(tick?.state ?? "none");
  const playing = $derived(playState === "playing");
  const position = $derived(tick?.position ?? 0);
  const length = $derived(song?.length ?? null);

  // Where the playhead is on a bar that covers the intro and one pass of the loop.
  const progress = $derived.by(() => {
    if (!song || !length) return null;
    if (length.kind === "loops") {
      const total = length.intro + length.period;
      const inLoop = position >= length.intro;
      const p = inLoop ? length.intro + ((position - length.intro) % length.period) : position;
      return {
        frac: p / total,
        loopStart: length.intro / total,
        lap: inLoop ? Math.floor((position - length.intro) / length.period) + 1 : 0,
      };
    }
    if (length.kind === "ends") {
      return { frac: Math.min(position / length.at, 1), loopStart: null, lap: 0 };
    }
    return null;
  });

  // Meters with a slowly falling peak marker.
  let held = [0, 0, 0, 0];
  const peaks = $derived.by(() => {
    const levels = tick?.levels ?? [0, 0, 0, 0];
    held = held.map((p, i) => Math.max(levels[i], p - 0.012));
    return held;
  });

  let volume = $state(0.8);
  $effect(() => {
    volume = library.volume;
  });
</script>

<aside class="now">
  <div class="lcd">
    <div class="lcd-head">
      <span class="num">{song ? `#${String(song.index + 1).padStart(2, "0")}` : "--"}</span>
      <span class="lcd-state">{t(`state_${playState}`)}</span>
    </div>
    <div class="lcd-title" title={song?.title}>{song?.title ?? t("not_playing")}</div>
    <div class="scope">
      <Scope samples={tick?.scope ?? []} {volume} active={playing} />
    </div>
  </div>

  <div class="progress">
    <div class="bar" class:indeterminate={!!song && !progress}>
      {#if progress}
        {#if progress.loopStart !== null}
          <div class="intro" style:width="{progress.loopStart * 100}%"></div>
          <div class="marker" style:left="{progress.loopStart * 100}%" title={t("loop_point")}></div>
        {/if}
        <div class="fill" style:width="{progress.frac * 100}%"></div>
        <div class="head" style:left="{progress.frac * 100}%"></div>
      {/if}
    </div>
    <div class="times">
      <span class="num">{fmtTime(position)}</span>
      {#if progress && progress.lap > 0}
        <span class="lap"><Icon name="repeat" size={13} />{t("loop_lap", { n: progress.lap })}</span>
      {:else if progress && progress.loopStart !== null}
        <span class="lap dim">{t("intro")}</span>
      {/if}
      <span class="spacer"></span>
      <span class="num dim">{song ? describe(t, length) : ""}</span>
    </div>
  </div>

  {#if tick?.failure}
    <div class="failure">{tick.failure}</div>
  {/if}

  <div class="transport">
    <button class="round" onclick={() => api.skip(-1)} title={t("prev")} aria-label={t("prev")}>
      <Icon name="prev" />
    </button>
    <button class="round" onclick={() => api.stop()} title={t("stop")} aria-label={t("stop")}>
      <Icon name="stop" />
    </button>
    <button
      class="round big"
      onclick={() => api.toggle()}
      title="{playing ? t('pause') : t('play')} (Space)"
      aria-label={playing ? t("pause") : t("play")}
    >
      <Icon name={playing ? "pause" : "play"} size={28} />
    </button>
    <button class="round" onclick={() => api.skip(1)} title={t("next")} aria-label={t("next")}>
      <Icon name="next" />
    </button>
  </div>

  <div class="group">
    <div class="label">{t("loop_label")}</div>
    <div class="segments" role="radiogroup" aria-label={t("loop_label")}>
      {#each LOOP_COUNTS as n}
        <button
          role="radio"
          aria-checked={library.loopCount === n}
          class:on={library.loopCount === n}
          onclick={() => api.setLoopCount(n)}
          title={n === null ? t("loop_forever") : t("loop_playlist", { n })}
        >
          {n === null ? "∞" : `×${n}`}
        </button>
      {/each}
    </div>
    <div class="hint">
      {library.loopCount === null ? t("loop_forever") : t("loop_playlist", { n: library.loopCount })}
    </div>
    <label class="switch">
      <input
        type="checkbox"
        role="switch"
        checked={library.autoNext}
        onchange={(e) => api.setAutoNext(e.currentTarget.checked)}
      />
      <span class="track"><span class="thumb"></span></span>
      <span>{t("auto_next")}</span>
    </label>
  </div>

  <div class="group volume">
    <Icon name="volume" size={18} />
    <input
      type="range"
      min="0"
      max="1"
      step="0.01"
      bind:value={volume}
      oninput={() => api.setVolume(volume)}
      aria-label={t("volume")}
      title={t("volume")}
      style:--v="{volume * 100}%"
    />
    <span class="num dim">{Math.round(volume * 100)}</span>
  </div>

  <div class="group">
    <div class="label">{t("channels")}</div>
    <div class="channels">
      {#each CHANNELS as key, ch}
        {@const muted = library.muted[ch]}
        <button
          class="channel"
          class:muted
          onclick={() => api.setMuted(ch, !muted)}
          title={t("toggle_mute")}
          aria-pressed={!muted}
        >
          <span class="ch-name"><span class="dot"></span>{t(key)}</span>
          <span class="meter">
            <span class="level" style:width="{Math.min(tick?.levels[ch] ?? 0, 1) * 100}%"></span>
            <span class="peak" style:left="{Math.min(peaks[ch], 1) * 100}%"></span>
          </span>
        </button>
      {/each}
    </div>
  </div>
</aside>

<style>
  .now {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 14px 18px;
    background: var(--panel);
    overflow-y: auto;
  }
  .lcd {
    border-radius: 12px;
    padding: 12px 14px 10px;
    background: linear-gradient(160deg, var(--lcd-hi), var(--lcd) 60%, var(--lcd-lo));
    color: var(--lcd-ink);
    box-shadow:
      inset 0 0 0 1px rgba(0, 0, 0, 0.25),
      inset 0 2px 10px rgba(0, 0, 0, 0.25),
      0 0 0 6px var(--bezel);
  }
  .lcd-head {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
    font-weight: 700;
    opacity: 0.8;
  }
  .lcd-state {
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }
  .lcd-title {
    margin-top: 2px;
    font-size: 18px;
    font-weight: 700;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .scope {
    height: 72px;
    margin-top: 6px;
  }
  .bar {
    position: relative;
    height: 8px;
    border-radius: 4px;
    background: var(--panel-2);
    box-shadow: inset 0 0 0 1px var(--line);
  }
  .bar.indeterminate {
    background: repeating-linear-gradient(
      -45deg,
      var(--panel-2) 0 6px,
      var(--line) 6px 12px
    );
  }
  .intro {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: 4px 0 0 4px;
    background: var(--overlay);
  }
  .fill {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: 4px;
    background: var(--accent);
  }
  .marker {
    position: absolute;
    top: -4px;
    bottom: -4px;
    width: 2px;
    margin-left: -1px;
    background: var(--text);
    opacity: 0.6;
    border-radius: 1px;
  }
  .head {
    position: absolute;
    top: 50%;
    width: 14px;
    height: 14px;
    margin: -7px 0 0 -7px;
    border-radius: 50%;
    background: var(--text);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 40%, transparent);
  }
  .times {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 8px;
    font-size: 12px;
  }
  .lap {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 7px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
  }
  .lap.dim {
    background: var(--panel-2);
    color: var(--muted);
  }
  .spacer {
    flex: 1;
  }
  .dim {
    color: var(--muted);
  }
  .failure {
    color: var(--danger);
    font-size: 12px;
  }
  .transport {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 14px;
  }
  .round {
    display: grid;
    place-items: center;
    width: 42px;
    height: 42px;
    border-radius: 50%;
    color: var(--text);
    background: var(--panel-2);
    border: 1px solid var(--line);
  }
  .round:hover {
    border-color: var(--faint);
  }
  .round.big {
    width: 62px;
    height: 62px;
    background: var(--accent);
    border: 0;
    color: var(--accent-ink);
    box-shadow: 0 6px 18px color-mix(in srgb, var(--accent) 25%, transparent);
  }
  .round.big:hover {
    filter: brightness(1.08);
  }
  .group {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .label {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--faint);
  }
  .segments {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    padding: 3px;
    border-radius: 9px;
    background: var(--panel-2);
    border: 1px solid var(--line);
  }
  .segments button {
    height: 30px;
    border-radius: 6px;
    color: var(--muted);
    font-weight: 600;
  }
  .segments button:hover {
    color: var(--text);
  }
  .segments button.on {
    background: var(--accent);
    color: var(--accent-ink);
  }
  .hint {
    font-size: 12px;
    color: var(--muted);
  }
  .switch {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 4px;
    font-size: 13px;
    color: var(--muted);
    cursor: pointer;
  }
  .switch input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .track {
    position: relative;
    flex: none;
    width: 34px;
    height: 20px;
    border-radius: 10px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    transition: background 0.15s;
  }
  .thumb {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--muted);
    transition:
      transform 0.15s,
      background 0.15s;
  }
  .switch input:checked + .track {
    background: var(--accent);
    border-color: var(--accent);
  }
  .switch input:checked + .track .thumb {
    transform: translateX(14px);
    background: var(--accent-ink);
  }
  .switch input:focus-visible + .track {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .switch:hover {
    color: var(--text);
  }
  .volume {
    flex-direction: row;
    align-items: center;
    gap: 10px;
    color: var(--muted);
  }
  .volume input {
    flex: 1;
    -webkit-appearance: none;
    appearance: none;
    height: 6px;
    border-radius: 3px;
    background: linear-gradient(90deg, var(--accent) var(--v), var(--panel-2) var(--v));
    box-shadow: inset 0 0 0 1px var(--line);
    cursor: pointer;
  }
  .volume input::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--text);
    border: 0;
  }
  .volume span {
    width: 28px;
    text-align: right;
    font-size: 12px;
  }
  .channels {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .channel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 9px 10px;
    border-radius: 9px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    text-align: left;
  }
  .channel:hover {
    border-color: var(--faint);
  }
  .ch-name {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    font-weight: 600;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 6px var(--accent);
  }
  .meter {
    position: relative;
    height: 6px;
    border-radius: 3px;
    background: var(--bg);
    overflow: hidden;
  }
  .level {
    position: absolute;
    inset: 0 auto 0 0;
    background: linear-gradient(90deg, var(--accent-2), var(--accent));
    transition: width 60ms linear;
  }
  .peak {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    background: var(--text);
    opacity: 0.7;
  }
  .channel.muted {
    opacity: 0.45;
  }
  .channel.muted .dot {
    background: var(--faint);
    box-shadow: none;
  }
  .channel.muted .ch-name {
    text-decoration: line-through;
  }
</style>
