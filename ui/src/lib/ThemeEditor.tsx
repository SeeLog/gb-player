// Side panel for editing a theme; the page shows the draft live while it is open.
import { useEffect, useMemo, useState } from "react";
import { api } from "./api";
import type { Translate } from "./format";
import type { Background, Library } from "./types";
import Icon from "./Icon";
import {
  TOKENS,
  forgetImage,
  isColor,
  joinColor,
  paletteColors,
  splitColor,
  type Draft,
} from "./theme";

const heading = "mb-0.5 text-[11px] font-bold tracking-[0.08em] uppercase text-faint";
// Class lists are combined only where they set different properties, so no utility overrides
// another.
const fieldBase =
  "rounded-[7px] border bg-panel-2 px-2 font-normal tracking-normal normal-case text-text outline-none focus:border-accent";
const field = `${fieldBase} h-[30px] border-line text-[13px]`;
const buttonBase =
  "rounded-lg border font-semibold enabled:hover:border-faint disabled:opacity-50";
const button = `${buttonBase} h-8 px-3.5 border-line bg-panel-2 text-text`;
const smallButton = `${buttonBase} h-7 px-2.5 text-xs whitespace-nowrap border-line bg-panel-2 text-text`;
const primaryButton = `${buttonBase} h-8 px-3.5 border-accent bg-accent text-accent-ink`;
const iconButton =
  "grid place-items-center rounded-[7px] text-muted enabled:hover:bg-panel-2 enabled:hover:text-text disabled:opacity-25";
const setting = "grid grid-cols-[72px_1fr_44px] items-center gap-2 text-xs text-muted";
const mono = "font-[ui-monospace,'Cascadia_Mono',Consolas,monospace] text-xs";

