//! The commands the web UI calls (`ui/src/lib/api.ts`); the work is done by [`Session`].
//!
//! [`Session`]: super::session::Session

use std::path::{Path, PathBuf};

use gb_music::Profile;
use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use super::views::LibraryView;
use super::{emit_library, App};
use super::{mini, playlists};
use crate::i18n::Text;
use crate::settings::Shuffle;
use crate::themes::{self, NewTheme};

#[tauri::command]
pub fn library(app: State<'_, App>) -> LibraryView {
    app.lock().library()
}

/// Open ROMs (dropped on the window); returns the id of the last one that could be opened.
#[tauri::command]
pub fn open_roms(handle: AppHandle, app: State<'_, App>, paths: Vec<PathBuf>) -> Option<String> {
    let mut s = app.lock();
    let id = s.open_roms(&paths);
    emit_library(&handle, &s);
    id
}

#[tauri::command]
pub fn close_rom(handle: AppHandle, app: State<'_, App>, id: String) {
    let mut s = app.lock();
    s.close_rom(&id);
    emit_library(&handle, &s);
}

const ROM_FILTER: (&str, &[&str]) = (
    "Game Boy ROM / GBS",
    &["gb", "gbc", "gbs", "GB", "GBC", "GBS"],
);

/// The file dialog for a ROM or GBS file.
fn pick_rom_file(handle: &AppHandle) -> Option<PathBuf> {
    let picked = handle
        .dialog()
        .file()
        .add_filter(ROM_FILTER.0, ROM_FILTER.1)
        .blocking_pick_file();
    picked.and_then(|f| f.into_path().ok())
}

/// Let the user pick ROMs and open them; returns the id of the last one that could be opened.
#[tauri::command]
pub async fn pick_roms(handle: AppHandle) -> Option<String> {
    let picked = handle
        .dialog()
        .file()
        .add_filter(ROM_FILTER.0, ROM_FILTER.1)
        .blocking_pick_files()?;
    let paths: Vec<PathBuf> = picked
        .into_iter()
        .filter_map(|f| f.into_path().ok())
        .collect();
    let app = handle.state::<App>();
    let mut s = app.lock();
    let id = s.open_roms(&paths);
    emit_library(&handle, &s);
    id
}

#[tauri::command]
pub fn play(handle: AppHandle, app: State<'_, App>, rom: String, index: usize) {
    let mut s = app.lock();
    s.play_song(&rom, index);
    emit_library(&handle, &s);
}

/// Make a playlist holding `songs` of the open ROM `rom`; returns its id.
#[tauri::command]
pub fn create_playlist(
    handle: AppHandle,
    app: State<'_, App>,
    name: String,
    rom: String,
    songs: Vec<usize>,
) -> String {
    let mut s = app.lock();
    let id = s.create_playlist(&name, &rom, &songs);
    emit_library(&handle, &s);
    id
}

#[tauri::command]
pub fn rename_playlist(handle: AppHandle, app: State<'_, App>, id: String, name: String) {
    let mut s = app.lock();
    s.rename_playlist(&id, &name);
    emit_library(&handle, &s);
}

/// Delete a playlist (the UI has asked the user first).
#[tauri::command]
pub fn delete_playlist(handle: AppHandle, app: State<'_, App>, id: String) {
    let mut s = app.lock();
    s.delete_playlist(&id);
    emit_library(&handle, &s);
}

#[tauri::command]
pub fn add_to_playlist(
    handle: AppHandle,
    app: State<'_, App>,
    id: String,
    rom: String,
    songs: Vec<usize>,
) {
    let mut s = app.lock();
    s.add_to_playlist(&id, &rom, &songs);
    emit_library(&handle, &s);
}

#[tauri::command]
pub fn remove_from_playlist(
    handle: AppHandle,
    app: State<'_, App>,
    id: String,
    positions: Vec<usize>,
) {
    let mut s = app.lock();
    s.remove_from_playlist(&id, &positions);
    emit_library(&handle, &s);
}

