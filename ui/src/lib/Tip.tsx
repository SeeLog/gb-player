// Tooltips in the app's colours, in place of the system's `title` ones. Every <Tip> shares one
// tooltip, so going from one to the next shows it at once and slides it over.
import type { ReactElement } from "react";
import { Tooltip } from "@base-ui/react/tooltip";

const tooltip = Tooltip.createHandle<string>();

/** Shows `text` on hovering or focusing `children` (one element, which must take a ref); with no
 * text, shows nothing. */
export default function Tip({ text, children }: { text?: string; children: ReactElement }) {
  if (!text) return children;
  return <Tooltip.Trigger handle={tooltip} payload={text} delay={500} render={children} />;
}

/** The one tooltip; placed once, in App. */
export function Tooltips() {
  return (
    <Tooltip.Root handle={tooltip}>
      {({ payload }) => (
        <Tooltip.Portal>
          <Tooltip.Positioner
            side="bottom"
            sideOffset={6}
            collisionPadding={4}
            className="z-50 transition-[top,left,right,bottom] duration-200 ease-[cubic-bezier(0.22,1,0.36,1)] data-instant:transition-none motion-reduce:transition-none"
          >
            <Tooltip.Popup className="max-w-[360px] origin-[var(--transform-origin)] rounded-md border border-line px-2 py-1 text-xs break-words whitespace-pre-line text-text shadow-[0_4px_16px_rgba(0,0,0,0.35)] [background:linear-gradient(var(--panel-2),var(--panel-2)),var(--backdrop)] transition-[scale,opacity] duration-100 ease-out data-ending-style:scale-[0.97] data-ending-style:opacity-0 data-instant:transition-none data-starting-style:scale-[0.97] data-starting-style:opacity-0">
              {payload}
            </Tooltip.Popup>
          </Tooltip.Positioner>
        </Tooltip.Portal>
      )}
    </Tooltip.Root>
  );
}
