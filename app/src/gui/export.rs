//! Exporting songs to WAV files from the GUI: songs of a ROM or a playlist into a folder, on a
//! thread of its own so that playback goes on, with the progress sent to the UI as `export`
//! events. The rendering is [`crate::export`]'s, as for `--wav`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use gb_music::{Profile, SongLength};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use super::App;
use crate::export::{self, file_name, length_of, Options, Part, Source, DEFAULT_LOOPS};
use crate::i18n::Text;

/// What to export.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Target {
    /// Songs of an open ROM (or folder); all of them for `None`.
    Rom {
        rom: String,
        songs: Option<Vec<usize>>,
    },
    /// Entries of a playlist; all of them for `None`.
    Playlist {
        id: String,
        positions: Option<Vec<usize>>,
    },
}

/// A song to export.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// The ROM, GBS or audio file.
    pub path: PathBuf,
    /// Its song number in the file (0 for an audio file).
    pub song: usize,
    pub title: String,
    /// The number its file name starts with, padded to `width` digits.
    pub number: usize,
    pub width: usize,
    /// `None` when it is still to be analysed.
    pub length: Option<SongLength>,
    /// Whether it has APU channels to write stems of: not an audio file.
    pub channels: bool,
}

/// The export running, if any: set to cancel it.
#[derive(Default)]
pub struct Exports {
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}

/// What the export dialog shows before it starts.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub songs: usize,
    /// How many of them have channels, for stems.
    pub with_channels: usize,
    /// The player's loop count; `None` loops forever.
    pub loop_count: Option<u32>,
    /// The loop count to start the dialog with.
    pub loops: u32,
    pub profile: Profile,
    pub noise: bool,
    pub dir: Option<PathBuf>,
}

/// The progress sent with the `export` event.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Progress {
    Running {
        /// Songs written so far, of `total`.
        done: usize,
        total: usize,
        title: String,
        /// How far the song being written is, from 0 to 1.
        fraction: f64,
    },
    /// Finished or cancelled: `errors` are the songs that could not be written.
    Ended {
        written: usize,
        total: usize,
        cancelled: bool,
        errors: Vec<String>,
        dir: PathBuf,
    },
}

/// How often the progress of a song is sent.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

#[tauri::command]
pub fn export_plan(app: State<'_, App>, target: Target) -> Plan {
    let s = app.lock();
    let items = s.export_items(&target);
    let (loop_count, profile, noise, dir) = s.export_settings();
    Plan {
        songs: items.len(),
        with_channels: items.iter().filter(|i| i.channels).count(),
        loop_count,
        loops: loop_count.unwrap_or(DEFAULT_LOOPS),
        profile,
        noise,
        dir: dir.filter(|d| d.is_dir()),
    }
}

/// Let the user pick the folder to export to, starting at `dir`.
#[tauri::command]
pub async fn pick_export_dir(handle: AppHandle, dir: Option<PathBuf>) -> Option<PathBuf> {
    let mut dialog = handle.dialog().file();
    if let Some(d) = dir.filter(|d| d.is_dir()) {
        dialog = dialog.set_directory(d);
    }
    dialog
        .blocking_pick_folder()
        .and_then(|f| f.into_path().ok())
}

/// Start exporting `target` into `dir`, with the player's output; `stems` adds a file per
/// channel for the songs that have channels.
#[tauri::command]
pub fn start_export(
    handle: AppHandle,
    app: State<'_, App>,
    exports: State<'_, Exports>,
    target: Target,
    loops: u32,
    stems: bool,
    dir: PathBuf,
) -> Result<(), String> {
    let mut running = exports
        .cancel
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let mut s = app.lock();
    if running.is_some() {
        return Err(s.text().get("export_busy").to_string());
    }
    if !dir.is_dir() {
        return Err(s.text().fmt("export_no_dir", &[("path", &dir.display())]));
    }
    let items = s.export_items(&target);
    let (_, profile, noise, _) = s.export_settings();
    s.set_export_dir(dir.clone());
    let text = Text::new(s.text().language());
    drop(s);
    let options = Options {
        loops: loops.max(1),
        profile,
        noise,
    };
    let cancel = Arc::new(AtomicBool::new(false));
    *running = Some(cancel.clone());
    std::thread::spawn(move || {
        let ended = run(&handle, &text, &items, &options, stems, &dir, &cancel);
        let exports = handle.state::<Exports>();
        *exports
            .cancel
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
        let _ = handle.emit("export", ended);
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_export(exports: State<'_, Exports>) {
    if let Some(c) = &*exports
        .cancel
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
    {
        c.store(true, Ordering::Relaxed);
    }
}

/// Export `items` one after another; returns how it ended.
fn run(
    handle: &AppHandle,
    text: &Text,
    items: &[Item],
    options: &Options,
    stems: bool,
    dir: &Path,
    cancel: &AtomicBool,
) -> Progress {
    let total = items.len();
    let mut sources: HashMap<PathBuf, Result<Source, String>> = HashMap::new();
    let mut errors = Vec::new();
    let mut written = 0;
    for (done, item) in items.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let running = |fraction| Progress::Running {
            done,
            total,
            title: item.title.clone(),
            fraction,
        };
        let _ = handle.emit("export", running(0.0));
        let source = sources
            .entry(item.path.clone())
            .or_insert_with(|| Source::open(&item.path));
        let mut sent = Instant::now();
        let result = source.as_ref().map_err(String::clone).and_then(|source| {
            song(text, source, item, options, stems, dir, cancel, &mut |f| {
                if sent.elapsed() >= PROGRESS_EVERY {
                    sent = Instant::now();
                    let _ = handle.emit("export", running(f));
                }
            })
        });
        match result {
            Ok(true) => written += 1,
            Ok(false) => break,
            Err(e) => errors.push(format!("{}: {e}", item.title)),
        }
    }
    Progress::Ended {
        written,
        total,
        cancelled: cancel.load(Ordering::Relaxed),
        errors,
        dir: dir.to_path_buf(),
    }
}

/// Export one song; returns `false` when it was cancelled.
#[allow(clippy::too_many_arguments)]
fn song(
    text: &Text,
    source: &Source,
    item: &Item,
    options: &Options,
    stems: bool,
    dir: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f64),
) -> Result<bool, String> {
    let player = source.player()?;
    let song = player
        .songs()
        .get(item.song)
        .ok_or_else(|| text.get("err_no_song").to_string())?;
    let length = match item.length {
        Some(l) => l,
        None => length_of(&player, song)?,
    };
    let parts = if stems && item.channels {
        Part::with_stems(options)
    } else {
        vec![Part::Mix]
    };
    let files: Vec<(Part, PathBuf)> = parts
        .into_iter()
        .map(|part| {
            let name = file_name(
                item.number,
                item.width,
                &item.title,
                part.text_key().map(|k| text.get(k)),
            );
            (part, dir.join(name))
        })
        .collect();
    match export::render(
        &|| source.player(),
        item.song,
        length,
        options,
        &files,
        cancel,
        progress,
    ) {
        Ok(_) => Ok(true),
        Err(export::Error::Cancelled) => Ok(false),
        Err(export::Error::Failed(e)) => Err(e),
    }
}
