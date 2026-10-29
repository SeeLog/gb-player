//! Playback control behind the GUI commands: the open ROMs, the playlists, the settings and
//! the analysis results, independent of Tauri.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::PoisonError;

use gb_music::songdb::{Names, LANGUAGES};
use gb_music::{
    frames_to_secs, is_audio, midi_note, read_rom, track, ChannelState, Driver, PlayState, Player,
    Profile, Song, SongLength, FRAME_RATE,
};

use super::playlists::{self, Entry, Playlist};
use super::shuffle::{Order, Rng};
use super::views::{
    EntryView, LanguageView, LengthView, LibraryView, NoteView, PlaylistView, QueueView, RomView,
    SongView, TickView,
};
use crate::analyzer::Analyzer;
use crate::audio::{Scope, SharedPlayer};
use crate::i18n::Text;
use crate::settings::{Mini, SavedEntry, SavedSong, Settings, Shuffle};
use crate::themes::{self, NewTheme, Theme};

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
    /// In the order they were opened.
    roms: Vec<Rom>,
    /// The id of the ROM whose player is in the audio output.
    loaded: Option<String>,
    /// The id of the ROM whose songs the UI shows (or showed before a playlist).
    shown: Option<String>,
    muted: [bool; 4],
    levels: [f32; 4],
    /// The channels' key-on counts in the last frame sent, to tell which frames key on.
    triggers: [u8; 4],
    themes: Vec<Theme>,
    /// Problems found in the custom theme files.
    theme_warnings: Vec<String>,
    playlists: Vec<Playlist>,
    /// Problems found in the playlist files.
    playlist_warnings: Vec<String>,
    queue: Option<Queue>,
    /// The order Next follows while shuffle is on, made when it is first needed.
    order: Option<(Pool, Order<Pick>)>,
    rng: Rng,
}

/// An open ROM, or a folder of audio files, opened and played the same way.
struct Rom {
    /// [`playlists::hash`] of the file (or [`folder_id`] of the folder), which the UI knows the
    /// ROM by.
    id: String,
    /// The ROM file, or the folder.
    path: PathBuf,
    /// Its player while another ROM's is in the audio output.
    player: Option<Player>,
    kind: Kind,
    songs: Vec<Song>,
    /// The cartridge header's title; for a folder, the artist of all its files, if they agree.
    title: String,
    /// Game and song names in the UI language.
    names: Names,
    /// Problems found in the user's song names for it, or in the files of a folder.
    warnings: Vec<String>,
    lengths: HashMap<usize, SongLength>,
}

enum Kind {
    /// A game's sound engine, whose loop points are found in the background.
    Game {
        driver: Box<Driver>,
        analyzer: Analyzer,
    },
    /// Audio files, in the order of the songs.
    Folder { files: Vec<PathBuf> },
}

impl Rom {
    fn has_song(&self, index: usize) -> bool {
        self.songs.iter().any(|s| s.index == index)
    }

    /// The song that a playlist entry of this ROM stands for: a folder's by its file, so that
    /// files added to the folder later do not move it.
    fn entry_song(&self, entry: &Entry) -> Option<usize> {
        match &self.kind {
            Kind::Game { .. } => self.has_song(entry.song).then_some(entry.song),
            Kind::Folder { files } => files.iter().position(|f| *f == entry.path),
        }
    }
}