#[tauri::command]
pub fn move_in_playlist(
    handle: AppHandle,
    app: State<'_, App>,
    id: String,
    from: usize,
    to: usize,
) {
    let mut s = app.lock();
    s.move_in_playlist(&id, from, to);
    emit_library(&handle, &s);
}

/// Play an entry of a playlist; returns whether it plays.
#[tauri::command]
pub fn play_entry(handle: AppHandle, app: State<'_, App>, id: String, position: usize) -> bool {
    let mut s = app.lock();
    let playing = s.play_entry(&id, position);
    emit_library(&handle, &s);
    playing
}

/// What the middle column shows: the songs of the ROM `rom`, or the playlist `playlist`.
#[tauri::command]
pub fn set_view(app: State<'_, App>, rom: Option<String>, playlist: Option<String>) {
    app.lock().set_view(rom, playlist);
}

/// How pointing a playlist entry at a file went. The UI shows the messages and asks about a
/// mismatch in its own dialogs.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Relink {
    Done,
    /// No file was picked, or the entry is gone.
    Cancelled,
    /// The file could not be read.
    Failed {
        message: String,
    },
    /// The file holds a different ROM than the one the entry was made from; taken only once the
    /// user agrees, by [`relink_entry_to`] with `path`.
    Mismatch {
        message: String,
        path: PathBuf,
    },
}

/// Let the user point an entry whose ROM is missing at its file.
#[tauri::command]
pub async fn relink_entry(handle: AppHandle, id: String, position: usize) -> Relink {
    if handle.state::<App>().lock().entry(&id, position).is_none() {
        return Relink::Cancelled;
    }
    let Some(path) = pick_rom_file(&handle) else {
        return Relink::Cancelled;
    };
    relink(&handle, &id, position, &path, false)
}

/// Point an entry at `path` even though it holds a different ROM (the user agreed to it).
#[tauri::command]
pub async fn relink_entry_to(
    handle: AppHandle,
    id: String,
    position: usize,
    path: PathBuf,
) -> Relink {
    relink(&handle, &id, position, &path, true)
}

fn relink(handle: &AppHandle, id: &str, position: usize, path: &Path, anyway: bool) -> Relink {
    let app = handle.state::<App>();
    let Some(entry) = app.lock().entry(id, position) else {
        return Relink::Cancelled;
    };
    let data = match gb_music::read_rom(path) {
        Ok(d) => d,
        Err(e) => {
            let message = app
                .lock()
                .text()
                .fmt("err_read", &[("path", &path.display()), ("error", &e)]);
            return Relink::Failed { message };
        }
    };
    let hash = playlists::hash(&data);
    if hash != entry.hash && !anyway {
        let message = app.lock().text().fmt(
            "playlist_relink_mismatch",
            &[("game", &entry.game), ("path", &path.display())],
        );
        return Relink::Mismatch {
            message,
            path: path.to_path_buf(),
        };
    }
    let mut s = app.lock();
    s.relink(id, position, &entry.hash, path, &hash);
    emit_library(handle, &s);
    Relink::Done
}

#[tauri::command]
pub fn toggle(app: State<'_, App>) {
    app.lock().toggle();
}

#[tauri::command]
pub fn set_shuffle(handle: AppHandle, app: State<'_, App>, shuffle: Shuffle) {
    let mut s = app.lock();
    s.set_shuffle(shuffle);
    emit_library(&handle, &s);
}

#[tauri::command]
pub fn stop(app: State<'_, App>) {
    app.lock().stop();
}

#[tauri::command]
pub fn seek(app: State<'_, App>, secs: f64) {
    app.lock().seek(secs);
}

/// Next or previous; that may open the ROM of a playlist entry.
#[tauri::command]
pub fn skip(handle: AppHandle, app: State<'_, App>, delta: i64) {
    let mut s = app.lock();
    s.skip(delta);
    emit_library(&handle, &s);
}

