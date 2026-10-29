//! Real-time song playback, from a [`Driver`] on the emulated machine or from audio files.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use gb_core::{CallError, CartridgeError, ChannelState, Machine, CYCLES_PER_FRAME};

use crate::driver::{Driver, Song};
use crate::effects::{Chain, Profile};
use crate::gbs::{self, is_gbs, GbsError};
use crate::length::{frames_to_secs, SongLength, FRAME_RATE};
use crate::track::{Loop, Track, TrackDecoder, TrackError};

#[derive(Debug)]
pub enum LoadError {
    Cartridge(CartridgeError),
    Gbs(GbsError),
    UnsupportedGame { title: String },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Cartridge(e) => write!(f, "{e}"),
            LoadError::Gbs(e) => write!(f, "{e}"),
            LoadError::UnsupportedGame { title } => {
                write!(f, "no supported sound engine found in \"{title}\"")
            }
        }
    }
}

impl std::error::Error for LoadError {}

impl From<CartridgeError> for LoadError {
    fn from(e: CartridgeError) -> Self {
        LoadError::Cartridge(e)
    }
}

/// Frames between the snapshots kept for seeking, about five seconds of music.
const SNAPSHOT_INTERVAL: u32 = 300;

/// Most frames of channel states kept between calls to [`Player::take_notes`].
const MAX_NOTE_FRAMES: usize = 256;

/// How many frames ahead of the music the notes come, about 16 seconds: a copy of the machine
/// runs that far ahead, so that a piano roll can show the notes before they sound, across a wide
/// screen at a steady speed.
pub const NOTES_AHEAD: u32 = 960;

/// How many frames more the copy runs on each frame until it is [`NOTES_AHEAD`] ahead, after
/// the song starts, a seek or a change of tempo: the notes nearest come first.
const NOTES_CATCH_UP: u32 = 8;

/// Most snapshots kept for one song (about 170 KiB each), enough for 20 minutes of music.
const MAX_SNAPSHOTS: usize = 240;

/// The fade-out after the last loop that a new player starts with.
pub const DEFAULT_FADE_SECS: f32 = 8.0;

/// The slowest and fastest tempo, in percent of the game's own.
pub const TEMPO_RANGE: std::ops::RangeInclusive<u32> = 50..=200;

/// One frame of music in hundredths of a T-cycle, the unit that a tempo in percent advances
/// the music by exactly.
const FRAME_SUB: u64 = CYCLES_PER_FRAME as u64 * 100;

/// The machine as it was at the start of a frame, to seek back to.
struct Snapshot {
    frame: u32,
    machine: Machine,
    phase: u32,
}

/// A move of the playing song to another frame, made by [`Player::begin_seek`].
pub struct Seek {
    /// The [`Player::play`] call this seek belongs to.
    play: u64,
    frame: u32,
    work: SeekWork,
}

enum SeekWork {
    Game {
        /// The first-pass frame that sounds like the frame sought.
        target: u32,
        at: u32,
        machine: Box<Machine>,
        phase: u32,
        driver: Arc<Driver>,
        /// Snapshots taken on the way, for the player.
        snapshots: Vec<Snapshot>,
        error: Option<CallError>,
    },
    File {
        path: PathBuf,
        looping: Option<Loop>,
        sample_rate: u32,
        /// Where in the file the frame sought is: past the first loop, the same point of the
        /// loop.
        secs: f64,
        /// The file opened and moved there, once [`Seek::run`] has.
        decoder: Option<Box<Result<TrackDecoder, TrackError>>>,
    },
}

impl Seek {
    /// Run the machine silently up to the frame sought, or open the file there.
    pub fn run(&mut self) {
        match &mut self.work {
            SeekWork::Game {
                target,
                at,
                machine,
                phase,
                driver,
                snapshots,
                error,
            } => {
                machine.mem.apu.set_output_enabled(false);
                while *at < *target {
                    if let Err(e) = driver.run_frame(machine, phase) {
                        *error = Some(e);
                        return;
                    }
                    *at += 1;
                    if at.is_multiple_of(SNAPSHOT_INTERVAL) {
                        snapshots.push(Snapshot {
                            frame: *at,
                            machine: (**machine).clone(),
                            phase: *phase,
                        });
                    }
                }
            }
            SeekWork::File {
                path,
                looping,
                sample_rate,
                secs,
                decoder,
            } => {
                *decoder = Some(Box::new(TrackDecoder::open(path, *sample_rate).and_then(
                    |mut d| {
                        d.set_loop(*looping);
                        d.seek(*secs)?;
                        Ok(d)
                    },
                )));
            }
        }
    }
}

/// Why a song stopped playing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The game's sound engine went wrong.
    Call(CallError),
    /// The audio file could not be read.
    File(TrackError),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Call(e) => write!(f, "{e}"),
            Failure::File(e) => write!(f, "{e}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayState {
    Stopped,
    Playing,
    /// The song finished (after the configured number of loops, or by itself).
    Finished,
    Failed(Failure),
}

/// Where the songs come from.
enum Source {
    /// A game's sound engine on the emulated machine.
    Game(Box<Game>),
    /// Audio files, a song each.
    Files(Box<Files>),
}

struct Game {
    rom: Arc<[u8]>,
    machine: Machine,
    driver: Arc<Driver>,
    /// T-cycles since the last update, for timer-driven engines.
    phase: u32,
    /// Snapshots of the current song, at most one per frame of its first pass (see
    /// [`pass_frame`]).
    snapshots: Vec<Snapshot>,
    /// Whether the machine is as it would be with the song played at its own tempo from the
    /// start: only then is it a snapshot of its frame, or a place for a seek to run on from.
    exact: bool,
    /// The copy of the machine the notes come from; `None` when it failed, until the next start.
    ahead: Option<Box<Ahead>>,
}

/// A copy of a game's machine run ahead of the music, without making any sound. The sound
/// engine does the same from the same state, so what its channels play is what the music will
/// play `lead` frames later.
struct Ahead {
    machine: Machine,
    phase: u32,
    /// As [`Player::sub`], for the copy.
    sub: u64,
    /// How many frames it is ahead of the music.
    lead: u32,
}

struct Files {
    tracks: Vec<Track>,
    songs: Vec<Song>,
    /// The current song's file, open at where it plays.
    decoder: Option<TrackDecoder>,
}

