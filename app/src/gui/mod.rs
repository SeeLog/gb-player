//! The Tauri application: commands for the web UI in `ui/`, ROM loading and playlist
//! handling, and a ticker that streams the playback state to the UI.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use gb_music::songdb::LANGUAGES;
use gb_music::{frames_to_secs, PlayState, Player, Song, SongLength};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, RunEvent, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_window_state::StateFlags;

use crate::analyzer::Analyzer;
use crate::audio::{AudioOut, Scope, SharedPlayer, SCOPE_LEN};
use crate::i18n::{system_language, Text};
use crate::settings::Settings;
use crate::themes::{self, Background, Theme};

/// How often the playback state is sent to the UI.
const TICK: Duration = Duration::from_millis(33);
/// Output samples sent with each tick for the oscilloscope.
const SCOPE_POINTS: usize = 1024;

struct Session {
    /// UI text in the chosen language; song names follow the same language.
    text: Text,
    settings: Settings,
    sample_rate: u32,
    error: Option<String>,
    rom_path: Option<PathBuf>,
    songs: Vec<Song>,
    game: String,
    title: String,
    lengths: HashMap<usize, SongLength>,
    analyzer: Option<Analyzer>,
    levels: [f32; 4],
    themes: Vec<Theme>,
    /// Problems found in the custom theme files.
    theme_warnings: Vec<String>,
}

struct App {
    player: SharedPlayer,
    scope: Scope,
    session: Mutex<Session>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum LengthView {
    Loops { intro: f64, period: f64 },
    Ends { at: f64 },
    Unknown,
}

impl From<SongLength> for LengthView {
    fn from(l: SongLength) -> LengthView {
        match l {
            SongLength::Loops { intro, period } => LengthView::Loops {
                intro: frames_to_secs(intro),
                period: frames_to_secs(period),
            },
            SongLength::Ends { at } => LengthView::Ends {
                at: frames_to_secs(at),
            },
            SongLength::Unknown => LengthView::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct SongView {
    index: usize,
    title: String,
    /// `None` while the song is still being analysed.
    length: Option<LengthView>,
}

#[derive(Debug, Clone, Serialize)]
struct LanguageView {
    code: &'static str,
    name: &'static str,
}

/// Everything about the loaded ROM and the settings, sent when any of it changes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryView {
    language: String,
    languages: Vec<LanguageView>,
    text: HashMap<String, String>,
    game: String,
    title: String,
    rom_path: Option<String>,
    songs: Vec<SongView>,
    loop_count: Option<u32>,
    auto_next: bool,
    volume: f32,
    muted: [bool; 4],
    theme: String,
    themes: Vec<Theme>,
    error: Option<String>,
}

/// The playback state, sent on every tick.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TickView {
    current: Option<usize>,
    /// `none` (no ROM), `stopped`, `playing`, `paused`, `finished` or `failed`.
    state: &'static str,
    failure: Option<String>,
    /// Seconds since the song started.
    position: f64,
    length: LengthView,
    levels: [f32; 4],
    /// The latest output samples, oldest first.
    scope: Vec<f32>,
}

impl App {
    fn lock(&self) -> MutexGuard<'_, Session> {
        self.session.lock().unwrap()
    }

    fn with_player<R>(&self, f: impl FnOnce(&mut Player) -> R) -> Option<R> {
        self.player.lock().unwrap().as_mut().map(f)
    }

    fn load_rom(&self, s: &mut Session, path: &Path) {
        let rom = match std::fs::read(path) {
            Ok(r) => r,
            Err(e) => {
                s.error = Some(
                    s.text
                        .fmt("err_read", &[("path", &path.display()), ("error", &e)]),
                );
                return;
            }
        };
        match Player::new(rom, s.sample_rate) {
            Ok(mut p) => {
                p.volume = s.settings.volume;
                let warnings = p.set_language(s.text.language());
                s.lengths.clear();
                s.analyzer = Some(Analyzer::start(p.rom(), p.driver().clone()));
                s.rom_path = Some(path.to_path_buf());
                *self.player.lock().unwrap() = Some(p);
                self.refresh_names(s, warnings);
            }
            Err(e) => s.error = Some(s.text.fmt("err_unsupported", &[("error", &e)])),
        }
    }

    /// Copy the names from the player after loading a ROM or switching languages.
    fn refresh_names(&self, s: &mut Session, warnings: Vec<String>) {
        if let Some((songs, game, title)) = self.with_player(|p| {
            (
                p.songs().to_vec(),
                p.game().to_string(),
                p.title().to_string(),
            )
        }) {
            s.songs = songs;
            s.game = game;
            s.title = title;
        }
        s.error = (!warnings.is_empty()).then(|| warnings.join("\n"));
    }

    fn reload_themes(&self, s: &mut Session) {
        (s.themes, s.theme_warnings) = themes::load(&s.text);
    }

