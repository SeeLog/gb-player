//! The commands the web UI calls (`ui/src/lib/api.ts`); the work is done by [`Session`].
//!
//! [`Session`]: super::session::Session

use std::collections::BTreeMap;
use std::path::PathBuf;

use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

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
    s.load_rom(&path);
    emit_library(&handle, &s);
}

#[tauri::command]
pub async fn pick_rom(handle: AppHandle) {
    let picked = handle
        .dialog()
        .file()
        .add_filter("Game Boy ROM", &["gb", "gbc", "GB", "GBC"])
        .blocking_pick_file();
    let Some(Ok(path)) = picked.map(|f| f.into_path()) else {
        return;
    };
    let app = handle.state::<App>();
    let mut s = app.lock();
    s.load_rom(&path);
    emit_library(&handle, &s);
}

#[tauri::command]
pub fn play(app: State<'_, App>, index: usize) {
    app.lock().play(index);
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
