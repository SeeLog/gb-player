//! Playback control behind the GUI commands: the loaded ROM, the playlist, the settings and
//! the analysis results, independent of Tauri.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use gb_music::songdb::{Names, LANGUAGES};
use gb_music::{frames_to_secs, PlayState, Player, Song, SongLength};

use super::views::{LanguageView, LengthView, LibraryView, SongView, TickView};
use crate::analyzer::Analyzer;
use crate::audio::{Scope, SharedPlayer};
use crate::i18n::Text;
use crate::settings::Settings;
use crate::themes::{self, Background, Theme};

/// Output samples sent with each tick for the oscilloscope.
const SCOPE_POINTS: usize = 1024;

pub struct Session {
    player: SharedPlayer,
    scope: Scope,
    /// UI text in the chosen language; song names follow the same language.
    text: Text,
    settings: Settings,
    sample_rate: u32,
    error: Option<String>,
    rom_path: Option<PathBuf>,
    songs: Vec<Song>,
    /// Game and song names in the UI language.
    names: Names,
    title: String,
    lengths: HashMap<usize, SongLength>,
    analyzer: Option<Analyzer>,
    levels: [f32; 4],
    themes: Vec<Theme>,
    /// Problems found in the custom theme files.
    theme_warnings: Vec<String>,
}

/// What play/pause does in a given playback state.
#[derive(Debug, PartialEq, Eq)]
enum Toggle {
    SetPaused(bool),
    Play(usize),
    Nothing,
}

/// Play or pause; starts the current (or first) song again when nothing is playing.
fn toggle_action(
    current: Option<usize>,
    state: PlayState,
    paused: bool,
    song_count: usize,
) -> Toggle {
    match (current, state) {
        (Some(_), PlayState::Playing) => Toggle::SetPaused(!paused),
        (Some(c), _) => Toggle::Play(c),
        (None, _) if song_count > 0 => Toggle::Play(0),
        _ => Toggle::Nothing,
    }
}

/// The song `delta` places away from `current`, wrapping around the list; the first song
/// when nothing was played yet.
fn skip_target(current: Option<usize>, delta: i64, song_count: usize) -> Option<usize> {
    let n = song_count as i64;
    if n == 0 {
        return None;
    }
    Some(current.map_or(0, |c| (c as i64 + delta).rem_euclid(n)) as usize)
}

impl Session {
    pub fn new(
        player: SharedPlayer,
        scope: Scope,
        text: Text,
        settings: Settings,
        sample_rate: u32,
        error: Option<String>,
    ) -> Session {
        Session {
            player,
            scope,
            text,
            settings,
            sample_rate,
            error,
            rom_path: None,
            songs: Vec::new(),
            names: Names::default(),
            title: String::new(),
            lengths: HashMap::new(),
            analyzer: None,
            levels: [0.0; 4],
            themes: Vec::new(),
            theme_warnings: Vec::new(),
        }
    }

    pub fn text(&self) -> &Text {
        &self.text
    }

    fn with_player<R>(&self, f: impl FnOnce(&mut Player) -> R) -> Option<R> {
        self.player.lock().unwrap().as_mut().map(f)
    }

    pub fn load_rom(&mut self, path: &Path) {
        let rom = match std::fs::read(path) {
            Ok(r) => r,
            Err(e) => {
                self.error = Some(
                    self.text
                        .fmt("err_read", &[("path", &path.display()), ("error", &e)]),
                );
                return;
            }
        };
        match Player::new(rom, self.sample_rate) {
            Ok(mut p) => {
                p.volume = self.settings.volume;
                self.lengths.clear();
                self.analyzer = Some(Analyzer::start(p.rom(), p.driver().clone()));
                self.rom_path = Some(path.to_path_buf());
                *self.player.lock().unwrap() = Some(p);
                self.refresh_names();
            }
            Err(e) => self.error = Some(self.text.fmt("err_unsupported", &[("error", &e)])),
        }
    }

