//! GBS files: the sound code and music of a game cut out of its ROM, with a header that says
//! where to load it and which routines start a song and advance it.
//!
//! The data is placed into a cartridge image that the emulated machine runs like any ROM, and
//! the header becomes a [`Playback`]. Only the file itself is read; playlists or other files
//! next to it are not.

use std::fmt;

use gb_core::MAX_ROM_SIZE;

use crate::driver::{Driver, Engine, Song};
use crate::playback::{Playback, Rate, Reg, Step, Target, Value};

/// Size of the GBS header; the data follows it.
const HEADER: usize = 0x70;

/// Song database series that GBS files are named under.
pub const SERIES: &str = "gbs";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GbsError {
    TooShort,
    Version(u8),
    NoSongs,
    /// The load address leaves no room for the header and RST vectors, or is not in ROM.
    LoadAddress(u16),
    TooLarge,
    /// A routine address is where host-initiated calls return to.
    Routine(u16),
    /// The timer registers ask for more updates per frame than the player runs.
    TimerRate {
        tac: u8,
        tma: u8,
    },
}

impl fmt::Display for GbsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GbsError::TooShort => write!(f, "the GBS file is shorter than its header"),
            GbsError::Version(v) => write!(f, "unsupported GBS version {v}"),
            GbsError::NoSongs => write!(f, "the GBS file has no songs"),
            GbsError::LoadAddress(a) => write!(f, "unsupported GBS load address ${a:04X}"),
            GbsError::TooLarge => write!(f, "the GBS data does not fit into a cartridge"),
            GbsError::Routine(a) => write!(f, "unsupported GBS routine address ${a:04X}"),
            GbsError::TimerRate { tac, tma } => {
                write!(
                    f,
                    "unsupported GBS timer rate (TAC ${tac:02X}, TMA ${tma:02X})"
                )
            }
        }
    }
}

impl std::error::Error for GbsError {}

pub fn is_gbs(data: &[u8]) -> bool {
    data.starts_with(b"GBS")
}

fn word(data: &[u8], pos: usize) -> u16 {
    u16::from_le_bytes([data[pos], data[pos + 1]])
}

