import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api, onLibrary, onTick, previous } from "./lib/api";
import { translator } from "./lib/format";
import type { Library } from "./lib/types";
import Icon from "./lib/Icon";
import SongList from "./lib/SongList";
import NowPlaying from "./lib/NowPlaying";
import ThemeEditor from "./lib/ThemeEditor";
import WindowControls from "./lib/WindowControls";
import { applyTheme, imageUrl, type Draft } from "./lib/theme";
import { tickStore } from "./lib/tick";

const headerControl =
  "flex h-[30px] items-center gap-1.5 rounded-lg border border-line bg-panel-2 text-text hover:border-faint";
const headerButton = `${headerControl} px-2.5`;

export default function App() {
  const [library, setLibrary] = useState<Library | null>(null);
  const [dragging, setDragging] = useState(false);
  const search = useRef<HTMLInputElement>(null);

  const t = useMemo(() => translator(library?.text ?? {}), [library?.text]);
  const hasRom = !!library && library.songs.length > 0;

  useEffect(() => {
    const unlisten = [
      onLibrary(setLibrary),
      // Ticks go to the tick store, not to state: see lib/tick.ts.
      onTick((tick) => tickStore.set(tick)),
      getCurrentWebview().onDragDropEvent((e) => {
        const p = e.payload;
        setDragging(p.type === "enter" || p.type === "over");
        if (p.type === "drop" && p.paths.length > 0) api.openRom(p.paths[0]);
      }),
    ];
    api.library().then(setLibrary);
    return () => unlisten.forEach((u) => void u.then((f) => f()));
  }, []);

  useEffect(() => {
    if (library) document.documentElement.lang = library.language;
  }, [library?.language]);

  // A draft from the theme editor is shown instead of the chosen theme while it is open.
  const [draft, setDraft] = useState<Draft | null>(null);
  const theme = library?.themes.find((th) => th.id === library.theme);
  useEffect(() => {
    if (draft) applyTheme(draft.base, draft.colors);
    else if (theme) applyTheme(theme.base, theme.colors);
  }, [draft, theme]);

  // The background image of the shown theme, loaded as a data: URL.
  const background = draft ? draft.background : (theme?.background ?? null);
  const [backgroundUrl, setBackgroundUrl] = useState<string | null>(null);
  const [backgroundError, setBackgroundError] = useState<string | null>(null);
  const image = background?.image;
  useEffect(() => {
    setBackgroundError(null);
    if (!image) {
      setBackgroundUrl(null);
      return;
    }
    let current = true;
    imageUrl(image).then(
      (url) => current && setBackgroundUrl(url),
      (e) => {
        if (!current) return;
        setBackgroundUrl(null);
        setBackgroundError(String(e));
      },
    );
    return () => {
      current = false;
    };
  }, [image]);
  const errorText = [library?.error, backgroundError].filter(Boolean).join("\n");

  function editTheme() {
    if (!theme) return;
    setDraft({
      name: theme.custom ? theme.name : t("theme_new_name"),
      base: theme.base,
      colors: { ...theme.colors },
      background: theme.background && { ...theme.background },
    });
  }
  const closeEditor = useCallback(() => setDraft(null), []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing =
        e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement;
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "o") {
        e.preventDefault();
        api.pickRom();
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        search.current?.focus();
      } else if (typing) {
        if (e.key === "Escape") (e.target as HTMLElement).blur();
      } else if (e.key === " ") {
        e.preventDefault();
        api.toggle();
      } else if (e.key === "ArrowLeft") {
        previous(tickStore.get());
      } else if (e.key === "ArrowRight") {
        api.skip(1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <>
      {background && backgroundUrl && (
        <div className="fixed inset-0 overflow-hidden" aria-hidden="true">
          <div
            className="absolute bg-center"
            style={{
              backgroundImage: `url(${backgroundUrl})`,
              backgroundSize: background.fit === "tile" ? "auto" : background.fit,
              backgroundRepeat: background.fit === "tile" ? "repeat" : "no-repeat",
              filter: background.blur > 0 ? `blur(${background.blur}px)` : undefined,
              inset: `${-background.blur * 2}px`,
            }}
          />
          <div className="absolute inset-0 bg-black" style={{ opacity: background.dim }} />
        </div>
      )}

      <div className="relative flex h-full flex-col bg-bg">
        {/* The window has no system frame: this bar is the title bar (drag it, double-click
            it). Only the bar itself drags, so clicks on its text fall through to it. */}
        <header
          data-tauri-drag-region
          className="flex h-10 flex-none items-center gap-2.5 border-b border-line bg-panel pl-3"
        >
          <div
            data-tauri-drag-region
            className="flex items-center gap-2 font-bold tracking-[0.02em] *:pointer-events-none"
          >
            <span
              className="size-[22px] rounded-md [background:radial-gradient(circle_at_50%_50%,var(--accent)_0_5px,transparent_5.5px),var(--accent-ink)] shadow-[inset_0_0_0_2px_var(--accent)]"
              aria-hidden="true"
            />
            <span>GB Player</span>
          </div>
          {library?.game && (
            <div
              data-tauri-drag-region
              title={library.romPath ?? ""}
              className="flex min-w-0 items-baseline gap-2 border-l border-line pl-3.5 *:pointer-events-none"
            >
              <span className="truncate font-semibold">{library.game}</span>
              <span className="rounded border border-line bg-panel-2 px-1.5 py-px text-[11px] text-muted">
                {library.title}
              </span>
              <span className="whitespace-nowrap text-muted">
                {t("song_count", { n: library.songs.length })}
              </span>
            </div>
          )}
          <div data-tauri-drag-region className="flex-1 self-stretch" />
          <button className={headerButton} onClick={() => api.pickRom()} title="Ctrl+O">
            <Icon name="open" size={18} />
            <span>{t("open_rom")}</span>
          </button>
          {library && (
            <>
              <label className={headerButton} title={t("theme")}>
                <Icon name="palette" size={18} />
                <select
                  value={library.theme}
                  onChange={(e) => api.setTheme(e.currentTarget.value)}
                  disabled={!!draft}
                  aria-label={t("theme")}
                  className="cursor-pointer bg-transparent outline-none"
                >
                  {library.themes.map((th) => (
                    <option key={th.id} value={th.id}>
                      {th.name}
                    </option>
                  ))}
                </select>
              </label>
              <button
                className={`${headerControl} w-[30px] justify-center`}
                onClick={editTheme}
                title={t("edit_theme")}
                aria-label={t("edit_theme")}
              >
                <Icon name="edit" size={18} />
              </button>
              <label className={headerButton} title={t("language")}>
                <Icon name="globe" size={18} />
                <select
                  value={library.language}
                  onChange={(e) => api.setLanguage(e.currentTarget.value)}
                  className="cursor-pointer bg-transparent outline-none"
                >
                  {library.languages.map((l) => (
                    <option key={l.code} value={l.code}>
                      {l.name}
                    </option>
                  ))}
                </select>
              </label>
            </>
          )}
          <WindowControls t={t} />
        </header>

        {errorText && (
          <div
            role="alert"
            className="border-b border-danger/35 bg-danger/16 px-4 py-2 whitespace-pre-wrap text-danger"
          >
            {errorText}
          </div>
        )}

        {library && hasRom ? (
          <main className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_380px]">
            <SongList library={library} t={t} search={search} />
            <NowPlaying library={library} t={t} />
          </main>
        ) : (
          <div className="flex flex-1 flex-col items-center justify-center gap-[18px] text-muted">
            <div
              aria-hidden="true"
              className="h-[136px] w-[120px] rounded-[10px_10px_18px_10px] border border-line opacity-90 [background:linear-gradient(var(--lcd),var(--lcd))_50%_30%/76px_56px_no-repeat,repeating-linear-gradient(90deg,var(--line)_0_6px,transparent_6px_10px)_50%_6px/70px_10px_no-repeat,var(--panel-2)]"
            />
            <p>{t("drop_hint")}</p>
            <button
              className="flex items-center gap-2 rounded-[10px] bg-accent px-[18px] py-2.5 font-semibold text-accent-ink hover:brightness-108"
              onClick={() => api.pickRom()}
            >
              <Icon name="open" size={18} />
              <span>{t("open_rom")}</span>
            </button>
          </div>
        )}

        {library && draft && (
          <ThemeEditor
            library={library}
            t={t}
            draft={draft}
            setDraft={setDraft}
            onClose={closeEditor}
          />
        )}

        {dragging && (
          <div className="pointer-events-none fixed inset-0 grid place-items-center bg-[color-mix(in_srgb,var(--backdrop)_75%,transparent)]">
            <div className="rounded-2xl border-2 border-dashed border-accent px-10 py-7 text-lg font-semibold text-accent">
              {t("drop_here")}
            </div>
          </div>
        )}
      </div>
    </>
  );
}