/// What a folder of audio files is known by: its path, as the files in it may change.
pub fn folder_id(dir: &Path) -> String {
    let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    format!(
        "folder:{}",
        playlists::hash(dir.as_os_str().as_encoded_bytes())
    )
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

/// What a shuffled order is made of.
#[derive(Debug, Clone, PartialEq)]
enum Pool {
    Rom(String),
    AllRoms,
    Playlist(String),
}

/// A song in a shuffled order.
#[derive(Debug, Clone, PartialEq)]
enum Pick {
    Song {
        rom: String,
        index: usize,
    },
    /// A position in the playlist being played.
    Entry(usize),
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
        mut settings: Settings,
        sample_rate: u32,
        error: Option<String>,
    ) -> Session {
        // A built-in theme chosen before it was renamed.
        settings.theme = themes::current_id(&settings.theme).to_string();
        Session {
            player,
            scope,
            text,
            settings,
            sample_rate,
            error,
            roms: Vec::new(),
            loaded: None,
            shown: None,
            muted: [false; 4],
            levels: [0.0; 4],
            triggers: [0; 4],
            themes: Vec::new(),
            theme_warnings: Vec::new(),
            playlists: Vec::new(),
            playlist_warnings: Vec::new(),
            queue: None,
            order: None,
            rng: Rng::from_clock(),
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

    fn rom(&self, id: &str) -> Option<&Rom> {
        self.roms.iter().find(|r| r.id == id)
    }

    fn loaded_rom(&self) -> Option<&Rom> {
        self.rom(self.loaded.as_deref()?)
    }

    /// The ROM the UI shows, or else the first one.
    fn shown_rom(&self) -> Option<&Rom> {
        self.shown
            .as_deref()
            .and_then(|id| self.rom(id))
            .or(self.roms.first())
    }

    /// Open ROMs picked by the user and show the last one that could be; returns its id. A ROM
    /// that is open already is not opened twice.
    pub fn open_roms(&mut self, paths: &[PathBuf]) -> Option<String> {
        let mut last = None;
        for path in paths {
            if let Some(id) = self.open(path) {
                last = Some(id);
            }
        }
        if last.is_some() {
            self.shown = last.clone();
            self.remember();
        }
        last
    }

    /// Open a ROM, or find it among the open ones; returns its id. A folder or an audio file
    /// opens the folder. The first one opened goes into the audio output, so that
    /// Play has something to play.
    fn open(&mut self, path: &Path) -> Option<String> {
        if path.is_dir() || is_audio(path) {
            return self.open_folder(path);
        }
        let rom = match read_rom(path) {
            Ok(r) => r,
            Err(e) => {
                self.error = Some(
                    self.text
                        .fmt("err_read", &[("path", &path.display()), ("error", &e)]),
                );
                return None;
            }
        };
        let id = playlists::hash(&rom);
        if self.rom(&id).is_some() {
            return Some(id);
        }
        let player = match Player::new(rom, self.sample_rate) {
            Ok(p) => p,
            Err(e) => {
                self.error = Some(self.text.fmt("err_unsupported", &[("error", &e)]));
                return None;
            }
        };
        let (Some(driver), Some(image)) = (player.driver().cloned(), player.rom()) else {
            return None;
        };
        let (names, warnings) = Names::new(&driver, self.text.language());
        self.add(Rom {
            id: id.clone(),
            path: path.to_path_buf(),
            songs: player.songs().to_vec(),
            title: player.title().unwrap_or_default().to_string(),
            player: Some(player),
            kind: Kind::Game {
                analyzer: Analyzer::start(image, driver.clone()),
                driver: Box::new(driver),
            },
            names,
            warnings,
            lengths: HashMap::new(),
        });
        Some(id)
    }

    /// Open the folder `path`, or the one holding the audio file `path`, or find it among the
    /// open ones; returns its id.
    fn open_folder(&mut self, path: &Path) -> Option<String> {
        let dir = if path.is_dir() { path } else { path.parent()? };
        let id = folder_id(dir);
        if self.rom(&id).is_some() {
            return Some(id);
        }
        let track::Folder { tracks, unreadable } = match track::folder(dir) {
            Ok(f) => f,
            Err(e) => {
                self.error = Some(
                    self.text
                        .fmt("err_read", &[("path", &dir.display()), ("error", &e)]),
                );
                return None;
            }
        };
        let warnings: Vec<String> = unreadable
            .iter()
            .map(|(path, e)| {
                self.text
                    .fmt("err_read", &[("path", &path.display()), ("error", &e)])
            })
            .collect();
        if tracks.is_empty() {
            self.error = Some(
                warnings
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| self.text.fmt("err_no_audio", &[("path", &dir.display())])),
            );
            return None;
        }
        let folder = dir.file_name().map_or_else(
            || dir.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let artist = tracks[0]
            .artist
            .clone()
            .filter(|a| tracks.iter().all(|t| t.artist.as_ref() == Some(a)));
        let names = Names {
            game: folder,
            songs: tracks.iter().map(|t| t.title.clone()).collect(),
        };
        let lengths = tracks.iter().map(|t| t.length).enumerate().collect();
        let files = tracks.iter().map(|t| t.path.clone()).collect();
        let player = Player::files(tracks, self.sample_rate);
        self.add(Rom {
            id: id.clone(),
            path: dir.to_path_buf(),
            songs: player.songs().to_vec(),
            title: artist.unwrap_or_default(),
            player: Some(player),
            kind: Kind::Folder { files },
            names,
            warnings,
            lengths,
        });
        Some(id)
    }

    /// Keep a ROM just opened; the first one goes into the audio output.
    fn add(&mut self, rom: Rom) {
        let id = rom.id.clone();
        self.roms.push(rom);
        self.error = None;
        self.forget_order();
        if self.loaded.is_none() {
            self.load(&id);
        }
    }

    /// Close an open ROM. When it is the one playing, playback stops (along with the playlist
    /// being played) and the shown ROM takes its place in the audio output.
    pub fn close_rom(&mut self, id: &str) {
        let Some(i) = self.roms.iter().position(|r| r.id == id) else {
            return;
        };
        if self.loaded.as_deref() == Some(id) {
            *self.player.lock().unwrap_or_else(PoisonError::into_inner) = None;
            self.loaded = None;
            self.queue = None;
        }
        self.roms.remove(i);
        if self.shown.as_deref() == Some(id) {
            let next = self.roms.get(i).or(self.roms.last());
            self.shown = next.map(|r| r.id.clone());
        }
        if self.loaded.is_none() {
            if let Some(next) = self.shown_rom().map(|r| r.id.clone()) {
                self.load(&next);
            }
        }
        self.forget_order();
        self.remember();
    }

    /// Put the player of the open ROM `id` in the audio output, stopping the one there and
    /// keeping it with its ROM; returns whether the ROM is open.
    fn load(&mut self, id: &str) -> bool {
        if self.loaded.as_deref() == Some(id) {
            return true;
        }
        let Some(mut next) = self
            .roms
            .iter_mut()
            .find(|r| r.id == id)
            .and_then(|r| r.player.take())
        else {
            return false;
        };
        next.volume = self.settings.volume;
        next.set_profile(self.settings.profile, self.settings.speaker_noise);
        for (ch, &m) in self.muted.iter().enumerate() {
            next.set_muted(ch, m);
            next.set_channel_volume(ch, self.settings.channel_volumes[ch]);
        }
        let old = self
            .player
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(next);
        if let (Some(mut old), Some(was)) = (old, self.loaded.take()) {
            old.stop();
            if let Some(r) = self.roms.iter_mut().find(|r| r.id == was) {
                r.player = Some(old);
            }
        }
        self.loaded = Some(id.to_string());
        true
    }

    /// Look up the game and song names in the UI language after switching languages.
    fn refresh_names(&mut self) {
        for rom in &mut self.roms {
            if let Kind::Game { driver, .. } = &rom.kind {
                (rom.names, rom.warnings) = Names::new(driver, self.text.language());
            }
        }
    }

    pub fn reload_themes(&mut self) {
        (self.themes, self.theme_warnings) = themes::load(&self.text);
    }

    /// Play a song of an open ROM. That leaves the playlist being played, unless it is the song
    /// the playlist is on (Previous starts it again this way).
    pub fn play_song(&mut self, rom: &str, index: usize) {
        let on_it = self.queue_entry().is_some_and(|e| {
            e.hash == rom && self.rom(rom).and_then(|r| r.entry_song(e)) == Some(index)
        });
        if !on_it {
            self.queue = None;
        }
        if self.load(rom) {
            self.play(index);
        }
    }

    /// Play a song of the loaded ROM.
    fn play(&mut self, index: usize) {
        self.start(index, Player::play);
    }

    /// Select a song of the loaded ROM without playing it.
    fn cue(&mut self, index: usize) {
        self.start(index, Player::cue);
    }

    fn start(&mut self, index: usize, start: fn(&mut Player, usize)) {
        let Some(rom) = self.loaded_rom() else {
            return;
        };
        if let Kind::Game { analyzer, .. } = &rom.kind {
            analyzer.prioritize(index);
        }
        let length = rom.lengths.get(&index).copied();
        let count = self.settings.loop_count;
        self.with_player(|p| {
            start(p, index);
            p.loop_count = count;
            if let Some(l) = length {
                p.set_length(index, l);
            }
        });
        self.remember();
    }

    /// Open the ROMs of the last run, and the one given on the command line (shown).
    pub fn open_at_start(&mut self, rom: Option<&Path>) {
        self.restore();
        if let Some(path) = rom {
            self.settings.open_playlist = None;
            self.open_roms(&[path.to_path_buf()]);
        }
    }

    /// Open the ROMs of the last run and select the song that was playing, stopped, along with
    /// the playlist entry it was played from. ROMs that are gone or cannot be loaded any more
    /// are forgotten without a word.
    fn restore(&mut self) {
        let mut paths = std::mem::take(&mut self.settings.open_roms);
        // Settings from before several ROMs could be open name only the one.
        if paths.is_empty() {
            paths.extend(self.settings.last_rom.clone());
        }
        let last = self.settings.last_rom.take();
        let shown = self.settings.shown_rom.take();
        let song = self.settings.last_song.take();
        let entry = self.settings.last_entry.take();
        let error = self.error.take();
        let mut opened = Vec::new();
        for path in paths {
            if let Some(id) = self.open(&path) {
                opened.push((path, id));
            }
        }
        self.error = error;
        let id_of = |p: &Option<PathBuf>| {
            opened
                .iter()
                .find(|(path, _)| Some(path) == p.as_ref())
                .map(|(_, id)| id.clone())
        };
        self.shown = id_of(&shown).or_else(|| id_of(&last));
        let last = id_of(&last);
        let index = last.as_ref().and_then(|id| {
            self.load(id);
            song.and_then(|s| s.find(&self.rom(id)?.songs))
        });
        let (Some(last), Some(index)) = (last, index) else {
            self.remember();
            return;
        };
        self.queue = entry
            .filter(|e| {
                self.entry(&e.playlist, e.position).is_some_and(|x| {
                    x.hash == last && self.rom(&last).and_then(|r| r.entry_song(&x)) == Some(index)
                })
            })
            .map(|e| Queue {
                playlist: e.playlist,
                position: e.position,
                on_entry: true,
            });
        self.cue(index);
    }

    /// Keep the open ROMs, the selected song and the playlist entry it came from for the next
    /// run.
    fn remember(&mut self) {
        let current = self.with_player(|p| p.current()).flatten();
        self.settings.open_roms = self.roms.iter().map(|r| r.path.clone()).collect();
        self.settings.shown_rom = self.shown_rom().map(|r| r.path.clone());
        let loaded = self.loaded_rom();
        let last_rom = loaded.map(|r| r.path.clone());
        let last_song = current
            .and_then(|i| loaded?.songs.iter().find(|s| s.index == i))
            .map(SavedSong::new);
        (self.settings.last_rom, self.settings.last_song) = (last_rom, last_song);
        self.settings.last_entry = self
            .queue
            .as_ref()
            .filter(|q| q.on_entry)
            .map(|q| SavedEntry {
                playlist: q.playlist.clone(),
                position: q.position,
            });
        self.settings.save();
    }

    /// Remember what the middle column shows: the songs of the ROM `rom`, or the playlist
    /// `playlist` in their place (`rom` is then the ROM shown before it).
    pub fn set_view(&mut self, rom: Option<String>, playlist: Option<String>) {
        let rom = rom.filter(|id| self.rom(id).is_some());
        if (rom.is_none() || rom == self.shown) && self.settings.open_playlist == playlist {
            return;
        }
        if rom.is_some() {
            self.shown = rom;
        }
        self.settings.open_playlist = playlist;
        self.remember();
    }

    /// Play or pause; starts the current song again when nothing is playing, or the first song
    /// of the shown ROM when nothing was played yet.
    pub fn toggle(&mut self) {
        let (current, state, paused) = self
            .with_player(|p| (p.current(), p.state(), p.paused()))
            .unwrap_or((None, PlayState::Stopped, false));
        let shown = self.shown_rom().map(|r| (r.id.clone(), r.songs.len()));
        let count = shown.as_ref().map_or(0, |s| s.1);
        match toggle_action(current, state, paused, count) {
            Toggle::SetPaused(paused) => {
                self.with_player(|p| p.set_paused(paused));
            }
            Toggle::Play(index) if current.is_some() => self.play(index),
            Toggle::Play(index) => {
                if let Some((id, _)) = shown {
                    self.play_song(&id, index);
                }
            }
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
    /// the playlist being played), or the next or previous one of the shuffled order.
    pub fn skip(&mut self, delta: i64) {
        if self.settings.shuffle != Shuffle::Off {
            self.skip_shuffled(delta);
            return;
        }
        if self.queue.is_some() {
            self.advance_queue(delta);
            return;
        }
        let current = self.with_player(|p| p.current()).flatten();
        let Some(rom) = self.loaded_rom().or(self.shown_rom()) else {
            return;
        };
        let (id, count) = (rom.id.clone(), rom.songs.len());
        if let Some(next) = skip_target(current, delta, count) {
            self.play_song(&id, next);
        }
    }

    /// What a shuffled order is made of now: the playlist being played, or else the ROM
    /// playing (or all open ones).
    fn pool(&self) -> Option<Pool> {
        if let Some(q) = &self.queue {
            return Some(Pool::Playlist(q.playlist.clone()));
        }
        if self.settings.shuffle == Shuffle::All {
            return Some(Pool::AllRoms);
        }
        let rom = self.loaded_rom().or(self.shown_rom())?;
        Some(Pool::Rom(rom.id.clone()))
    }

    fn picks(&self, pool: &Pool) -> Vec<Pick> {
        let songs = |r: &Rom| {
            r.songs
                .iter()
                .map(|s| Pick::Song {
                    rom: r.id.clone(),
                    index: s.index,
                })
                .collect::<Vec<_>>()
        };
        match pool {
            Pool::Rom(id) => self.rom(id).map(songs).unwrap_or_default(),
            Pool::AllRoms => self.roms.iter().flat_map(songs).collect(),
            Pool::Playlist(id) => {
                let len = self.playlist(id).map_or(0, |l| l.entries.len());
                (0..len).map(Pick::Entry).collect()
            }
        }
    }

    /// The song playing, as it appears in a shuffled order.
    fn current_pick(&self) -> Option<Pick> {
        if let Some(q) = &self.queue {
            return q.on_entry.then_some(Pick::Entry(q.position));
        }
        let index = self.with_player(|p| p.current()).flatten()?;
        Some(Pick::Song {
            rom: self.loaded.clone()?,
            index,
        })
    }

    /// Make the shuffled order again from the song playing when it is next needed: the songs
    /// it was made of changed.
    fn forget_order(&mut self) {
        self.order = None;
    }

    /// Play the next (or previous) song of the shuffled order. The order is made first when
    /// there is none for what is being played, or the song playing is not where the order is
    /// (the user picked another). Songs that cannot be played are passed over; going back
    /// past the start of the order starts the song playing again.
    fn skip_shuffled(&mut self, delta: i64) {
        let Some(pool) = self.pool() else { return };
        let current = self.current_pick();
        let fits = self.order.as_ref().is_some_and(|(p, order)| {
            *p == pool && current.is_some() && order.current() == current.as_ref()
        });
        let mut order = match self.order.take() {
            Some((_, order)) if fits => order,
            _ => Order::new(self.picks(&pool), current.as_ref(), &mut self.rng),
        };
        let mut tries = order.len();
        while tries > 0 {
            tries -= 1;
            let played = match order.step(delta, &mut self.rng) {
                // Before the start: the song playing from the top.
                None => {
                    tries = 0;
                    match &current {
                        Some(pick) => self.play_pick(&pool, pick),
                        None => false,
                    }
                }
                Some(pick) => self.play_pick(&pool, &pick),
            };
            if played {
                break;
            }
        }
        // Set after playing: opening the ROM of a playlist entry forgets the order.
        self.order = Some((pool, order));
    }

    /// Play a song of a shuffled order; returns whether it plays.
    fn play_pick(&mut self, pool: &Pool, pick: &Pick) -> bool {
        match (pool, pick) {
            (Pool::Playlist(id), &Pick::Entry(position)) => {
                let id = id.clone();
                self.play_entry(&id, position)
            }
            (_, Pick::Song { rom, index }) => {
                if !self.rom(rom).is_some_and(|r| r.has_song(*index)) {
                    return false;
                }
                let rom = rom.clone();
                self.play_song(&rom, *index);
                true
            }
            _ => false,
        }
    }

    pub fn set_loop_count(&mut self, count: Option<u32>) {
        self.settings.loop_count = count;
        self.with_player(|p| p.loop_count = count);
    }

    pub fn set_auto_next(&mut self, auto_next: bool) {
        self.settings.auto_next = auto_next;
    }

    pub fn set_shuffle(&mut self, shuffle: Shuffle) {
        self.settings.shuffle = shuffle;
        self.forget_order();
        self.settings.save();
    }

    pub fn set_volume(&mut self, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        self.settings.volume = volume;
        self.with_player(|p| p.volume = volume);
    }

    pub fn set_profile(&mut self, profile: Profile, noise: bool) {
        self.settings.profile = profile;
        self.settings.speaker_noise = noise;
        self.with_player(|p| p.set_profile(profile, noise));
        self.settings.save();
    }

    pub fn set_channel_volume(&mut self, channel: usize, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        self.settings.channel_volumes[channel] = volume;
        self.with_player(|p| p.set_channel_volume(channel, volume));
    }

    pub fn set_muted(&mut self, channel: usize, muted: bool) {
        self.muted[channel] = muted;
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
    pub fn save_theme(&mut self, theme: NewTheme) -> Result<String, String> {
        let id = themes::save(theme, &self.text)?;
        self.reload_themes();
        self.settings.theme = id.clone();
        self.settings.save();
        Ok(id)
    }

    pub fn save_settings(&mut self) {
        self.remember();
    }

    /// The mini player settings, for [`super::mini`] to change; saved with the rest.
    pub fn mini(&mut self) -> &mut Mini {
        &mut self.settings.mini
    }

    pub fn library(&self) -> LibraryView {
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
            roms: self
                .roms
                .iter()
                .map(|rom| RomView {
                    id: rom.id.clone(),
                    folder: matches!(rom.kind, Kind::Folder { .. }),
                    game: rom.names.game.clone(),
                    title: rom.title.clone(),
                    path: rom.path.display().to_string(),
                    songs: rom
                        .songs
                        .iter()
                        .map(|song| SongView {
                            index: song.index,
                            title: rom.names.song(song.index).to_string(),
                            length: rom.lengths.get(&song.index).map(|&l| l.into()),
                        })
                        .collect(),
                })
                .collect(),
            shown_rom: self.shown_rom().map(|r| r.id.clone()),
            loop_count: self.settings.loop_count,
            auto_next: self.settings.auto_next,
            shuffle: self.settings.shuffle,
            volume: self.settings.volume,
            profile: self.settings.profile,
            speaker_noise: self.settings.speaker_noise,
            channel_volumes: self.settings.channel_volumes,
            muted: self.muted,
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
            open_playlist: self
                .settings
                .open_playlist
                .clone()
                .filter(|id| self.playlist(id).is_some()),
            mini: self.settings.mini.on,
            mini_on_top: self.settings.mini.on_top,
            mini_notes: self.settings.mini.notes,
            error: {
                let errors: Vec<&str> = self
                    .error
                    .iter()
                    .chain(self.roms.iter().flat_map(|r| &r.warnings))
                    .chain(&self.theme_warnings)
                    .chain(&self.playlist_warnings)
                    .map(String::as_str)
                    .collect();
                (!errors.is_empty()).then(|| errors.join("\n"))
            },
        }
    }

    /// Clear the errors and warnings shown above the library (the user closed them). Warnings
    /// come back when what they are about is loaded again.
    pub fn dismiss_errors(&mut self) {
        self.error = None;
        for rom in &mut self.roms {
            rom.warnings.clear();
        }
        self.theme_warnings.clear();
        self.playlist_warnings.clear();
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

    /// Entries for songs of the open ROM `rom`. A folder's entry is its song's file.
    fn entries_for(&self, rom: &str, songs: &[usize]) -> Vec<Entry> {
        let Some(rom) = self.rom(rom) else {
            return Vec::new();
        };
        songs
            .iter()
            .filter(|&&i| rom.has_song(i))
            .map(|&song| Entry {
                path: match &rom.kind {
                    Kind::Game { .. } => rom.path.clone(),
                    Kind::Folder { files } => files[song].clone(),
                },
                hash: rom.id.clone(),
                song,
                game: rom.names.game.clone(),
                title: rom.names.song(song).to_string(),
                length: rom.lengths.get(&song).map(|&l| l.into()),
            })
            .collect()
    }

    /// Make a playlist holding `songs` of the open ROM `rom`; returns its id.
    pub fn create_playlist(&mut self, name: &str, rom: &str, songs: &[usize]) -> String {
        let list = Playlist {
            id: playlists::new_id(&self.playlists),
            name: name.trim().to_string(),
            entries: self.entries_for(rom, songs),
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

    /// Add songs of the open ROM `rom` to the end of a playlist.
    pub fn add_to_playlist(&mut self, id: &str, rom: &str, songs: &[usize]) {
        let entries = self.entries_for(rom, songs);
        if !entries.is_empty() {
            self.edit_playlist(id, |l| l.entries.extend(entries));
            self.forget_playlist_order(id);
        }
    }

    /// Forget the shuffled order of the playlist `id` when it is the one being played: its
    /// entries moved.
    fn forget_playlist_order(&mut self, id: &str) {
        if self.queue.as_ref().is_some_and(|q| q.playlist == id) {
            self.forget_order();
        }
    }

    pub fn remove_from_playlist(&mut self, id: &str, positions: &[usize]) {
        self.forget_playlist_order(id);
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
        self.forget_playlist_order(id);
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
    /// For an audio file, those are the songs of the same folder, each found by its file name
    /// in the folder of `path`.
    pub fn relink(
        &mut self,
        id: &str,
        position: usize,
        old_hash: &str,
        path: &Path,
        new_hash: &str,
    ) {
        let audio = is_audio(path);
        let mut changed = Vec::new();
        for list in &mut self.playlists {
            let mut any = false;
            for (i, e) in list.entries.iter_mut().enumerate() {
                let to = if list.id == id && i == position {
                    Some(path.to_path_buf())
                } else if e.hash == old_hash && !e.path.is_file() {
                    match (audio, path.parent(), e.path.file_name()) {
                        (false, _, _) => Some(path.to_path_buf()),
                        (true, Some(dir), Some(name)) => {
                            Some(dir.join(name)).filter(|p| p.is_file())
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(to) = to {
                    e.path = to;
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

    /// Play the entry at `position` of the playlist `id`, opening its ROM when it is not open;
    /// returns whether it plays.
    pub fn play_entry(&mut self, id: &str, position: usize) -> bool {
        let Some(entry) = self.entry(id, position) else {
            return false;
        };
        if !entry.path.is_file() {
            return false;
        }
        let rom = match self.rom(&entry.hash) {
            Some(r) => r.id.clone(),
            None => match self.open(&entry.path) {
                Some(id) => id,
                None => return false,
            },
        };
        let Some(song) = self.rom(&rom).and_then(|r| r.entry_song(&entry)) else {
            self.error = Some(self.text.fmt(
                "playlist_no_song",
                &[("n", &(entry.song + 1)), ("path", &entry.path.display())],
            ));
            return false;
        };
        self.queue = Some(Queue {
            playlist: id.to_string(),
            position,
            on_entry: true,
        });
        self.load(&rom);
        self.play(song);
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

    /// Keep the lengths found for the ROM `hash` in the playlist entries of its songs.
    fn store_lengths(&mut self, hash: &str, results: &[(usize, SongLength)]) {
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
                    // An open ROM gives the names in the current language.
                    let rom = self.rom(&e.hash);
                    let song = rom.and_then(|r| r.entry_song(e));
                    EntryView {
                        game: rom.map_or_else(|| e.game.clone(), |r| r.names.game.clone()),
                        title: match (rom, song) {
                            (Some(r), Some(i)) => r.names.song(i).to_string(),
                            _ => e.title.clone(),
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
        let mut changed = false;
        for i in 0..self.roms.len() {
            let Kind::Game { analyzer, .. } = &self.roms[i].kind else {
                continue;
            };
            let results: Vec<_> = analyzer.results.try_iter().collect();
            if results.is_empty() {
                continue;
            }
            changed = true;
            let loaded = self.loaded.as_deref() == Some(self.roms[i].id.as_str());
            for &(index, length) in &results {
                self.roms[i].lengths.insert(index, length);
                if loaded {
                    self.with_player(|p| p.set_length(index, length));
                } else if let Some(p) = &mut self.roms[i].player {
                    p.set_length(index, length);
                }
            }
            let id = self.roms[i].id.clone();
            self.store_lengths(&id, &results);
        }
        // Go on to the next song when one has finished, if asked to. That may open the ROM of
        // a playlist entry.
        let finished = self
            .with_player(|p| p.state() == PlayState::Finished)
            .unwrap_or(false);
        if finished && self.settings.auto_next {
            self.skip(1);
            changed = true;
        }
        let info = self.with_player(|p| {
            (
                p.current(),
                p.state(),
                p.paused(),
                p.position_frames(),
                p.length(),
                p.take_levels(),
                p.take_notes(),
            )
        });
        let scope = {
            let samples = self.scope.lock().unwrap_or_else(PoisonError::into_inner);
            let skip = samples.len().saturating_sub(SCOPE_POINTS);
            samples.iter().skip(skip).copied().collect()
        };
        let Some((current, state, paused, pos, length, levels, notes)) = info else {
            let view = TickView {
                rom: None,
                current: None,
                queue: None,
                state: "none",
                failure: None,
                position: 0.0,
                length: LengthView::Unknown,
                levels: [0.0; 4],
                scope,
                notes: Vec::new(),
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
            rom: self.loaded.clone(),
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
            notes: notes.iter().map(|f| self.note_frame(f)).collect(),
        };
        (view, changed)
    }

    /// The notes of a frame for the UI, telling which channels were keyed on since the last.
    fn note_frame(&mut self, states: &[ChannelState; 4]) -> [NoteView; 4] {
        std::array::from_fn(|ch| {
            let s = states[ch];
            let keyed = s.triggers != self.triggers[ch];
            self.triggers[ch] = s.triggers;
            let note = midi_note(s.hz(ch)).clamp(0, 255) as u8;
            (note, if s.sounding { s.volume } else { 0 }, keyed)
        })
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
