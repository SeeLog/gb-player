import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api, onLibrary, onThemeDraft, onTick, previous } from "./lib/api";
import { translator, type Translate } from "./lib/format";
import type { Library } from "./lib/types";
import BigView from "./lib/BigView";
import ContextMenu, { type MenuState } from "./lib/ContextMenu";
import Icon from "./lib/Icon";
import { menuFor, wantsNativeMenu, type MenuActions } from "./lib/menus";
import PlaylistView from "./lib/PlaylistView";
import Tip, { Tooltips } from "./lib/Tip";
import { Dialogs } from "./lib/Dialog";
import Export from "./lib/Export";
import { Toasts } from "./lib/Toasts";
import Sidebar, { type View } from "./lib/Sidebar";
import SongList from "./lib/SongList";
import MiniPlayer from "./lib/MiniPlayer";
import NowPlaying from "./lib/NowPlaying";
import WindowControls from "./lib/WindowControls";
import { applyTheme, imageUrl, type Draft } from "./lib/theme";
import { tickStore } from "./lib/tick";
import { useLeaving } from "./lib/leaving";
import { watchRefreshRate } from "./lib/refresh";

const headerControl =
  "flex h-[30px] flex-none items-center gap-1.5 rounded-lg border border-line bg-panel-2 whitespace-nowrap text-text hover:border-faint";
const headerButton = `${headerControl} px-2.5`;
/** The words on the header's buttons, left out when the window is narrow (the icons stay). */
const headerLabel = "max-[1000px]:hidden";

