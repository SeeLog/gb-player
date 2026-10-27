// A colour swatch that opens a picker beside it, in place of the WebView's colour input.
import { Popover } from "@base-ui/react/popover";
import { HexColorPicker } from "react-colorful";
import { popupClass } from "./ContextMenu";

export default function ColorPicker({
  hex,
  onChange,
  label,
}: {
  /** The colour as `#rrggbb`. */
  hex: string;
  onChange: (hex: string) => void;
  /** Its name, for screen readers. */
  label: string;
}) {
  return (
    <Popover.Root>
      <Popover.Trigger
        aria-label={label}
        className="h-[26px] w-[30px] rounded-md border border-line bg-panel-2 p-[3px] hover:border-faint data-popup-open:border-accent"
      >
        <span className="block size-full rounded-[3px]" style={{ background: hex }} />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Positioner side="left" align="start" sideOffset={8} collisionPadding={4} className="z-50">
          <Popover.Popup className={`${popupClass} min-w-0 p-2`}>
            <HexColorPicker color={hex} onChange={onChange} className="color-picker" />
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  );
}