pub struct Player {
    source: Source,
    sample_rate: u32,
    state: PlayState,
    current: Option<usize>,
    /// Counts [`Player::play`] calls, so that a seek can tell it is still for the same song.
    plays: u64,
    /// The frame of music the song is at, and how far into the next one in hundredths of a
    /// T-cycle: with another tempo, a frame of output plays more or less than a frame of music.
    frame: u32,
    sub: u64,
    length: SongLength,
    /// Number of times to play the looping part before fading out; `None` loops forever. An
    /// audio file has no loop of its own: it plays once, or over and over for `None`.
    pub loop_count: Option<u32>,
    pub fade_secs: f32,
    /// Applied after the [`Profile`]'s effects, as the console's volume wheel comes after the
    /// chip: turning it down makes everything quieter but changes nothing else.
    pub volume: f32,
    /// In percent of the game's own; see [`Player::set_tempo`].
    tempo: u32,
    profile: Profile,
    noise: bool,
    effects: Chain,
    paused: bool,
    buf: Vec<f32>,
    buf_pos: usize,
    levels: [f32; 4],
    /// What the channels played at the end of each frame since the last [`Player::take_notes`].
    notes: Vec<[ChannelState; 4]>,
    /// Whether the notes come from a copy of the machine run ahead of the music (see
    /// [`NOTES_AHEAD`]); off where nothing shows them, as in an export.
    pub notes_ahead: bool,
}

impl Player {
    /// Play a ROM, or a GBS file.
    pub fn new(rom: Vec<u8>, sample_rate: u32) -> Result<Player, LoadError> {
        if is_gbs(&rom) {
            let (image, driver) = gbs::load(&rom).map_err(LoadError::Gbs)?;
            return Player::with_driver(image, sample_rate, driver);
        }
        let machine = Machine::new(rom.clone(), sample_rate)?;
        let driver = Driver::detect(&rom).ok_or_else(|| LoadError::UnsupportedGame {
            title: machine.mem.cart.header.title.clone(),
        })?;
        Player::with_driver(rom, sample_rate, driver)
    }

    /// Play `rom` with a driver found or built elsewhere.
    pub fn with_driver(
        rom: Vec<u8>,
        sample_rate: u32,
        driver: Driver,
    ) -> Result<Player, LoadError> {
        let machine = Machine::new(rom.clone(), sample_rate)?;
        Ok(Player::with_source(
            Source::Game(Box::new(Game {
                rom: rom.into(),
                machine,
                driver: Arc::new(driver),
                phase: 0,
                snapshots: Vec::new(),
                exact: true,
                ahead: None,
            })),
            sample_rate,
        ))
    }

    /// Play audio files, in the order of `tracks`.
    pub fn files(tracks: Vec<Track>, sample_rate: u32) -> Player {
        let songs = tracks
            .iter()
            .enumerate()
            .map(|(index, t)| Song {
                index,
                key: t.path.file_name().map(|n| n.to_string_lossy().into_owned()),
                id: 0,
                bank: 0,
                length: Some(t.length),
            })
            .collect();
        Player::with_source(
            Source::Files(Box::new(Files {
                tracks,
                songs,
                decoder: None,
            })),
            sample_rate,
        )
    }

    fn with_source(source: Source, sample_rate: u32) -> Player {
        Player {
            source,
            sample_rate,
            state: PlayState::Stopped,
            current: None,
            plays: 0,
            frame: 0,
            sub: 0,
            length: SongLength::Unknown,
            loop_count: None,
            fade_secs: DEFAULT_FADE_SECS,
            volume: 0.8,
            tempo: 100,
            profile: Profile::Line,
            noise: true,
            effects: Chain::new(Profile::Line, sample_rate, true),
            paused: false,
            buf: Vec::new(),
            buf_pos: 0,
            levels: [0.0; 4],
            notes: Vec::new(),
            notes_ahead: true,
        }
    }

    fn game(&self) -> Option<&Game> {
        match &self.source {
            Source::Game(g) => Some(g),
            Source::Files(_) => None,
        }
    }

    fn game_mut(&mut self) -> Option<&mut Game> {
        match &mut self.source {
            Source::Game(g) => Some(g),
            Source::Files(_) => None,
        }
    }

    /// The ROM image played; `None` for audio files.
    pub fn rom(&self) -> Option<Arc<[u8]>> {
        self.game().map(|g| g.rom.clone())
    }

    pub fn driver(&self) -> Option<&Driver> {
        self.game().map(|g| &*g.driver)
    }

    /// The title in the cartridge header; `None` for audio files.
    pub fn title(&self) -> Option<&str> {
        self.game()
            .map(|g| g.machine.mem.cart.header.title.as_str())
    }

    pub fn songs(&self) -> &[Song] {
        match &self.source {
            Source::Game(g) => &g.driver.songs,
            Source::Files(f) => &f.songs,
        }
    }

    pub fn current(&self) -> Option<usize> {
        self.current
    }

