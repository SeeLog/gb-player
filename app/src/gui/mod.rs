//! The egui application: ROM loading, playlist handling and the main window.

mod format;
mod song_list;
mod transport;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;
use gb_music::{PlayState, Player, Song, SongLength};

use crate::analyzer::Analyzer;
use crate::audio::{AudioOut, SharedPlayer};

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

    /// The player's loop count for this mode.
    fn loop_count(self) -> Option<u32> {
        match self {
            LoopMode::Forever => None,
            LoopMode::Playlist(n) => Some(n),
        }
    }
}

pub struct GbPlayerApp {
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

impl GbPlayerApp {
    pub fn new(initial_rom: Option<PathBuf>) -> GbPlayerApp {
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
            p.loop_count = mode.loop_count();
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
