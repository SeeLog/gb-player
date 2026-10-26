//! Real-time song playback on top of a [`Driver`].

use std::fmt;
use std::sync::Arc;

use gb_core::{CallError, CartridgeError, Machine, CYCLES_PER_FRAME};

use crate::driver::{Driver, Song};
use crate::length::{SongLength, FRAME_RATE};

#[derive(Debug)]
pub enum LoadError {
    Cartridge(CartridgeError),
    UnsupportedGame { title: String },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Cartridge(e) => write!(f, "{e}"),
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
    driver: Driver,
    state: PlayState,
    current: Option<usize>,
    frame: u32,
    length: SongLength,
    /// Number of times to play the looping part before fading out; `None` loops forever.
    pub loop_count: Option<u32>,
    pub fade_secs: f32,
    pub volume: f32,
    paused: bool,
    buf: Vec<f32>,
    buf_pos: usize,
    levels: [f32; 4],
}

impl Player {
    pub fn new(rom: Vec<u8>, sample_rate: u32) -> Result<Player, LoadError> {
        let machine = Machine::new(rom.clone(), sample_rate)?;
        let driver = Driver::detect(&rom).ok_or_else(|| LoadError::UnsupportedGame {
            title: machine.mem.cart.header.title.clone(),
        })?;
        Ok(Player {
            rom: rom.into(),
            machine,
            driver,
            state: PlayState::Stopped,
            current: None,
            frame: 0,
            length: SongLength::Unknown,
            loop_count: None,
            fade_secs: 8.0,
            volume: 0.8,
            paused: false,
            buf: Vec::new(),
            buf_pos: 0,
            levels: [0.0; 4],
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

    /// Peak level of each channel since the last call, for meters.
    pub fn take_levels(&mut self) -> [f32; 4] {
        std::mem::take(&mut self.levels)
    }

    pub fn play(&mut self, index: usize) {
        let Some(song) = self.driver.songs.get(index).cloned() else {
            return;
        };
        let sample_rate = self.machine.mem.apu.sample_rate();
        let mute = self.machine.mem.apu.mute;
        // A fresh machine gives every song the same clean starting state.
        self.machine =
            Machine::new(self.rom.to_vec(), sample_rate).expect("ROM was already validated");
        self.machine.mem.apu.mute = mute;
        self.current = Some(index);
        self.frame = 0;
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
    }

    pub fn stop(&mut self) {
        self.state = PlayState::Stopped;
        self.buf.clear();
        self.buf_pos = 0;
    }

    /// The frame at which the fade-out starts, if the song will end.
    fn fade_start(&self) -> Option<u32> {
        match (self.length, self.loop_count) {
            (SongLength::Loops { intro, period }, Some(n)) => Some(intro + period * n.max(1)),
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
        let spent = match self.driver.update(&mut self.machine) {
            Ok(c) => c,
            Err(e) => {
                self.state = PlayState::Failed(e);
                return;
            }
        };
        self.machine.idle(CYCLES_PER_FRAME.saturating_sub(spent));
        let samples = self.machine.mem.apu.take_samples();
        let (g0, g1) = (self.gain_at(self.frame), self.gain_at(self.frame + 1));
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
        self.frame += 1;

        let finished = match (self.fade_start(), self.length) {
            (Some(start), _) => self.frame >= start + self.fade_frames(),
            // Let a jingle's last note ring out for a second.
            (_, SongLength::Ends { at }) => {
                self.loop_count.is_some() && self.frame >= at + FRAME_RATE as u32
            }
            _ => false,
        };
        if finished {
            self.state = PlayState::Finished;
        }
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
