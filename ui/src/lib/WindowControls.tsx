// Minimize / maximize / close for the custom title bar (the window has no system frame).
import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { Translate } from "./format";

const base = "grid w-[46px] place-items-center text-muted focus-visible:outline-offset-[-2px]";
const button = `${base} hover:bg-panel-2 hover:text-text`;

function Glyph({ d }: { d: string }) {
  return (
    <svg
      viewBox="0 0 10 10"
      aria-hidden="true"
      className="size-2.5 fill-none stroke-current"
      strokeWidth="1"
      shapeRendering="crispEdges"
    >
      <path d={d} />
    </svg>
  );
}

export default function WindowControls({ t }: { t: Translate }) {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const win = getCurrentWindow();
    const update = () => win.isMaximized().then(setMaximized);
    update();
    const unlisten = win.onResized(update);
    return () => void unlisten.then((f) => f());
  }, []);

  const win = getCurrentWindow();
  const toggle = maximized ? t("window_restore") : t("window_maximize");
  return (
    <div className="flex self-stretch">
      <button
        className={button}
        onClick={() => win.minimize()}
        title={t("window_minimize")}
        aria-label={t("window_minimize")}
      >
        <Glyph d="M0 5.5h10" />
      </button>
      <button className={button} onClick={() => win.toggleMaximize()} title={toggle} aria-label={toggle}>
        <Glyph d={maximized ? "M0.5 2.5h7v7h-7zM2.5 2.5v-2h7v7h-2" : "M0.5 0.5h9v9h-9z"} />
      </button>
      <button
        className={`${base} hover:bg-[#c42b1c] hover:text-white`}
        onClick={() => win.close()}
        title={t("window_close")}
        aria-label={t("window_close")}
      >
        <Glyph d="M0.5 0.5l9 9M9.5 0.5l-9 9" />
      </button>
    </div>
  );
}