#[tauri::command]
pub fn set_loop_count(handle: AppHandle, app: State<'_, App>, count: Option<u32>) {
    let mut s = app.lock();
    s.set_loop_count(count);
    emit_library(&handle, &s);
}

#[tauri::command]
pub fn set_auto_next(handle: AppHandle, app: State<'_, App>, auto_next: bool) {
    let mut s = app.lock();
    s.set_auto_next(auto_next);
    emit_library(&handle, &s);
}

#[tauri::command]
pub fn set_volume(app: State<'_, App>, volume: f32) {
    app.lock().set_volume(volume);
}

#[tauri::command]
pub fn set_profile(handle: AppHandle, app: State<'_, App>, profile: Profile, noise: bool) {
    let mut s = app.lock();
    s.set_profile(profile, noise);
    emit_library(&handle, &s);
}

#[tauri::command]
pub fn set_channel_volume(app: State<'_, App>, channel: usize, volume: f32) {
    if channel >= 4 {
        return;
    }
    app.lock().set_channel_volume(channel, volume);
}

#[tauri::command]
pub fn set_muted(handle: AppHandle, app: State<'_, App>, channel: usize, muted: bool) {
    if channel >= 4 {
        return;
    }
    let mut s = app.lock();
    s.set_muted(channel, muted);
    emit_library(&handle, &s);
}

/// Turn the window into the mini player, or back into the full window.
#[tauri::command]
pub fn set_mini(
    handle: AppHandle,
    app: State<'_, App>,
    window: WebviewWindow,
    on: bool,
) -> Result<(), String> {
    let mut s = app.lock();
    let result = mini::set(&window, s.mini(), on);
    s.save_settings();
    emit_library(&handle, &s);
    result.map_err(|e| e.to_string())
}

/// Keep the mini player above other windows, or not.
#[tauri::command]
pub fn set_mini_on_top(
    handle: AppHandle,
    app: State<'_, App>,
    window: WebviewWindow,
    on: bool,
) -> Result<(), String> {
    let mut s = app.lock();
    s.mini().on_top = on;
    let result = if s.mini().on {
        window.set_always_on_top(on)
    } else {
        Ok(())
    };
    s.save_settings();
    emit_library(&handle, &s);
    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_language(handle: AppHandle, app: State<'_, App>, language: String) {
    let mut s = app.lock();
    s.set_language(language);
    emit_library(&handle, &s);
}

#[tauri::command]
pub fn set_theme(handle: AppHandle, app: State<'_, App>, theme: String) {
    let mut s = app.lock();
    s.set_theme(theme);
    emit_library(&handle, &s);
}

/// Save a custom theme and switch to it; returns its id.
#[tauri::command]
pub fn save_theme(
    handle: AppHandle,
    app: State<'_, App>,
    theme: NewTheme,
) -> Result<String, String> {
    let mut s = app.lock();
    let id = s.save_theme(theme)?;
    emit_library(&handle, &s);
    Ok(id)
}

/// A background image of the themes folder as a `data:` URL.
#[tauri::command]
pub fn theme_image(app: State<'_, App>, image: String) -> Result<String, String> {
    themes::image_url(&image, app.lock().text())
}

/// Let the user pick a background image and copy it into the themes folder; returns its name
/// for the theme, or `None` when nothing was picked.
#[tauri::command]
pub async fn pick_theme_image(handle: AppHandle) -> Result<Option<String>, String> {
    let picked = handle
        .dialog()
        .file()
        .add_filter(
            "Image",
            &[
                "png", "jpg", "jpeg", "webp", "gif", "PNG", "JPG", "JPEG", "WEBP", "GIF",
            ],
        )
        .blocking_pick_file();
    let Some(Ok(path)) = picked.map(|f| f.into_path()) else {
        return Ok(None);
    };
    let app = handle.state::<App>();
    let text = Text::new(app.lock().text().language());
    themes::import_image(&path, &text).map(Some)
}
