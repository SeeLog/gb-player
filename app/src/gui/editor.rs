//! The theme editor's window, put beside the main window so the theme can be seen whole while
//! it is changed. The editor sends its draft to the main window and the settings' (the
//! `theme-draft` event), and the main window goes back to the chosen theme when the editor's
//! window is gone.

use tauri::{AppHandle, LogicalSize, Runtime};

use super::side;

/// The label of the editor's window, which the web UI also goes by.
pub const LABEL: &str = "theme-editor";
/// The event that carries the draft (or `null` once the editor is gone) to the other windows.
pub const DRAFT_EVENT: &str = "theme-draft";

const SIZE: LogicalSize<f64> = LogicalSize {
    width: 400.0,
    height: 760.0,
};
const MIN_SIZE: LogicalSize<f64> = LogicalSize {
    width: 380.0,
    height: 420.0,
};

/// Open the editor's window, or bring it to the front when it is open.
pub fn open<R: Runtime>(handle: &AppHandle<R>, title: &str) -> tauri::Result<()> {
    side::open(handle, LABEL, title, SIZE, MIN_SIZE)
}
