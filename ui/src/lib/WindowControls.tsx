// Minimize / maximize / close for the custom title bar (the window has no system frame). On macOS
// the system's buttons do those, on the left, and only the switch to the mini player is here.
import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { MAC, MOD, type Translate } from "./format";
import Tip from "./Tip";

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

export default function WindowControls({ t, onMini }: { t: Translate; onMini: () => void }) {
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
      <Tip text={`${t("mini_player")} (${MOD}+M)`}>
        <button className={button} onClick={onMini} aria-label={t("mini_player")}>
          <Glyph d="M0.5 0.5h9v9h-9zM4.5 5.5h3v2h-3z" />
        </button>
      </Tip>
      {!MAC && (
        <>
          <Tip text={t("window_minimize")}>
            <button
              className={button}
              onClick={() => win.minimize()}
              aria-label={t("window_minimize")}
            >
              <Glyph d="M0 5.5h10" />
            </button>
          </Tip>
          <Tip text={toggle}>
            <button className={button} onClick={() => win.toggleMaximize()} aria-label={toggle}>
              <Glyph d={maximized ? "M0.5 2.5h7v7h-7zM2.5 2.5v-2h7v7h-2" : "M0.5 0.5h9v9h-9z"} />
            </button>
          </Tip>
          <Tip text={t("window_close")}>
            <button
              className={`${base} hover:bg-[#c42b1c] hover:text-white`}
              onClick={() => win.close()}
              aria-label={t("window_close")}
            >
              <Glyph d="M0.5 0.5l9 9M9.5 0.5l-9 9" />
            </button>
          </Tip>
        </>
      )}
    </div>
  );
}
