//! Real-time song playback, from a [`Driver`] on the emulated machine or from audio files.

use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;

use gb_core::{CallError, CartridgeError, Machine};

use crate::driver::{Driver, Song};
use crate::effects::{Chain, Profile};
use crate::gbs::{self, is_gbs, GbsError};
use crate::length::{frames_to_secs, SongLength, FRAME_RATE};
use crate::track::{Track, TrackDecoder, TrackError};

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

/// Most snapshots kept for one song (about 170 KiB each), enough for 20 minutes of music.
const MAX_SNAPSHOTS: usize = 240;

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
        sample_rate: u32,
        /// The file opened and moved to the frame sought, once [`Seek::run`] has.
        decoder: Option<Result<TrackDecoder, TrackError>>,
    },
}

impl Seek {
    /// Run the machine silently up to the frame sought, or open the file there.
    pub fn run(&mut self) {
        let secs = frames_to_secs(self.frame);
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
                sample_rate,
                decoder,
            } => {
                *decoder = Some(TrackDecoder::open(path, *sample_rate).and_then(|mut d| {
                    d.seek(secs)?;
                    Ok(d)
                }));
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
    frame: u32,
    length: SongLength,
    /// Number of times to play the looping part before fading out; `None` loops forever. An
    /// audio file has no loop of its own: it plays once, or over and over for `None`.
    pub loop_count: Option<u32>,
    pub fade_secs: f32,
    /// Applied after the [`Profile`]'s effects, as the console's volume wheel comes after the
    /// chip: turning it down makes everything quieter but changes nothing else.
    pub volume: f32,
    profile: Profile,
    noise: bool,
    effects: Chain,
    paused: bool,
    buf: Vec<f32>,
    buf_pos: usize,
    levels: [f32; 4],
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
            length: SongLength::Unknown,
            loop_count: None,
            fade_secs: 8.0,
            volume: 0.8,
            profile: Profile::Line,
            noise: true,
            effects: Chain::new(Profile::Line, sample_rate, true),
            paused: false,
            buf: Vec::new(),
            buf_pos: 0,
            levels: [0.0; 4],
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

    pub fn position_frames(&self) -> u32 {
        self.frame
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
        self.length = SongLength::Unknown;
        self.paused = false;
        self.buf.clear();
        self.buf_pos = 0;
        self.effects.reset();
        let sample_rate = self.sample_rate;
        let started = match &mut self.source {
            Source::Game(g) => g.start(index, sample_rate).map_err(Failure::Call),
            Source::Files(f) => {
                let track = &f.tracks[index];
                self.length = track.length;
                f.decoder = None;
                TrackDecoder::open(&track.path, sample_rate)
                    .map(|d| f.decoder = Some(d))
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

    fn gain_at(&self, frame: u32) -> f32 {
        match self.fade_start() {
            Some(start) if frame >= start => {
                1.0 - ((frame - start) as f32 / self.fade_frames() as f32).min(1.0)
            }
            _ => 1.0,
        }
    }

    fn run_frame(&mut self) {
        let mut samples = match &mut self.source {
            Source::Game(g) => {
                if let Err(e) = g.driver.run_frame(&mut g.machine, &mut g.phase) {
                    self.state = PlayState::Failed(Failure::Call(e));
                    return;
                }
                let lv = g.machine.mem.apu.take_levels();
                for (a, b) in self.levels.iter_mut().zip(lv) {
                    *a = a.max(b);
                }
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
                if got < want && self.loop_count.is_none() && decoder.seek(0.0).is_ok() {
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
            self.gain_at(self.frame),
            self.gain_at(self.frame.saturating_add(1)),
        );
        let n = (samples.len() / 2).max(1) as f32;
        self.buf.clear();
        self.buf_pos = 0;
        for (i, s) in samples.iter().enumerate() {
            let t = (i / 2) as f32 / n;
            self.buf.push(s * (g0 + (g1 - g0) * t) * self.volume);
        }
        self.frame = self.frame.saturating_add(1);
        self.keep_snapshot();

        if self.end_frame().is_some_and(|end| self.frame >= end) {
            self.state = PlayState::Finished;
        }
    }

    /// Remember the machine at the current frame if it starts a new snapshot interval.
    fn keep_snapshot(&mut self) {
        let (frame, length) = (self.frame, self.length);
        if let Some(g) = self.game_mut() {
            if wants_snapshot(&g.snapshots, length, pass_frame(length, frame)) {
                g.snapshots.push(Snapshot {
                    frame,
                    machine: g.machine.clone(),
                    phase: g.phase,
                });
            }
        }
    }

    /// The frame at which a game's song stops with the current settings, if it does; a
    /// jingle's last note rings out for a second. An audio file stops at its end.
    fn end_frame(&self) -> Option<u32> {
        self.game()?;
        match (self.fade_start(), self.length) {
            (Some(start), _) => Some(start.saturating_add(self.fade_frames())),
            (_, SongLength::Ends { at }) if self.loop_count.is_some() => {
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
                let frame = match track.length {
                    SongLength::Ends { at } => frame.min(at.saturating_sub(1)),
                    _ => frame,
                };
                return Some(Seek {
                    play: self.plays,
                    frame,
                    work: SeekWork::File {
                        path: track.path.clone(),
                        sample_rate: self.sample_rate,
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
            if self.state == PlayState::Playing && from <= now && now <= target {
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
                        None
                    }
                }
            }
            (SeekWork::File { decoder, .. }, Source::Files(f)) => match decoder {
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
        let started = self.driver.play(&mut self.machine, &song);
        // Samples produced while starting the song are discarded.
        self.machine.mem.apu.take_samples();
        started
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
}