    pub fn state(&self) -> PlayState {
        self.state.clone()
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// The frame of music the song is at: at another tempo, frames of music go by more or
    /// less often than video frames.
    pub fn position_frames(&self) -> u32 {
        self.frame
    }

    /// Play a game's songs at `percent` of their own tempo (within [`TEMPO_RANGE`]), at the
    /// same pitch: the sound engine's updates come more or less often while the APU runs as
    /// ever. The song's length, its loops and the fade-out all stay in frames of music, so
    /// they take longer or shorter in step. It takes effect from the next frame, without a
    /// jump. Audio files keep their own tempo.
    pub fn set_tempo(&mut self, percent: u32) {
        let percent = percent.clamp(*TEMPO_RANGE.start(), *TEMPO_RANGE.end());
        if percent == self.tempo {
            return;
        }
        self.tempo = percent;
        // The notes ahead were run at the old tempo.
        let sub = self.sub;
        if let Some(g) = self.game_mut().filter(|g| g.ahead.is_some()) {
            g.run_ahead_from_here(sub);
        }
    }

    /// The tempo songs play at, in percent: always 100 for audio files.
    pub fn tempo(&self) -> u32 {
        if self.game().is_some() {
            self.tempo
        } else {
            100
        }
    }

    pub fn length(&self) -> SongLength {
        self.length
    }

    /// Provide the result of [`crate::analyze`] for the current song.
    pub fn set_length(&mut self, song: usize, length: SongLength) {
        if self.current == Some(song) {
            self.length = length;
        }
    }

    /// Mute one of the APU's channels; audio files have none.
    pub fn set_muted(&mut self, channel: usize, muted: bool) {
        if let Some(g) = self.game_mut() {
            g.machine.mem.apu.mute[channel] = muted;
        }
    }

    pub fn muted(&self, channel: usize) -> bool {
        self.game().is_some_and(|g| g.machine.mem.apu.mute[channel])
    }

    /// Set the volume of a channel, from 0 to 1, on top of the game's.
    pub fn set_channel_volume(&mut self, channel: usize, volume: f32) {
        if let Some(g) = self.game_mut() {
            g.machine.mem.apu.gain[channel] = volume.clamp(0.0, 1.0);
        }
    }

    pub fn channel_volume(&self, channel: usize) -> f32 {
        self.game().map_or(1.0, |g| g.machine.mem.apu.gain[channel])
    }

    /// Peak level of each channel since the last call, for meters.
    pub fn take_levels(&mut self) -> [f32; 4] {
        std::mem::take(&mut self.levels)
    }

    /// What the APU's channels played at the end of each frame since the last call, oldest
    /// first, for showing the notes; empty for audio files. The last of them is
    /// [`Player::notes_lead`] frames ahead of the music.
    pub fn take_notes(&mut self) -> Vec<[ChannelState; 4]> {
        std::mem::take(&mut self.notes)
    }

    /// How many frames the latest notes are ahead of the music: up to [`NOTES_AHEAD`], or 0
    /// when they come from the machine playing.
    pub fn notes_lead(&self) -> u32 {
        match self.game().and_then(|g| g.ahead.as_ref()) {
            Some(a) if self.notes_ahead => a.lead,
            _ => 0,
        }
    }

    /// Where the sound comes out, and whether the console's own noise comes with it.
    pub fn set_profile(&mut self, profile: Profile, noise: bool) {
        if (profile, noise) == (self.profile, self.noise) {
            return;
        }
        self.profile = profile;
        self.noise = noise;
        self.effects = Chain::new(profile, self.sample_rate, noise);
    }

    pub fn profile(&self) -> (Profile, bool) {
        (self.profile, self.noise)
    }

    pub fn play(&mut self, index: usize) {
        if index >= self.songs().len() {
            return;
        }
        self.current = Some(index);
        self.plays += 1;
        self.frame = 0;
        self.sub = 0;
        self.length = SongLength::Unknown;
        self.paused = false;
        self.buf.clear();
        self.buf_pos = 0;
        self.effects.reset();
        let sample_rate = self.sample_rate;
        let started = match &mut self.source {
            Source::Game(g) => {
                let started = g.start(index, sample_rate).map_err(Failure::Call);
                if self.notes_ahead {
                    g.run_ahead_from_here(0);
                }
                started
            }
            Source::Files(f) => {
                let track = &f.tracks[index];
                self.length = track.length;
                f.decoder = None;
                TrackDecoder::open(&track.path, sample_rate)
                    .map(|mut d| {
                        d.set_loop(track.looping);
                        f.decoder = Some(d);
                    })
                    .map_err(Failure::File)
            }
        };
        self.state = match started {
            Ok(()) => PlayState::Playing,
            Err(e) => PlayState::Failed(e),
        };
        self.keep_snapshot();
    }

    /// Make `index` the current song, ready at its start but stopped: nothing plays until it
    /// is played (or sought).
    pub fn cue(&mut self, index: usize) {
        self.play(index);
        if self.state == PlayState::Playing {
            self.state = PlayState::Stopped;
        }
    }

    pub fn stop(&mut self) {
        self.state = PlayState::Stopped;
        self.buf.clear();
        self.buf_pos = 0;
    }

    /// The frame at which the fade-out starts, if the song will end.
    fn fade_start(&self) -> Option<u32> {
        match (self.length, self.loop_count) {
            (SongLength::Loops { intro, period }, Some(n)) => {
                Some(intro.saturating_add(period.saturating_mul(n.max(1))))
            }
            _ => None,
        }
    }

    fn fade_frames(&self) -> u32 {
        ((self.fade_secs as f64 * FRAME_RATE) as u32).max(1)
    }

    /// The gain at a point of the song, in frames of music.
    fn gain_at(&self, frame: f64) -> f32 {
        match self.fade_start() {
            Some(start) if frame >= start as f64 => {
                1.0 - ((frame - start as f64) / self.fade_frames() as f64).min(1.0) as f32
            }
            _ => 1.0,
        }
    }

    fn run_frame(&mut self) {
        let from = self.frame as f64 + self.sub as f64 / FRAME_SUB as f64;
        let mut music = FRAME_SUB;
        let mut samples = match &mut self.source {
            Source::Game(g) => {
                music = CYCLES_PER_FRAME as u64 * self.tempo as u64;
                let cycles = (self.sub + music) / 100 - self.sub / 100;
                g.exact &= self.tempo == 100;
                if let Err(e) = g
                    .driver
                    .run_scaled(&mut g.machine, &mut g.phase, cycles as u32)
                {
                    self.state = PlayState::Failed(Failure::Call(e));
                    return;
                }
                let lv = g.machine.mem.apu.take_levels();
                for (a, b) in self.levels.iter_mut().zip(lv) {
                    *a = a.max(b);
                }
                g.run_notes(self.tempo, self.notes_ahead, &mut self.notes);
                g.machine.mem.apu.take_samples()
            }
            Source::Files(f) => {
                let Some(decoder) = &mut f.decoder else {
                    self.state = PlayState::Finished;
                    return;
                };
                // As many samples as the APU makes in this frame.
                let at = |frame: u32| (frame as f64 * self.sample_rate as f64 / FRAME_RATE) as u64;
                let want = (at(self.frame + 1) - at(self.frame)) as usize;
                let mut samples = Vec::with_capacity(want * 2);
                let mut got = decoder.read(&mut samples, want);
                // A file with a loop goes round it by itself (see `TrackDecoder::set_loop`).
                let whole = self.current.is_some_and(|i| f.tracks[i].looping.is_none());
                if got < want && whole && self.loop_count.is_none() && decoder.seek(0.0).is_ok() {
                    // Played over and over: on from the start.
                    let more = decoder.read(&mut samples, want - got);
                    if more > 0 {
                        self.frame = 0;
                    }
                    got += more;
                }
                if got == 0 {
                    self.state = PlayState::Finished;
                    return;
                }
                samples
            }
        };
        self.effects.process(&mut samples);
        let (g0, g1) = (
            self.gain_at(from),
            self.gain_at(from + music as f64 / FRAME_SUB as f64),
        );
        let n = (samples.len() / 2).max(1) as f32;
        self.buf.clear();
        self.buf_pos = 0;
        for (i, s) in samples.iter().enumerate() {
            let t = (i / 2) as f32 / n;
            self.buf.push(s * (g0 + (g1 - g0) * t) * self.volume);
        }
        self.sub += music;
        self.frame = self.frame.saturating_add((self.sub / FRAME_SUB) as u32);
        self.sub %= FRAME_SUB;
        self.keep_snapshot();

        if self.end_frame().is_some_and(|end| self.frame >= end) {
            self.state = PlayState::Finished;
        }
    }

    /// Remember the machine at the current frame if it starts a new snapshot interval.
    fn keep_snapshot(&mut self) {
        let (frame, length) = (self.frame, self.length);
        if let Some(g) = self.game_mut() {
            if g.exact && wants_snapshot(&g.snapshots, length, pass_frame(length, frame)) {
                g.snapshots.push(Snapshot {
                    frame,
                    machine: g.machine.clone(),
                    phase: g.phase,
                });
            }
        }
    }

    /// The frame at which the song stops with the current settings, if it does: after the
    /// fade-out of a looping song; a game's jingle once its last note has rung out for a
    /// second. An audio file without a loop stops at its end.
    fn end_frame(&self) -> Option<u32> {
        match (self.fade_start(), self.length) {
            (Some(start), _) => Some(start.saturating_add(self.fade_frames())),
            (_, SongLength::Ends { at }) if self.loop_count.is_some() && self.game().is_some() => {
                Some(at.saturating_add(FRAME_RATE as u32))
            }
            _ => None,
        }
    }

    /// Continue the current song from `frame`; see [`Player::begin_seek`].
    pub fn seek(&mut self, frame: u32) {
        if let Some(mut seek) = self.begin_seek(frame) {
            seek.run();
            self.finish_seek(seek);
        }
    }

    /// Start moving the current song to `frame`. A game's machine is restored from the
    /// nearest earlier snapshot and run silently up to that frame; beyond the first loop it is
    /// run up to the same point of the first pass instead, which sounds the same. An audio
    /// file is opened again there. That can take a while, so [`Seek::run`] does it apart from
    /// the player, which keeps playing until [`Player::finish_seek`].
    pub fn begin_seek(&self, frame: u32) -> Option<Seek> {
        let current = self.current?;
        if matches!(self.state, PlayState::Failed(_)) {
            return None;
        }
        let g = match &self.source {
            Source::Game(g) => g,
            Source::Files(f) => {
                let track = &f.tracks[current];
                let frame = match (self.end_frame(), self.length) {
                    (Some(end), _) | (None, SongLength::Ends { at: end }) => {
                        frame.min(end.saturating_sub(1))
                    }
                    _ => frame,
                };
                return Some(Seek {
                    play: self.plays,
                    frame,
                    work: SeekWork::File {
                        path: track.path.clone(),
                        looping: track.looping,
                        sample_rate: self.sample_rate,
                        secs: frames_to_secs(pass_frame(self.length, frame)),
                        decoder: None,
                    },
                });
            }
        };
        let frame = match self.end_frame() {
            Some(end) => frame.min(end.saturating_sub(1)),
            None => frame,
        };
        let pass = |f| pass_frame(self.length, f);
        let target = pass(frame);
        // Run on from the latest snapshot at or before the target, or from where the song
        // is now when that is closer.
        let (from, snapshot) = g
            .snapshots
            .iter()
            .map(|s| (pass(s.frame), s))
            .filter(|&(f, _)| f <= target)
            .max_by_key(|&(f, _)| f)?;
        let now = pass(self.frame);
        let (at, machine, phase) =
            if self.state == PlayState::Playing && g.exact && from <= now && now <= target {
                (now, Box::new(g.machine.clone()), g.phase)
            } else {
                (from, Box::new(snapshot.machine.clone()), snapshot.phase)
            };
        Some(Seek {
            play: self.plays,
            frame,
            work: SeekWork::Game {
                target,
                at,
                machine,
                phase,
                driver: g.driver.clone(),
                snapshots: Vec::new(),
                error: None,
            },
        })
    }

    /// Continue from where `seek` got to, unless another song was started meanwhile. A
    /// stopped or finished song starts playing again; a paused one stays paused.
    pub fn finish_seek(&mut self, seek: Seek) {
        if seek.play != self.plays {
            return;
        }
        let length = self.length;
        let failure = match (seek.work, &mut self.source) {
            (
                SeekWork::Game {
                    machine: mut m,
                    phase,
                    snapshots,
                    error,
                    ..
                },
                Source::Game(g),
            ) => {
                for s in snapshots {
                    if wants_snapshot(&g.snapshots, length, s.frame) {
                        g.snapshots.push(s);
                    }
                }
                match error {
                    Some(e) => Some(Failure::Call(e)),
                    None => {
                        m.mem.apu.set_output_enabled(true);
                        m.mem.apu.mute = g.machine.mem.apu.mute;
                        m.mem.apu.gain = g.machine.mem.apu.gain;
                        m.mem.apu.take_samples();
                        m.mem.apu.take_levels();
                        g.machine = *m;
                        g.phase = phase;
                        g.exact = true;
                        if self.notes_ahead {
                            g.run_ahead_from_here(0);
                        }
                        None
                    }
                }
            }
            (SeekWork::File { decoder, .. }, Source::Files(f)) => match decoder.map(|d| *d) {
                Some(Ok(d)) => {
                    f.decoder = Some(d);
                    None
                }
                Some(Err(e)) => Some(Failure::File(e)),
                None => return,
            },
            _ => return,
        };
        if let Some(e) = failure {
            self.state = PlayState::Failed(e);
            return;
        }
        self.frame = seek.frame;
        self.sub = 0;
        self.buf.clear();
        self.buf_pos = 0;
        self.effects.reset();
        self.state = PlayState::Playing;
    }

    /// Fill `out` with interleaved stereo samples.
    pub fn render(&mut self, out: &mut [f32]) {
        let mut i = 0;
        while i < out.len() {
            if self.buf_pos >= self.buf.len() {
                if self.state != PlayState::Playing || self.paused {
                    out[i..].fill(0.0);
                    return;
                }
                self.run_frame();
                continue;
            }
            let n = (out.len() - i).min(self.buf.len() - self.buf_pos);
            out[i..i + n].copy_from_slice(&self.buf[self.buf_pos..self.buf_pos + n]);
            self.buf_pos += n;
            i += n;
        }
    }
}

impl Game {
    /// Start song `index` on a fresh machine, which gives every song the same clean starting
    /// state.
    fn start(&mut self, index: usize, sample_rate: u32) -> Result<(), CallError> {
        let song = self.driver.songs[index].clone();
        let (mute, gain) = (self.machine.mem.apu.mute, self.machine.mem.apu.gain);
        self.machine =
            Machine::new(self.rom.to_vec(), sample_rate).expect("ROM was already validated");
        self.machine.mem.apu.mute = mute;
        self.machine.mem.apu.gain = gain;
        self.phase = 0;
        self.snapshots.clear();
        self.exact = true;
        let started = self.driver.play(&mut self.machine, &song);
        // Samples produced while starting the song are discarded.
        self.machine.mem.apu.take_samples();
        self.ahead = None;
        started
    }

