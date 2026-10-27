// Our own right-click menu, in place of the WebView's (Back, Refresh, Print…): see App.tsx for
// what it holds where. An item can open a submenu beside it (on hover, click or →).
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
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
      /** Items shown beside this one instead of choosing it. */
      submenu?: MenuItem[];
      onSelect?: () => void;
    };

export interface MenuState {
  x: number;
  y: number;
  items: MenuItem[];
}

/** Keeps the menu this far from the window edges. */
const MARGIN = 4;

/** Counts the menus opened, so each starts afresh. */
let opened = 0;

export default function ContextMenu({ menu, onClose }: { menu: MenuState; onClose: () => void }) {
  const container = useRef<HTMLDivElement>(null);
  const at = useMemo(() => ({ x: menu.x, y: menu.y }), [menu]);
  const key = useMemo(() => ++opened, [menu]);

  useEffect(() => {
    const inside = (e: Event) => container.current?.contains(e.target as Node);
    const onPointerDown = (e: PointerEvent) => {
      if (!inside(e)) onClose();
    };
    // A long menu scrolls by itself; anything else scrolling moves what it was opened on.
    const onScroll = (e: Event) => {
      if (!inside(e)) onClose();
    };
    const close = () => onClose();
    document.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    document.addEventListener("scroll", onScroll, true);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
      document.removeEventListener("scroll", onScroll, true);
    };
  }, [onClose]);

  return createPortal(
    <div ref={container}>
      <Panel
        key={key}
        items={menu.items}
        at={at}
        idPrefix="menu-item"
        onClose={onClose}
      />
    </div>,
    document.body,
  );
}

/** Where a panel opens: at a point (the cursor), or beside the item that opened it. */
type Anchor = { x: number; y: number } | { beside: DOMRect };

function Panel({
  items,
  at,
  idPrefix,
  onClose,
  onBack,
  keyboard = false,
}: {
  items: MenuItem[];
  at: Anchor;
  idPrefix: string;
  /** Closes the whole menu. */
  onClose: () => void;
  /** Closes this submenu and goes back to its parent. */
  onBack?: () => void;
  /** Opened from the keyboard: starts on the first item. */
  keyboard?: boolean;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  const enabled = items.flatMap((item, i) => (item !== "separator" && !item.disabled ? [i] : []));
  const [active, setActive] = useState(keyboard ? (enabled[0] ?? -1) : -1);
  /** The item whose submenu is open, where that item is, and whether the keyboard opened it. */
  const [open, setOpen] = useState<{ index: number; at: Anchor; keyboard: boolean } | null>(
    null,
  );

  // Open at the cursor (or beside the parent item, on the left when there is no room on the
  // right), moved back inside the window when it would stick out.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const { width, height } = el.getBoundingClientRect();
    let x: number;
    let y: number;
    if ("beside" in at) {
      const r = at.beside;
      x = r.right + width + MARGIN <= window.innerWidth ? r.right - 2 : r.left - width + 2;
      y = r.top - 5;
    } else {
      ({ x, y } = at);
    }
    setPos({
      left: Math.max(MARGIN, Math.min(x, window.innerWidth - width - MARGIN)),
      top: Math.max(MARGIN, Math.min(y, window.innerHeight - height - MARGIN)),
    });
    el.focus();
  }, [at]);

  function openSubmenu(i: number, keyboard: boolean) {
    const rect = ref.current?.querySelector(`#${idPrefix}-${i}`)?.getBoundingClientRect();
    if (rect) setOpen({ index: i, at: { beside: rect }, keyboard });
  }

  function choose(i: number, byKeyboard = false) {
    const item = items[i];
    if (item === "separator" || item.disabled) return;
    if (item.submenu) {
      setActive(i);
      if (open?.index !== i) openSubmenu(i, byKeyboard);
      return;
    }
    onClose();
    item.onSelect?.();
  }

  function onKeyDown(e: KeyboardEvent) {
    // Keys used here must not also reach the app's shortcuts (Space plays, arrows skip).
    e.stopPropagation();
    if (e.key === "Tab") {
      e.preventDefault();
      onClose();
    } else if (e.key === "Escape" || (e.key === "ArrowLeft" && onBack)) {
      e.preventDefault();
      if (onBack) onBack();
      else onClose();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (enabled.length === 0) return;
      const at = enabled.indexOf(active);
      const step = e.key === "ArrowDown" ? 1 : -1;
      const next = at < 0 ? (step > 0 ? 0 : enabled.length - 1) : at + step;
      setActive(enabled[(next + enabled.length) % enabled.length]);
      setOpen(null);
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      const item = items[active];
      if (item && item !== "separator" && item.submenu) choose(active, true);
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      if (active >= 0) choose(active, true);
    }
  }

  const openItem = open && items[open.index];

  return (
    <>
      <div
        ref={ref}
        role="menu"
        tabIndex={-1}
        aria-activedescendant={active >= 0 ? `${idPrefix}-${active}` : undefined}
        onKeyDown={onKeyDown}
        onContextMenu={(e) => e.preventDefault()}
        // Keys go to the panel under the pointer.
        onPointerEnter={() => ref.current?.focus({ preventScroll: true })}
        className="fixed z-50 min-w-[200px] overflow-y-auto rounded-lg border border-line p-1 text-[13px] shadow-[0_8px_28px_rgba(0,0,0,0.45)] outline-none"
        // The panel colour is see-through; lay it over the opaque backdrop so the menu is solid.
        style={{
          left: pos?.left ?? 0,
          top: pos?.top ?? 0,
          maxHeight: window.innerHeight - 2 * MARGIN,
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
              id={`${idPrefix}-${i}`}
              role={item.checked === undefined ? "menuitem" : "menuitemcheckbox"}
              aria-checked={item.checked}
              aria-disabled={item.disabled || undefined}
              aria-haspopup={item.submenu ? "menu" : undefined}
              aria-expanded={item.submenu ? open?.index === i : undefined}
              onPointerEnter={() => {
                setActive(item.disabled ? -1 : i);
                if (open?.index === i) return;
                if (item.submenu && !item.disabled) openSubmenu(i, false);
                else setOpen(null);
              }}
              onPointerLeave={() => {
                if (open?.index !== i) setActive(-1);
              }}
              onClick={() => choose(i)}
              className={`grid cursor-default grid-cols-[16px_1fr_auto] items-center gap-2 rounded-md py-[5px] pr-2.5 pl-1.5 ${
                item.disabled ? "text-faint" : i === active ? "bg-line text-text" : "text-text"
              }`}
            >
              <span className="text-center text-accent" aria-hidden="true">
                {item.checked ? "✓" : ""}
              </span>
              <span className="max-w-[320px] truncate">{item.label}</span>
              <span className="pl-6 text-xs text-faint">{item.submenu ? "›" : item.shortcut}</span>
            </div>
          ),
        )}
      </div>
      {open && openItem && openItem !== "separator" && openItem.submenu && (
        <Panel
          key={open.index}
          items={openItem.submenu}
          at={open.at}
          idPrefix={`${idPrefix}-${open.index}`}
          onClose={onClose}
          onBack={() => {
            setOpen(null);
            ref.current?.focus();
          }}
          keyboard={open.keyboard}
        />
      )}
    </>
  );
}
