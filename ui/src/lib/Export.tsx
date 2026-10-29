// Exporting songs to WAV files: `openExport` from a menu shows the dialog of the one <Export>
// placed in App, which also shows the progress of the export running, with a way to cancel it.
import { useEffect, useState } from "react";
import { Dialog } from "@base-ui/react/dialog";
import { Switch } from "@base-ui/react/switch";
import { api, onExport } from "./api";
import { ask } from "./Dialog";
import type { Translate } from "./format";
import { notify } from "./Toasts";
import type { ExportPlan, ExportProgress, ExportTarget } from "./types";

interface Opened {
  target: ExportTarget;
  /** What is exported: the song, the ROM or the playlist. */
  subject: string;
  plan: ExportPlan;
}

let open: ((opened: Opened) => void) | null = null;

/** Shows the export dialog for `target`. */
export async function openExport(target: ExportTarget, subject: string) {
  const plan = await api.exportPlan(target);
  if (plan.songs > 0) open?.({ target, subject, plan });
}

/** Stems stay chosen for the next export until the app is closed. */
let lastStems = false;

const button = "h-8 rounded-lg border px-3.5 font-semibold";

export default function Export({ t }: { t: Translate }) {
  const [opened, setOpened] = useState<Opened | null>(null);
  const [shown, setShown] = useState(false);
  const [loops, setLoops] = useState(2);
  const [stems, setStems] = useState(lastStems);
  const [dir, setDir] = useState<string | null>(null);
  const [progress, setProgress] = useState<ExportProgress | null>(null);

  useEffect(() => {
    open = (o) => {
      setOpened(o);
      setLoops(o.plan.loops);
      setStems(lastStems);
      setDir(o.plan.dir);
      setShown(true);
    };
    return () => {
      open = null;
    };
  }, []);

  useEffect(() => {
    const unlisten = onExport((p) => {
      if (p.state === "running") {
        setProgress(p);
        return;
      }
      setProgress(null);
      notify(
        p.cancelled ? t("export_cancelled", { n: p.written }) : t("export_done", { n: p.written }),
      );
      if (p.errors.length > 0) {
        ask({
          message: `${t("export_errors")}\n${p.errors.join("\n")}`,
          ok: t("ok"),
          cancel: null,
        });
      }
    });
    return () => void unlisten.then((f) => f());
  }, [t]);

  const plan = opened?.plan;
  const canStem = !!plan && plan.withChannels > 0;
  const output = plan
    ? t(`output_${plan.profile}`) +
      (plan.profile === "gbcSpeaker" && plan.noise ? ` (${t("export_noise")})` : "")
    : "";

  async function start() {
    if (!opened || !dir) return;
    lastStems = stems;
    setShown(false);
    try {
      await api.startExport(opened.target, loops, stems && canStem, dir);
    } catch (e) {
      ask({ message: String(e), ok: t("ok"), cancel: null });
    }
  }

  const stemsNote = !plan
    ? ""
    : plan.withChannels === 0
      ? t("export_stems_none")
      : [
          t("export_stems_hint"),
          plan.withChannels < plan.songs ? t("export_stems_some", { n: plan.withChannels }) : "",
          plan.profile === "gbcSpeaker" && plan.noise ? t("export_stems_noise") : "",
        ]
          .filter(Boolean)
          .join(" ");

  return (
    <>
      <Dialog.Root open={shown} onOpenChange={setShown}>
        <Dialog.Portal>
          <Dialog.Backdrop className="fixed inset-0 z-40 bg-[color-mix(in_srgb,var(--backdrop)_60%,transparent)] transition-opacity duration-150 data-ending-style:opacity-0 data-starting-style:opacity-0" />
          <Dialog.Popup className="fixed top-1/2 left-1/2 z-50 flex w-[440px] max-w-[calc(100vw-48px)] -translate-x-1/2 -translate-y-1/2 flex-col gap-4 rounded-xl border border-line p-5 text-[13px] text-text shadow-[0_16px_48px_rgba(0,0,0,0.5)] outline-none [background:linear-gradient(var(--panel),var(--panel)),var(--backdrop)] transition-[scale,opacity] duration-150 ease-out data-ending-style:scale-[0.96] data-ending-style:opacity-0 data-starting-style:scale-[0.96] data-starting-style:opacity-0 motion-reduce:transition-none">
            <div>
              <Dialog.Title className="text-[15px] font-semibold">{t("export_title")}</Dialog.Title>
              <Dialog.Description className="mt-1 truncate text-muted">
                {opened?.subject} · {t("song_count", { n: plan?.songs ?? 0 })}
              </Dialog.Description>
            </div>

            <div className="grid grid-cols-[auto_minmax(0,1fr)] items-center gap-x-4 gap-y-3">
              <label htmlFor="export-loops" className="text-muted">
                {t("export_loops")}
              </label>
              <input
                id="export-loops"
                type="number"
                min={1}
                max={99}
                value={loops}
                onChange={(e) => {
                  const n = Math.round(Number(e.currentTarget.value));
                  if (Number.isFinite(n)) setLoops(Math.min(99, Math.max(1, n)));
                }}
                className="num h-8 w-20 rounded-lg border border-line bg-panel-2 px-2.5 text-text outline-none focus:border-accent-2"
              />
              {plan?.loopCount === null && (
                <p className="col-start-2 -mt-1.5 text-xs leading-relaxed text-muted">
                  {t("export_loops_forever")}
                </p>
              )}

              <span className="text-muted">{t("export_output")}</span>
              <span>{output}</span>

              <span className="text-muted">{t("export_folder")}</span>
              <div className="flex min-w-0 items-center gap-2">
                <span
                  className={`min-w-0 flex-1 truncate ${dir ? "" : "text-muted"}`}
                  title={dir ?? ""}
                >
                  {dir ?? t("export_folder_none")}
                </span>
                <button
                  className={`${button} flex-none border-line bg-panel-2 text-text hover:border-faint`}
                  onClick={() => api.pickExportDir(dir).then((d) => d && setDir(d))}
                >
                  {t("export_choose")}
                </button>
              </div>
            </div>

            <div className="flex flex-col gap-1.5">
              <label
                className={`flex items-center gap-2.5 ${canStem ? "cursor-pointer" : "cursor-default text-muted"}`}
              >
                <Switch.Root
                  checked={stems && canStem}
                  disabled={!canStem}
                  onCheckedChange={setStems}
                  className="flex h-5 w-[34px] flex-none rounded-[10px] border border-line bg-panel-2 p-[2px] transition-colors data-checked:border-[var(--accent-edge,var(--accent))] data-checked:bg-accent data-disabled:opacity-50 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent motion-reduce:transition-none"
                >
                  <Switch.Thumb className="size-3.5 rounded-full bg-muted transition-[translate,background-color] data-checked:translate-x-3.5 data-checked:bg-[var(--knob-on,var(--accent-ink))] data-checked:shadow-[0_1px_2px_rgba(0,0,0,0.3)] motion-reduce:transition-none" />
                </Switch.Root>
                <span>{t("export_stems")}</span>
              </label>
              <p className="pl-[44px] text-xs leading-relaxed text-muted">{stemsNote}</p>
            </div>

            <div className="flex justify-end gap-2">
              <Dialog.Close
                className={`${button} border-line bg-panel-2 text-text hover:border-faint`}
              >
                {t("cancel")}
              </Dialog.Close>
              <button
                disabled={!dir}
                className={`${button} border-accent bg-accent text-accent-ink enabled:hover:brightness-108 disabled:opacity-50`}
                onClick={start}
              >
                {t("export_start")}
              </button>
            </div>
          </Dialog.Popup>
        </Dialog.Portal>
      </Dialog.Root>

      {progress?.state === "running" && <Progress t={t} progress={progress} />}
    </>
  );
}

