//! How to drive a game's sound engine, as data: the steps that start a song and the steps
//! run once per update. Engine detectors and song database entries both produce a
//! [`Playback`]; running one only ever touches the emulated machine.

use gb_core::{CallError, Machine, CYCLES_PER_FRAME};

use crate::driver::Song;

/// Upper bound for one call into the sound engine (about 10 frames of CPU time).
pub const CALL_BUDGET: u32 = 700_000;

/// Upper bound for all calls of the steps that start a song, not counting [`Step::Frames`].
pub const START_BUDGET: u32 = 4 * CALL_BUDGET;

/// Upper bound for all calls of one update.
pub const UPDATE_BUDGET: u32 = CALL_BUDGET;

/// A number put into a register or memory when a step runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value {
    Const(u16),
    /// The song's ID ([`Song::id`]).
    Song,
    /// The song's ROM bank ([`Song::bank`]).
    SongBank,
    /// The byte at this address when the step runs.
    Ram(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reg {
    A,
    B,
    C,
    D,
    E,
    H,
    L,
    Bc,
    De,
    Hl,
}

/// Where a call goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Fixed(u16),
    /// Chosen by the byte at `ram`: the routine of the matching case, or `default`.
    ByRam {
        ram: u16,
        cases: Vec<(u8, u16)>,
        default: u16,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Write the low byte of a value to an address (RAM, I/O or a bank controller).
    Write { addr: u16, value: Value },
    /// Switch the $4000-$7FFF window to a ROM bank.
    Bank(Value),
    /// Set registers, then call a routine and wait for it to return.
    Call {
        target: Target,
        regs: Vec<(Reg, Value)>,
    },
    /// Run this many updates, a frame apart.
    Frames(u16),
}

/// How often the update steps run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rate {
    /// Once per frame, from the VBlank interrupt.
    VBlank,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Playback {
    pub rate: Rate,
    /// Run on a freshly reset machine before every song.
    pub setup: Vec<Step>,
    /// Start a song.
    pub start: Vec<Step>,
    /// Advance the music by one update.
    pub update: Vec<Step>,
}

impl Value {
    fn get(self, m: &mut Machine, song: Option<&Song>) -> u16 {
        match self {
            Value::Const(v) => v,
            Value::Song => song.map_or(0, |s| s.id),
            Value::SongBank => song.map_or(0, |s| s.bank as u16),
            Value::Ram(addr) => m.read(addr) as u16,
        }
    }
}

impl Target {
    fn resolve(&self, m: &mut Machine) -> u16 {
        match self {
            Target::Fixed(addr) => *addr,
            Target::ByRam {
                ram,
                cases,
                default,
            } => {
                let v = m.read(*ram);
                cases
                    .iter()
                    .find(|&&(case, _)| case == v)
                    .map_or(*default, |&(_, addr)| addr)
            }
        }
    }
}

fn set_reg(m: &mut Machine, reg: Reg, v: u16) {
    let cpu = &mut m.cpu;
    match reg {
        Reg::A => cpu.a = v as u8,
        Reg::B => cpu.b = v as u8,
        Reg::C => cpu.c = v as u8,
        Reg::D => cpu.d = v as u8,
        Reg::E => cpu.e = v as u8,
        Reg::H => cpu.h = v as u8,
        Reg::L => cpu.l = v as u8,
        Reg::Bc => cpu.set_bc(v),
        Reg::De => cpu.set_de(v),
        Reg::Hl => cpu.set_hl(v),
    }
}

impl Playback {
    /// Start `song` on a freshly reset machine.
    pub fn start(&self, m: &mut Machine, song: &Song) -> Result<(), CallError> {
        let mut budget = START_BUDGET;
        self.run(&self.setup, m, Some(song), &mut budget)?;
        self.run(&self.start, m, Some(song), &mut budget)
    }

    /// Run one update; returns the T-cycles it took.
    pub fn update(&self, m: &mut Machine) -> Result<u32, CallError> {
        let mut budget = UPDATE_BUDGET;
        self.run(&self.update, m, None, &mut budget)?;
        Ok(UPDATE_BUDGET - budget)
    }

    fn run(
        &self,
        steps: &[Step],
        m: &mut Machine,
        song: Option<&Song>,
        budget: &mut u32,
    ) -> Result<(), CallError> {
        for step in steps {
            match step {
                Step::Write { addr, value } => {
                    let v = value.get(m, song);
                    m.write(*addr, v as u8);
                }
                Step::Bank(value) => {
                    let v = value.get(m, song);
                    m.switch_rom_bank(v as u8);
                }
                Step::Call { target, regs } => {
                    for &(reg, value) in regs {
                        let v = value.get(m, song);
                        set_reg(m, reg, v);
                    }
                    let addr = target.resolve(m);
                    let spent = m.call(addr, (*budget).min(CALL_BUDGET))?;
                    *budget = budget.saturating_sub(spent);
                }
                Step::Frames(n) => {
                    for _ in 0..*n {
                        let spent = self.update(m)?;
                        m.idle(CYCLES_PER_FRAME.saturating_sub(spent));
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 32 KiB ROM without a bank controller holding `code` at each address.
    fn machine(code: &[(u16, &[u8])]) -> Machine {
        let mut rom = vec![0u8; 0x8000];
        for (addr, bytes) in code {
            rom[*addr as usize..*addr as usize + bytes.len()].copy_from_slice(bytes);
        }
        Machine::new(rom, 48_000).unwrap()
    }

    fn song(id: u16, bank: u8) -> Song {
        Song {
            index: 0,
            key: None,
            id,
            bank,
            length: None,
        }
    }

    #[test]
    fn steps_put_song_values_into_registers_and_memory() {
        // ld [$C000],a / ld a,c / ld [$C001],a / ret
        let mut m = machine(&[(0x0200, &[0xEA, 0x00, 0xC0, 0x79, 0xEA, 0x01, 0xC0, 0xC9])]);
        let playback = Playback {
            rate: Rate::VBlank,
            setup: vec![Step::Write {
                addr: 0xC010,
                value: Value::Const(0x1234),
            }],
            start: vec![Step::Call {
                target: Target::Fixed(0x0200),
                regs: vec![(Reg::A, Value::Song), (Reg::C, Value::SongBank)],
            }],
            update: Vec::new(),
        };
        playback.start(&mut m, &song(0x42, 7)).unwrap();
        assert_eq!(m.read(0xC010), 0x34);
        assert_eq!(m.read(0xC000), 0x42);
        assert_eq!(m.read(0xC001), 7);
    }

    #[test]
    fn calls_can_be_picked_by_a_ram_value() {
        // $0200: ld a,1 / ld [$C000],a / ret    $0210: ld a,2 / ld [$C000],a / ret
        let mut m = machine(&[
            (0x0200, &[0x3E, 0x01, 0xEA, 0x00, 0xC0, 0xC9]),
            (0x0210, &[0x3E, 0x02, 0xEA, 0x00, 0xC0, 0xC9]),
        ]);
        let playback = Playback {
            rate: Rate::VBlank,
            setup: Vec::new(),
            start: Vec::new(),
            update: vec![Step::Call {
                target: Target::ByRam {
                    ram: 0xC001,
                    cases: vec![(5, 0x0210)],
                    default: 0x0200,
                },
                regs: Vec::new(),
            }],
        };
        playback.update(&mut m).unwrap();
        assert_eq!(m.read(0xC000), 1);
        m.write(0xC001, 5);
        playback.update(&mut m).unwrap();
        assert_eq!(m.read(0xC000), 2);
    }

    #[test]
    fn a_routine_that_never_returns_runs_out_of_budget() {
        // jr -2
        let mut m = machine(&[(0x0200, &[0x18, 0xFE])]);
        let call = Step::Call {
            target: Target::Fixed(0x0200),
            regs: Vec::new(),
        };
        let playback = Playback {
            rate: Rate::VBlank,
            setup: Vec::new(),
            start: vec![call.clone(); 10],
            update: vec![call; 10],
        };
        assert!(matches!(
            playback.start(&mut m, &song(1, 0)),
            Err(CallError::Timeout { .. })
        ));
        assert!(matches!(
            playback.update(&mut m),
            Err(CallError::Timeout { .. })
        ));
    }
}
