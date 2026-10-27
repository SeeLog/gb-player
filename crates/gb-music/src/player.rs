//! Real-time song playback on top of a [`Driver`].

use std::fmt;
use std::sync::Arc;

use gb_core::{CallError, CartridgeError, Machine};

use crate::driver::{Driver, Song};
use crate::gbs::{self, is_gbs, GbsError};
use crate::length::{SongLength, FRAME_RATE};

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
    /// The first-pass frame that sounds like `frame`.
    target: u32,
    at: u32,
    machine: Machine,
    phase: u32,
    driver: Arc<Driver>,
    /// Snapshots taken on the way, for the player.
    snapshots: Vec<Snapshot>,
    error: Option<CallError>,
}

impl Seek {
    /// Run the machine silently up to the frame sought.
    pub fn run(&mut self) {
        self.machine.mem.apu.set_output_enabled(false);
        while self.at < self.target {
            if let Err(e) = self.driver.run_frame(&mut self.machine, &mut self.phase) {
                self.error = Some(e);
                return;
            }
            self.at += 1;
            if self.at.is_multiple_of(SNAPSHOT_INTERVAL) {
                self.snapshots.push(Snapshot {
                    frame: self.at,
                    machine: self.machine.clone(),
                    phase: self.phase,
                });
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayState {
    Stopped,
    Playing,
    /// The song finished (after the configured number of loops, or by itself).
    Finished,
    Failed(CallError),
}

pub struct Player {
    rom: Arc<[u8]>,
    machine: Machine,
    driver: Arc<Driver>,
    state: PlayState,
    current: Option<usize>,
    /// Counts [`Player::play`] calls, so that a seek can tell it is still for the same song.
    plays: u64,
    frame: u32,
    /// T-cycles since the last update, for timer-driven engines.
    phase: u32,
    length: SongLength,
    /// Number of times to play the looping part before fading out; `None` loops forever.
    pub loop_count: Option<u32>,
    pub fade_secs: f32,
    pub volume: f32,
    paused: bool,
    buf: Vec<f32>,
    buf_pos: usize,
    levels: [f32; 4],
    /// Snapshots of the current song, at most one per frame of its first pass (see
    /// [`Player::pass_frame`]).
    snapshots: Vec<Snapshot>,
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
        Ok(Player {
            rom: rom.into(),
            machine,
            driver: Arc::new(driver),
            state: PlayState::Stopped,
            current: None,
            plays: 0,
            frame: 0,
            phase: 0,
            length: SongLength::Unknown,
            loop_count: None,
            fade_secs: 8.0,
            volume: 0.8,
            paused: false,
            buf: Vec::new(),
            buf_pos: 0,
            levels: [0.0; 4],
            snapshots: Vec::new(),
        })
    }

    pub fn rom(&self) -> Arc<[u8]> {
        self.rom.clone()
    }

    pub fn driver(&self) -> &Driver {
        &self.driver
    }

    pub fn title(&self) -> &str {
        &self.machine.mem.cart.header.title
    }

    pub fn songs(&self) -> &[Song] {
        &self.driver.songs
    }

    pub fn current(&self) -> Option<usize> {
        self.current
    }

    pub fn state(&self) -> PlayState {
        self.state
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

    pub fn set_muted(&mut self, channel: usize, muted: bool) {
        self.machine.mem.apu.mute[channel] = muted;
    }

    pub fn muted(&self, channel: usize) -> bool {
        self.machine.mem.apu.mute[channel]
    }

    /// Set the volume of a channel, from 0 to 1, on top of the game's.
    pub fn set_channel_volume(&mut self, channel: usize, volume: f32) {
        self.machine.mem.apu.gain[channel] = volume.clamp(0.0, 1.0);
    }

    pub fn channel_volume(&self, channel: usize) -> f32 {
        self.machine.mem.apu.gain[channel]
    }

    /// Peak level of each channel since the last call, for meters.
    pub fn take_levels(&mut self) -> [f32; 4] {
        std::mem::take(&mut self.levels)
    }

    pub fn play(&mut self, index: usize) {
        let Some(song) = self.driver.songs.get(index).cloned() else {
            return;
        };
        let sample_rate = self.machine.mem.apu.sample_rate();
        let (mute, gain) = (self.machine.mem.apu.mute, self.machine.mem.apu.gain);
        // A fresh machine gives every song the same clean starting state.
        self.machine =
            Machine::new(self.rom.to_vec(), sample_rate).expect("ROM was already validated");
        self.machine.mem.apu.mute = mute;
        self.machine.mem.apu.gain = gain;
        self.current = Some(index);
        self.plays += 1;
        self.frame = 0;
        self.phase = 0;
        self.length = SongLength::Unknown;
        self.paused = false;
        self.buf.clear();
        self.buf_pos = 0;
        self.state = match self.driver.play(&mut self.machine, &song) {
            Ok(()) => PlayState::Playing,
            Err(e) => PlayState::Failed(e),
        };
        // Samples produced while starting the song are discarded.
        self.machine.mem.apu.take_samples();
        self.snapshots.clear();
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
        if let Err(e) = self.driver.run_frame(&mut self.machine, &mut self.phase) {
            self.state = PlayState::Failed(e);
            return;
        }
        let samples = self.machine.mem.apu.take_samples();
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
        let lv = self.machine.mem.apu.take_levels();
        for (a, b) in self.levels.iter_mut().zip(lv) {
            *a = a.max(b);
        }
        self.frame = self.frame.saturating_add(1);
        self.keep_snapshot();

        if self.end_frame().is_some_and(|end| self.frame >= end) {
            self.state = PlayState::Finished;
        }
    }

    /// The frame of the first pass through the song that sounds the same as `frame`: past the
    /// first loop, the machine repeats the state it had one or more periods earlier.
    fn pass_frame(&self, frame: u32) -> u32 {
        match self.length {
            SongLength::Loops { intro, period } if period > 0 && frame >= intro => {
                intro + (frame - intro) % period
            }
            _ => frame,
        }
    }

    /// Whether a snapshot of the first-pass frame `frame` is still missing.
    fn wants_snapshot(&self, frame: u32) -> bool {
        frame.is_multiple_of(SNAPSHOT_INTERVAL)
            && self.snapshots.len() < MAX_SNAPSHOTS
            && !self
                .snapshots
                .iter()
                .any(|s| self.pass_frame(s.frame) == frame)
    }

    /// Remember the machine at the current frame if it starts a new snapshot interval.
    fn keep_snapshot(&mut self) {
        if self.wants_snapshot(self.pass_frame(self.frame)) {
            self.snapshots.push(Snapshot {
                frame: self.frame,
                machine: self.machine.clone(),
                phase: self.phase,
            });
        }
    }

    /// The frame at which the song stops with the current settings, if it does; a jingle's
    /// last note rings out for a second.
    fn end_frame(&self) -> Option<u32> {
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

    /// Start moving the current song to `frame`. The machine is restored from the nearest
    /// earlier snapshot and run silently up to that frame; beyond the first loop it is run up
    /// to the same point of the first pass instead, which sounds the same. That can take a
    /// while, so [`Seek::run`] does it apart from the player, which keeps playing until
    /// [`Player::finish_seek`].
    pub fn begin_seek(&self, frame: u32) -> Option<Seek> {
        if self.current.is_none() || matches!(self.state, PlayState::Failed(_)) {
            return None;
        }
        let frame = match self.end_frame() {
            Some(end) => frame.min(end.saturating_sub(1)),
            None => frame,
        };
        let target = self.pass_frame(frame);
        // Run on from the latest snapshot at or before the target, or from where the song
        // is now when that is closer.
        let (from, snapshot) = self
            .snapshots
            .iter()
            .map(|s| (self.pass_frame(s.frame), s))
            .filter(|&(f, _)| f <= target)
            .max_by_key(|&(f, _)| f)?;
        let now = self.pass_frame(self.frame);
        let (at, machine, phase) =
            if self.state == PlayState::Playing && from <= now && now <= target {
                (now, self.machine.clone(), self.phase)
            } else {
                (from, snapshot.machine.clone(), snapshot.phase)
            };
        Some(Seek {
            play: self.plays,
            frame,
            target,
            at,
            machine,
            phase,
            driver: self.driver.clone(),
            snapshots: Vec::new(),
            error: None,
        })
    }

    /// Continue from where `seek` got to, unless another song was started meanwhile. A
    /// stopped or finished song starts playing again; a paused one stays paused.
    pub fn finish_seek(&mut self, seek: Seek) {
        if seek.play != self.plays {
            return;
        }
        for s in seek.snapshots {
            if self.wants_snapshot(s.frame) {
                self.snapshots.push(s);
            }
        }
        if let Some(e) = seek.error {
            self.state = PlayState::Failed(e);
            return;
        }
        let mut machine = seek.machine;
        machine.mem.apu.set_output_enabled(true);
        machine.mem.apu.mute = self.machine.mem.apu.mute;
        machine.mem.apu.gain = self.machine.mem.apu.gain;
        machine.mem.apu.take_samples();
        machine.mem.apu.take_levels();
        self.machine = machine;
        self.phase = seek.phase;
        self.frame = seek.frame;
        self.buf.clear();
        self.buf_pos = 0;
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
        p.machine.state_hash()
    }

    #[test]
    fn seeking_back_restores_the_state_of_that_frame() {
        let mut p = counting_player();
        run_frames(&mut p, 1500);
        for frame in [700, 0, 301, 1499] {
            p.seek(frame);
            assert_eq!(p.position_frames(), frame);
            assert_eq!(p.machine.state_hash(), state_at(frame), "{frame}");
        }
    }

    #[test]
    fn seeking_ahead_runs_the_song_on() {
        let mut p = counting_player();
        run_frames(&mut p, 10);
        p.seek(2000);
        assert_eq!(p.machine.state_hash(), state_at(2000));
        // The frames run through were kept for seeking back.
        assert!(p.snapshots.iter().any(|s| s.frame == 1800));
        p.seek(1900);
        assert_eq!(p.machine.state_hash(), state_at(1900));
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
        assert_eq!(p.machine.state_hash(), state_at(78));
        // Only the first pass was run.
        assert!(p.snapshots.iter().all(|s| s.frame <= 300));
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
        assert_eq!(p.machine.state_hash(), state_at(20));
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
        assert_eq!(p.machine.state_hash(), state_at(20));
    }
}