/// The printable ASCII of a NUL-padded header string.
fn text(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take_while(|&&b| b != 0)
        .filter(|b| (0x20..0x7F).contains(*b))
        .map(|&b| b as char)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Turn a GBS file into a cartridge image and the driver that plays it.
pub fn load(data: &[u8]) -> Result<(Vec<u8>, Driver), GbsError> {
    if data.len() < HEADER {
        return Err(GbsError::TooShort);
    }
    if data[3] != 1 {
        return Err(GbsError::Version(data[3]));
    }
    let count = data[4];
    if count == 0 {
        return Err(GbsError::NoSongs);
    }
    let (load, init, play) = (word(data, 6), word(data, 8), word(data, 10));
    let (tma, tac) = (data[0x0E], data[0x0F]);
    let title = text(&data[0x10..0x30]);
    if !(0x0400..0x8000).contains(&load) {
        return Err(GbsError::LoadAddress(load));
    }
    for addr in [init, play] {
        if (0xFEA0..=0xFEFF).contains(&addr) {
            return Err(GbsError::Routine(addr));
        }
    }
    let body = &data[HEADER..];
    let end = load as usize + body.len();
    if end > MAX_ROM_SIZE {
        return Err(GbsError::TooLarge);
    }
    let rate = if tac & 0x04 != 0 {
        Rate::timer(tac, tma).ok_or(GbsError::TimerRate { tac, tma })?
    } else {
        Rate::VBlank
    };

    // At least two banks, so that the header fits and $4000-$7FFF exists.
    let mut image = vec![0u8; end.div_ceil(0x4000).max(2) * 0x4000];
    image[load as usize..end].copy_from_slice(body);
    // RST $xx jumps to the load address plus $xx.
    for rst in (0..0x40).step_by(8) {
        let [lo, hi] = (load + rst).to_le_bytes();
        image[rst as usize..rst as usize + 3].copy_from_slice(&[0xC3, lo, hi]);
    }
    // A cartridge header: the title, MBC3 (bank writes to $2000, bank 0 means 1) and the CGB
    // flag when the timer asks for the double-speed clock.
    let header_title: Vec<u8> = title.bytes().take(15).collect();
    image[0x134..0x134 + header_title.len()].copy_from_slice(&header_title);
    image[0x143] = if tac & 0x80 != 0 { 0x80 } else { 0x00 };
    image[0x147] = 0x13;

    let write = |addr: u16, v: u16| Step::Write {
        addr,
        value: Value::Const(v),
    };
    let playback = Playback {
        rate,
        setup: vec![
            // Cartridge RAM is always there in a GBS player.
            write(0x0000, 0x0A),
            write(0xFF26, 0x80),
            write(0xFF25, 0xFF),
            write(0xFF24, 0x77),
            write(0xFF06, tma as u16),
            write(0xFF07, tac as u16),
        ],
        start: vec![
            Step::Bank(Value::Const(1)),
            Step::Call {
                target: Target::Fixed(init),
                regs: vec![(Reg::A, Value::Song)],
            },
        ],
        update: vec![Step::Call {
            target: Target::Fixed(play),
            regs: Vec::new(),
        }],
    };
    // The routine is passed the song number counting from 0.
    let songs = (0..count as u16)
        .map(|id| Song {
            index: id as usize,
            key: None,
            id,
            bank: 0,
            length: None,
        })
        .collect();
    let driver = Driver {
        engine: Engine::Configured,
        probe: None,
        series: SERIES.to_string(),
        game_id: if title.is_empty() {
            "GBS".to_string()
        } else {
            title
        },
        songs,
        playback,
        warnings: Vec::new(),
    };
    Ok((image, driver))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(count: u8, load: u16, init: u16, play: u16, tma: u8, tac: u8) -> Vec<u8> {
        let mut h = vec![0u8; HEADER];
        h[0..3].copy_from_slice(b"GBS");
        h[3] = 1;
        h[4] = count;
        h[5] = 1;
        h[6..8].copy_from_slice(&load.to_le_bytes());
        h[8..10].copy_from_slice(&init.to_le_bytes());
        h[10..12].copy_from_slice(&play.to_le_bytes());
        h[12..14].copy_from_slice(&0xDFFFu16.to_le_bytes());
        h[0x0E] = tma;
        h[0x0F] = tac;
        h[0x10..0x18].copy_from_slice(b"Toy Tune");
        h
    }

    #[test]
    fn header_becomes_image_and_playback() {
        let mut gbs = header(3, 0x0400, 0x0400, 0x0410, 0xC0, 0x04);
        gbs.extend([0xC9; 0x20]);
        let (image, driver) = load(&gbs).unwrap();
        assert_eq!(image.len(), 0x8000);
        assert_eq!(&image[0x400..0x420], &[0xC9; 0x20]);
        assert_eq!(&image[0x08..0x0B], &[0xC3, 0x08, 0x04]);
        assert_eq!(&image[0x134..0x13C], b"Toy Tune");
        assert_eq!(driver.songs.len(), 3);
        assert_eq!(driver.game_id, "Toy Tune");
        assert_eq!(driver.playback.rate, Rate::Timer { period: 64 * 1024 });
    }

    #[test]
    fn bad_headers_are_refused() {
        let ok = header(1, 0x0400, 0x0400, 0x0400, 0, 0);
        assert!(load(&ok).is_ok());
        assert_eq!(load(&ok[..0x6F]).unwrap_err(), GbsError::TooShort);
        let mut v2 = ok.clone();
        v2[3] = 2;
        assert_eq!(load(&v2).unwrap_err(), GbsError::Version(2));
        assert_eq!(
            load(&header(0, 0x0400, 0x0400, 0x0400, 0, 0)).unwrap_err(),
            GbsError::NoSongs
        );
        for addr in [0x0000, 0x03FF, 0x8000, 0xFFFF] {
            assert_eq!(
                load(&header(1, addr, 0x0400, 0x0400, 0, 0)).unwrap_err(),
                GbsError::LoadAddress(addr)
            );
        }
        assert_eq!(
            load(&header(1, 0x0400, 0xFEA0, 0x0400, 0, 0)).unwrap_err(),
            GbsError::Routine(0xFEA0)
        );
        assert!(matches!(
            load(&header(1, 0x0400, 0x0400, 0x0400, 0xFF, 0x05)),
            Err(GbsError::TimerRate { .. })
        ));
        let mut big = header(1, 0x7000, 0x7000, 0x7000, 0, 0);
        big.resize(HEADER + MAX_ROM_SIZE, 0);
        assert_eq!(load(&big).unwrap_err(), GbsError::TooLarge);
    }
}
