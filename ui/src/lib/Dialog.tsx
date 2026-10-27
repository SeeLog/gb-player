// Questions and messages in the app's colours, in place of the system's message boxes: `ask` from
// anywhere, answered by the one <Dialogs> placed in App.
import { useEffect, useState } from "react";
import { AlertDialog } from "@base-ui/react/alert-dialog";

export interface Question {
  message: string;
  /** The button that agrees, such as "Delete". */
  ok: string;
  /** The button that backs out; `null` for a message with only `ok`. */
  cancel: string | null;
  /** Whether agreeing loses something, which colours `ok` as a warning. */
  danger?: boolean;
}

type Asked = Question & { answer: (yes: boolean) => void };

let show: ((asked: Asked) => void) | null = null;

/** Shows `question` and resolves to whether the user agreed (`false` if no <Dialogs> is there). */
export function ask(question: Question): Promise<boolean> {
  return new Promise((answer) => {
    if (show) show({ ...question, answer });
    else answer(false);
  });
}

const button = "h-8 rounded-lg border px-3.5 font-semibold";

export function Dialogs() {
  const [asked, setAsked] = useState<Asked | null>(null);
  const [open, setOpen] = useState(false);
  useEffect(() => {
    show = (next) => {
      // A question still open is backed out of.
      setAsked((old) => {
        old?.answer(false);
        return next;
      });
      setOpen(true);
    };
    return () => {
      show = null;
    };
  }, []);

  const close = (yes: boolean) => {
    asked?.answer(yes);
    setOpen(false);
  };

  return (
    <AlertDialog.Root open={open} onOpenChange={(o) => o || close(false)}>
      <AlertDialog.Portal>
        <AlertDialog.Backdrop className="fixed inset-0 z-40 bg-[color-mix(in_srgb,var(--backdrop)_60%,transparent)] transition-opacity duration-150 data-ending-style:opacity-0 data-starting-style:opacity-0" />
        <AlertDialog.Popup className="fixed top-1/2 left-1/2 z-50 flex w-[400px] max-w-[calc(100vw-48px)] -translate-x-1/2 -translate-y-1/2 flex-col gap-5 rounded-xl border border-line p-5 shadow-[0_16px_48px_rgba(0,0,0,0.5)] outline-none [background:linear-gradient(var(--panel),var(--panel)),var(--backdrop)] transition-[scale,opacity] duration-150 ease-out data-ending-style:scale-[0.96] data-ending-style:opacity-0 data-starting-style:scale-[0.96] data-starting-style:opacity-0 motion-reduce:transition-none">
          <AlertDialog.Description className="text-[13px] leading-relaxed break-words whitespace-pre-line text-text">
            {asked?.message}
          </AlertDialog.Description>
          <div className="flex justify-end gap-2">
            {asked?.cancel != null && (
              <AlertDialog.Close className={`${button} border-line bg-panel-2 text-text hover:border-faint`}>
                {asked.cancel}
              </AlertDialog.Close>
            )}
            <button
              className={`${button} ${
                asked?.danger
                  ? "border-danger bg-danger text-white hover:brightness-110"
                  : "border-accent bg-accent text-accent-ink hover:brightness-108"
              }`}
              onClick={() => close(true)}
            >
              {asked?.ok}
            </button>
          </div>
        </AlertDialog.Popup>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
