//! The Game Boy memory map wired to the CPU and APU.
//!
//! There is no PPU: video memory is plain RAM and LY just counts scanlines, which is all a
//! sound engine needs.

use crate::apu::Apu;
use crate::cartridge::{Cartridge, CartridgeError};
use crate::cpu::{Bus, Cpu};

/// T-cycles per video frame (154 scanlines of 456 cycles), i.e. the VBlank period.
pub const CYCLES_PER_FRAME: u32 = 70_224;

/// Address the CPU "returns" to when a host-initiated call finishes. It lies in the
/// unusable $FEA0-$FEFF region, so real game code never jumps there.
const RETURN_ADDR: u16 = 0xFEA0;

/// Stack used for host-initiated calls. It grows down from the top of VRAM, which a sound
/// engine never touches, so the game's own stack and variables are left alone.
const CALL_STACK: u16 = 0xA000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallError {
    /// The routine did not return within the cycle budget.
    Timeout { pc: u16 },
    /// The CPU executed an undefined opcode.
    Locked { pc: u16 },
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::Timeout { pc } => write!(f, "routine did not return (pc=${pc:04X})"),
            CallError::Locked { pc } => write!(f, "CPU locked up (pc=${pc:04X})"),
        }
    }
}

impl std::error::Error for CallError {}

pub struct Memory {
    pub cart: Cartridge,
    pub apu: Apu,
    vram: Vec<u8>,
    wram: Vec<u8>,
    wram_bank: usize,
    oam: [u8; 0xA0],
    hram: [u8; 0x7F],
    io: [u8; 0x80],
    ie: u8,
    cgb: bool,
    /// Cycles elapsed in the current frame, used for LY.
    frame_cycles: u32,
    div: u16,
}

impl Memory {
    fn ly(&self) -> u8 {
        (self.frame_cycles / 456) as u8
    }

    pub fn wram_bank(&self) -> usize {
        self.wram_bank
    }

    /// Advance timers and the APU by `cycles` T-cycles.
    pub fn tick(&mut self, cycles: u32) {
        self.apu.step(cycles);
        self.frame_cycles = (self.frame_cycles + cycles) % CYCLES_PER_FRAME;
        self.div = self.div.wrapping_add(cycles as u16);
    }
}

impl Bus for Memory {
    fn read(&mut self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF | 0xA000..=0xBFFF => self.cart.read(addr),
            0x8000..=0x9FFF => self.vram[addr as usize - 0x8000],
            0xC000..=0xCFFF => self.wram[addr as usize - 0xC000],
            0xD000..=0xDFFF => self.wram[self.wram_bank * 0x1000 + (addr as usize - 0xD000)],
            0xE000..=0xFDFF => self.read(addr - 0x2000),
            0xFE00..=0xFE9F => self.oam[addr as usize - 0xFE00],
            0xFEA0..=0xFEFF => 0xFF,
            0xFF04 => (self.div >> 8) as u8,
            0xFF10..=0xFF3F => self.apu.read(addr),
            0xFF44 => self.ly(),
            0xFF4D if self.cgb => 0x7E,
            0xFF70 if self.cgb => 0xF8 | self.wram_bank as u8,
            0xFF00..=0xFF7F => self.io[addr as usize - 0xFF00],
            0xFF80..=0xFFFE => self.hram[addr as usize - 0xFF80],
            0xFFFF => self.ie,
        }
    }

    fn write(&mut self, addr: u16, value: u8) {
        match addr {
            0x0000..=0x7FFF | 0xA000..=0xBFFF => self.cart.write(addr, value),
            0x8000..=0x9FFF => self.vram[addr as usize - 0x8000] = value,
            0xC000..=0xCFFF => self.wram[addr as usize - 0xC000] = value,
            0xD000..=0xDFFF => {
                self.wram[self.wram_bank * 0x1000 + (addr as usize - 0xD000)] = value
            }
            0xE000..=0xFDFF => self.write(addr - 0x2000, value),
            0xFE00..=0xFE9F => self.oam[addr as usize - 0xFE00] = value,
            0xFEA0..=0xFEFF => {}
            0xFF04 => self.div = 0,
            0xFF10..=0xFF3F => self.apu.write(addr, value),
            0xFF70 => {
                if self.cgb {
                    self.wram_bank = ((value & 7) as usize).max(1);
                }
            }
            0xFF00..=0xFF7F => self.io[addr as usize - 0xFF00] = value,
            0xFF80..=0xFFFE => self.hram[addr as usize - 0xFF80] = value,
            0xFFFF => self.ie = value,
        }
    }
}

