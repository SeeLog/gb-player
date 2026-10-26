//! GB Player: plays the music of Game Boy games from their ROM.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod cli;

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;
use gb_music::{analyze, frames_to_secs, Driver, PlayState, Player, Song, SongLength, FRAME_RATE};

use audio::{AudioOut, SharedPlayer};

/// Ten minutes of music: long enough for any loop in the supported games.
const ANALYZE_FRAMES: u32 = (FRAME_RATE * 600.0) as u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopMode {
    /// Keep looping the current song.
    Forever,
    /// Play the loop N times, fade out, then continue with the next song.
    Playlist(u32),
}

impl LoopMode {
    fn label(self) -> String {
        match self {
            LoopMode::Forever => "無限ループ".into(),
            LoopMode::Playlist(n) => format!("{n}ループ後フェードアウトして次の曲へ"),
        }
    }
}

/// Finds song loop points in the background, current song first.
struct Analyzer {
    queue: Arc<Mutex<VecDeque<usize>>>,
    results: Receiver<(usize, SongLength)>,
    cancel: Arc<AtomicBool>,
}

impl Analyzer {
    fn start(rom: Arc<[u8]>, driver: Driver) -> Analyzer {
        let queue = Arc::new(Mutex::new((0..driver.songs.len()).collect::<VecDeque<_>>()));
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, results) = channel();
        let (q, c) = (queue.clone(), cancel.clone());
        std::thread::spawn(move || loop {
            if c.load(Ordering::Relaxed) {
                return;
            }
            let Some(index) = q.lock().unwrap().pop_front() else {
                return;
            };
            let song: &Song = &driver.songs[index];
            let length =
                analyze(&rom, &driver, song, ANALYZE_FRAMES).unwrap_or(SongLength::Unknown);
            if tx.send((index, length)).is_err() {
                return;
            }
        });
        Analyzer {
            queue,
            results,
            cancel,
        }
    }

    fn prioritize(&self, index: usize) {
        let mut q = self.queue.lock().unwrap();
        if let Some(pos) = q.iter().position(|&i| i == index) {
            q.remove(pos);
            q.push_front(index);
        }
    }
}

impl Drop for Analyzer {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

struct GbPlayerApp {
    player: SharedPlayer,
    audio: Option<AudioOut>,
    error: Option<String>,
    rom_path: Option<PathBuf>,
    songs: Vec<Song>,
    game: String,
    lengths: HashMap<usize, SongLength>,
    analyzer: Option<Analyzer>,
    loop_mode: LoopMode,
    volume: f32,
    filter: String,
    levels: [f32; 4],
}

fn fmt_time(frames: u32) -> String {
    let s = frames_to_secs(frames);
    format!("{}:{:04.1}", (s / 60.0) as u32, s % 60.0)
}

fn describe(length: Option<&SongLength>) -> String {
    match length {
        None => "解析中…".into(),
        Some(SongLength::Loops { intro, period }) => {
            format!(
                "イントロ {} / ループ {}",
                fmt_time(*intro),
                fmt_time(*period)
            )
        }
        Some(SongLength::Ends { at }) => format!("{} で終了", fmt_time(*at)),
        Some(SongLength::Unknown) => "ループ不明".into(),
    }
}

impl GbPlayerApp {
    fn new(initial_rom: Option<PathBuf>) -> GbPlayerApp {
        let player: SharedPlayer = Arc::new(Mutex::new(None));
        let (audio, error) = match AudioOut::start(player.clone()) {
            Ok(a) => (Some(a), None),
            Err(e) => (None, Some(format!("オーディオ出力を開けません: {e}"))),
        };
        let mut app = GbPlayerApp {
            player,
            audio,
            error,
            rom_path: None,
            songs: Vec::new(),
            game: String::new(),
            lengths: HashMap::new(),
            analyzer: None,
            loop_mode: LoopMode::Forever,
            volume: 0.8,
            filter: String::new(),
            levels: [0.0; 4],
        };
        if let Some(p) = initial_rom {
            app.load_rom(&p);
        }
        app
    }