/** The export running, at the bottom right of the window. */
function Progress({
  t,
  progress,
}: {
  t: Translate;
  progress: Extract<ExportProgress, { state: "running" }>;
}) {
  const whole = (progress.done + progress.fraction) / Math.max(1, progress.total);
  return (
    <div
      role="status"
      className="fixed right-4 bottom-4 z-30 flex w-[300px] flex-col gap-2 rounded-lg border border-line px-3.5 py-2.5 text-[13px] text-text shadow-[0_8px_28px_rgba(0,0,0,0.45)] [background:linear-gradient(var(--panel),var(--panel)),var(--backdrop)]"
    >
      <div className="flex items-center gap-2">
        <div className="min-w-0 flex-1">
          <div className="font-semibold">
            {t("export_running", {
              done: Math.min(progress.done + 1, progress.total),
              total: progress.total,
            })}
          </div>
          <div className="truncate text-xs text-muted">{progress.title}</div>
        </div>
        <button
          className="h-7 flex-none rounded-md border border-line bg-panel-2 px-2.5 text-text hover:border-faint"
          onClick={() => api.cancelExport()}
        >
          {t("cancel")}
        </button>
      </div>
      {/* As the seek bar, but filled in the plain accent: a theme's sheen is too pale for it. */}
      <div
        className="h-2 overflow-hidden rounded bg-panel-2 shadow-[inset_0_0_0_1px_var(--line)]"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(whole * 100)}
      >
        <div className="h-full rounded bg-[var(--accent)]" style={{ width: `${whole * 100}%` }} />
      </div>
    </div>
  );
}
