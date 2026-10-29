// The settings window, beside the main window: the choices made once and seldom changed, which
// the main window leaves out (see app/src/gui/prefs.rs).
import { useEffect, useLayoutEffect, useMemo, useState, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, onLibrary, onThemeDraft } from "./api";
import { clearHistory } from "./actions";
import { translator, type Translate } from "./format";
import type { Folder, Library } from "./types";
import Icon from "./Icon";
import { Toggle } from "./NowPlaying";
import Select from "./Select";
import Slider from "./Slider";
import Tip, { Tooltips } from "./Tip";
import { Dialogs } from "./Dialog";
import { applyTheme } from "./theme";

const heading = "text-xs font-bold tracking-[0.08em] uppercase text-muted";
const field =
  "flex h-[30px] items-center justify-between gap-1.5 rounded-[7px] border border-line bg-panel-2 px-2 text-left text-[13px] text-text outline-none focus:border-accent data-popup-open:border-accent";
const button =
  "h-[30px] rounded-[7px] border border-line bg-panel-2 px-3 text-[13px] font-semibold whitespace-nowrap text-text enabled:hover:border-faint disabled:opacity-50";
const iconButton =
  "grid size-[30px] place-items-center rounded-[7px] text-muted hover:bg-panel-2 hover:text-text";
/** The shortest and longest fade-out, in seconds (`FADE_RANGE` in app/src/gui/session.rs). */
const FADE_MIN = 1;
const FADE_MAX = 20;
const FOLDERS: Folder[] = ["themes", "songNames", "playlists"];

