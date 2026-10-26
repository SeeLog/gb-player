//! ROM header parsing and memory bank controllers.

use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MbcKind {
    None,
    Mbc1,
    Mbc2,
    Mbc3,
    Mbc5,
}

#[derive(Debug)]
pub enum CartridgeError {
    TooSmall(usize),
    TooLarge(usize),
    UnsupportedMbc(u8),
}

impl fmt::Display for CartridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CartridgeError::TooSmall(n) => write!(f, "ROM is too small ({n} bytes)"),
            CartridgeError::TooLarge(n) => write!(f, "ROM is too large ({n} bytes)"),
            CartridgeError::UnsupportedMbc(t) => write!(f, "unsupported cartridge type ${t:02X}"),
        }
    }
}

impl std::error::Error for CartridgeError {}

/// The largest ROM a cartridge can have (512 banks of 16 KiB, the MBC5 maximum).
pub const MAX_ROM_SIZE: usize = 512 * 0x4000;

/// Read a ROM file without reading more than [`MAX_ROM_SIZE`] bytes of it; a bigger file
/// comes back one byte too long, which [`Cartridge::new`] then rejects.
pub fn read_rom(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut rom = Vec::new();
    File::open(path)?
        .take(MAX_ROM_SIZE as u64 + 1)
        .read_to_end(&mut rom)?;
    Ok(rom)
}

/// Information read from the cartridge header ($0100-$014F).
#[derive(Debug, Clone)]
pub struct Header {
    pub title: String,
    pub cgb_flag: u8,
    pub cart_type: u8,
    pub rom_version: u8,
    pub global_checksum: u16,
    pub mbc: MbcKind,
}

impl Header {
    pub fn parse(rom: &[u8]) -> Result<Header, CartridgeError> {
        if rom.len() < 0x8000 {
            return Err(CartridgeError::TooSmall(rom.len()));
        }
        // The title is 16 bytes on DMG and 11-15 on CGB; trailing bytes may be a
        // manufacturer code or the CGB flag, so stop at the first non-printable byte.
        let title: String = rom[0x134..0x144]
            .iter()
            .take_while(|&&b| (0x20..0x7F).contains(&b))
            .map(|&b| b as char)
            .collect();
        let cart_type = rom[0x147];
        let mbc = match cart_type {
            0x00 | 0x08 | 0x09 => MbcKind::None,
            0x01..=0x03 => MbcKind::Mbc1,
            0x05 | 0x06 => MbcKind::Mbc2,
            0x0F..=0x13 => MbcKind::Mbc3,
            0x19..=0x1E => MbcKind::Mbc5,
            t => return Err(CartridgeError::UnsupportedMbc(t)),
        };
        Ok(Header {
            title: title.trim_end().to_string(),
            cgb_flag: rom[0x143],
            cart_type,
            rom_version: rom[0x14C],
            global_checksum: u16::from_be_bytes([rom[0x14E], rom[0x14F]]),
            mbc,
        })
    }

    pub fn is_cgb(&self) -> bool {
        self.cgb_flag & 0x80 != 0
    }
}

pub struct Cartridge {
    rom: Vec<u8>,
    ram: Vec<u8>,
    pub header: Header,
    rom_bank: usize,
    ram_bank: usize,
    ram_enabled: bool,
    // MBC1 state
    mbc1_bank_lo: u8,
    mbc1_bank_hi: u8,
    mbc1_mode: bool,
    rom_bank_count: usize,
}

impl Cartridge {
    pub fn new(rom: Vec<u8>) -> Result<Cartridge, CartridgeError> {
        if rom.len() > MAX_ROM_SIZE {
            return Err(CartridgeError::TooLarge(rom.len()));
        }
        let header = Header::parse(&rom)?;
        let rom_bank_count = rom.len().div_ceil(0x4000).max(2);
        Ok(Cartridge {
            rom,
            // Allocate the maximum so that games never write out of bounds.
            ram: vec![0; 0x20000],
            header,
            rom_bank: 1,
            ram_bank: 0,
            ram_enabled: false,
            mbc1_bank_lo: 1,
            mbc1_bank_hi: 0,
            mbc1_mode: false,
            rom_bank_count,
        })
    }

    pub fn rom(&self) -> &[u8] {
        &self.rom
    }

    pub fn rom_bank(&self) -> usize {
        self.rom_bank
    }

    pub fn bank_count(&self) -> usize {
        self.rom_bank_count
    }

