//! Transport bar: playback buttons, position, loop mode, volume and channel meters.

use eframe::egui;
use gb_music::{PlayState, SongLength};

use super::format::{describe, fmt_time};
use super::{GbPlayerApp, LoopMode};

impl GbPlayerApp {
    pub(super) fn transport(&mut self, ui: &mut egui::Ui) {
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
                self.with_player(|p| p.loop_count = mode.loop_count());
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
}
