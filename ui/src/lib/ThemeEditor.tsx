// The theme editor, in a window of its own beside the main window, which shows the draft live
// while the editor is open.
import { useEffect, useLayoutEffect, useMemo, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, onLibrary, sendThemeDraft } from "./api";
import { translator, type Translate } from "./format";
import type { Background, Console, Library, Sheen } from "./types";
import Icon from "./Icon";
import {
  BASE_SHEENS,
  BODY_COLORS,
  FALLBACKS,
  TOKENS,
  applyTheme,
  forgetImage,
  isColor,
  joinColor,
  paletteColors,
  sheenGradient,
  splitColor,
  type Draft,
} from "./theme";
import Tip, { Tooltips } from "./Tip";
import ColorPicker from "./ColorPicker";
import Select from "./Select";
import Slider from "./Slider";

const heading = "mb-0.5 text-xs font-bold tracking-[0.08em] uppercase text-muted";
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
/** Lays out a drop-down's value and arrow, and marks it while its list is open. */
const choice = "flex items-center justify-between gap-1.5 text-left data-popup-open:border-accent";
const FITS: Background["fit"][] = ["cover", "contain", "tile"];
const CONSOLES: Console[] = ["dmg", "gbc"];
/** The most colours a sheen may run through (as in themes.rs). */
const MAX_SHEEN_COLORS = 8;
const mono = "font-[ui-monospace,'Cascadia_Mono',Consolas,monospace] text-xs";

/** The editor's window: starts from the chosen theme and sends each change to the main window. */
export default function ThemeEditorWindow() {
  const [library, setLibrary] = useState<Library | null>(null);
  const [draft, setDraft] = useState<Draft | null>(null);
  const t = useMemo(() => translator(library?.text ?? {}), [library?.text]);

  useEffect(() => {
    const unlisten = onLibrary(setLibrary);
    api.library().then((l) => {
      setLibrary(l);
      const theme = l.themes.find((th) => th.id === l.theme);
      if (!theme) return;
      setDraft({
        name: theme.custom ? theme.name : translator(l.text)("theme_new_name"),
        base: theme.base,
        colors: { ...theme.colors },
        background: theme.background && { ...theme.background },
        console: theme.console,
        sheen: theme.sheen && structuredClone(theme.sheen),
      });
    });
    return () => void unlisten.then((f) => f());
  }, []);

  useEffect(() => {
    if (draft) sendThemeDraft(draft);
  }, [draft]);

  // The window itself is in the base palette as it is, so it stays readable whatever the draft.
  const base = draft?.base;
  useLayoutEffect(() => {
    if (base) applyTheme(base, {}, null);
  }, [base]);
  useEffect(() => {
    if (library) document.documentElement.lang = library.language;
  }, [library?.language]);

  if (!library || !draft) return null;
  return (
    <>
      <ThemeEditor
        library={library}
        t={t}
        draft={draft}
        setDraft={setDraft}
        onClose={() => getCurrentWindow().close()}
      />
      <Tooltips />
    </>
  );
}

