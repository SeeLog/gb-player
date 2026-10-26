//! Searchable song list.

use eframe::egui;

use super::format::describe;
use super::GbPlayerApp;

impl GbPlayerApp {
    pub(super) fn song_list(&mut self, ui: &mut egui::Ui) {
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