export default function ThemeEditor({
  library,
  t,
  draft,
  setDraft,
  onClose,
}: {
  library: Library;
  t: Translate;
  draft: Draft;
  setDraft: (d: Draft) => void;
  onClose: () => void;
}) {
  // What Reset goes back to.
  const [initial] = useState(() => structuredClone(draft));
  const builtins = library.themes.filter((th) => !th.custom);
  const baseColors = useMemo(() => paletteColors(draft.base), [draft.base]);
  /** Text being typed into a colour field, which may not be a colour yet. */
  const [typing, setTyping] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const value = (token: string) => draft.colors[token] ?? baseColors[token] ?? "";
  /** bg so opaque that a background image barely shows through it. */
  const bgHidesImage = splitColor(value("bg")).alpha >= 0.9;

  const stopTyping = (token: string) =>
    setTyping(({ [token]: _, ...rest }) => rest);

  const setColor = (token: string, color: string) =>
    setDraft({ ...draft, colors: { ...draft.colors, [token]: color } });

  function clearColor(token: string) {
    const { [token]: _, ...rest } = draft.colors;
    setDraft({ ...draft, colors: rest });
    stopTyping(token);
  }

  /** Change the colour or the opacity of a token, keeping the other. */
  function setPart(token: string, part: { hex?: string; alpha?: number }) {
    const current = splitColor(value(token));
    stopTyping(token);
    setColor(token, joinColor(part.hex ?? current.hex, part.alpha ?? current.alpha));
  }

  function setBackground(part: Partial<Background>) {
    if (draft.background) setDraft({ ...draft, background: { ...draft.background, ...part } });
  }

  async function pickImage() {
    setError(null);
    try {
      const image = await api.pickThemeImage();
      if (!image) return;
      forgetImage(image);
      setDraft({
        ...draft,
        background: { fit: "cover", blur: 0, dim: 0, ...draft.background, image },
      });
    } catch (e) {
      setError(String(e));
    }
  }

  function onText(token: string, text: string) {
    setTyping((typing) => ({ ...typing, [token]: text }));
    if (isColor(text)) setColor(token, text.trim());
  }

  function reset() {
    setDraft(structuredClone(initial));
    setTyping({});
    setError(null);
  }

  async function save() {
    setSaving(true);
    setError(null);
    try {
      await api.saveTheme(draft.name, draft.base, draft.colors, draft.background);
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  const bg = draft.background;
  return (
    // Painted solid with the base palette's panel over its bg and backdrop, so it stays readable
    // whatever the theme colours and background image are.
    <aside
      className="fixed top-10 right-0 bottom-0 z-20 flex w-[380px] flex-col gap-3 border-l border-line px-4 py-3.5 shadow-[-12px_0_32px_rgba(0,0,0,0.3)]"
      aria-label={t("theme_editor")}
      style={{
        background: `linear-gradient(${baseColors.panel}, ${baseColors.panel}), linear-gradient(${baseColors.bg}, ${baseColors.bg}), ${baseColors.backdrop}`,
      }}
    >
      <div className="flex items-center justify-between">
        <h2 className="text-[15px] font-bold">{t("theme_editor")}</h2>
        <button
          className={`${iconButton} size-[30px]`}
          onClick={onClose}
          title={t("theme_close")}
          aria-label={t("theme_close")}
        >
          <Icon name="close" size={18} />
        </button>
      </div>

      <div className="-mx-1.5 flex flex-1 flex-col gap-[18px] overflow-y-auto px-1.5">
        <label className={`${heading} flex flex-col gap-1`}>
          <span>{t("theme_base")}</span>
          <select
            className={field}
            value={draft.base}
            onChange={(e) => setDraft({ ...draft, base: e.currentTarget.value })}
          >
            {builtins.map((th) => (
              <option key={th.id} value={th.id}>
                {th.name}
              </option>
            ))}
          </select>
        </label>

        <section className="flex flex-col gap-1.5">
          <h3 className={heading}>{t("theme_background")}</h3>
          <span className="truncate text-xs text-muted" title={bg?.image}>
            {bg?.image ?? t("theme_no_image")}
          </span>
          <div className="flex items-center gap-1.5">
            <button className={smallButton} onClick={pickImage}>
              {t("theme_choose_image")}
            </button>
            <button
              className={smallButton}
              onClick={() => setDraft({ ...draft, background: null })}
              disabled={!bg}
            >
              {t("theme_remove_image")}
            </button>
          </div>
          {bg && (
            <>
              {bgHidesImage && (
                <p className="rounded-[7px] border border-accent/35 bg-accent/14 px-2 py-1.5 text-xs text-text">
                  {t("theme_bg_opaque")}
                </p>
              )}
              <label className={setting}>
                <span>{t("theme_fit")}</span>
                <select
                  className="col-span-2 h-7 rounded-[7px] border border-line bg-panel-2 px-1.5 text-text outline-none"
                  value={bg.fit}
                  onChange={(e) =>
                    setBackground({ fit: e.currentTarget.value as Background["fit"] })
                  }
                >
                  <option value="cover">{t("theme_fit_cover")}</option>
                  <option value="contain">{t("theme_fit_contain")}</option>
                  <option value="tile">{t("theme_fit_tile")}</option>
                </select>
              </label>
              <label className={setting}>
                <span>{t("theme_blur")}</span>
                <input
                  type="range"
                  min="0"
                  max="40"
                  step="1"
                  value={bg.blur}
                  onChange={(e) => setBackground({ blur: e.currentTarget.valueAsNumber })}
                  className="w-full accent-accent"
                />
                <span className="num text-right">{bg.blur}px</span>
              </label>
              <label className={setting}>
                <span>{t("theme_dim")}</span>
                <input
                  type="range"
                  min="0"
                  max="100"
                  step="1"
                  value={Math.round(bg.dim * 100)}
                  onChange={(e) => setBackground({ dim: e.currentTarget.valueAsNumber / 100 })}
                  className="w-full accent-accent"
                />
                <span className="num text-right">{Math.round(bg.dim * 100)}%</span>
              </label>
            </>
          )}
        </section>

        <section className="flex flex-col gap-1.5">
          <h3 className={heading}>{t("theme_colors")}</h3>
          <div className="flex flex-col gap-1">
            {TOKENS.map((token) => {
              const set = token in draft.colors;
              const text = typing[token];
              const bad = text !== undefined && !isColor(text);
              const parts = splitColor(value(token));
              return (
                <div
                  key={token}
                  className="grid grid-cols-[1fr_30px_56px_112px_22px] items-center gap-1.5"
                >
                  <span className={`${mono} ${set ? "font-bold text-text" : "text-muted"}`}>
                    {token}
                  </span>
                  <input
                    type="color"
                    value={parts.hex}
                    onChange={(e) => setPart(token, { hex: e.currentTarget.value })}
                    aria-label={token}
                    className="h-[26px] w-[30px] cursor-pointer rounded-md border border-line bg-panel-2 p-0.5"
                  />
                  <input
                    type="range"
                    min="0"
                    max="100"
                    step="1"
                    value={Math.round(parts.alpha * 100)}
                    onChange={(e) =>
                      setPart(token, { alpha: e.currentTarget.valueAsNumber / 100 })
                    }
                    title={`${t("theme_opacity")} ${Math.round(parts.alpha * 100)}%`}
                    aria-label={`${token} ${t("theme_opacity")}`}
                    className="w-full accent-accent"
                  />
                  <input
                    className={`${fieldBase} ${mono} h-[26px] ${bad ? "border-danger" : "border-line"}`}
                    value={text ?? value(token)}
                    onChange={(e) => onText(token, e.currentTarget.value)}
                    onBlur={() => stopTyping(token)}
                    spellCheck={false}
                    aria-label={token}
                  />
                  <button
                    className={`${iconButton} size-[22px]`}
                    onClick={() => clearColor(token)}
                    disabled={!set}
                    title={t("theme_reset")}
                    aria-label={t("theme_reset")}
                  >
                    <Icon name="close" size={14} />
                  </button>
                </div>
              );
            })}
          </div>
        </section>
      </div>

      {error && (
        <div className="text-xs whitespace-pre-line text-danger" role="alert">
          {error}
        </div>
      )}

      <div className="flex flex-col gap-2.5 border-t border-line pt-3">
        <label className={`${heading} flex flex-col gap-1`}>
          <span>{t("theme_name")}</span>
          <input
            className={field}
            value={draft.name}
            onChange={(e) => setDraft({ ...draft, name: e.currentTarget.value })}
            spellCheck={false}
          />
        </label>
        <div className="flex justify-end gap-2">
          <button className={button} onClick={reset}>
            {t("theme_reset")}
          </button>
          <button
            className={primaryButton}
            onClick={save}
            disabled={saving || !draft.name.trim()}
          >
            {t("theme_save_as")}
          </button>
        </div>
      </div>
    </aside>
  );
}