    fn play(&self, s: &mut Session, index: usize) {
        if let Some(a) = &s.analyzer {
            a.prioritize(index);
        }
        let length = s.lengths.get(&index).copied();
        let count = s.settings.loop_count;
        self.with_player(|p| {
            p.play(index);
            p.loop_count = count;
            if let Some(l) = length {
                p.set_length(index, l);
            }
        });
    }

    fn library(&self, s: &Session) -> LibraryView {
        let muted = self
            .with_player(|p| std::array::from_fn(|ch| p.muted(ch)))
            .unwrap_or_default();
        LibraryView {
            language: s.text.language().to_string(),
            languages: LANGUAGES
                .iter()
                .map(|&(code, name)| LanguageView { code, name })
                .collect(),
            text: s
                .text
                .table()
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            game: s.game.clone(),
            title: s.title.clone(),
            rom_path: s.rom_path.as_ref().map(|p| p.display().to_string()),
            songs: s
                .songs
                .iter()
                .map(|song| SongView {
                    index: song.index,
                    title: song.title.clone(),
                    length: s.lengths.get(&song.index).map(|&l| l.into()),
                })
                .collect(),
            loop_count: s.settings.loop_count,
            auto_next: s.settings.auto_next,
            volume: s.settings.volume,
            muted,
            theme: if s.themes.iter().any(|t| t.id == s.settings.theme) {
                s.settings.theme.clone()
            } else {
                themes::BUILTIN[0].0.to_string()
            },
            themes: s.themes.clone(),
            error: {
                let errors: Vec<&str> = s
                    .error
                    .iter()
                    .chain(&s.theme_warnings)
                    .map(String::as_str)
                    .collect();
                (!errors.is_empty()).then(|| errors.join("\n"))
            },
        }
    }

