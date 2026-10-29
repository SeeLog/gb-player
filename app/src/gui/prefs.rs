//! The settings window, beside the main window: the choices made once and seldom changed, which
//! the main window leaves out.

use std::path::PathBuf;

use serde::Deserialize;
use tauri::{AppHandle, LogicalSize, Runtime};

use super::{playlists, side};
use crate::themes;

/// The label of the settings window, which the web UI also goes by.
pub const LABEL: &str = "settings";

const SIZE: LogicalSize<f64> = LogicalSize {
    width: 420.0,
    height: 640.0,
};
const MIN_SIZE: LogicalSize<f64> = LogicalSize {
    width: 380.0,
    height: 360.0,
};

/// Open the settings window, or bring it to the front when it is open.
pub fn open<R: Runtime>(handle: &AppHandle<R>, title: &str) -> tauri::Result<()> {
    side::open(handle, LABEL, title, SIZE, MIN_SIZE)
}

/// A folder of the user's own files that the settings can show.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Folder {
    Themes,
    SongNames,
    Playlists,
}

impl Folder {
    pub fn path(self) -> Option<PathBuf> {
        match self {
            Folder::Themes => themes::dir(),
            Folder::SongNames => gb_music::songdb::user_dir(),
            Folder::Playlists => playlists::dir(),
        }
    }
}

/// Show `path` in the system's file manager, making it first if it is not there yet.
pub fn reveal(path: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)?;
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    std::process::Command::new(program).arg(path).spawn()?;
    Ok(())
}
