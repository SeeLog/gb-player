//! Where a sound engine keeps each music channel's position in the score, so that loop
//! analysis can tell when a channel comes back to a place it has been before.

use gb_core::Machine;

use crate::driver::Song;

/// Number of music channels a probe reads.
pub const CHANNELS: usize = 4;

/// One per-channel value in RAM: channel `c` keeps it at `at + c * step`, as one byte or a
/// little-endian word, of which only the bits in `mask` count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    pub at: u16,
    pub step: u16,
    pub word: bool,
    pub mask: u16,
}

impl Field {
    pub const fn byte(at: u16, step: u16) -> Field {
        Field {
            at,
            step,
            word: false,
            mask: 0xFF,
        }
    }

    pub const fn word(at: u16, step: u16) -> Field {
        Field {
            at,
            step,
            word: true,
            mask: 0xFFFF,
        }
    }

    pub const fn masked(self, mask: u16) -> Field {
        Field { mask, ..self }
    }

    pub fn read(&self, m: &mut Machine, channel: usize) -> u16 {
        let addr = self.at.wrapping_add(self.step.wrapping_mul(channel as u16));
        let v = if self.word {
            m.read16(addr)
        } else {
            m.read(addr) as u16
        };
        v & self.mask
    }
}

/// How to check right after a song starts that a probe points at real channel state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SongCheck {
    /// The ID of the song each channel plays.
    pub song_id: Field,
    /// Every active channel must play the song (otherwise one is enough).
    pub all: bool,
}

/// The layout of the per-channel playback state of a sound engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelProbe {
    /// Non-zero while the channel plays.
    pub active: Field,
    /// Address of the next command in the score.
    pub address: Field,
    /// Where a subroutine of the score returns to.
    pub return_address: Field,
    pub loop_count: Field,
    /// State bits that decide what the channel reads next (e.g. "inside a subroutine").
    pub flags: Field,
    /// Frames left on the current note.
    pub note_timer: Field,
    pub check: Option<SongCheck>,
}

/// The position of one music channel in its score: everything that decides which commands it
/// reads next. Tempo remainders and similar timing state are deliberately left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelKey {
    pub address: u16,
    pub return_address: u16,
    pub loop_count: u8,
    pub flags: u8,
}

impl ChannelProbe {
    /// Read the score position of the music channels; `None` for an inactive channel.
    pub fn read(&self, m: &mut Machine) -> [Option<ChannelKey>; CHANNELS] {
        std::array::from_fn(|c| {
            (self.active.read(m, c) != 0).then(|| ChannelKey {
                address: self.address.read(m, c),
                return_address: self.return_address.read(m, c),
                loop_count: self.loop_count.read(m, c) as u8,
                flags: self.flags.read(m, c) as u8,
            })
        })
    }

    /// Frames left on each channel's current note. A rise means a new note was read, which
    /// catches a channel whose loop is a single note and whose position therefore never
    /// changes between frames.
    pub fn note_timers(&self, m: &mut Machine) -> [u8; CHANNELS] {
        std::array::from_fn(|c| self.note_timer.read(m, c) as u8)
    }

    /// Check right after starting `song` that the probe points at real channel state.
    pub fn validate(&self, m: &mut Machine, song: &Song) -> bool {
        let Some(check) = self.check else {
            return true;
        };
        let mut playing = 0;
        let mut others = 0;
        for c in 0..CHANNELS {
            if self.active.read(m, c) == 0 {
                continue;
            }
            if check.song_id.read(m, c) == song.id & check.song_id.mask {
                playing += 1;
            } else {
                others += 1;
            }
        }
        playing > 0 && (!check.all || others == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_near_the_top_of_memory_wrap_around() {
        let mut m = Machine::new(vec![0; 0x8000], 48_000).unwrap();
        m.write(0xC000, 0x34);
        m.write(0xC001, 0x12);
        // Channel 1 of a word at $FFFF with a step of $C001 is at $C000.
        assert_eq!(Field::word(0xFFFF, 0xC001).read(&mut m, 1), 0x1234);
        let top = Field::word(0xFFFF, 0xFFFF);
        let probe = ChannelProbe {
            active: top,
            address: top,
            return_address: top,
            loop_count: top,
            flags: top,
            note_timer: top,
            check: Some(SongCheck {
                song_id: top,
                all: true,
            }),
        };
        probe.read(&mut m);
        probe.note_timers(&mut m);
        let song = Song {
            index: 0,
            key: None,
            id: 1,
            bank: 0,
            length: None,
        };
        probe.validate(&mut m, &song);
    }
}
