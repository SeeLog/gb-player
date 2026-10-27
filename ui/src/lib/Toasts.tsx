// Short notes at the bottom of the window, such as "Title copied": `notify` from anywhere,
// shown by the one <Toasts> placed in App.
import { Toast } from "@base-ui/react/toast";

const manager = Toast.createToastManager();

/** Shows `text` for a moment. */
export function notify(text: string) {
  manager.add({ title: text, timeout: 2000 });
}

export function Toasts() {
  return (
    <Toast.Provider toastManager={manager} limit={1}>
      <Toast.Portal>
        <Toast.Viewport className="fixed bottom-4 left-1/2 z-50 -translate-x-1/2">
          <List />
        </Toast.Viewport>
      </Toast.Portal>
    </Toast.Provider>
  );
}

function List() {
  const { toasts } = Toast.useToastManager();
  return toasts.map((toast) => (
    <Toast.Root
      key={toast.id}
      toast={toast}
      className="absolute bottom-0 left-1/2 -translate-x-1/2 rounded-lg border border-line px-3.5 py-2 text-[13px] whitespace-nowrap text-text shadow-[0_8px_28px_rgba(0,0,0,0.45)] [background:linear-gradient(var(--panel-2),var(--panel-2)),var(--backdrop)] transition-[translate,opacity] duration-200 ease-out data-ending-style:translate-y-2 data-ending-style:opacity-0 data-limited:opacity-0 data-starting-style:translate-y-2 data-starting-style:opacity-0 motion-reduce:transition-none"
    >
      <Toast.Title />
    </Toast.Root>
  ));
}