export default function SettingsWindow() {
  const [library, setLibrary] = useState<Library | null>(null);
  const t = useMemo(() => translator(library?.text ?? {}), [library?.text]);
  // Whether the theme editor is open, which has the theme to itself until it closes.
  const [editing, setEditing] = useState(false);

  useEffect(() => {
    const unlisten = [onLibrary(setLibrary), onThemeDraft((d) => setEditing(!!d))];
    api.library().then(setLibrary);
    return () => unlisten.forEach((u) => void u.then((f) => f()));
  }, []);

  // In the chosen theme, so a change of it shows here as well.
  const theme = library?.themes.find((th) => th.id === library.theme);
  useLayoutEffect(() => {
    if (theme) applyTheme(theme.base, theme.colors, theme.sheen);
  }, [theme]);
  useEffect(() => {
    if (library) document.documentElement.lang = library.language;
  }, [library?.language]);

  useEffect(() => {
    // Escape in an open list or a question closes just that.
    const onKey = (e: KeyboardEvent) => {
      const inPopup =
        e.target instanceof Element &&
        e.target.closest('[data-base-ui-focusable], [role="alertdialog"]');
      if (e.key === "Escape" && !inPopup) getCurrentWindow().close();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  if (!library) return null;
  return (
    <>
      <Settings library={library} t={t} editing={editing} />
      <Tooltips />
      <Dialogs />
    </>
  );
}

function Settings({ library, t, editing }: { library: Library; t: Translate; editing: boolean }) {
  // Shown as it is dragged; the player is told each step.
  const [fade, setFade] = useState(library.fadeSecs);
  useEffect(() => setFade(library.fadeSecs), [library.fadeSecs]);
  const history = library.playlists.find((l) => l.kind === "history");

  return (
    // Painted solid: the panel over the bg and the backdrop, as the main window's panels.
    <main
      className="flex h-full flex-col gap-3 px-4 pb-4 [background:linear-gradient(var(--panel),var(--panel)),linear-gradient(var(--bg),var(--bg)),var(--backdrop)]"
      aria-label={t("settings")}
    >
      {/* The window has no system frame: this bar is its title bar. */}
      <div data-tauri-drag-region className="-mx-4 flex items-center justify-between px-4 pt-3">
        <h2 className="pointer-events-none text-[15px] font-bold">{t("settings")}</h2>
        <Tip text={t("settings_close")}>
          <button
            className={iconButton}
            onClick={() => getCurrentWindow().close()}
            aria-label={t("settings_close")}
          >
            <Icon name="close" size={18} />
          </button>
        </Tip>
      </div>

      <div className="-mx-1.5 flex flex-1 flex-col gap-5 overflow-y-auto px-1.5">
        <Section title={t("settings_appearance")}>
          <Field label={t("theme")} hint={editing ? t("settings_theme_editing") : undefined}>
            <div className="flex gap-1.5">
              <Select
                className={`${field} min-w-0 flex-1`}
                valueClassName="truncate"
                label={t("theme")}
                tip={false}
                value={library.theme}
                options={library.themes.map((th) => ({ value: th.id, label: th.name }))}
                onChange={(id) => api.setTheme(id)}
                disabled={editing}
              />
              <button className={button} onClick={() => api.openThemeEditor()}>
                {t("edit_theme")}
              </button>
            </div>
          </Field>
          <Field label={t("settings_fps")} hint={t("settings_fps_hint")}>
            <Select
              className={field}
              label={t("settings_fps")}
              tip={false}
              value={String(library.fps)}
              options={[
                { value: "0", label: t("settings_fps_auto", { n: String(library.displayRate) }) },
                ...library.fpsChoices.map((n) => ({
                  value: String(n),
                  label: t("settings_fps_n", { n: String(n) }),
                })),
              ]}
              onChange={(n) => api.setFps(Number(n))}
            />
          </Field>
          <Field label={t("language")}>
            <Select
              className={field}
              label={t("language")}
              tip={false}
              value={library.language}
              options={library.languages.map((l) => ({ value: l.code, label: l.name }))}
              onChange={(code) => api.setLanguage(code)}
            />
          </Field>
        </Section>

        <Section title={t("settings_playback")}>
          <Field label={t("settings_fade")} hint={t("settings_fade_hint")}>
            <div className="flex items-center gap-2.5">
              <Slider
                className="flex-1"
                value={fade}
                min={FADE_MIN}
                max={FADE_MAX}
                step={1}
                onChange={(secs) => {
                  setFade(secs);
                  api.setFade(secs);
                }}
                label={t("settings_fade")}
              />
              <span className="num w-12 text-right text-[13px] text-text">
                {t("settings_seconds", { n: String(fade) })}
              </span>
            </div>
          </Field>
          <Toggle checked={library.reopen} onChange={(on) => api.setReopen(on)}>
            {t("settings_reopen")}
          </Toggle>
        </Section>

        <Section title={t("history")}>
          <Field label={t("settings_history_len")}>
            <div className="flex gap-1.5">
              <Select
                className={`${field} min-w-0 flex-1`}
                label={t("settings_history_len")}
                tip={false}
                value={String(library.historyLen)}
                options={library.historyLens.map((n) => ({
                  value: String(n),
                  label: t("settings_songs", { n: String(n) }),
                }))}
                onChange={(n) => api.setHistoryLen(Number(n))}
              />
              <button
                className={button}
                onClick={() => clearHistory(t)}
                disabled={!history || history.entries.length === 0}
              >
                {t("history_clear")}
              </button>
            </div>
          </Field>
        </Section>

        <Section title={t("settings_folders")}>
          <p className="-mt-1 text-xs text-muted">{t("settings_folders_hint")}</p>
          {FOLDERS.map((folder) => (
            <div key={folder} className="flex items-center justify-between gap-3">
              <span className="text-[13px] text-text">{t(`settings_folder_${folder}`)}</span>
              <button className={button} onClick={() => api.openFolder(folder)}>
                {t("settings_open_folder")}
              </button>
            </div>
          ))}
        </Section>
      </div>
    </main>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3">
      <h3 className={heading}>{title}</h3>
      {children}
    </section>
  );
}

/** A setting with its name above it, and a note under it when `hint` says. */
function Field({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-[13px] text-muted">{label}</span>
      {children}
      {hint && <p className="text-xs text-muted">{hint}</p>}
    </div>
  );
}
