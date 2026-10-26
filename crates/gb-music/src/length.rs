//! Song lengths in frames, the unit the sound engines count in.

use gb_core::{CPU_CLOCK, CYCLES_PER_FRAME};

/// Video frames per second; sound engines advance once per frame.
pub const FRAME_RATE: f64 = CPU_CLOCK as f64 / CYCLES_PER_FRAME as f64;

pub fn frames_to_secs(frames: u32) -> f64 {
    frames as f64 / FRAME_RATE
}

/// The structure of a song in frames, found by [`crate::analyze`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongLength {
    /// The song plays `intro` frames once and then repeats every `period` frames forever.
    Loops { intro: u32, period: u32 },
    /// The song stops by itself after `at` frames (jingles, fanfares).
    Ends { at: u32 },
    /// No repetition was found within the analysed duration.
    Unknown,
}
