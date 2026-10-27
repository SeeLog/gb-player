//! The commands the web UI calls (`ui/src/lib/api.ts`); the work is done by [`Session`].
//!
//! [`Session`]: super::session::Session

use std::collections::BTreeMap;
use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use super::playlists;
use super::views::LibraryView;
use super::{emit_library, App};
use crate::i18n::Text;
use crate::themes::{self, Background};

#[tauri::command]
pub fn library(app: State<'_, App>) -> LibraryView {
    app.lock().library()
}

#[tauri::command]
pub fn open_rom(handle: AppHandle, app: State<'_, App>, path: PathBuf) {
    let mut s = app.lock();
    s.open_rom(&path);
    emit_library(&handle, &s);
}

/// The file dialog for a ROM or GBS file.
fn pick_rom_file(handle: &AppHandle) -> Option<PathBuf> {
    let picked = handle
        .dialog()
        .file()
        .add_filter(
            "Game Boy ROM / GBS",
            &["gb", "gbc", "gbs", "GB", "GBC", "GBS"],
        )
        .blocking_pick_file();
    picked.and_then(|f| f.into_path().ok())
}

/// Ask a yes/no question; returns whether the user agreed.
fn confirm(handle: &AppHandle, message: String, ok: &str, cancel: &str) -> bool {
    handle
        .dialog()
        .message(message)
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            ok.to_string(),
            cancel.to_string(),
        ))
        .blocking_show()
}

/// Let the user pick a ROM and open it; returns whether one was picked.
#[tauri::command]
pub async fn pick_rom(handle: AppHandle) -> bool {
    let Some(path) = pick_rom_file(&handle) else {
        return false;
    };
    let app = handle.state::<App>();
    let mut s = app.lock();
    s.open_rom(&path);
    emit_library(&handle, &s);
    true
}

#[tauri::command]
pub fn play(app: State<'_, App>, index: usize) {
    app.lock().play_song(index);
}

/// Make a playlist holding `songs` of the loaded ROM; returns its id.
#[tauri::command]
pub fn create_playlist(
    handle: AppHandle,
    app: State<'_, App>,
    name: String,
    songs: Vec<usize>,
) -> String {
    let mut s = app.lock();
    let id = s.create_playlist(&name, &songs);
    emit_library(&handle, &s);
    id
}

#[tauri::command]
pub fn rename_playlist(handle: AppHandle, app: State<'_, App>, id: String, name: String) {
    let mut s = app.lock();
    s.rename_playlist(&id, &name);
    emit_library(&handle, &s);
}

/// Delete a playlist once the user has agreed; returns whether it was deleted.
#[tauri::command]
pub async fn delete_playlist(handle: AppHandle, id: String, name: String) -> bool {
    let (message, ok, cancel) = {
        let app = handle.state::<App>();
        let s = app.lock();
        let t = s.text();
        (
            t.fmt("playlist_delete_confirm", &[("name", &name)]),
            t.get("playlist_delete").to_string(),
            t.get("cancel").to_string(),
        )
    };
    if !confirm(&handle, message, &ok, &cancel) {
        return false;
    }
    let app = handle.state::<App>();
    let mut s = app.lock();
    s.delete_playlist(&id);
    emit_library(&handle, &s);
    true
}

#[tauri::command]
pub fn add_to_playlist(handle: AppHandle, app: State<'_, App>, id: String, songs: Vec<usize>) {
    let mut s = app.lock();
    s.add_to_playlist(&id, &songs);
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

/// The playlist shown instead of the ROM's songs, or `None` for the songs.
#[tauri::command]
pub fn set_open_playlist(app: State<'_, App>, id: Option<String>) {
    app.lock().set_open_playlist(id);
}

/// Let the user point an entry whose ROM is missing at its file. A file holding a different
/// ROM than the one the entry was made from is taken only once the user agrees.
#[tauri::command]
pub async fn relink_entry(handle: AppHandle, id: String, position: usize) {
    let Some(entry) = handle.state::<App>().lock().entry(&id, position) else {
        return;
    };
    let Some(path) = pick_rom_file(&handle) else {
        return;
    };
    let data = match gb_music::read_rom(&path) {
        Ok(d) => d,
        Err(e) => {
            let message = {
                let app = handle.state::<App>();
                let s = app.lock();
                s.text()
                    .fmt("err_read", &[("path", &path.display()), ("error", &e)])
            };
            handle
                .dialog()
                .message(message)
                .kind(MessageDialogKind::Error)
                .blocking_show();
            return;
        }
    };
    let hash = playlists::hash(&data);
    if hash != entry.hash {
        let (message, ok, cancel) = {
            let app = handle.state::<App>();
            let s = app.lock();
            let t = s.text();
            (
                t.fmt(
                    "playlist_relink_mismatch",
                    &[("game", &entry.game), ("path", &path.display())],
                ),
                t.get("playlist_relink_anyway").to_string(),
                t.get("cancel").to_string(),
            )
        };
        if !confirm(&handle, message, &ok, &cancel) {
            return;
        }
    }
    let app = handle.state::<App>();
    let mut s = app.lock();
    s.relink(&id, position, &entry.hash, &path, &hash);
    emit_library(&handle, &s);
}

#[tauri::command]
pub fn toggle(app: State<'_, App>) {
    app.lock().toggle();
}

#[tauri::command]
pub fn stop(app: State<'_, App>) {
    app.lock().stop();
}

#[tauri::command]
pub fn seek(app: State<'_, App>, secs: f64) {
    app.lock().seek(secs);
}

#[tauri::command]
pub fn skip(app: State<'_, App>, delta: i64) {
    app.lock().skip(delta);
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
pub fn set_muted(handle: AppHandle, app: State<'_, App>, channel: usize, muted: bool) {
    if channel >= 4 {
        return;
    }
    let s = app.lock();
    s.set_muted(channel, muted);
    emit_library(&handle, &s);
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
    name: String,
    base: String,
    colors: BTreeMap<String, String>,
    background: Option<Background>,
) -> Result<String, String> {
    let mut s = app.lock();
    let id = s.save_theme(&name, &base, colors, background)?;
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