    /// Start the copy for the notes afresh from the machine as it is, `sub` into its frame.
    fn run_ahead_from_here(&mut self, sub: u64) {
        let mut machine = self.machine.clone();
        machine.mem.apu.set_output_enabled(false);
        self.ahead = Some(Box::new(Ahead {
            machine,
            phase: self.phase,
            sub,
            lead: 0,
        }));
    }

    /// Add the notes of the frame just played to `notes`: those of the copy run ahead, which
    /// runs on one frame, or a few more while it catches up; without it (or with `ahead` off),
    /// the machine's own.
    fn run_notes(&mut self, tempo: u32, ahead: bool, notes: &mut Vec<[ChannelState; 4]>) {
        let mut push = |frame| {
            if notes.len() == MAX_NOTE_FRAMES {
                notes.remove(0);
            }
            notes.push(frame);
        };
        let Some(a) = self.ahead.as_mut().filter(|_| ahead) else {
            push(self.machine.mem.apu.channel_states());
            return;
        };
        let frames = 1 + NOTES_CATCH_UP.min(NOTES_AHEAD - a.lead);
        let music = CYCLES_PER_FRAME as u64 * tempo as u64;
        for _ in 0..frames {
            // As `Player::run_frame` steps the music.
            let cycles = (a.sub + music) / 100 - a.sub / 100;
            a.sub = (a.sub + music) % FRAME_SUB;
            let run = self
                .driver
                .run_scaled(&mut a.machine, &mut a.phase, cycles as u32);
            if run.is_err() {
                self.ahead = None;
                push(self.machine.mem.apu.channel_states());
                return;
            }
            push(a.machine.mem.apu.channel_states());
        }
        a.lead += frames - 1;
    }
}

/// The frame of the first pass through the song that sounds the same as `frame`: past the
/// first loop, the machine repeats the state it had one or more periods earlier.
fn pass_frame(length: SongLength, frame: u32) -> u32 {
    match length {
        SongLength::Loops { intro, period } if period > 0 && frame >= intro => {
            intro + (frame - intro) % period
        }
        _ => frame,
    }
}

/// Whether a snapshot of the first-pass frame `frame` is still missing.
fn wants_snapshot(snapshots: &[Snapshot], length: SongLength, frame: u32) -> bool {
    frame.is_multiple_of(SNAPSHOT_INTERVAL)
        && snapshots.len() < MAX_SNAPSHOTS
        && !snapshots
            .iter()
            .any(|s| pass_frame(length, s.frame) == frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::songdb::Series;

    /// A ROM whose toy engine counts updates in $C003 once a song has started, so that the
    /// RAM repeats every 256 frames.
    fn counting_player() -> Player {
        let mut rom = vec![0u8; 4 * 0x4000];
        rom[0x134..0x13B].copy_from_slice(b"SYNTHGB");
        rom[0x147] = 0x01;
        // Start: ld [$C000],a / ld a,1 / ld [$C002],a / ret
        rom[0x4000..0x4009]
            .copy_from_slice(&[0xEA, 0x00, 0xC0, 0x3E, 0x01, 0xEA, 0x02, 0xC0, 0xC9]);
        // Update: ld a,[$C002] / and a / ret z / ld hl,$C003 / inc [hl] / ret
        rom[0x4100..0x4109]
            .copy_from_slice(&[0xFA, 0x02, 0xC0, 0xA7, 0xC8, 0x21, 0x03, 0xC0, 0x34]);
        rom[0x4109] = 0xC9;
        let series: Series = toml::from_str(
            r#"
            [[game]]
            id = "toy"
            titles = ["SYNTHGB"]
            song_range = { first = 1, last = 1 }
            [game.playback]
            setup = [{ write = 0xFF26, value = 0x80 }]
            start = [{ bank = 1, call = 0x4000, a = "song" }]
            update = [{ bank = 1, call = 0x4100 }]
            "#,
        )
        .unwrap();
        let driver = Driver::from_game("toy", &series.games[0]).unwrap();
        let mut player = Player::with_driver(rom, 48_000, driver).unwrap();
        player.play(0);
        player
    }

    fn run_frames(p: &mut Player, frames: u32) {
        for _ in 0..frames {
            p.run_frame();
        }
    }

    /// The machine state of a song played straight through to `frame`.
    fn state_at(frame: u32) -> u64 {
        let mut p = counting_player();
        run_frames(&mut p, frame);
        p.game().unwrap().machine.state_hash()
    }

    #[test]
    fn the_volume_comes_after_the_profile() {
        let render = |volume: f32| {
            let mut p = counting_player();
            p.set_profile(Profile::GbcSpeaker, true);
            p.volume = volume;
            let mut out = vec![0.0f32; 9600];
            p.render(&mut out);
            out
        };
        // The toy song is silent, so all there is is the console's noise, from the speaker.
        let (full, half) = (render(0.8), render(0.4));
        assert!(full.iter().any(|&v| v != 0.0));
        assert!(full.chunks(2).all(|f| f[0] == f[1]));
        for (a, b) in full.iter().zip(&half) {
            assert!((a * 0.5 - b).abs() < 1e-6);
        }
    }

    #[test]
    fn seeking_back_restores_the_state_of_that_frame() {
        let mut p = counting_player();
        run_frames(&mut p, 1500);
        for frame in [700, 0, 301, 1499] {
            p.seek(frame);
            assert_eq!(p.position_frames(), frame);
            assert_eq!(
                p.game().unwrap().machine.state_hash(),
                state_at(frame),
                "{frame}"
            );
        }
    }

    #[test]
    fn seeking_ahead_runs_the_song_on() {
        let mut p = counting_player();
        run_frames(&mut p, 10);
        p.seek(2000);
        assert_eq!(p.game().unwrap().machine.state_hash(), state_at(2000));
        // The frames run through were kept for seeking back.
        assert!(p.game().unwrap().snapshots.iter().any(|s| s.frame == 1800));
        p.seek(1900);
        assert_eq!(p.game().unwrap().machine.state_hash(), state_at(1900));
    }

    #[test]
    fn later_loops_are_restored_from_the_first_pass() {
        let mut p = counting_player();
        // The counter starts with the first update, so the RAM repeats from frame 1.
        p.set_length(
            0,
            SongLength::Loops {
                intro: 1,
                period: 256,
            },
        );
        run_frames(&mut p, 300);
        let frame = 1 + 256 * 40 + 77;
        p.seek(frame);
        assert_eq!(p.position_frames(), frame);
        assert_eq!(p.game().unwrap().machine.state_hash(), state_at(78));
        // Only the first pass was run.
        assert!(p.game().unwrap().snapshots.iter().all(|s| s.frame <= 300));
    }

    #[test]
    fn seeking_restarts_a_finished_song_and_stays_before_its_end() {
        let mut p = counting_player();
        p.loop_count = Some(1);
        p.fade_secs = 1.0;
        p.set_length(
            0,
            SongLength::Loops {
                intro: 1,
                period: 256,
            },
        );
        let mut out = vec![0.0; 48_000 * 2 * 7];
        p.render(&mut out);
        assert_eq!(p.state(), PlayState::Finished);
        p.seek(10);
        assert_eq!(p.state(), PlayState::Playing);
        assert_eq!(p.position_frames(), 10);
        p.seek(u32::MAX);
        assert_eq!(p.state(), PlayState::Playing);
        assert_eq!(p.position_frames(), p.end_frame().unwrap() - 1);
    }

    #[test]
    fn a_seek_is_dropped_when_another_song_started_meanwhile() {
        let mut p = counting_player();
        run_frames(&mut p, 100);
        let mut seek = p.begin_seek(50).unwrap();
        p.play(0);
        seek.run();
        p.finish_seek(seek);
        assert_eq!(p.position_frames(), 0);
    }

    /// Output frames the toy song plays for with one loop and a second's fade at `percent`.
    fn frames_to_finish(percent: u32) -> u32 {
        let mut p = counting_player();
        p.loop_count = Some(1);
        p.fade_secs = 1.0;
        p.set_length(
            0,
            SongLength::Loops {
                intro: 1,
                period: 256,
            },
        );
        p.set_tempo(percent);
        let mut frames = 0;
        while p.state() == PlayState::Playing {
            p.run_frame();
            frames += 1;
        }
        assert_eq!(p.state(), PlayState::Finished);
        frames
    }

    #[test]
    fn the_song_length_scales_with_the_tempo() {
        // The intro, one loop and the fade, in frames of music.
        let end = 1 + 256 + (FRAME_RATE as u32);
        assert_eq!(frames_to_finish(100), end);
        assert_eq!(frames_to_finish(50), end * 2);
        assert_eq!(frames_to_finish(200), end.div_ceil(2));
        assert_eq!(frames_to_finish(150), (end * 2).div_ceil(3));
    }

    #[test]
    fn the_tempo_changes_how_often_the_engine_runs_but_not_the_output_rate() {
        let mut samples = Vec::new();
        for percent in [50, 100, 200] {
            let mut p = counting_player();
            p.set_tempo(percent);
            let mut n = 0;
            for _ in 0..120 {
                p.run_frame();
                n += p.buf.len();
            }
            samples.push(n);
            // Each frame of music is one update of the toy engine.
            let updates = p.game().unwrap().machine.clone().read(0xC003);
            assert_eq!(updates as u32, 120 * percent / 100 % 256, "{percent}%");
            assert_eq!(p.position_frames(), 120 * percent / 100);
        }
        assert!(samples.iter().all(|&n| n == samples[0]), "{samples:?}");
    }

    #[test]
    fn seeking_at_another_tempo_lands_on_the_same_state() {
        let mut p = counting_player();
        p.set_tempo(150);
        run_frames(&mut p, 400);
        assert_eq!(p.position_frames(), 600);
        // Snapshots of a song played at another tempo would not be those of its frames.
        assert_eq!(p.game().unwrap().snapshots.len(), 1);
        for frame in [700, 100, 650] {
            p.seek(frame);
            assert_eq!(p.position_frames(), frame);
            assert_eq!(p.game().unwrap().machine.state_hash(), state_at(frame));
        }
    }

    /// The state of the copy run ahead for the notes, and how far ahead it is.
    fn ahead(p: &Player) -> (u64, u32) {
        let a = p.game().unwrap().ahead.as_ref().unwrap();
        (a.machine.state_hash(), a.lead)
    }

    #[test]
    fn the_notes_come_from_where_the_music_will_be() {
        let mut p = counting_player();
        // It catches up a few frames at a time, then stays as far ahead.
        let catching = NOTES_AHEAD.div_ceil(NOTES_CATCH_UP);
        p.run_frame();
        assert_eq!(ahead(&p).1, NOTES_CATCH_UP);
        run_frames(&mut p, catching + 9);
        assert_eq!(p.notes_lead(), NOTES_AHEAD);
        assert_eq!(
            ahead(&p),
            (state_at(catching + 10 + NOTES_AHEAD), NOTES_AHEAD)
        );
        // A frame of notes for each frame it ran: one a frame, and the catching up.
        assert_eq!(p.take_notes().len(), MAX_NOTE_FRAMES);
        p.run_frame();
        assert_eq!(p.take_notes().len(), 1);
        // A seek starts it again from there.
        p.seek(40);
        assert_eq!(ahead(&p), (state_at(40), 0));
        run_frames(&mut p, catching);
        assert_eq!(ahead(&p).0, state_at(40 + catching + NOTES_AHEAD));
    }

    #[test]
    fn the_notes_ahead_follow_a_change_of_tempo() {
        let mut p = counting_player();
        run_frames(&mut p, 50);
        p.set_tempo(150);
        assert_eq!(ahead(&p).1, 0);
        run_frames(&mut p, 120);
        // At 150%, the music is 50 + 180 frames in, and the copy 1.5 times its lead further.
        assert_eq!(p.position_frames(), 230);
        assert_eq!(ahead(&p).1, NOTES_AHEAD);
        assert_eq!(ahead(&p).0, state_at(230 + NOTES_AHEAD * 3 / 2));
    }

    #[test]
    fn without_notes_ahead_there_is_no_copy() {
        let mut p = counting_player();
        p.notes_ahead = false;
        p.play(0);
        run_frames(&mut p, 10);
        assert!(p.game().unwrap().ahead.is_none());
        assert_eq!(p.notes_lead(), 0);
        assert_eq!(p.take_notes().len(), 10);
    }

    #[test]
    fn a_paused_song_stays_paused() {
        let mut p = counting_player();
        run_frames(&mut p, 100);
        p.set_paused(true);
        p.seek(20);
        assert!(p.paused());
        assert_eq!(p.game().unwrap().machine.state_hash(), state_at(20));
    }

    #[test]
    fn a_cued_song_stays_stopped_at_its_start() {
        let mut p = counting_player();
        run_frames(&mut p, 100);
        p.cue(0);
        assert_eq!(p.current(), Some(0));
        assert_eq!(p.state(), PlayState::Stopped);
        assert_eq!(p.position_frames(), 0);
        let mut out = vec![1.0; 4800];
        p.render(&mut out);
        assert!(out.iter().all(|&s| s == 0.0));
        assert_eq!(p.position_frames(), 0);
        // Seeking starts it from there, as for a stopped song.
        p.seek(20);
        assert_eq!(p.state(), PlayState::Playing);
        assert_eq!(p.game().unwrap().machine.state_hash(), state_at(20));
    }

    /// A WAV file of a tone `secs` long at 44.1 kHz, in a folder of its own.
    fn tone_wav(name: &str, secs: f64) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gb-player-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rate = 44_100u32;
        let frames = (secs * rate as f64) as u32;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + frames * 2).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        for v in [16u32, 1 | 1 << 16, rate, rate * 2, 2 | 16 << 16] {
            wav.extend_from_slice(&v.to_le_bytes());
        }
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(frames * 2).to_le_bytes());
        for i in 0..frames {
            let s = (std::f64::consts::TAU * 440.0 * i as f64 / rate as f64).sin() * 16_000.0;
            wav.extend_from_slice(&(s as i16).to_le_bytes());
        }
        let path = dir.join("tone.wav");
        std::fs::write(&path, wav).unwrap();
        path
    }

    fn file_player(path: &std::path::Path) -> Player {
        let track = crate::track::probe(path).unwrap();
        let mut p = Player::files(vec![track], 48_000);
        p.play(0);
        p
    }

    #[test]
    fn a_file_plays_once_and_finishes() {
        let mut p = file_player(&tone_wav("once", 1.0));
        assert_eq!(p.length(), SongLength::Ends { at: 60 });
        p.loop_count = Some(1);
        let mut out = vec![0.0f32; 48_000 * 2 * 2];
        p.render(&mut out);
        assert_eq!(p.state(), PlayState::Finished);
        // The tone is there, in mono on both sides, and silence after it.
        let peak = out[..48_000].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!((peak - 0.8 * 16_000.0 / 32_768.0).abs() < 0.01, "{peak}");
        assert!(out.chunks(2).all(|f| f[0] == f[1]));
        assert!(out[48_000 * 2 + 200..].iter().all(|&s| s == 0.0));
    }

    #[test]
    fn a_file_looped_forever_starts_over() {
        let mut p = file_player(&tone_wav("forever", 1.0));
        let mut out = vec![0.0f32; 48_000 * 2 * 5 / 2];
        p.render(&mut out);
        assert_eq!(p.state(), PlayState::Playing);
        assert!(
            (25..35).contains(&p.position_frames()),
            "{}",
            p.position_frames()
        );
        assert!(out[48_000 * 4..].iter().any(|&s| s.abs() > 0.1));
    }

    #[test]
    fn seeking_in_a_file_opens_it_there() {
        let mut p = file_player(&tone_wav("seek", 2.0));
        p.seek(90);
        assert_eq!(p.position_frames(), 90);
        assert_eq!(p.state(), PlayState::Playing);
        // Past the end: the last frame.
        p.seek(1000);
        assert_eq!(p.position_frames(), 118);
    }

    #[test]
    fn a_file_that_is_gone_fails() {
        let path = tone_wav("gone", 0.5);
        let track = crate::track::probe(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        let mut p = Player::files(vec![track], 48_000);
        p.play(0);
        assert!(matches!(p.state(), PlayState::Failed(Failure::File(_))));
    }

    /// A mono file at 44.1 kHz whose sample `i` is `i % 30_000`, so that where the output
    /// comes from can be read back from it.
    fn ramp(frames: u32) -> Vec<i16> {
        (0..frames).map(|i| (i % 30_000) as i16).collect()
    }

    /// The ramp as a WAV file, with a sampler loop from `start` to the sample `last`.
    fn looped_wav(name: &str, frames: u32, start: u32, last: u32) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gb-player-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut smpl = vec![0u32; 9];
        smpl[7] = 1;
        smpl.extend([0, 0, start, last, 0, 0]);
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(4 + 24 + 8 + frames * 2 + 8 + 60).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        for v in [16u32, 1 | 1 << 16, 44_100, 88_200, 2 | 16 << 16] {
            wav.extend_from_slice(&v.to_le_bytes());
        }
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&(frames * 2).to_le_bytes());
        for s in ramp(frames) {
            wav.extend_from_slice(&s.to_le_bytes());
        }
        wav.extend_from_slice(b"smpl");
        wav.extend_from_slice(&60u32.to_le_bytes());
        for v in smpl {
            wav.extend_from_slice(&v.to_le_bytes());
        }
        let path = dir.join("loop.wav");
        std::fs::write(&path, wav).unwrap();
        path
    }

    /// The ramp as a FLAC file (verbatim frames of 4096 samples), with Vorbis comments.
    fn tagged_flac(name: &str, frames: u32, comments: &[&str]) -> PathBuf {
        assert!(frames.is_multiple_of(4096));
        let crc8 = |data: &[u8]| {
            data.iter().fold(0u8, |mut c, &b| {
                c ^= b;
                for _ in 0..8 {
                    c = if c & 0x80 != 0 { c << 1 ^ 0x07 } else { c << 1 };
                }
                c
            })
        };
        let crc16 = |data: &[u8]| {
            data.iter().fold(0u16, |mut c, &b| {
                c ^= (b as u16) << 8;
                for _ in 0..8 {
                    c = if c & 0x8000 != 0 {
                        c << 1 ^ 0x8005
                    } else {
                        c << 1
                    };
                }
                c
            })
        };
        let mut f = b"fLaC".to_vec();
        // STREAMINFO: block sizes, unknown frame sizes, 44.1 kHz mono 16-bit, the length.
        f.extend([0, 0, 0, 34, 0x10, 0x00, 0x10, 0x00, 0, 0, 0, 0, 0, 0]);
        let info: u64 = 44_100 << 44 | 15 << 36 | frames as u64;
        f.extend(info.to_be_bytes());
        f.extend([0; 16]);
        let mut vc = Vec::new();
        vc.extend(0u32.to_le_bytes());
        vc.extend((comments.len() as u32).to_le_bytes());
        for c in comments {
            vc.extend((c.len() as u32).to_le_bytes());
            vc.extend(c.as_bytes());
        }
        f.push(0x84);
        f.extend(&(vc.len() as u32).to_be_bytes()[1..]);
        f.extend(vc);
        for (n, block) in ramp(frames).chunks(4096).enumerate() {
            let mut frame = vec![0xFF, 0xF8, 0xC9, 0x08, n as u8];
            frame.push(crc8(&frame));
            frame.push(0x02);
            for s in block {
                frame.extend(s.to_be_bytes());
            }
            frame.extend(crc16(&frame).to_be_bytes());
            f.extend(frame);
        }
        let dir = std::env::temp_dir().join(format!("gb-player-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("loop.flac");
        std::fs::write(&path, f).unwrap();
        path
    }

    /// The output of `p` at the file's rate, read back as the ramp's values.
    fn ramp_out(p: &mut Player, frames: usize) -> Vec<i32> {
        let mut out = vec![0.0f32; frames * 2];
        p.render(&mut out);
        out.iter()
            .step_by(2)
            .map(|s| (s / 0.8 * 32_768.0).round() as i32)
            .collect()
    }

    fn file_player_at(path: &std::path::Path, rate: u32) -> Player {
        let track = crate::track::probe(path).unwrap();
        let mut p = Player::files(vec![track], rate);
        p.play(0);
        p
    }

    #[test]
    fn a_file_goes_round_its_loop_without_a_seam() {
        let (start, end) = (11_025, 33_075);
        let mut p = file_player_at(&looped_wav("seam", 44_100, start, end - 1), 44_100);
        assert_eq!(
            p.length(),
            SongLength::Loops {
                intro: 15,
                period: 30
            }
        );
        let got = ramp_out(&mut p, 120_000);
        let want: Vec<i32> = (0..end)
            .chain((0..).flat_map(|_| start..end))
            .take(120_000)
            .map(|i| (i % 30_000) as i32)
            .collect();
        assert_eq!(got, want);
        assert_eq!(p.state(), PlayState::Playing);
    }

    #[test]
    fn loop_tags_loop_a_file() {
        let path = tagged_flac("tags", 40_960, &["LOOPSTART=4096", "LOOPLENGTH=20000"]);
        let mut p = file_player_at(&path, 44_100);
        let got = ramp_out(&mut p, 60_000);
        let want: Vec<i32> = (0..24_096)
            .chain((0..).flat_map(|_| 4096..24_096))
            .take(60_000)
            .collect();
        assert_eq!(got, want);
        // Without a length, the loop runs to the end of the file.
        let path = tagged_flac("tags-end", 40_960, &["loopstart=8192"]);
        let track = crate::track::probe(&path).unwrap();
        assert_eq!(
            track.looping,
            Some(crate::track::Loop {
                start: 8192,
                end: 40_960
            })
        );
    }

    #[test]
    fn a_looped_file_fades_out_after_its_loops() {
        let mut p = file_player_at(&looped_wav("fade", 44_100, 11_025, 33_074), 48_000);
        p.loop_count = Some(2);
        p.fade_secs = 1.0;
        let mut out = vec![0.0f32; 48_000 * 2 * 4];
        p.render(&mut out);
        assert_eq!(p.state(), PlayState::Finished);
        // Two loops after the intro, then the fade.
        assert_eq!(p.position_frames(), 15 + 2 * 30 + 59);
    }

    #[test]
    fn seeking_past_the_first_loop_lands_in_it() {
        let (start, end) = (11_025u32, 33_075u32);
        let mut p = file_player_at(&looped_wav("lap", 44_100, start, end - 1), 44_100);
        let frame = 15 + 30 * 3 + 10;
        p.seek(frame);
        assert_eq!(p.position_frames(), frame);
        // The same point of the first pass: 25 frames in.
        let at = (frames_to_secs(25) * 44_100.0) as i32;
        let first = ramp_out(&mut p, 1)[0];
        assert!((first - at).abs() <= 1, "{first} vs {at}");
    }
}
