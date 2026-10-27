// A drop-down list in the app's colours, in place of the WebView's <select> (whose list is drawn
// by the system). Base UI's select does the keys, typing to jump and the positioning.
import type { ReactNode } from "react";
import { Select as Base } from "@base-ui/react/select";
import { popupClass } from "./ContextMenu";
import Icon from "./Icon";
import Tip from "./Tip";

export interface Option {
  value: string;
  label: string;
}

export default function Select({
  value,
  options,
  onChange,
  label,
  icon,
  disabled,
  className,
}: {
  value: string;
  options: Option[];
  onChange: (value: string) => void;
  /** Its name, for the tooltip and screen readers. */
  label: string;
  /** Shown before the value. */
  icon?: ReactNode;
  disabled?: boolean;
  /** Classes for the button that opens the list. */
  className?: string;
}) {
  return (
    <Base.Root
      items={options}
      value={value}
      onValueChange={(v) => v !== null && onChange(v)}
      disabled={disabled}
      modal={false}
    >
      <Tip text={label}>
        <Base.Trigger
          className={`${className ?? ""} cursor-pointer data-disabled:cursor-default data-disabled:opacity-60`}
          aria-label={label}
        >
          {icon}
          <Base.Value />
          <Base.Icon className="-mr-1 text-muted">
            <Icon name="chevron" size={16} />
          </Base.Icon>
        </Base.Trigger>
      </Tip>
      <Base.Portal>
        {/* Below the button, as on Windows, not over it with the chosen item in line. */}
        <Base.Positioner
          alignItemWithTrigger={false}
          side="bottom"
          align="start"
          sideOffset={4}
          collisionPadding={4}
          className="z-50 outline-none"
        >
          <Base.Popup className={`${popupClass} min-w-[var(--anchor-width)]`}>
            <Base.List>
              {options.map((o) => (
                <Base.Item
                  key={o.value}
                  value={o.value}
                  className="grid cursor-default grid-cols-[16px_1fr] items-center gap-2 rounded-md py-[5px] pr-2.5 pl-1.5 text-text outline-none select-none data-highlighted:bg-line"
                >
                  <Base.ItemIndicator className="col-start-1 text-center text-accent">
                    ✓
                  </Base.ItemIndicator>
                  <Base.ItemText className="col-start-2 max-w-[320px] truncate">
                    {o.label}
                  </Base.ItemText>
                </Base.Item>
              ))}
            </Base.List>
          </Base.Popup>
        </Base.Positioner>
      </Base.Portal>
    </Base.Root>
  );
}
