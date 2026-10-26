<script lang="ts">
  // Side panel for editing a theme; the page shows the draft live while it is open.
  import { api } from "./api";
  import type { Translate } from "./format";
  import type { Library } from "./types";
  import Icon from "./Icon.svelte";
  import { TOKENS, isColor, paletteColors, toHex, type Draft } from "./theme";

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
      await api.saveTheme(draft.name, draft.base, $state.snapshot(draft.colors));
      onclose();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<aside class="editor" aria-label={t("theme_editor")}>
  <div class="head">
    <h2>{t("theme_editor")}</h2>
    <button class="icon" onclick={onclose} title={t("theme_close")} aria-label={t("theme_close")}>
      <Icon name="close" size={18} />
    </button>
  </div>

  <label class="field">
    <span>{t("theme_base")}</span>
    <select bind:value={draft.base}>
      {#each builtins as th (th.id)}
        <option value={th.id}>{th.name}</option>
      {/each}
    </select>
  </label>

  <div class="tokens">
    {#each TOKENS as token (token)}
      {@const set = token in draft.colors}
      {@const bad = token in typing && !isColor(typing[token])}
      <div class="token" class:set>
        <span class="name">{token}</span>
        <input
          type="color"
          value={toHex(value(token))}
          oninput={(e) => {
            delete typing[token];
            setColor(token, e.currentTarget.value);
          }}
          aria-label={token}
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
    top: 0;
    right: 0;
    bottom: 0;
    z-index: 20;
    width: 340px;
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
  .tokens {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0 -6px;
    padding: 0 6px;
  }
  .token {
    display: grid;
    grid-template-columns: 1fr 34px 128px 24px;
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
    width: 34px;
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
    width: 24px;
    height: 24px;
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
  .button:disabled {
    opacity: 0.5;
  }
</style>
