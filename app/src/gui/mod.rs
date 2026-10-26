//! The Tauri application: the audio output, the commands for the web UI in `ui/` and a
//! ticker that streams the playback state to the UI. ROM loading and playlist handling live
//! in [`session`].

mod commands;
mod session;
mod views;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, RunEvent};
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
            // Only the size and place: the frame comes from tauri.conf.json.
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED)
                .build(),
        )
        .manage(app)
        .invoke_handler(tauri::generate_handler![
            commands::library,
            commands::open_rom,
            commands::pick_rom,
            commands::play,
            commands::toggle,
            commands::stop,
            commands::skip,
            commands::set_loop_count,
            commands::set_auto_next,
            commands::set_volume,
            commands::set_muted,
            commands::set_language,
            commands::set_theme,
            commands::save_theme,
            commands::theme_image,
            commands::pick_theme_image,
        ])
        .setup(move |a| {
            let handle = a.handle().clone();
            {
                let app = handle.state::<App>();
                let mut s = app.lock();
                s.reload_themes();
                if let Some(path) = initial_rom {
                    s.load_rom(&path);
                }
            }
            std::thread::spawn(move || loop {
                std::thread::sleep(TICK);
                let app = handle.state::<App>();
                let mut s = app.lock();
                let (view, changed) = s.tick();
                if changed {
                    emit_library(&handle, &s);
                }
                drop(s);
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