export default function App() {
  const [library, setLibrary] = useState<Library | null>(null);
  const shownLibrary = useRef(library);
  shownLibrary.current = library;
  /** Shows a new library, fading the whole window over when the theme or language changes. */
  const showLibrary = useCallback((l: Library) => {
    const last = shownLibrary.current;
    const restyled = !!last && (last.theme !== l.theme || last.language !== l.language);
    const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (restyled && document.startViewTransition && !still) {
      document.startViewTransition(() => flushSync(() => setLibrary(l)));
    } else {
      setLibrary(l);
    }
  }, []);
  const [dragging, setDragging] = useState(false);
  const drop = useLeaving(dragging || null, 150);
  const search = useRef<HTMLInputElement>(null);

  const t = useMemo(() => translator(library?.text ?? {}), [library?.text]);
  const hasRom = !!library && library.roms.length > 0;

  // What the middle column shows; back to the ROM when the playlist shown is deleted.
  const [view, setView] = useState<View>({ kind: "rom" });
  const [renaming, setRenaming] = useState<string | null>(null);
  const shownList =
    view.kind === "playlist" ? library?.playlists.find((l) => l.id === view.id) : undefined;
  useEffect(() => {
    if (library && view.kind === "playlist" && !shownList) setView({ kind: "rom" });
  }, [library, view, shownList]);
  // The ROM whose songs are shown, or were before a playlist; when it is closed, the one the
  // Rust side moved on to.
  const [romId, setRomId] = useState<string | null>(null);
  const shownRom =
    library?.roms.find((r) => r.id === romId) ??
    library?.roms.find((r) => r.id === library.shownRom) ??
    library?.roms[0];
  useEffect(() => {
    if (shownRom && shownRom.id !== romId) setRomId(shownRom.id);
  }, [shownRom, romId]);
  const showRom = useCallback((id: string) => {
    setRomId(id);
    setView({ kind: "rom" });
  }, []);
  // The view is remembered for the next run once the one from the last run is restored.
  const restored = useRef(false);
  useEffect(() => {
    if (restored.current) {
      api.setView(shownRom?.id ?? null, view.kind === "playlist" ? view.id : null);
    }
  }, [view, shownRom?.id]);

  /** Open ROMs the user picks, and show the songs of the last one. */
  const pickRom = useCallback(() => {
    api.pickRoms().then((id) => id && showRom(id));
  }, [showRom]);

  useEffect(() => {
    const unlisten = [
      onLibrary(showLibrary),
      // Ticks go to the tick store, not to state: see lib/tick.ts.
      onTick((tick) => tickStore.set(tick)),
      getCurrentWebview().onDragDropEvent((e) => {
        const p = e.payload;
        setDragging(p.type === "enter" || p.type === "over");
        if (p.type === "drop" && p.paths.length > 0) {
          api.openRoms(p.paths).then((id) => id && showRom(id));
        }
      }),
    ];
    api.library().then((l) => {
      setLibrary(l);
      setRomId(l.shownRom);
      if (l.openPlaylist) setView({ kind: "playlist", id: l.openPlaylist });
      restored.current = true;
    });
    return () => unlisten.forEach((u) => void u.then((f) => f()));
  }, [showRom, showLibrary]);

  useEffect(() => {
    if (library) document.documentElement.lang = library.language;
  }, [library?.language]);

  useEffect(watchRefreshRate, []);

  // A draft from the theme editor's window is shown instead of the chosen theme while it is open.
  const [draft, setDraft] = useState<Draft | null>(null);
  useEffect(() => {
    const unlisten = onThemeDraft(setDraft);
    return () => void unlisten.then((f) => f());
  }, []);
  const theme = library?.themes.find((th) => th.id === library.theme);
  // Before paint, so a theme change is in place for the fade (see showLibrary).
  useLayoutEffect(() => {
    if (draft) applyTheme(draft.base, draft.colors, draft.sheen);
    else if (theme) applyTheme(theme.base, theme.colors, theme.sheen);
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

  // The mini player draws no backdrop, so the desktop shows around the console (the window is
  // transparent).
  const mini = !!library?.mini;
  useLayoutEffect(() => {
    document.documentElement.toggleAttribute("data-mini", mini);
  }, [mini]);

  // Whether the main view is up (it is not without a ROM or a list to show), and whether the
  // screen is shown big over it.
  const hasMain =
    !!library &&
    !mini &&
    (hasRom || library.playlists.some((l) => l.kind === "playlist" || l.entries.length > 0));
  const [big, setBig] = useState(false);
  const showBig = useCallback(() => setBig(true), []);
  const hideBig = useCallback(() => setBig(false), []);
  const bigState = useRef({ big, hasMain });
  bigState.current = { big, hasMain };
  useEffect(() => {
    if (!hasMain) setBig(false);
  }, [hasMain]);

  // Our right-click menu in place of the WebView's. Shift+right-click in a dev build still gives
  // the WebView's, for Inspect.
  const [menu, setMenu] = useState<MenuState | null>(null);
  const closeMenu = useCallback(() => setMenu(null), []);
  const latest = useRef({ library, t });
  latest.current = { library, t };
  const actions = useRef<MenuActions>({
    showPlaylist: (id) => setView({ kind: "playlist", id }),
    renamePlaylist: setRenaming,
    pickRom,
    big: () => (bigState.current.hasMain ? bigState.current.big : null),
    setBig,
  });
  useEffect(() => {
    const onContextMenu = async (e: MouseEvent) => {
      const target = e.target as Element;
      if ((import.meta.env.DEV && e.shiftKey) || wantsNativeMenu(target)) return;
      e.preventDefault();
      // The mini player is too small for a menu: its buttons are all there is.
      if (target.closest('[role="menu"]') || latest.current.library?.mini) return;
      const items = await menuFor(
        target,
        latest.current.library,
        latest.current.t,
        actions.current,
      );
      setMenu(items && { x: e.clientX, y: e.clientY, items });
    };
    document.addEventListener("contextmenu", onContextMenu);
    return () => document.removeEventListener("contextmenu", onContextMenu);
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = e.target instanceof HTMLInputElement;
      // Menus and drop-down lists take their own keys (arrows, Space, Escape).
      const inPopup =
        e.target instanceof Element &&
        !!e.target.closest(
          '[data-base-ui-focusable], [role="combobox"], [role="alertdialog"], [role="radio"], [role="switch"]',
        );
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "o") {
        e.preventDefault();
        pickRom();
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
        e.preventDefault();
        search.current?.focus();
      } else if ((e.ctrlKey || e.metaKey) && e.key === ",") {
        e.preventDefault();
        api.openSettings();
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "m") {
        e.preventDefault();
        const library = latest.current.library;
        if (library) api.setMini(!library.mini);
      } else if (typing) {
        if (e.key === "Escape") (e.target as HTMLElement).blur();
      } else if (inPopup) {
        return;
      } else if (e.key.toLowerCase() === "f" && !(e.ctrlKey || e.metaKey || e.altKey)) {
        if (bigState.current.hasMain) setBig((on) => !on);
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
      {!mini && background && backgroundUrl && (
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

      <div className={`relative flex h-full flex-col ${mini ? "" : "bg-bg"}`}>
        {/* The window has no system frame: this bar is the title bar (drag it, double-click
            it). Only the bar itself drags, so clicks on its text fall through to it. */}
        {!mini && (
          <header
            data-tauri-drag-region
            className="flex h-10 flex-none items-center gap-2.5 border-b border-line bg-panel pl-3"
          >
            <div
              data-tauri-drag-region
              className="flex flex-none items-center gap-2 font-bold tracking-[0.02em] whitespace-nowrap *:pointer-events-none"
            >
              <span
                className="size-[22px] rounded-md [background:radial-gradient(circle_at_50%_50%,var(--accent-edge,var(--accent))_0_5px,transparent_5.5px),var(--accent-sheen,linear-gradient(var(--accent-ink),var(--accent-ink)))] shadow-[inset_0_0_0_2px_var(--accent-edge,var(--accent))]"
                aria-hidden="true"
              />
              <span>GB Player</span>
            </div>
            <div data-tauri-drag-region className="flex-1 self-stretch" />
            <Tip text={`${t("open_rom")} (Ctrl+O)`}>
              <button className={headerButton} onClick={pickRom} aria-label={t("open_rom")}>
                <Icon name="open" size={18} />
                <span className={headerLabel}>{t("open_rom")}</span>
              </button>
            </Tip>
            <Tip text={`${t("settings")} (Ctrl+,)`}>
              <button
                className={`${headerControl} w-[30px] justify-center`}
                onClick={() => api.openSettings()}
                aria-label={t("settings")}
              >
                <Icon name="settings" size={18} />
              </button>
            </Tip>
            <WindowControls t={t} onMini={() => api.setMini(true)} />
          </header>
        )}

        {!mini && errorText && (
          <div
            role="alert"
            className="flex items-start gap-2 border-b border-danger/35 bg-danger/16 py-2 pr-2 pl-4 text-danger"
          >
            <span className="min-w-0 flex-1 whitespace-pre-wrap">{errorText}</span>
            <Tip text={t("err_dismiss")}>
              <button
                className="-my-1 grid size-7 flex-none place-items-center rounded-[7px] hover:bg-danger/16"
                onClick={() => {
                  setBackgroundError(null);
                  api.dismissErrors();
                }}
                aria-label={t("err_dismiss")}
              >
                <Icon name="close" size={16} />
              </button>
            </Tip>
          </div>
        )}

        {library?.mini ? (
          <MiniPlayer library={library} t={t} />
        ) : library && hasMain ? (
          <main className="relative grid min-h-0 flex-1 grid-cols-[200px_minmax(0,1fr)_380px]">
            <Sidebar
              library={library}
              t={t}
              view={view}
              setView={setView}
              shownRom={shownRom?.id ?? null}
              showRom={showRom}
              pickRom={pickRom}
              renaming={renaming}
              setRenaming={setRenaming}
            />
            {shownList ? (
              <PlaylistView library={library} list={shownList} t={t} />
            ) : shownRom ? (
              // A new list for each ROM: the search starts empty (the sort carries over).
              <SongList key={shownRom.id} library={library} rom={shownRom} t={t} search={search} />
            ) : (
              <div className="flex flex-col items-center justify-center gap-3 border-r border-line">
                <OpenPrompt t={t} onOpen={pickRom} />
              </div>
            )}
            <NowPlaying library={library} t={t} onBig={showBig} />
            {big && <BigView library={library} t={t} onClose={hideBig} />}
          </main>
        ) : (
          <div className="flex flex-1 flex-col items-center justify-center gap-3">
            <div
              aria-hidden="true"
              className="mb-2 h-[136px] w-[120px] rounded-[10px_10px_18px_10px] border border-line opacity-90 [background:linear-gradient(var(--lcd),var(--lcd))_50%_30%/76px_56px_no-repeat,repeating-linear-gradient(90deg,var(--line)_0_6px,transparent_6px_10px)_50%_6px/70px_10px_no-repeat,var(--panel-2)]"
            />
            <OpenPrompt t={t} onOpen={pickRom} />
          </div>
        )}

        <ContextMenu menu={menu} onClose={closeMenu} />
        <Tooltips />
        <Dialogs />
        <Export t={t} />
        <Toasts />

        {drop.shown && (
          <div
            className={`pointer-events-none fixed inset-0 grid place-items-center bg-[color-mix(in_srgb,var(--backdrop)_75%,transparent)] transition-opacity duration-150 starting:opacity-0 motion-reduce:transition-none ${
              drop.leaving ? "opacity-0" : ""
            }`}
          >
            <div className="rounded-2xl border-2 border-dashed border-accent px-10 py-7 text-lg font-semibold text-accent">
              {t("drop_here")}
            </div>
          </div>
        )}
      </div>
    </>
  );
}

/** Where there is nothing to show: the button to open a ROM, and the file types under it. */
function OpenPrompt({ t, onOpen }: { t: Translate; onOpen: () => void }) {
  return (
    <>
      <button
        className="flex items-center gap-2 rounded-[10px] bg-accent px-[18px] py-2.5 font-semibold text-accent-ink hover:brightness-108"
        onClick={onOpen}
      >
        <Icon name="open" size={18} />
        <span>{t("open_rom")}</span>
      </button>
      <p className="text-xs text-muted">{t("drop_hint")}</p>
    </>
  );
}
