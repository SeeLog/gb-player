//! The menu bar on macOS, in the app's language. Other systems have none: the page's title bar
//! and right-click menus do its work.
//!
//! The items that act on the page go to the web UI (`onMenu` in `ui/src/lib/api.ts`), which does
//! the same when they are clicked as for its buttons. The others are the system's own, except
//! Minimize: the system's cannot give up Cmd+M, which is the mini player's on every platform.

use tauri::menu::MenuEvent;
#[cfg(target_os = "macos")]
use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem as Native, Submenu};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::i18n::Text;

const OPEN_ROM: &str = "open_rom";
const SETTINGS: &str = "settings";
const MINI_PLAYER: &str = "mini_player";
const MINIMIZE: &str = "minimize";

/// Make the menu bar in the language of `t`, in place of the one there is.
#[cfg(target_os = "macos")]
pub fn install<R: Runtime>(handle: &AppHandle<R>, t: &Text) -> tauri::Result<()> {
    let info = handle.package_info();
    let about = AboutMetadata {
        name: Some(info.name.clone()),
        version: Some(info.version.to_string()),
        ..Default::default()
    };
    let h = handle;
    let label = |key| Some(t.get(key));
    let menu = Menu::with_items(
        h,
        &[
            // macOS titles this one with the app's name, whatever it is given here.
            &Submenu::with_items(
                h,
                &info.name,
                true,
                &[
                    &Native::about(h, label("menu_about"), Some(about))?,
                    &Native::separator(h)?,
                    &MenuItem::with_id(
                        h,
                        SETTINGS,
                        t.get("menu_settings"),
                        true,
                        Some("Cmd+Comma"),
                    )?,
                    &Native::separator(h)?,
                    &Native::hide(h, label("menu_hide"))?,
                    &Native::hide_others(h, label("menu_hide_others"))?,
                    &Native::show_all(h, label("menu_show_all"))?,
                    &Native::separator(h)?,
                    &Native::quit(h, label("menu_quit"))?,
                ],
            )?,
            &Submenu::with_items(
                h,
                t.get("menu_file"),
                true,
                &[
                    &MenuItem::with_id(h, OPEN_ROM, t.get("menu_open_rom"), true, Some("Cmd+O"))?,
                    &Native::separator(h)?,
                    &Native::close_window(h, label("menu_close_window"))?,
                ],
            )?,
            // The text fields' Cut, Copy and Paste keys only work through these.
            &Submenu::with_items(
                h,
                t.get("menu_edit"),
                true,
                &[
                    &Native::undo(h, label("menu_undo"))?,
                    &Native::redo(h, label("menu_redo"))?,
                    &Native::separator(h)?,
                    &Native::cut(h, label("menu_cut"))?,
                    &Native::copy(h, label("menu_copy"))?,
                    &Native::paste(h, label("menu_paste"))?,
                    &Native::select_all(h, label("menu_select_all"))?,
                ],
            )?,
            &Submenu::with_items(
                h,
                t.get("menu_view"),
                true,
                &[
                    &MenuItem::with_id(
                        h,
                        MINI_PLAYER,
                        t.get("menu_mini_player"),
                        true,
                        Some("Cmd+M"),
                    )?,
                    &Native::fullscreen(h, label("menu_fullscreen"))?,
                ],
            )?,
            &Submenu::with_items(
                h,
                t.get("menu_window"),
                true,
                &[
                    &MenuItem::with_id(h, MINIMIZE, t.get("window_minimize"), true, None::<&str>)?,
                    &Native::close_window(h, label("menu_close_window"))?,
                ],
            )?,
        ],
    )?;
    handle.set_menu(menu)?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn install<R: Runtime>(_handle: &AppHandle<R>, _t: &Text) -> tauri::Result<()> {
    Ok(())
}

/// Do what a chosen item says. The system's own items do not come here.
pub fn on_event<R: Runtime>(handle: &AppHandle<R>, event: MenuEvent) {
    let id = event.id().as_ref();
    let result = match id {
        OPEN_ROM | SETTINGS | MINI_PLAYER => handle.emit_to("main", "menu", id),
        MINIMIZE => handle
            .webview_windows()
            .values()
            .find(|w| w.is_focused().unwrap_or(false))
            .map_or(Ok(()), |w| w.minimize()),
        _ => Ok(()),
    };
    if let Err(e) = result {
        eprintln!("{e}");
    }
}
