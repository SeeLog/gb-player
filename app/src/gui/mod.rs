//! The Tauri application: the audio output, the commands for the web UI in `ui/` and a
//! ticker that streams the playback state to the UI. The open ROMs and the playlists live in
//! [`session`].

mod commands;
mod editor;
mod media;
mod mini;
mod playlists;
mod session;
mod shuffle;
mod views;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_window_state::StateFlags;

use crate::audio::{AudioOut, Scope, SharedPlayer, SCOPE_LEN};
use crate::i18n::{system_language, Text};
use crate::settings::Settings;
use session::Session;

/// How often the playback state is sent to the UI.
const TICK: Duration = Duration::from_millis(33);

struct App {
    session: Mutex<Session>,
}

impl App {
    fn lock(&self) -> MutexGuard<'_, Session> {
        self.session.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn emit_library(handle: &AppHandle, s: &Session) {
    let _ = handle.emit("library", s.library());
}

/// Start the audio output on a thread of its own, which keeps the stream alive.
fn start_audio(player: SharedPlayer, scope: Scope, t: &Text) -> Result<u32, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let language = t.language().to_string();
    std::thread::spawn(
        move || match AudioOut::start(player, scope, &Text::new(&language)) {
            Ok(out) => {
                let _ = tx.send(Ok(out.sample_rate));
                loop {
                    std::thread::park();
                }
            }
            Err(e) => {
                let _ = tx.send(Err(e));
            }
        },
    );
    rx.recv().unwrap_or_else(|e| Err(e.to_string()))
}

/// Run the GUI. `language` overrides the saved language for this run.
pub fn run(initial_rom: Option<PathBuf>, language: Option<String>) {
    let settings = Settings::load();
    let text = Text::new(
        &language
            .or_else(|| settings.language.clone())
            .unwrap_or_else(system_language),
    );
    let player: SharedPlayer = Arc::new(Mutex::new(None));
    let scope: Scope = Arc::new(Mutex::new(std::collections::VecDeque::with_capacity(
        SCOPE_LEN,
    )));
    let (sample_rate, error) = match start_audio(player.clone(), scope.clone(), &text) {
        Ok(rate) => (rate, None),
        Err(e) => (48_000, Some(text.fmt("err_audio", &[("error", &e)]))),
    };
    let app = App {
        session: Mutex::new(Session::new(
            player,
            scope,
            text,
            settings,
            sample_rate,
            error,
        )),
    };
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            // Only the size and place: the frame comes from tauri.conf.json. The theme editor
            // opens beside the main window each time instead.
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED)
                .with_denylist(&[editor::LABEL])
                .build(),
        )
        .manage(app)
        .invoke_handler(tauri::generate_handler![
            commands::library,
            commands::open_roms,
            commands::close_rom,
            commands::pick_roms,
            commands::play,
            commands::create_playlist,
            commands::rename_playlist,
            commands::delete_playlist,
            commands::add_to_playlist,
            commands::remove_from_playlist,
            commands::move_in_playlist,
            commands::play_entry,
            commands::set_view,
            commands::relink_entry,
            commands::relink_entry_to,
            commands::toggle,
            commands::stop,
            commands::seek,
            commands::skip,
            commands::set_loop_count,
            commands::set_auto_next,
            commands::set_shuffle,
            commands::set_volume,
            commands::set_tempo,
            commands::set_profile,
            commands::set_channel_volume,
            commands::set_muted,
            commands::set_mini,
            commands::set_mini_on_top,
            commands::set_mini_notes,
            commands::set_language,
            commands::dismiss_errors,
            commands::set_theme,
            commands::save_theme,
            commands::theme_image,
            commands::pick_theme_image,
            commands::open_theme_editor,
        ])
        .on_window_event(|window, event| match event {
            // The window is gone by the time the app exits: keep where the mini player is now.
            // The theme editor goes with it.
            WindowEvent::CloseRequested { .. } if window.label() == "main" => {
                if let Some(main) = window.get_webview_window("main") {
                    mini::remember(&main, window.state::<App>().lock().mini());
                }
                if let Some(editor) = window.get_webview_window(editor::LABEL) {
                    let _ = editor.close();
                }
            }
            // However the editor was closed, the main window shows the chosen theme again.
            WindowEvent::Destroyed if window.label() == editor::LABEL => {
                let _ = window.emit_to("main", editor::DRAFT_EVENT, ());
            }
            _ => {}
        })
        .setup(move |a| {
            let handle = a.handle().clone();
            {
                let app = handle.state::<App>();
                let mut s = app.lock();
                s.reload_themes();
                s.reload_playlists();
                s.open_at_start(initial_rom.as_deref());
                // The window starts hidden (see tauri.conf.json), so the full window does not
                // flash up before it becomes the mini player.
                if let Some(window) = handle.get_webview_window("main") {
                    let shaped = if s.mini().on {
                        mini::show_mini(&window, s.mini())
                    } else {
                        mini::check_full(&window, s.mini())
                    };
                    if let Err(e) = shaped {
                        eprintln!("{e}");
                    }
                    let _ = window.show();
                }
            }
            let main = handle.get_webview_window("main");
            handle.manage(media::Media::new(main.as_ref()));
            let mut shown = None;
            std::thread::spawn(move || loop {
                std::thread::sleep(TICK);
                let app = handle.state::<App>();
                let mut s = app.lock();
                let (view, changed) = s.tick();
                if changed {
                    emit_library(&handle, &s);
                }
                let now = s.now_playing();
                drop(s);
                if shown.as_ref() != Some(&now) {
                    media::show(&handle, now.clone());
                    shown = Some(now);
                }
                let _ = handle.emit("tick", view);
            });
            Ok(())
        })
        .build(tauri::generate_context!());
    match result {
        Ok(tauri_app) => tauri_app.run(|handle, event| {
            if let RunEvent::Exit = event {
                handle.state::<App>().lock().save_settings();
            }
        }),
        Err(e) => eprintln!("{e}"),
    }
}
