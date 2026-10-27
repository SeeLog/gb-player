// Our own right-click menu, in place of the WebView's (Back, Refresh, Print…): see App.tsx for
// what it holds where.
import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { createPortal } from "react-dom";

export type MenuItem =
  | "separator"
  | {
      label: string;
      /** A key hint shown on the right, such as `Ctrl+O`. */
      shortcut?: string;
      disabled?: boolean;
      /** Makes it a check item, ticked when true. */
      checked?: boolean;
      onSelect: () => void;
    };

export interface MenuState {
  x: number;
  y: number;
  items: MenuItem[];
}

/** Keeps the menu this far from the window edges. */
const MARGIN = 4;

export default function ContextMenu({ menu, onClose }: { menu: MenuState; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  const [active, setActive] = useState(-1);
  const { items } = menu;

  // Open at the cursor, moved back inside the window when it would stick out.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    setPos({
      left: Math.max(MARGIN, Math.min(menu.x, window.innerWidth - width - MARGIN)),
      top: Math.max(MARGIN, Math.min(menu.y, window.innerHeight - height - MARGIN)),
    });
    setActive(-1);
    el.focus();
  }, [menu]);

  useEffect(() => {
    const onPointerDown = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const close = () => onClose();
    document.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    document.addEventListener("scroll", close, true);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
      document.removeEventListener("scroll", close, true);
    };
  }, [onClose]);

  const enabled = items.flatMap((item, i) => (item !== "separator" && !item.disabled ? [i] : []));

  function choose(i: number) {
    const item = items[i];
    if (item === "separator" || item.disabled) return;
    onClose();
    item.onSelect();
  }

  function onKeyDown(e: KeyboardEvent) {
    // Keys used here must not also reach the app's shortcuts (Space plays, arrows skip).
    e.stopPropagation();
    if (e.key === "Escape" || e.key === "Tab") {
      e.preventDefault();
      onClose();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (enabled.length === 0) return;
      const at = enabled.indexOf(active);
      const step = e.key === "ArrowDown" ? 1 : -1;
      const next = at < 0 ? (step > 0 ? 0 : enabled.length - 1) : at + step;
      setActive(enabled[(next + enabled.length) % enabled.length]);
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      if (active >= 0) choose(active);
    }
  }

  return createPortal(
    <div
      ref={ref}
      role="menu"
      tabIndex={-1}
      aria-activedescendant={active >= 0 ? `menu-item-${active}` : undefined}
      onKeyDown={onKeyDown}
      onContextMenu={(e) => e.preventDefault()}
      className="fixed z-50 min-w-[200px] rounded-lg border border-line p-1 text-[13px] shadow-[0_8px_28px_rgba(0,0,0,0.45)] outline-none"
      // The panel colour is see-through; lay it over the opaque backdrop so the menu is solid.
      style={{
        left: pos?.left ?? menu.x,
        top: pos?.top ?? menu.y,
        // Not `visibility: hidden`, which would stop it taking the focus before it is placed.
        opacity: pos ? 1 : 0,
        background: "linear-gradient(var(--panel-2), var(--panel-2)), var(--backdrop)",
      }}
    >
      {items.map((item, i) =>
        item === "separator" ? (
          <div key={i} role="separator" className="mx-2 my-1 h-px bg-line" />
        ) : (
          <div
            key={i}
            id={`menu-item-${i}`}
            role={item.checked === undefined ? "menuitem" : "menuitemcheckbox"}
            aria-checked={item.checked}
            aria-disabled={item.disabled || undefined}
            onPointerEnter={() => setActive(item.disabled ? -1 : i)}
            onPointerLeave={() => setActive(-1)}
            onClick={() => choose(i)}
            className={`grid cursor-default grid-cols-[16px_1fr_auto] items-center gap-2 rounded-md py-[5px] pr-2.5 pl-1.5 ${
              item.disabled ? "text-faint" : i === active ? "bg-line text-text" : "text-text"
            }`}
          >
            <span className="text-center text-accent" aria-hidden="true">
              {item.checked ? "✓" : ""}
            </span>
            <span className="whitespace-nowrap">{item.label}</span>
            <span className="pl-6 text-xs text-faint">{item.shortcut}</span>
          </div>
        ),
      )}
    </div>,
    document.body,
  );
}