    fn sample_rate(&self) -> u32 {
        self.audio.as_ref().map_or(48_000, |a| a.sample_rate)
    }

    fn load_rom(&mut self, path: &Path) {
        let rom = match std::fs::read(path) {
            Ok(r) => r,
            Err(e) => {
                self.error = Some(format!("{} を読み込めません: {e}", path.display()));
                return;
            }
        };
        match Player::new(rom, self.sample_rate()) {
            Ok(mut p) => {
                p.volume = self.volume;
                self.songs = p.songs().to_vec();
                self.game = format!("{} ({})", p.game(), p.title());
                self.lengths.clear();
                self.analyzer = Some(Analyzer::start(p.rom(), p.driver().clone()));
                self.rom_path = Some(path.to_path_buf());
                self.error = None;
                *self.player.lock().unwrap() = Some(p);
            }
            Err(e) => self.error = Some(format!("このROMは再生できません: {e}")),
        }
    }

    fn with_player<R>(&self, f: impl FnOnce(&mut Player) -> R) -> Option<R> {
        self.player.lock().unwrap().as_mut().map(f)
    }

    fn play(&mut self, index: usize) {
        if let Some(a) = &self.analyzer {
            a.prioritize(index);
        }
        let length = self.lengths.get(&index).copied();
        let mode = self.loop_mode;
        self.with_player(|p| {
            p.play(index);
            p.loop_count = match mode {
                LoopMode::Forever => None,
                LoopMode::Playlist(n) => Some(n),
            };
            if let Some(l) = length {
                p.set_length(index, l);
            }
        });
    }

    fn poll_background(&mut self) {
        if let Some(a) = &self.analyzer {
            while let Ok((index, length)) = a.results.try_recv() {
                self.lengths.insert(index, length);
                self.with_player(|p| p.set_length(index, length));
            }
        }
        // Advance the playlist when a song has finished.
        let finished = self.with_player(|p| {
            (p.state() == PlayState::Finished)
                .then(|| p.current())
                .flatten()
        });
        if let Some(Some(cur)) = finished {
            if !self.songs.is_empty() {
                self.play((cur + 1) % self.songs.len());
            }
        }
        if let Some(lv) = self.with_player(|p| p.take_levels()) {
            for (a, b) in self.levels.iter_mut().zip(lv) {
                *a = (*a * 0.85).max(b);
            }
        }
    }

    fn transport(&mut self, ui: &mut egui::Ui) {
        let info = self.with_player(|p| {
            (
                p.current(),
                p.state(),
                p.paused(),
                p.position_frames(),
                p.length(),
            )
        });
        let Some((current, state, paused, pos, length)) = info else {
            ui.label("ROMファイル(.gb / .gbc)を開くか、ウィンドウにドロップしてください。");
            return;
        };
        ui.horizontal(|ui| {
            let n = self.songs.len();
            if ui.button("⏮").clicked() && n > 0 {
                self.play(current.map_or(0, |c| (c + n - 1) % n));
            }
            let playing = state == PlayState::Playing && !paused;
            if ui.button(if playing { "⏸" } else { "▶" }).clicked() {
                match (current, state) {
                    (Some(c), PlayState::Stopped | PlayState::Finished) => self.play(c),
                    (Some(_), _) => {
                        self.with_player(|p| p.set_paused(playing));
                    }
                    (None, _) if n > 0 => self.play(0),
                    _ => {}
                }
            }
            if ui.button("⏹").clicked() {
                self.with_player(|p| p.stop());
            }
            if ui.button("⏭").clicked() && n > 0 {
                self.play(current.map_or(0, |c| (c + 1) % n));
            }
            ui.separator();
            if let Some(c) = current {
                ui.strong(&self.songs[c].title);
                ui.label(fmt_time(pos));
                if let SongLength::Loops { intro, period } = length {
                    let loops = pos.saturating_sub(intro) / period;
                    ui.label(format!("({} 周目)", loops + 1));
                }
            }
            if let PlayState::Failed(e) = state {
                ui.colored_label(egui::Color32::RED, format!("再生エラー: {e}"));
            }
        });
        if let Some(c) = current {
            ui.label(describe(self.lengths.get(&c)));
        }
        ui.horizontal(|ui| {
            ui.label("ループ:");
            let before = self.loop_mode;
            egui::ComboBox::from_id_salt("loop_mode")
                .selected_text(self.loop_mode.label())
                .show_ui(ui, |ui| {
                    for m in [
                        LoopMode::Forever,
                        LoopMode::Playlist(1),
                        LoopMode::Playlist(2),
                        LoopMode::Playlist(3),
                    ] {
                        ui.selectable_value(&mut self.loop_mode, m, m.label());
                    }
                });
            if before != self.loop_mode {
                let mode = self.loop_mode;
                self.with_player(|p| {
                    p.loop_count = match mode {
                        LoopMode::Forever => None,
                        LoopMode::Playlist(n) => Some(n),
                    }
                });
            }
            ui.separator();
            ui.label("音量");
            if ui
                .add(egui::Slider::new(&mut self.volume, 0.0..=1.0).show_value(false))
                .changed()
            {
                let v = self.volume;
                self.with_player(|p| p.volume = v);
            }
        });
        ui.horizontal(|ui| {
            const NAMES: [&str; 4] = ["矩形波1", "矩形波2", "波形メモリ", "ノイズ"];
            for (ch, name) in NAMES.iter().enumerate() {
                let mut on = !self.with_player(|p| p.muted(ch)).unwrap_or(false);
                if ui.checkbox(&mut on, *name).changed() {
                    self.with_player(|p| p.set_muted(ch, !on));
                }
                ui.add(egui::ProgressBar::new(self.levels[ch]).desired_width(60.0));
            }
        });
    }

