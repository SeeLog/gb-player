// What the app does where the Rust side needs the user to agree first, asked in our own dialogs.
import { api } from "./api";
import { ask } from "./Dialog";
import type { Translate } from "./format";

/** Delete a playlist once the user agrees. */
export async function deletePlaylist(id: string, name: string, t: Translate) {
  const yes = await ask({
    message: t("playlist_delete_confirm", { name }),
    ok: t("playlist_delete"),
    cancel: t("cancel"),
    danger: true,
  });
  if (yes) await api.deletePlaylist(id);
}

/** Clear the history once the user agrees. */
export async function clearHistory(t: Translate) {
  const yes = await ask({
    message: t("history_clear_confirm"),
    ok: t("history_clear"),
    cancel: t("cancel"),
    danger: true,
  });
  if (yes) await api.clearHistory();
}

/** Let the user point an entry whose ROM is missing at its file. A file holding a different ROM
 * than the one the entry was made from is taken only once the user agrees. */
export async function locateEntry(id: string, position: number, t: Translate) {
  let result = await api.relinkEntry(id, position);
  if (result.kind === "mismatch") {
    const yes = await ask({
      message: result.message,
      ok: t("playlist_relink_anyway"),
      cancel: t("cancel"),
    });
    if (yes) result = await api.relinkEntryTo(id, position, result.path);
  }
  if (result.kind === "failed") await ask({ message: result.message, ok: t("ok"), cancel: null });
}