    /// Advance the playlist and collect analysis results; returns the state for the UI and
    /// whether the library changed.
    fn tick(&self, s: &mut Session) -> (TickView, bool) {
        let results: Vec<_> = s
            .analyzer
            .as_ref()
            .map(|a| a.results.try_iter().collect())
            .unwrap_or_default();
        let changed = !results.is_empty();
        for (index, length) in results {
            s.lengths.insert(index, length);
            self.with_player(|p| p.set_length(index, length));
        }
        // Advance the playlist when a song has finished, if asked to.
        let finished = self
            .with_player(|p| (p.state() == PlayState::Finished).then(|| p.current()))
            .flatten()
            .flatten();
        if let Some(cur) = finished {
            if s.settings.auto_next && !s.songs.is_empty() {
                self.play(s, (cur + 1) % s.songs.len());
            }
        }
        let info = self.with_player(|p| {
            (
                p.current(),
                p.state(),
                p.paused(),
                p.position_frames(),
                p.length(),
                p.take_levels(),
            )
        });
        let scope = {
            let samples = self.scope.lock().unwrap();
            let skip = samples.len().saturating_sub(SCOPE_POINTS);
            samples.iter().skip(skip).copied().collect()
        };
        let Some((current, state, paused, pos, length, levels)) = info else {
            let view = TickView {
                current: None,
                state: "none",
                failure: None,
                position: 0.0,
                length: LengthView::Unknown,
                levels: [0.0; 4],
                scope,
            };
            return (view, changed);
        };
        for (a, b) in s.levels.iter_mut().zip(levels) {
            *a = (*a * 0.85).max(b);
        }
        let (state, failure) = match state {
            PlayState::Stopped => ("stopped", None),
            PlayState::Playing if paused => ("paused", None),
            PlayState::Playing => ("playing", None),
            PlayState::Finished => ("finished", None),
            PlayState::Failed(e) => (
                "failed",
                Some(s.text.fmt("err_playback_ui", &[("error", &e)])),
            ),
        };
        let view = TickView {
            current,
            state,
            failure,
            position: frames_to_secs(pos),
            length: length.into(),
            levels: s.levels,
            scope,
        };
        (view, changed)
    }
}

fn emit_library(handle: &AppHandle, app: &App, s: &Session) {
    let _ = handle.emit("library", app.library(s));
}

#[tauri::command]
fn library(app: State<'_, App>) -> LibraryView {
    app.library(&app.lock())
}

#[tauri::command]
fn open_rom(handle: AppHandle, app: State<'_, App>, path: PathBuf) {
    let mut s = app.lock();
    app.load_rom(&mut s, &path);
    emit_library(&handle, &app, &s);
}

#[tauri::command]
async fn pick_rom(handle: AppHandle) {
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
    app.load_rom(&mut s, &path);
    emit_library(&handle, &app, &s);
}

#[tauri::command]
fn play(app: State<'_, App>, index: usize) {
    let mut s = app.lock();
    app.play(&mut s, index);
}

/// Play or pause; starts the current (or first) song again when nothing is playing.
#[tauri::command]
fn toggle(app: State<'_, App>) {
    let mut s = app.lock();
    let Some((current, state, paused)) = app.with_player(|p| (p.current(), p.state(), p.paused()))
    else {
        return;
    };
    match (current, state) {
        (Some(_), PlayState::Playing) => {
            app.with_player(|p| p.set_paused(!paused));
        }
        (Some(c), _) => app.play(&mut s, c),
        (None, _) if !s.songs.is_empty() => app.play(&mut s, 0),
        _ => {}
    }
}

#[tauri::command]
fn stop(app: State<'_, App>) {
    app.with_player(|p| p.stop());
}

/// Play the song `delta` places away from the current one, wrapping around the list.
#[tauri::command]
fn skip(app: State<'_, App>, delta: i64) {
    let mut s = app.lock();
    let n = s.songs.len() as i64;
    if n == 0 {
        return;
    }
    let current = app.with_player(|p| p.current()).flatten();
    let next = current.map_or(0, |c| (c as i64 + delta).rem_euclid(n));
    app.play(&mut s, next as usize);
}

#[tauri::command]
fn set_loop_count(handle: AppHandle, app: State<'_, App>, count: Option<u32>) {
    let mut s = app.lock();
    s.settings.loop_count = count;
    app.with_player(|p| p.loop_count = count);
    emit_library(&handle, &app, &s);
}

#[tauri::command]
fn set_auto_next(handle: AppHandle, app: State<'_, App>, auto_next: bool) {
    let mut s = app.lock();
    s.settings.auto_next = auto_next;
    emit_library(&handle, &app, &s);
}

#[tauri::command]
fn set_volume(app: State<'_, App>, volume: f32) {
    let volume = volume.clamp(0.0, 1.0);
    app.lock().settings.volume = volume;
    app.with_player(|p| p.volume = volume);
}

#[tauri::command]
fn set_muted(handle: AppHandle, app: State<'_, App>, channel: usize, muted: bool) {
    if channel >= 4 {
        return;
    }
    let s = app.lock();
    app.with_player(|p| p.set_muted(channel, muted));
    emit_library(&handle, &app, &s);
}

/// Switch the UI text and the game and song names to `language`, and remember it.
#[tauri::command]
fn set_language(handle: AppHandle, app: State<'_, App>, language: String) {
    let mut s = app.lock();
    s.text = Text::new(&language);
    if let Some(warnings) = app.with_player(|p| p.set_language(&language)) {
        app.refresh_names(&mut s, warnings);
    }
    s.settings.language = Some(language);
    s.settings.save();
    app.reload_themes(&mut s);
    emit_library(&handle, &app, &s);
}

#[tauri::command]
fn set_theme(handle: AppHandle, app: State<'_, App>, theme: String) {
    let mut s = app.lock();
    s.settings.theme = theme;
    s.settings.save();
    emit_library(&handle, &app, &s);
}

/// Save a custom theme and switch to it; returns its id.
#[tauri::command]
fn save_theme(
    handle: AppHandle,
    app: State<'_, App>,
    name: String,
    base: String,
    colors: BTreeMap<String, String>,
    background: Option<Background>,
) -> Result<String, String> {
    let mut s = app.lock();
    let id = themes::save(&name, &base, colors, background, &s.text)?;
    app.reload_themes(&mut s);
    s.settings.theme = id.clone();
    s.settings.save();
    emit_library(&handle, &app, &s);
    Ok(id)
}

/// A background image of the themes folder as a `data:` URL.
#[tauri::command]
fn theme_image(app: State<'_, App>, image: String) -> Result<String, String> {
    themes::image_url(&image, &app.lock().text)
}

/// Let the user pick a background image and copy it into the themes folder; returns its name
/// for the theme, or `None` when nothing was picked.
#[tauri::command]
async fn pick_theme_image(handle: AppHandle) -> Result<Option<String>, String> {
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
    let text = Text::new(app.lock().text.language());
    themes::import_image(&path, &text).map(Some)
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
        player,
        scope,
        session: Mutex::new(Session {
            text,
            settings,
            sample_rate,
            error,
            rom_path: None,
            songs: Vec::new(),
            game: String::new(),
            title: String::new(),
            lengths: HashMap::new(),
            analyzer: None,
            levels: [0.0; 4],
            themes: Vec::new(),
            theme_warnings: Vec::new(),
        }),
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
            library,
            open_rom,
            pick_rom,
            play,
            toggle,
            stop,
            skip,
            set_loop_count,
            set_auto_next,
            set_volume,
            set_muted,
            set_language,
            set_theme,
            save_theme,
            theme_image,
            pick_theme_image,
        ])
        .setup(move |a| {
            let handle = a.handle().clone();
            {
                let app = handle.state::<App>();
                let mut s = app.lock();
                app.reload_themes(&mut s);
                if let Some(path) = initial_rom {
                    app.load_rom(&mut s, &path);
                }
            }
            std::thread::spawn(move || loop {
                std::thread::sleep(TICK);
                let app = handle.state::<App>();
                let mut s = app.lock();
                let (view, changed) = app.tick(&mut s);
                if changed {
                    emit_library(&handle, &app, &s);
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
                handle.state::<App>().lock().settings.save();
            }
        }),
        Err(e) => eprintln!("{e}"),
    }
}
