//! Playback control behind the GUI commands: the loaded ROM, the playlists, the settings and
//! the analysis results, independent of Tauri.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::PoisonError;

use gb_music::songdb::{Names, LANGUAGES};
use gb_music::{frames_to_secs, read_rom, PlayState, Player, Song, SongLength, FRAME_RATE};

use super::playlists::{self, Entry, Playlist};
use super::views::{
    EntryView, LanguageView, LengthView, LibraryView, PlaylistView, QueueView, SongView, TickView,
};
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
    /// [`playlists::hash`] of the loaded ROM.
    rom_hash: Option<String>,
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
    playlists: Vec<Playlist>,
    /// Problems found in the playlist files.
    playlist_warnings: Vec<String>,
    queue: Option<Queue>,
}

/// The playlist being played.
#[derive(Debug, Clone, PartialEq)]
struct Queue {
    playlist: String,
    position: usize,
    /// False once the playing entry was removed: `position` is then the entry that took its
    /// place, which comes next.
    on_entry: bool,
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
            rom_hash: None,
            songs: Vec::new(),
            names: Names::default(),
            title: String::new(),
            lengths: HashMap::new(),
            analyzer: None,
            levels: [0.0; 4],
            themes: Vec::new(),
            theme_warnings: Vec::new(),
            playlists: Vec::new(),
            playlist_warnings: Vec::new(),
            queue: None,
        }
    }

    pub fn text(&self) -> &Text {
        &self.text
    }

    fn with_player<R>(&self, f: impl FnOnce(&mut Player) -> R) -> Option<R> {
        self.player
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_mut()
            .map(f)
    }

    /// Open a ROM picked by the user, leaving the playlist being played.
    pub fn open_rom(&mut self, path: &Path) {
        self.queue = None;
        self.load_rom(path);
    }

    /// Load a ROM; returns whether it could be.
    fn load_rom(&mut self, path: &Path) -> bool {
        let rom = match read_rom(path) {
            Ok(r) => r,
            Err(e) => {
                self.error = Some(
                    self.text
                        .fmt("err_read", &[("path", &path.display()), ("error", &e)]),
                );
                return false;
            }
        };
        let hash = playlists::hash(&rom);
        match Player::new(rom, self.sample_rate) {
            Ok(mut p) => {
                p.volume = self.settings.volume;
                self.lengths.clear();
                self.analyzer = Some(Analyzer::start(p.rom(), p.driver().clone()));
                self.rom_path = Some(path.to_path_buf());
                self.rom_hash = Some(hash);
                *self.player.lock().unwrap_or_else(PoisonError::into_inner) = Some(p);
                self.refresh_names();
                true
            }
            Err(e) => {
                self.error = Some(self.text.fmt("err_unsupported", &[("error", &e)]));
                false
            }
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

    /// Play a song of the loaded ROM's list. That leaves the playlist being played, unless it
    /// is the song the playlist is on (Previous starts it again this way).
    pub fn play_song(&mut self, index: usize) {
        let on_it = self
            .queue_entry()
            .is_some_and(|e| e.song == index && self.rom_hash.as_deref() == Some(e.hash.as_str()));
        if !on_it {
            self.queue = None;
        }
        self.play(index);
    }

    fn play(&mut self, index: usize) {
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

    /// Continue the current song from `secs` seconds in. The machine is run up to there
    /// without holding on to the player, so the audio keeps playing meanwhile.
    pub fn seek(&self, secs: f64) {
        let frame = (secs.max(0.0) * FRAME_RATE) as u32;
        let Some(mut seek) = self.with_player(|p| p.begin_seek(frame)).flatten() else {
            return;
        };
        seek.run();
        self.with_player(|p| p.finish_seek(seek));
    }

    /// Play the song `delta` places away from the current one, wrapping around the list (or
    /// the playlist being played).
    pub fn skip(&mut self, delta: i64) {
        if self.queue.is_some() {
            self.advance_queue(delta);
            return;
        }
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
            playlists: self
                .playlists
                .iter()
                .map(|l| self.playlist_view(l))
                .collect(),
            error: {
                let errors: Vec<&str> = self
                    .error
                    .iter()
                    .chain(&self.theme_warnings)
                    .chain(&self.playlist_warnings)
                    .map(String::as_str)
                    .collect();
                (!errors.is_empty()).then(|| errors.join("\n"))
            },
        }
    }

    pub fn reload_playlists(&mut self) {
        (self.playlists, self.playlist_warnings) = playlists::load(&self.text);
    }

    fn playlist(&self, id: &str) -> Option<&Playlist> {
        self.playlists.iter().find(|l| l.id == id)
    }

    /// Change the playlist `id` and save it.
    fn edit_playlist(&mut self, id: &str, f: impl FnOnce(&mut Playlist)) {
        let Some(list) = self.playlists.iter_mut().find(|l| l.id == id) else {
            return;
        };
        f(list);
        if let Err(e) = playlists::save(list, &self.text) {
            self.error = Some(e);
        }
    }

    fn sort_playlists(&mut self) {
        self.playlists
            .sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    }

    pub fn entry(&self, id: &str, position: usize) -> Option<Entry> {
        self.playlist(id)?.entries.get(position).cloned()
    }

    fn queue_entry(&self) -> Option<&Entry> {
        let q = self.queue.as_ref().filter(|q| q.on_entry)?;
        self.playlist(&q.playlist)?.entries.get(q.position)
    }

    /// Entries for songs of the loaded ROM.
    fn entries_for(&self, songs: &[usize]) -> Vec<Entry> {
        let (Some(path), Some(hash)) = (&self.rom_path, &self.rom_hash) else {
            return Vec::new();
        };
        songs
            .iter()
            .filter(|&&i| self.songs.iter().any(|s| s.index == i))
            .map(|&song| Entry {
                path: path.clone(),
                hash: hash.clone(),
                song,
                game: self.names.game.clone(),
                title: self.names.song(song).to_string(),
                length: self.lengths.get(&song).map(|&l| l.into()),
            })
            .collect()
    }

    /// Make a playlist holding `songs` of the loaded ROM; returns its id.
    pub fn create_playlist(&mut self, name: &str, songs: &[usize]) -> String {
        let list = Playlist {
            id: playlists::new_id(&self.playlists),
            name: name.trim().to_string(),
            entries: self.entries_for(songs),
        };
        let id = list.id.clone();
        if let Err(e) = playlists::save(&list, &self.text) {
            self.error = Some(e);
        }
        self.playlists.push(list);
        self.sort_playlists();
        id
    }

    pub fn rename_playlist(&mut self, id: &str, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        self.edit_playlist(id, |l| l.name = name.to_string());
        self.sort_playlists();
    }

    pub fn delete_playlist(&mut self, id: &str) {
        self.playlists.retain(|l| l.id != id);
        playlists::delete(id);
        if self.queue.as_ref().is_some_and(|q| q.playlist == id) {
            self.queue = None;
        }
    }

    /// Add songs of the loaded ROM to the end of a playlist.
    pub fn add_to_playlist(&mut self, id: &str, songs: &[usize]) {
        let entries = self.entries_for(songs);
        if !entries.is_empty() {
            self.edit_playlist(id, |l| l.entries.extend(entries));
        }
    }

    pub fn remove_from_playlist(&mut self, id: &str, positions: &[usize]) {
        if let Some(q) = self.queue.as_mut().filter(|q| q.playlist == id) {
            let before = positions.iter().filter(|&&p| p < q.position).count();
            if positions.contains(&q.position) {
                q.on_entry = false;
            }
            q.position -= before;
        }
        self.edit_playlist(id, |l| {
            let mut at = 0;
            l.entries.retain(|_| {
                at += 1;
                !positions.contains(&(at - 1))
            });
        });
    }

    /// Move the entry at `from` so that it ends up at `to`.
    pub fn move_in_playlist(&mut self, id: &str, from: usize, to: usize) {
        let Some(len) = self.playlist(id).map(|l| l.entries.len()) else {
            return;
        };
        if from >= len || to >= len || from == to {
            return;
        }
        if let Some(q) = self.queue.as_mut().filter(|q| q.playlist == id) {
            q.position = moved_position(q.position, from, to);
        }
        self.edit_playlist(id, |l| {
            let e = l.entries.remove(from);
            l.entries.insert(to, e);
        });
    }

    /// Point the entry at `position` of `id` at the file `path` holding the ROM `new_hash`,
    /// along with every entry, in any playlist, that cannot find the ROM `old_hash` either.
    pub fn relink(
        &mut self,
        id: &str,
        position: usize,
        old_hash: &str,
        path: &Path,
        new_hash: &str,
    ) {
        let mut changed = Vec::new();
        for list in &mut self.playlists {
            let mut any = false;
            for (i, e) in list.entries.iter_mut().enumerate() {
                let chosen = list.id == id && i == position;
                if chosen || (e.hash == old_hash && !e.path.is_file()) {
                    e.path = path.to_path_buf();
                    e.hash = new_hash.to_string();
                    any = true;
                }
            }
            if any {
                changed.push(list.id.clone());
            }
        }
        for id in changed {
            self.edit_playlist(&id, |_| {});
        }
    }

    /// Play the entry at `position` of the playlist `id`, loading its ROM when another is
    /// loaded; returns whether it plays.
    pub fn play_entry(&mut self, id: &str, position: usize) -> bool {
        let Some(entry) = self.entry(id, position) else {
            return false;
        };
        if !entry.path.is_file() {
            return false;
        }
        if self.rom_hash.as_deref() != Some(entry.hash.as_str()) && !self.load_rom(&entry.path) {
            return false;
        }
        if !self.songs.iter().any(|s| s.index == entry.song) {
            self.error = Some(self.text.fmt(
                "playlist_no_song",
                &[("n", &(entry.song + 1)), ("path", &entry.path.display())],
            ));
            return false;
        }
        self.queue = Some(Queue {
            playlist: id.to_string(),
            position,
            on_entry: true,
        });
        self.play(entry.song);
        true
    }

    /// Play the entry `delta` places away in the playlist being played, passing over the ones
    /// whose ROM is missing or cannot be played.
    fn advance_queue(&mut self, delta: i64) {
        let Some(q) = self.queue.clone() else { return };
        let Some(missing) = self.playlist(&q.playlist).map(|l| {
            l.entries
                .iter()
                .map(|e| !e.path.is_file())
                .collect::<Vec<_>>()
        }) else {
            self.queue = None;
            return;
        };
        let len = missing.len();
        let mut from = q.position;
        // After the playing entry was removed, the one now in its place comes next.
        let mut delta = if !q.on_entry && delta > 0 {
            delta - 1
        } else {
            delta
        };
        let mut failed = vec![false; len];
        for _ in 0..len {
            let Some(next) = playlists::step(Some(from), delta, len, |i| missing[i] || failed[i])
            else {
                break;
            };
            if self.play_entry(&q.playlist, next) {
                return;
            }
            failed[next] = true;
            from = next;
            delta = if delta < 0 { -1 } else { 1 };
        }
    }

    /// Keep the lengths found for the loaded ROM in the playlist entries of its songs.
    fn store_lengths(&mut self, results: &[(usize, SongLength)]) {
        let Some(hash) = self.rom_hash.clone() else {
            return;
        };
        let stale = |e: &Entry| {
            e.hash == hash
                && results
                    .iter()
                    .any(|&(i, len)| i == e.song && e.length != Some(len.into()))
        };
        let ids: Vec<String> = self
            .playlists
            .iter()
            .filter(|l| l.entries.iter().any(stale))
            .map(|l| l.id.clone())
            .collect();
        for id in ids {
            self.edit_playlist(&id, |l| {
                for e in l.entries.iter_mut().filter(|e| e.hash == hash) {
                    if let Some(&(_, len)) = results.iter().find(|&&(i, _)| i == e.song) {
                        e.length = Some(len.into());
                    }
                }
            });
        }
    }

    fn playlist_view(&self, list: &Playlist) -> PlaylistView {
        PlaylistView {
            id: list.id.clone(),
            name: list.name.clone(),
            entries: list
                .entries
                .iter()
                .map(|e| {
                    // The loaded ROM gives the names in the current language.
                    let loaded = self.rom_hash.as_deref() == Some(e.hash.as_str());
                    EntryView {
                        game: if loaded {
                            self.names.game.clone()
                        } else {
                            e.game.clone()
                        },
                        title: if loaded {
                            self.names.song(e.song).to_string()
                        } else {
                            e.title.clone()
                        },
                        length: e.length.clone(),
                        path: e.path.display().to_string(),
                        missing: !e.path.is_file(),
                    }
                })
                .collect(),
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
        let mut changed = !results.is_empty();
        for &(index, length) in &results {
            self.lengths.insert(index, length);
            self.with_player(|p| p.set_length(index, length));
        }
        self.store_lengths(&results);
        // Advance the playlist when a song has finished, if asked to.
        let finished = self
            .with_player(|p| (p.state() == PlayState::Finished).then(|| p.current()))
            .flatten()
            .flatten();
        if let Some(cur) = finished {
            if self.settings.auto_next && self.queue.is_some() {
                self.advance_queue(1);
                changed = true;
            } else if self.settings.auto_next {
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
            let samples = self.scope.lock().unwrap_or_else(PoisonError::into_inner);
            let skip = samples.len().saturating_sub(SCOPE_POINTS);
            samples.iter().skip(skip).copied().collect()
        };
        let Some((current, state, paused, pos, length, levels)) = info else {
            let view = TickView {
                current: None,
                queue: None,
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
            queue: self
                .queue
                .as_ref()
                .filter(|q| q.on_entry)
                .map(|q| QueueView {
                    playlist: q.playlist.clone(),
                    position: q.position,
                }),
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

/// Where the entry at `pos` ends up when the one at `from` moves to `to`.
fn moved_position(pos: usize, from: usize, to: usize) -> usize {
    if pos == from {
        to
    } else if from < pos && pos <= to {
        pos - 1
    } else if to <= pos && pos < from {
        pos + 1
    } else {
        pos
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_an_entry_shifts_the_ones_between() {
        assert_eq!(moved_position(2, 2, 5), 5);
        assert_eq!(moved_position(3, 2, 5), 2);
        assert_eq!(moved_position(5, 2, 5), 4);
        assert_eq!(moved_position(6, 2, 5), 6);
        assert_eq!(moved_position(2, 5, 2), 3);
        assert_eq!(moved_position(4, 5, 2), 5);
        assert_eq!(moved_position(1, 5, 2), 1);
    }

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