function ThemeEditor({
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
    // Escape in an open list or menu closes just that.
    const onKey = (e: KeyboardEvent) => {
      const inPopup =
        e.target instanceof Element &&
        e.target.closest('[data-base-ui-focusable], [role="alertdialog"]');
      if (e.key === "Escape" && !inPopup) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const value = (token: string): string =>
    draft.colors[token] ??
    (baseColors[token] || (token in FALLBACKS ? value(FALLBACKS[token]) : ""));
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
      await api.saveTheme(draft);
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  const bg = draft.background;
  return (
    // Painted solid with the base palette's panel over its bg and backdrop.
    <main
      className="flex h-full flex-col gap-3 px-4 pb-3.5"
      aria-label={t("theme_editor")}
      style={{
        background: `linear-gradient(${baseColors.panel}, ${baseColors.panel}), linear-gradient(${baseColors.bg}, ${baseColors.bg}), ${baseColors.backdrop}`,
      }}
    >
      {/* The window has no system frame: this bar is its title bar. */}
      <div
        data-tauri-drag-region
        className="-mx-4 flex items-center justify-between px-4 pt-3"
      >
        <h2 className="pointer-events-none text-[15px] font-bold">{t("theme_editor")}</h2>
        <Tip text={t("theme_close")}>
          <button
            className={`${iconButton} size-[30px]`}
            onClick={onClose}
            aria-label={t("theme_close")}
          >
            <Icon name="close" size={18} />
          </button>
        </Tip>
      </div>

      <div className="-mx-1.5 flex flex-1 flex-col gap-[18px] overflow-y-auto px-1.5">
        <div className={`${heading} flex flex-col gap-1`}>
          <span>{t("theme_base")}</span>
          <Select
            className={`${field} ${choice}`}
            label={t("theme_base")}
            tip={false}
            value={draft.base}
            options={builtins.map((th) => ({ value: th.id, label: th.name }))}
            onChange={(base) => setDraft({ ...draft, base })}
          />
        </div>

        <section className="flex flex-col gap-1.5">
          <h3 className={heading}>{t("theme_background")}</h3>
          {bg && (
            <Tip text={bg.image}>
              <span className="truncate text-xs text-muted">{bg.image}</span>
            </Tip>
          )}
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
              <div className={setting}>
                <span>{t("theme_fit")}</span>
                <Select
                  className={`col-span-2 h-7 rounded-[7px] border border-line bg-panel-2 px-1.5 text-text outline-none ${choice}`}
                  label={t("theme_fit")}
                  tip={false}
                  value={bg.fit}
                  options={FITS.map((fit) => ({ value: fit, label: t(`theme_fit_${fit}`) }))}
                  onChange={(fit) => setBackground({ fit: fit as Background["fit"] })}
                />
              </div>
              <div className={setting}>
                <span>{t("theme_blur")}</span>
                <Slider
                  value={bg.blur}
                  max={40}
                  step={1}
                  onChange={(blur) => setBackground({ blur })}
                  label={t("theme_blur")}
                />
                <span className="num text-right">{bg.blur}px</span>
              </div>
              <div className={setting}>
                <span>{t("theme_dim")}</span>
                <Slider
                  value={bg.dim}
                  onChange={(dim) => setBackground({ dim })}
                  label={t("theme_dim")}
                />
                <span className="num text-right">{Math.round(bg.dim * 100)}%</span>
              </div>
            </>
          )}
        </section>

        <section className="flex flex-col gap-1.5">
          <h3 className={heading}>{t("mini_player")}</h3>
          <div className={setting}>
            <span>{t("mini_console")}</span>
            <Select
              className={`col-span-2 h-7 rounded-[7px] border border-line bg-panel-2 px-1.5 text-text outline-none ${choice}`}
              label={t("mini_console")}
              tip={false}
              value={draft.console}
              options={CONSOLES.map((c) => ({ value: c, label: t(`console_${c}`) }))}
              onChange={(console) => setDraft({ ...draft, console: console as Console })}
            />
          </div>
          <div className={setting}>
            <span>{t("mini_body")}</span>
            {/* Each sets all the body's colours at once; they can be changed one by one below. */}
            <div className="col-span-2 flex flex-wrap gap-1.5">
              {BODY_COLORS.map((preset) => {
                const chosen = Object.entries(preset.colors).every(
                  ([token, color]) => value(token).toLowerCase() === color,
                );
                return (
                  <Tip key={preset.name} text={t(preset.name)}>
                    <button
                      className={`grid size-7 place-items-center rounded-full border-2 ${
                        chosen ? "border-accent" : "border-line hover:border-faint"
                      }`}
                      style={{
                        // A clear body over a checkerboard, so it reads as see-through.
                        background: `linear-gradient(${preset.colors.shell}, ${preset.colors.shell}), repeating-conic-gradient(#999 0 25%, #ddd 0 50%) 0 0 / 8px 8px`,
                      }}
                      onClick={() =>
                        setDraft({ ...draft, colors: { ...draft.colors, ...preset.colors } })
                      }
                      aria-label={t(preset.name)}
                      aria-pressed={chosen}
                    >
                      <span
                        className="size-2.5 rounded-full"
                        style={{ background: preset.colors.key }}
                      />
                    </button>
                  </Tip>
                );
              })}
            </div>
          </div>
        </section>

        <SheenSection
          t={t}
          sheen={draft.sheen}
          base={BASE_SHEENS[draft.base] ?? null}
          onChange={(sheen) => setDraft({ ...draft, sheen })}
        />

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
                  <ColorPicker
                    hex={parts.hex}
                    onChange={(hex) => setPart(token, { hex })}
                    label={token}
                  />
                  <Tip text={`${t("theme_opacity")} ${Math.round(parts.alpha * 100)}%`}>
                    <Slider
                      value={parts.alpha}
                      onChange={(alpha) => setPart(token, { alpha })}
                      label={`${token} ${t("theme_opacity")}`}
                      small
                    />
                  </Tip>
                  <input
                    className={`${fieldBase} ${mono} h-[26px] ${bad ? "border-danger" : "border-line"}`}
                    value={text ?? value(token)}
                    onChange={(e) => onText(token, e.currentTarget.value)}
                    onBlur={() => stopTyping(token)}
                    spellCheck={false}
                    aria-label={token}
                  />
                  <Tip text={t("theme_reset")}>
                    <button
                      className={`${iconButton} size-[22px]`}
                      onClick={() => clearColor(token)}
                      disabled={!set}
                      aria-label={t("theme_reset")}
                    >
                      <Icon name="close" size={14} />
                    </button>
                  </Tip>
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
    </main>
  );
}

