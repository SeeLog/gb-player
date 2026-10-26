//! Loop-point analysis: running a song faster than real time to find its structure.

use std::collections::HashMap;

use gb_core::{CallError, Machine, CYCLES_PER_FRAME};

use crate::driver::{ChannelKey, ChannelProbe, Driver, Song};
use crate::length::{SongLength, FRAME_RATE};

/// The default number of frames [`analyze`] runs a song for: ten minutes of music, long
/// enough for any loop in the supported games.
pub const ANALYSIS_FRAMES: u32 = (FRAME_RATE * 600.0) as u32;

/// Find the loop structure of `song` by running it faster than real time.
///
/// When the driver knows where the engine keeps each channel's position in the score, a
/// channel has looped once it reaches a position (command address, return address, loop
/// counter) it was at before. Tempo remainders are ignored on purpose: Gen 1 and 2 carry the
/// fractional part of note lengths over, so the full RAM may take many loops to repeat even
/// though the music has long started over.
///
/// Otherwise the whole RAM is compared between frames. Only the sound engine runs, so it is
/// the engine's state: once a snapshot repeats, everything after it repeats exactly.
pub fn analyze(
    rom: &[u8],
    driver: &Driver,
    song: &Song,
    max_frames: u32,
) -> Result<SongLength, CallError> {
    let Ok(mut m) = Machine::new(rom.to_vec(), 48_000) else {
        return Ok(SongLength::Unknown);
    };
    if let Some(length) = song.length {
        return Ok(length);
    }
    m.mem.apu.set_output_enabled(false);
    driver.play(&mut m, song)?;
    match driver.probe {
        Some(probe) if probe.validate(&mut m, song) => {
            analyze_score(&mut m, driver, probe, max_frames)
        }
        _ => analyze_ram(&mut m, driver, max_frames),
    }
}

fn run_frame(m: &mut Machine, driver: &Driver) -> Result<(), CallError> {
    let spent = driver.update(m)?;
    m.idle(CYCLES_PER_FRAME.saturating_sub(spent));
    Ok(())
}

#[derive(Clone, Copy)]
enum ChannelLoop {
    Searching,
    Unused,
    Loops { intro: u32, period: u32 },
    Ended { at: u32 },
}

fn analyze_score(
    m: &mut Machine,
    driver: &Driver,
    probe: ChannelProbe,
    max_frames: u32,
) -> Result<SongLength, CallError> {
    let mut seen: [HashMap<ChannelKey, u32>; 4] = Default::default();
    let mut prev = probe.read(m);
    let mut prev_timers = probe.note_timers(m);
    let mut state = prev.map(|k| {
        if k.is_some() {
            ChannelLoop::Searching
        } else {
            ChannelLoop::Unused
        }
    });
    for (c, k) in prev.iter().enumerate() {
        if let Some(k) = k {
            seen[c].insert(*k, 0);
        }
    }
    for frame in 1..max_frames {
        if !state.iter().any(|s| matches!(s, ChannelLoop::Searching)) {
            break;
        }
        run_frame(m, driver)?;
        let keys = probe.read(m);
        let timers = probe.note_timers(m);
        for c in 0..4 {
            let new_note = keys[c] != prev[c] || timers[c] > prev_timers[c];
            if !matches!(state[c], ChannelLoop::Searching) || !new_note {
                continue;
            }
            match keys[c] {
                None => state[c] = ChannelLoop::Ended { at: frame },
                Some(k) => match seen[c].get(&k) {
                    Some(&first) => {
                        state[c] = ChannelLoop::Loops {
                            intro: first,
                            period: frame - first,
                        }
                    }
                    None => {
                        seen[c].insert(k, frame);
                    }
                },
            }
        }
        prev = keys;
        prev_timers = timers;
    }

    let mut intro = 0;
    let mut period = 0;
    let mut end = 0;
    for s in state {
        match s {
            ChannelLoop::Searching => return Ok(SongLength::Unknown),
            ChannelLoop::Unused => {}
            ChannelLoop::Loops {
                intro: i,
                period: p,
            } => {
                intro = intro.max(i);
                period = period.max(p);
            }
            ChannelLoop::Ended { at } => end = end.max(at),
        }
    }
    Ok(if period > 0 {
        SongLength::Loops { intro, period }
    } else {
        SongLength::Ends { at: end }
    })
}

fn analyze_ram(m: &mut Machine, driver: &Driver, max_frames: u32) -> Result<SongLength, CallError> {
    let mut seen: HashMap<u64, u32> = HashMap::new();
    for frame in 0..max_frames {
        let hash = m.state_hash();
        if let Some(&first) = seen.get(&hash) {
            let period = frame - first;
            return Ok(if period == 1 {
                SongLength::Ends { at: first }
            } else {
                SongLength::Loops {
                    intro: first,
                    period,
                }
            });
        }
        seen.insert(hash, frame);
        run_frame(m, driver)?;
    }
    Ok(SongLength::Unknown)
}
