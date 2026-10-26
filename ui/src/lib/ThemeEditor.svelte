<script lang="ts">
  // Side panel for editing a theme; the page shows the draft live while it is open.
  import { api } from "./api";
  import type { Translate } from "./format";
  import type { Background, Library } from "./types";
  import Icon from "./Icon.svelte";
  import {
    TOKENS,
    forgetImage,
    isColor,
    joinColor,
    paletteColors,
    splitColor,
    type Draft,
  } from "./theme";

  let {
    library,
    t,
    draft = $bindable(),
    onclose,
  }: { library: Library; t: Translate; draft: Draft; onclose: () => void } = $props();

  // What Reset goes back to.
  const initial: Draft = $state.snapshot(draft);
  const builtins = $derived(library.themes.filter((th) => !th.custom));
  const baseColors = $derived(paletteColors(draft.base));
  /** bg so opaque that a background image barely shows through it. */
  const bgHidesImage = $derived(splitColor(value("bg")).alpha >= 0.9);
  /** Text being typed into a colour field, which may not be a colour yet. */
  let typing = $state<Record<string, string>>({});
  let error = $state<string | null>(null);
  let saving = $state(false);

  function value(token: string): string {
    return draft.colors[token] ?? baseColors[token] ?? "";
  }

  function setColor(token: string, color: string) {
    draft.colors = { ...draft.colors, [token]: color };
  }

  function clearColor(token: string) {
    const { [token]: _, ...rest } = draft.colors;
    draft.colors = rest;
    delete typing[token];
  }

  /** Change the colour or the opacity of a token, keeping the other. */
  function setPart(token: string, part: { hex?: string; alpha?: number }) {
    const current = splitColor(value(token));
    delete typing[token];
    setColor(token, joinColor(part.hex ?? current.hex, part.alpha ?? current.alpha));
  }

  function setBackground(part: Partial<Background>) {
    if (draft.background) draft.background = { ...draft.background, ...part };
  }

  async function pickImage() {
    error = null;
    try {
      const image = await api.pickThemeImage();
      if (!image) return;
      forgetImage(image);
      draft.background = { fit: "cover", blur: 0, dim: 0, ...draft.background, image };
    } catch (e) {
      error = String(e);
    }
  }

  function onText(token: string, text: string) {
    typing[token] = text;
    if (isColor(text)) setColor(token, text.trim());
  }

  function reset() {
    draft = structuredClone(initial);
    typing = {};
    error = null;
  }

  async function save() {
    saving = true;
    error = null;
    try {
      await api.saveTheme(
        draft.name,
        draft.base,
        $state.snapshot(draft.colors),
        $state.snapshot(draft.background),
      );
      onclose();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<!-- Painted solid with the base palette's panel over its bg and backdrop, so it stays readable
     whatever the theme colours and background image are. -->
<aside
  class="editor"
  aria-label={t("theme_editor")}
  style:background="linear-gradient({baseColors.panel}, {baseColors.panel}),
    linear-gradient({baseColors.bg}, {baseColors.bg}), {baseColors.backdrop}"
>
  <div class="head">
    <h2>{t("theme_editor")}</h2>
    <button class="icon" onclick={onclose} title={t("theme_close")} aria-label={t("theme_close")}>
      <Icon name="close" size={18} />
    </button>
  </div>

  <div class="body">
    <label class="field">
      <span>{t("theme_base")}</span>
      <select bind:value={draft.base}>
        {#each builtins as th (th.id)}
          <option value={th.id}>{th.name}</option>
        {/each}
      </select>
    </label>

    <section>
      <h3>{t("theme_background")}</h3>
      <span class="image-name" title={draft.background?.image}>
        {draft.background?.image ?? t("theme_no_image")}
      </span>
      <div class="image">
        <button class="button small" onclick={pickImage}>{t("theme_choose_image")}</button>
        <button
          class="button small"
          onclick={() => (draft.background = null)}
          disabled={!draft.background}
        >
          {t("theme_remove_image")}
        </button>
      </div>
      {#if draft.background}
        {@const bg = draft.background}
        {#if bgHidesImage}
          <p class="hint">{t("theme_bg_opaque")}</p>
        {/if}
        <label class="setting">
          <span>{t("theme_fit")}</span>
          <select
            value={bg.fit}
            onchange={(e) => setBackground({ fit: e.currentTarget.value as Background["fit"] })}
          >
            <option value="cover">{t("theme_fit_cover")}</option>
            <option value="contain">{t("theme_fit_contain")}</option>
            <option value="tile">{t("theme_fit_tile")}</option>
          </select>
        </label>
        <label class="setting">
          <span>{t("theme_blur")}</span>
          <input
            type="range"
            min="0"
            max="40"
            step="1"
            value={bg.blur}
            oninput={(e) => setBackground({ blur: e.currentTarget.valueAsNumber })}
          />
          <span class="num">{bg.blur}px</span>
        </label>
        <label class="setting">
          <span>{t("theme_dim")}</span>
          <input
            type="range"
            min="0"
            max="100"
            step="1"
            value={Math.round(bg.dim * 100)}
            oninput={(e) => setBackground({ dim: e.currentTarget.valueAsNumber / 100 })}
          />
          <span class="num">{Math.round(bg.dim * 100)}%</span>
        </label>
      {/if}
    </section>

    <section>
      <h3>{t("theme_colors")}</h3>
      <div class="tokens">
        {#each TOKENS as token (token)}
          {@const set = token in draft.colors}
          {@const bad = token in typing && !isColor(typing[token])}
          {@const parts = splitColor(value(token))}
          <div class="token" class:set>
            <span class="name">{token}</span>
            <input
              type="color"
              value={parts.hex}
              oninput={(e) => setPart(token, { hex: e.currentTarget.value })}
              aria-label={token}
            />
            <input
              class="alpha"
              type="range"
              min="0"
              max="100"
              step="1"
              value={Math.round(parts.alpha * 100)}
              oninput={(e) => setPart(token, { alpha: e.currentTarget.valueAsNumber / 100 })}
              title="{t('theme_opacity')} {Math.round(parts.alpha * 100)}%"
              aria-label="{token} {t('theme_opacity')}"
            />
            <input
              class="text"
              class:bad
              value={typing[token] ?? value(token)}
              oninput={(e) => onText(token, e.currentTarget.value)}
              onblur={() => delete typing[token]}
              spellcheck="false"
              aria-label={token}
            />
            <button
              class="icon small"
              onclick={() => clearColor(token)}
              disabled={!set}
              title={t("theme_reset")}
              aria-label={t("theme_reset")}
            >
              <Icon name="close" size={14} />
            </button>
          </div>
        {/each}
      </div>
    </section>
  </div>

  {#if error}
    <div class="error" role="alert">{error}</div>
  {/if}

  <div class="foot">
    <label class="field">
      <span>{t("theme_name")}</span>
      <input bind:value={draft.name} spellcheck="false" />
    </label>
    <div class="actions">
      <button class="button" onclick={reset}>{t("theme_reset")}</button>
      <button class="button primary" onclick={save} disabled={saving || !draft.name.trim()}>
        {t("theme_save_as")}
      </button>
    </div>
  </div>
</aside>

<style>
  .editor {
    position: fixed;
    /* Below the title bar. */
    top: 40px;
    right: 0;
    bottom: 0;
    z-index: 20;
    width: 380px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px 16px;
    background: var(--panel);
    border-left: 1px solid var(--line);
    box-shadow: -12px 0 32px rgba(0, 0, 0, 0.3);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h2 {
    margin: 0;
    font-size: 15px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--faint);
  }
  .field select,
  .field input,
  .text {
    height: 30px;
    padding: 0 8px;
    border-radius: 7px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    color: var(--text);
    font: inherit;
    font-size: 13px;
    font-weight: 400;
    letter-spacing: 0;
    text-transform: none;
    outline: none;
  }
  .field select:focus,
  .field input:focus,
  .text:focus {
    border-color: var(--accent);
  }
  .body {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 18px;
    margin: 0 -6px;
    padding: 0 6px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  h3 {
    margin: 0 0 2px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--faint);
  }
  .image {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .image-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
    color: var(--muted);
  }
  .hint {
    margin: 0;
    padding: 6px 8px;
    border-radius: 7px;
    font-size: 12px;
    color: var(--text);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .setting {
    display: grid;
    grid-template-columns: 72px 1fr 44px;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .setting select {
    grid-column: 2 / 4;
    height: 28px;
    padding: 0 6px;
    border-radius: 7px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    color: var(--text);
    outline: none;
  }
  .setting .num {
    text-align: right;
  }
  input[type="range"] {
    width: 100%;
    margin: 0;
    accent-color: var(--accent);
  }
  .tokens {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .token {
    display: grid;
    grid-template-columns: 1fr 30px 56px 112px 22px;
    align-items: center;
    gap: 6px;
  }
  .name {
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12px;
    color: var(--muted);
  }
  .token.set .name {
    color: var(--text);
    font-weight: 700;
  }
  input[type="color"] {
    width: 30px;
    height: 26px;
    padding: 2px;
    border-radius: 6px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    cursor: pointer;
  }
  .text {
    height: 26px;
    font-family: ui-monospace, "Cascadia Mono", Consolas, monospace;
    font-size: 12px;
  }
  .text.bad {
    border-color: var(--danger);
  }
  .icon {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 7px;
    color: var(--muted);
  }
  .icon:hover:not(:disabled) {
    color: var(--text);
    background: var(--panel-2);
  }
  .icon.small {
    width: 22px;
    height: 22px;
  }
  .icon:disabled {
    opacity: 0.25;
  }
  .error {
    font-size: 12px;
    color: var(--danger);
    white-space: pre-line;
  }
  .foot {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding-top: 12px;
    border-top: 1px solid var(--line);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .button {
    height: 32px;
    padding: 0 14px;
    border-radius: 8px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    color: var(--text);
    font-weight: 600;
  }
  .button:hover:not(:disabled) {
    border-color: var(--faint);
  }
  .button.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-ink);
  }
  .button.small {
    height: 28px;
    padding: 0 10px;
    font-size: 12px;
    white-space: nowrap;
  }
  .button:disabled {
    opacity: 0.5;
  }
</style>