    fn song_list(&mut self, ui: &mut egui::Ui) {
        let current = self.with_player(|p| p.current()).flatten();
        ui.horizontal(|ui| {
            ui.label("検索");
            ui.text_edit_singleline(&mut self.filter);
        });
        ui.separator();
        let filter = self.filter.to_lowercase();
        let mut clicked = None;
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for song in &self.songs {
                    if !filter.is_empty() && !song.title.to_lowercase().contains(&filter) {
                        continue;
                    }
                    let text = format!("{:3}  {}", song.index + 1, song.title);
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(current == Some(song.index), text)
                            .clicked()
                        {
                            clicked = Some(song.index);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.weak(describe(self.lengths.get(&song.index)));
                        });
                    });
                }
            });
        if let Some(i) = clicked {
            self.play(i);
        }
    }
}

impl eframe::App for GbPlayerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let dropped = ui
            .ctx()
            .input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()));
        if let Some(path) = dropped {
            self.load_rom(&path);
        }
        self.poll_background();

        egui::Panel::top("top").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("ROMを開く…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Game Boy ROM", &["gb", "gbc", "GB", "GBC"])
                        .pick_file()
                    {
                        self.load_rom(&path);
                    }
                }
                if !self.game.is_empty() {
                    ui.label(&self.game);
                    ui.weak(format!("{} 曲", self.songs.len()));
                }
            });
            if let Some(e) = &self.error {
                ui.colored_label(egui::Color32::from_rgb(220, 80, 80), e);
            }
        });
        egui::Panel::bottom("transport").show(ui, |ui| {
            ui.add_space(4.0);
            self.transport(ui);
            ui.add_space(4.0);
        });
        egui::CentralPanel::default().show(ui, |ui| self.song_list(ui));

        ui.ctx().request_repaint_after(Duration::from_millis(33));
    }
}

fn main() -> eframe::Result {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    if args
        .first()
        .is_some_and(|a| a.to_string_lossy().starts_with("--"))
    {
        attach_console();
        std::process::exit(cli::run(&args));
    }
    let initial_rom = args.first().map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("GB Player")
            .with_inner_size([720.0, 560.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "GB Player",
        options,
        Box::new(|_cc| Ok(Box::new(GbPlayerApp::new(initial_rom)))),
    )
}

/// Release builds on Windows use the GUI subsystem, so the CLI has to borrow the console of
/// the shell it was started from to print anything.
#[cfg(windows)]
fn attach_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
    }
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

#[cfg(not(windows))]
fn attach_console() {}