    /// Look up the game and song names in the UI language after loading a ROM or switching
    /// languages.
    fn refresh_names(&mut self) {
        let Some((songs, title, (names, warnings))) = self.with_player(|p| {
            (
                p.songs().to_vec(),
                p.title().to_string(),
                Names::new(p.driver(), self.text.language()),
            )
        }) else {
            return;
        };
        self.songs = songs;
        self.title = title;
        self.names = names;
        self.error = (!warnings.is_empty()).then(|| warnings.join("\n"));
    }

    pub fn reload_themes(&mut self) {
        (self.themes, self.theme_warnings) = themes::load(&self.text);
    }

    pub fn play(&mut self, index: usize) {
        if let Some(a) = &self.analyzer {
            a.prioritize(index);
        }
        let length = self.lengths.get(&index).copied();
        let count = self.settings.loop_count;
        self.with_player(|p| {
            p.play(index);
            p.loop_count = count;
            if let Some(l) = length {
                p.set_length(index, l);
            }
        });
    }

    /// Play or pause; starts the current (or first) song again when nothing is playing.
    pub fn toggle(&mut self) {
        let Some((current, state, paused)) =
            self.with_player(|p| (p.current(), p.state(), p.paused()))
        else {
            return;
        };
        match toggle_action(current, state, paused, self.songs.len()) {
            Toggle::SetPaused(paused) => {
                self.with_player(|p| p.set_paused(paused));
            }
            Toggle::Play(index) => self.play(index),
            Toggle::Nothing => {}
        }
    }

    pub fn stop(&self) {
        self.with_player(|p| p.stop());
    }

    /// Play the song `delta` places away from the current one, wrapping around the list.
    pub fn skip(&mut self, delta: i64) {
        let current = self.with_player(|p| p.current()).flatten();
        if let Some(next) = skip_target(current, delta, self.songs.len()) {
            self.play(next);
        }
    }

    pub fn set_loop_count(&mut self, count: Option<u32>) {
        self.settings.loop_count = count;
        self.with_player(|p| p.loop_count = count);
    }

    pub fn set_auto_next(&mut self, auto_next: bool) {
        self.settings.auto_next = auto_next;
    }

    pub fn set_volume(&mut self, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        self.settings.volume = volume;
        self.with_player(|p| p.volume = volume);
    }

    pub fn set_muted(&self, channel: usize, muted: bool) {
        self.with_player(|p| p.set_muted(channel, muted));
    }

    /// Switch the UI text and the game and song names to `language`, and remember it.
    pub fn set_language(&mut self, language: String) {
        self.text = Text::new(&language);
        self.refresh_names();
        self.settings.language = Some(language);
        self.settings.save();
        self.reload_themes();
    }

    pub fn set_theme(&mut self, theme: String) {
        self.settings.theme = theme;
        self.settings.save();
    }

    /// Save a custom theme and switch to it; returns its id.
    pub fn save_theme(
        &mut self,
        name: &str,
        base: &str,
        colors: BTreeMap<String, String>,
        background: Option<Background>,
    ) -> Result<String, String> {
        let id = themes::save(name, base, colors, background, &self.text)?;
        self.reload_themes();
        self.settings.theme = id.clone();
        self.settings.save();
        Ok(id)
    }

    pub fn save_settings(&self) {
        self.settings.save();
    }