/** The sheen over the accent's fills: its colours (each a swatch), its angle, none, or back to
 * the base's (`sheen` is then `null`). */
function SheenSection({
  t,
  sheen,
  base,
  onChange,
}: {
  t: Translate;
  sheen: Sheen | null;
  /** The base palette's sheen, if it has one. */
  base: Sheen | null;
  onChange: (sheen: Sheen | null) => void;
}) {
  const shown = sheen ?? base ?? { colors: [], angle: 115 };
  const colors = shown.colors;
  const setColors = (colors: string[]) => onChange({ ...shown, colors });
  return (
    <section className="flex flex-col gap-1.5">
      <h3 className={heading}>{t("theme_sheen")}</h3>
      {colors.length > 0 ? (
        <>
          <div
            className="h-6 rounded-[7px] border border-line"
            style={{ background: sheenGradient(shown) }}
          />
          <div className="flex flex-wrap items-center gap-1.5">
            {colors.map((color, i) => {
              const parts = splitColor(color);
              return (
                <div key={i} className="flex items-center">
                  <ColorPicker
                    hex={parts.hex}
                    onChange={(hex) =>
                      setColors(colors.map((c, j) => (j === i ? joinColor(hex, parts.alpha) : c)))
                    }
                    label={t("theme_sheen_color", { n: i + 1 })}
                  />
                  <Tip text={t("theme_sheen_remove")}>
                    <button
                      className={`${iconButton} size-[18px]`}
                      onClick={() => setColors(colors.filter((_, j) => j !== i))}
                      aria-label={t("theme_sheen_remove")}
                    >
                      <Icon name="close" size={12} />
                    </button>
                  </Tip>
                </div>
              );
            })}
            {colors.length < MAX_SHEEN_COLORS && (
              <Tip text={t("theme_sheen_add")}>
                <button
                  className={`${iconButton} size-[26px]`}
                  onClick={() => setColors([...colors, colors[colors.length - 1]])}
                  aria-label={t("theme_sheen_add")}
                >
                  <Icon name="plus" size={16} />
                </button>
              </Tip>
            )}
          </div>
          <div className={setting}>
            <span>{t("theme_sheen_angle")}</span>
            <Slider
              value={shown.angle}
              max={360}
              step={5}
              onChange={(angle) => onChange({ ...shown, angle })}
              label={t("theme_sheen_angle")}
            />
            <span className="num text-right">{shown.angle}°</span>
          </div>
        </>
      ) : (
        // Starts from the pearly one of Prism, to change from there.
        <button
          className={`${smallButton} self-start`}
          onClick={() => onChange(structuredClone(BASE_SHEENS.prism))}
        >
          {t("theme_sheen_add_sheen")}
        </button>
      )}
      <div className="flex items-center gap-1.5">
        <button
          className={smallButton}
          onClick={() => onChange({ ...shown, colors: [] })}
          disabled={colors.length === 0}
        >
          {t("theme_sheen_none")}
        </button>
        <button className={smallButton} onClick={() => onChange(null)} disabled={sheen === null}>
          {t("theme_reset")}
        </button>
      </div>
    </section>
  );
}