    fn rom_byte(&self, offset: usize) -> u8 {
        self.rom.get(offset).copied().unwrap_or(0xFF)
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => {
                let bank = if self.header.mbc == MbcKind::Mbc1 && self.mbc1_mode {
                    ((self.mbc1_bank_hi as usize) << 5) % self.rom_bank_count
                } else {
                    0
                };
                self.rom_byte(bank * 0x4000 + addr as usize)
            }
            0x4000..=0x7FFF => self.rom_byte(
                (self.rom_bank % self.rom_bank_count) * 0x4000 + (addr as usize - 0x4000),
            ),
            0xA000..=0xBFFF => {
                if !self.ram_enabled && self.header.mbc != MbcKind::None {
                    return 0xFF;
                }
                self.ram[(self.ram_bank * 0x2000 + (addr as usize - 0xA000)) % self.ram.len()]
            }
            _ => 0xFF,
        }
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF => self.write_mbc(addr, value),
            0xA000..=0xBFFF if self.ram_enabled || self.header.mbc == MbcKind::None => {
                let i = (self.ram_bank * 0x2000 + (addr as usize - 0xA000)) % self.ram.len();
                self.ram[i] = value;
            }
            _ => {}
        }
    }

    fn write_mbc(&mut self, addr: u16, value: u8) {
        match self.header.mbc {
            MbcKind::None => {}
            MbcKind::Mbc1 => match addr {
                0x0000..=0x1FFF => self.ram_enabled = value & 0x0F == 0x0A,
                0x2000..=0x3FFF => {
                    self.mbc1_bank_lo = (value & 0x1F).max(1);
                    self.update_mbc1();
                }
                0x4000..=0x5FFF => {
                    self.mbc1_bank_hi = value & 0x03;
                    self.update_mbc1();
                }
                _ => {
                    self.mbc1_mode = value & 1 != 0;
                    self.update_mbc1();
                }
            },
            MbcKind::Mbc2 => {
                if addr < 0x4000 {
                    if addr & 0x100 == 0 {
                        self.ram_enabled = value & 0x0F == 0x0A;
                    } else {
                        self.rom_bank = ((value & 0x0F) as usize).max(1);
                    }
                }
            }
            MbcKind::Mbc3 => match addr {
                0x0000..=0x1FFF => self.ram_enabled = value & 0x0F == 0x0A,
                0x2000..=0x3FFF => self.rom_bank = ((value & 0x7F) as usize).max(1),
                // RTC registers ($08-$0C) are mapped onto spare RAM banks; music never needs them.
                0x4000..=0x5FFF => self.ram_bank = (value & 0x0F) as usize,
                _ => {}
            },
            MbcKind::Mbc5 => match addr {
                0x0000..=0x1FFF => self.ram_enabled = value & 0x0F == 0x0A,
                0x2000..=0x2FFF => self.rom_bank = (self.rom_bank & 0x100) | value as usize,
                0x3000..=0x3FFF => {
                    self.rom_bank = (self.rom_bank & 0xFF) | ((value as usize & 1) << 8)
                }
                0x4000..=0x5FFF => self.ram_bank = (value & 0x0F) as usize,
                _ => {}
            },
        }
    }

    fn update_mbc1(&mut self) {
        self.rom_bank = ((self.mbc1_bank_hi as usize) << 5) | self.mbc1_bank_lo as usize;
        self.ram_bank = if self.mbc1_mode {
            self.mbc1_bank_hi as usize
        } else {
            0
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rom_with_type(cart_type: u8, banks: usize) -> Vec<u8> {
        let mut rom = vec![0u8; banks * 0x4000];
        rom[0x134..0x13A].copy_from_slice(b"TESTGB");
        rom[0x147] = cart_type;
        for b in 0..banks {
            rom[b * 0x4000 + 0x10] = b as u8;
        }
        rom
    }

    #[test]
    fn header_title() {
        let cart = Cartridge::new(rom_with_type(0x13, 4)).unwrap();
        assert_eq!(cart.header.title, "TESTGB");
        assert_eq!(cart.header.mbc, MbcKind::Mbc3);
    }

    #[test]
    fn rejects_oversized_rom() {
        let rom = rom_with_type(0x19, 513);
        assert!(matches!(
            Cartridge::new(rom),
            Err(CartridgeError::TooLarge(_))
        ));
        assert!(Cartridge::new(rom_with_type(0x19, 512)).is_ok());
    }

    #[test]
    fn mbc3_switch() {
        let mut cart = Cartridge::new(rom_with_type(0x13, 8)).unwrap();
        cart.write(0x2000, 5);
        assert_eq!(cart.read(0x4010), 5);
        cart.write(0x2000, 0);
        assert_eq!(cart.read(0x4010), 1);
    }

    #[test]
    fn mbc1_switch() {
        let mut cart = Cartridge::new(rom_with_type(0x03, 64)).unwrap();
        cart.write(0x2000, 0x1F);
        assert_eq!(cart.read(0x4010), 0x1F);
        cart.write(0x4000, 1);
        assert_eq!(cart.read(0x4010), 0x3F);
    }

    #[test]
    fn mbc5_switch() {
        let mut cart = Cartridge::new(rom_with_type(0x1B, 128)).unwrap();
        cart.write(0x2000, 0x45);
        assert_eq!(cart.read(0x4010), 0x45);
        cart.write(0x2000, 0);
        assert_eq!(cart.read(0x4010), 0);
    }
}
