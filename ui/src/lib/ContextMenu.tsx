// Our own right-click menu, in place of the WebView's (Back, Refresh, Print…): see App.tsx for
// what it holds where. Base UI's menu does the focus, keys, hover delay and the path to a
// submenu (moving to it does not close it on the way); this places it at the cursor and styles it.
import { useEffect, useMemo, useState } from "react";
import { Menu } from "@base-ui/react/menu";

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

/** The panel colour is see-through; it is laid over the opaque backdrop so the menu is solid. */
export const popupClass =
  "max-h-[var(--available-height)] min-w-[200px] origin-[var(--transform-origin)] overflow-y-auto rounded-lg border border-line p-1 text-[13px] shadow-[0_8px_28px_rgba(0,0,0,0.45)] outline-none [background:linear-gradient(var(--panel-2),var(--panel-2)),var(--backdrop)] transition-[scale,opacity] duration-100 ease-out data-ending-style:scale-[0.97] data-ending-style:opacity-0 data-instant:transition-none data-starting-style:scale-[0.97] data-starting-style:opacity-0";

const itemClass =
  "grid cursor-default grid-cols-[16px_1fr_auto] items-center gap-2 rounded-md py-[5px] pr-2.5 pl-1.5 text-text outline-none select-none data-disabled:text-faint data-highlighted:bg-line data-popup-open:bg-line";

/** The menu shown, or `null` when there is none (the last one stays while it fades out). */
export default function ContextMenu({
  menu,
  onClose,
}: {
  menu: MenuState | null;
  onClose: () => void;
}) {
  const [shown, setShown] = useState(menu);
  if (menu && menu !== shown) setShown(menu);
  const anchor = useMemo(() => {
    const x = shown?.x ?? 0;
    const y = shown?.y ?? 0;
    return { getBoundingClientRect: () => DOMRect.fromRect({ x, y, width: 0, height: 0 }) };
  }, [shown]);

  useEffect(() => {
    if (!menu) return;
    // A long menu scrolls by itself; anything else scrolling moves what it was opened on.
    const onScroll = (e: Event) => {
      if (!(e.target instanceof Element && e.target.closest('[role="menu"]'))) onClose();
    };
    const close = () => onClose();
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    document.addEventListener("scroll", onScroll, true);
    return () => {
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
      document.removeEventListener("scroll", onScroll, true);
    };
  }, [menu, onClose]);

  return (
    // Not modal: a click outside closes it and still lands, and a right-click elsewhere opens
    // the menu for what is there.
    <Menu.Root open={!!menu} onOpenChange={(open) => open || onClose()} modal={false}>
      <Menu.Portal>
        <Menu.Positioner
          anchor={anchor}
          side="bottom"
          align="start"
          sideOffset={2}
          alignOffset={2}
          collisionPadding={MARGIN}
          className="z-50 outline-none"
        >
          <Menu.Popup
            className={popupClass}
            finalFocus={false}
            onContextMenu={(e) => e.preventDefault()}
          >
            {shown && <Items items={shown.items} />}
          </Menu.Popup>
        </Menu.Positioner>
      </Menu.Portal>
    </Menu.Root>
  );
}

function Items({ items }: { items: MenuItem[] }) {
  return items.map((item, i) => {
    if (item === "separator") return <Menu.Separator key={i} className="mx-2 my-1 h-px bg-line" />;
    const label = <span className="max-w-[320px] truncate">{item.label}</span>;
    if (item.submenu) {
      return (
        <Menu.SubmenuRoot key={i}>
          <Menu.SubmenuTrigger className={itemClass} disabled={item.disabled}>
            <span />
            {label}
            <span className="pl-6 text-xs text-faint">›</span>
          </Menu.SubmenuTrigger>
          <Menu.Portal>
            {/* Its first item level with the one that opened it. */}
            <Menu.Positioner
              sideOffset={-2}
              alignOffset={-5}
              collisionPadding={MARGIN}
              className="z-50 outline-none"
            >
              <Menu.Popup className={popupClass} onContextMenu={(e) => e.preventDefault()}>
                <Items items={item.submenu} />
              </Menu.Popup>
            </Menu.Positioner>
          </Menu.Portal>
        </Menu.SubmenuRoot>
      );
    }
    const contents = (
      <>
        <span className="text-center text-accent" aria-hidden="true">
          {item.checked ? "✓" : ""}
        </span>
        {label}
        <span className="pl-6 text-xs text-faint">{item.shortcut}</span>
      </>
    );
    return item.checked === undefined ? (
      <Menu.Item key={i} className={itemClass} disabled={item.disabled} onClick={item.onSelect}>
        {contents}
      </Menu.Item>
    ) : (
      <Menu.CheckboxItem
        key={i}
        className={itemClass}
        disabled={item.disabled}
        checked={item.checked}
        closeOnClick
        onClick={item.onSelect}
      >
        {contents}
      </Menu.CheckboxItem>
    );
  });
}