    pub fn library(&self) -> LibraryView {
        let muted = self
            .with_player(|p| std::array::from_fn(|ch| p.muted(ch)))
            .unwrap_or_default();
        LibraryView {
            language: self.text.language().to_string(),
            languages: LANGUAGES
                .iter()
                .map(|&(code, name)| LanguageView { code, name })
                .collect(),
            text: self
                .text
                .table()
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            game: self.names.game.clone(),
            title: self.title.clone(),
            rom_path: self.rom_path.as_ref().map(|p| p.display().to_string()),
            songs: self
                .songs
                .iter()
                .map(|song| SongView {
                    index: song.index,
                    title: self.names.song(song.index).to_string(),
                    length: self.lengths.get(&song.index).map(|&l| l.into()),
                })
                .collect(),
            loop_count: self.settings.loop_count,
            auto_next: self.settings.auto_next,
            volume: self.settings.volume,
            muted,
            theme: if self.themes.iter().any(|t| t.id == self.settings.theme) {
                self.settings.theme.clone()
            } else {
                themes::BUILTIN[0].0.to_string()
            },
            themes: self.themes.clone(),
            error: {
                let errors: Vec<&str> = self
                    .error
                    .iter()
                    .chain(&self.theme_warnings)
                    .map(String::as_str)
                    .collect();
                (!errors.is_empty()).then(|| errors.join("\n"))
            },
        }
    }

    /// Advance the playlist and collect analysis results; returns the state for the UI and
    /// whether the library changed.
    pub fn tick(&mut self) -> (TickView, bool) {
        let results: Vec<_> = self
            .analyzer
            .as_ref()
            .map(|a| a.results.try_iter().collect())
            .unwrap_or_default();
        let changed = !results.is_empty();
        for (index, length) in results {
            self.lengths.insert(index, length);
            self.with_player(|p| p.set_length(index, length));
        }
        // Advance the playlist when a song has finished, if asked to.
        let finished = self
            .with_player(|p| (p.state() == PlayState::Finished).then(|| p.current()))
            .flatten()
            .flatten();
        if let Some(cur) = finished {
            if self.settings.auto_next {
                if let Some(next) = skip_target(Some(cur), 1, self.songs.len()) {
                    self.play(next);
                }
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
        for (a, b) in self.levels.iter_mut().zip(levels) {
            *a = (*a * 0.85).max(b);
        }
        let (state, failure) = match state {
            PlayState::Stopped => ("stopped", None),
            PlayState::Playing if paused => ("paused", None),
            PlayState::Playing => ("playing", None),
            PlayState::Finished => ("finished", None),
            PlayState::Failed(e) => (
                "failed",
                Some(self.text.fmt("err_playback_ui", &[("error", &e)])),
            ),
        };
        let view = TickView {
            current,
            state,
            failure,
            position: frames_to_secs(pos),
            length: length.into(),
            levels: self.levels,
            scope,
        };
        (view, changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_pauses_and_resumes_a_playing_song() {
        let t = toggle_action(Some(2), PlayState::Playing, false, 5);
        assert_eq!(t, Toggle::SetPaused(true));
        let t = toggle_action(Some(2), PlayState::Playing, true, 5);
        assert_eq!(t, Toggle::SetPaused(false));
    }

    #[test]
    fn toggle_restarts_a_stopped_or_finished_song() {
        let t = toggle_action(Some(2), PlayState::Stopped, false, 5);
        assert_eq!(t, Toggle::Play(2));
        let t = toggle_action(Some(2), PlayState::Finished, false, 5);
        assert_eq!(t, Toggle::Play(2));
    }

    #[test]
    fn toggle_starts_the_first_song_when_nothing_was_played() {
        let t = toggle_action(None, PlayState::Stopped, false, 5);
        assert_eq!(t, Toggle::Play(0));
        let t = toggle_action(None, PlayState::Stopped, false, 0);
        assert_eq!(t, Toggle::Nothing);
    }

    #[test]
    fn skip_wraps_around_the_list() {
        assert_eq!(skip_target(Some(4), 1, 5), Some(0));
        assert_eq!(skip_target(Some(0), -1, 5), Some(4));
        assert_eq!(skip_target(Some(1), 1, 5), Some(2));
        assert_eq!(skip_target(Some(1), -7, 5), Some(4));
    }

    #[test]
    fn skip_starts_at_the_first_song_and_needs_songs() {
        assert_eq!(skip_target(None, -1, 5), Some(0));
        assert_eq!(skip_target(None, 1, 0), None);
        assert_eq!(skip_target(Some(0), 1, 0), None);
    }
}