/// A Game Boy reduced to what a sound driver needs: CPU, memory, cartridge and APU.
pub struct Machine {
    pub cpu: Cpu,
    pub mem: Memory,
}

impl Machine {
    pub fn new(rom: Vec<u8>, sample_rate: u32) -> Result<Machine, CartridgeError> {
        let cart = Cartridge::new(rom)?;
        let cgb = cart.header.is_cgb();
        let mut cpu = Cpu::new();
        if cgb {
            cpu.a = 0x11;
        }
        Ok(Machine {
            cpu,
            mem: Memory {
                cart,
                apu: Apu::new(sample_rate),
                vram: vec![0; 0x2000],
                wram: vec![0; 0x8000],
                wram_bank: 1,
                oam: [0; 0xA0],
                hram: [0; 0x7F],
                io: [0; 0x80],
                ie: 0,
                cgb,
                frame_cycles: 0,
                div: 0,
            },
        })
    }

    pub fn is_cgb(&self) -> bool {
        self.mem.cgb
    }

    pub fn rom(&self) -> &[u8] {
        self.mem.cart.rom()
    }

    pub fn read(&mut self, addr: u16) -> u8 {
        self.mem.read(addr)
    }

    pub fn write(&mut self, addr: u16, value: u8) {
        self.mem.write(addr, value)
    }

    pub fn read16(&mut self, addr: u16) -> u16 {
        u16::from_le_bytes([self.read(addr), self.read(addr.wrapping_add(1))])
    }

    /// Switch the $4000-$7FFF window to `bank` the way games do (write to $2000).
    pub fn switch_rom_bank(&mut self, bank: u8) {
        self.mem.write(0x2000, bank);
    }

    /// Call the routine at `addr` and run until it returns. The APU keeps running while the
    /// routine executes. Returns the number of T-cycles spent.
    pub fn call(&mut self, addr: u16, max_cycles: u32) -> Result<u32, CallError> {
        self.cpu.halted = false;
        self.cpu.locked = false;
        self.cpu.ime = false;
        self.cpu.sp = CALL_STACK;
        self.cpu.push(&mut self.mem, RETURN_ADDR);
        self.cpu.pc = addr;
        let mut spent = 0;
        while self.cpu.pc != RETURN_ADDR {
            if self.cpu.locked {
                return Err(CallError::Locked { pc: self.cpu.pc });
            }
            if self.cpu.halted {
                // Nothing will wake the CPU up; treat a HALT as the end of the routine.
                self.cpu.halted = false;
                break;
            }
            if spent >= max_cycles {
                return Err(CallError::Timeout { pc: self.cpu.pc });
            }
            let c = self.cpu.step(&mut self.mem);
            self.mem.tick(c);
            spent += c;
        }
        Ok(spent)
    }

    /// A hash of all RAM the game can use for its own state (WRAM, HRAM and the APU
    /// register file). Two equal hashes at frame boundaries mean the sound engine is in the
    /// same state, which is how song loops are detected.
    pub fn state_hash(&self) -> u64 {
        let mut h = Fnv64::new();
        h.write(&self.mem.wram);
        h.write(&self.mem.hram);
        h.write(self.mem.apu.registers());
        h.write(&[self.mem.wram_bank as u8]);
        h.finish()
    }

    /// Let time pass without running the CPU.
    pub fn idle(&mut self, cycles: u32) {
        self.mem.tick(cycles);
    }
}

/// FNV-1a, processing eight bytes at a time; good enough to fingerprint RAM snapshots.
struct Fnv64(u64);

impl Fnv64 {
    fn new() -> Fnv64 {
        Fnv64(0xcbf2_9ce4_8422_2325)
    }

    fn write(&mut self, data: &[u8]) {
        let (chunks, remainder) = data.as_chunks::<8>();
        for c in chunks {
            let v = u64::from_le_bytes(*c);
            self.0 = (self.0 ^ v)
                .wrapping_mul(0x0000_0100_0000_01B3)
                .rotate_left(23);
        }
        for &b in remainder {
            self.0 = (self.0 ^ b as u64).wrapping_mul(0x0000_0100_0000_01B3);
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}
