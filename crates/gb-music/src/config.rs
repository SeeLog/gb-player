//! The `[game.playback]` and `[game.probe]` tables of the song database: how to drive the
//! sound engine of a game that no built-in detector knows, written out as data.
//!
//! Everything here is plain data. A playback only ever writes bytes, sets registers and calls
//! addresses inside the emulated machine, and [`PlaybackDef::build`] and [`ProbeDef::build`]
//! refuse anything outside the limits below, so a song database file cannot make the player
//! do anything else, or run for long.

use serde::Deserialize;

use crate::playback::{Playback, Rate, Reg, Step, Target, Value, MAX_UPDATES_PER_FRAME};
use crate::probe::{ChannelProbe, Field, SongCheck};

/// Most songs one game may list (including those of `song_range`).
pub const MAX_SONGS: usize = 512;
/// Most steps in one of `setup`, `start` and `update`.
pub const MAX_STEPS: usize = 64;
/// Most updates the `frames` steps of `setup` and `start` may run together.
pub const MAX_FRAMES: u32 = 300;
/// Most cases of one `call_by` step.
pub const MAX_CASES: usize = 256;

/// The unusable region where host-initiated calls return to; never a routine.
const RETURN_REGION: std::ops::RangeInclusive<u16> = 0xFEA0..=0xFEFF;

/// A number in a step: a constant, `"song"`, `"song_bank"` or `{ ram = address }`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ValueDef {
    Const(u16),
    Name(String),
    Ram { ram: u16 },
}

/// One step. Exactly one of `write`, `call`, `call_by`, `frames` and `bank` says what it does;
/// `bank` may also come with `call` or `call_by` to switch banks right before the call.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepDef {
    pub write: Option<u16>,
    pub value: Option<ValueDef>,
    pub bank: Option<ValueDef>,
    pub call: Option<u16>,
    /// Call the routine of `cases` (pairs of byte value and address) matching the byte at
    /// this address, or `default`.
    pub call_by: Option<u16>,
    pub cases: Option<Vec<(u8, u16)>>,
    pub default: Option<u16>,
    pub frames: Option<u16>,
    pub a: Option<ValueDef>,
    pub b: Option<ValueDef>,
    pub c: Option<ValueDef>,
    pub d: Option<ValueDef>,
    pub e: Option<ValueDef>,
    pub h: Option<ValueDef>,
    pub l: Option<ValueDef>,
    pub bc: Option<ValueDef>,
    pub de: Option<ValueDef>,
    pub hl: Option<ValueDef>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaybackDef {
    /// `"vblank"` (the default) or `{ timer = { tac = .., tma = .. } }`.
    #[serde(default)]
    pub rate: Option<RateDef>,
    #[serde(default)]
    pub setup: Vec<StepDef>,
    pub start: Vec<StepDef>,
    pub update: Vec<StepDef>,
}

/// How often the update runs: once per frame (`"vblank"`) or at the rate the timer registers
/// TAC and TMA would give the timer interrupt.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum RateDef {
    Name(String),
    Timer { timer: TimerDef },
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimerDef {
    pub tac: u8,
    pub tma: u8,
}

/// A per-channel value: at `at + channel * step`, one byte (or a word with `size = 2`), of
/// which only the bits of `mask` count.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDef {
    pub at: u16,
    pub step: u16,
    #[serde(default)]
    pub size: Option<u8>,
    #[serde(default)]
    pub mask: Option<u16>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckDef {
    pub song_id: FieldDef,
    /// Every active channel must play the song, not just one.
    #[serde(default)]
    pub all: bool,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProbeDef {
    pub active: FieldDef,
    pub address: FieldDef,
    #[serde(rename = "return")]
    pub return_address: FieldDef,
    #[serde(rename = "loop")]
    pub loop_count: FieldDef,
    pub flags: FieldDef,
    pub timer: FieldDef,
    #[serde(default)]
    pub check: Option<CheckDef>,
}

impl ValueDef {
    /// `bits` is the width of the destination; constants must fit into it.
    fn build(&self, bits: u32) -> Result<Value, String> {
        match self {
            ValueDef::Const(v) if bits < 16 && *v >> bits != 0 => {
                Err(format!("{v:#X} does not fit into {bits} bits"))
            }
            ValueDef::Const(v) => Ok(Value::Const(*v)),
            ValueDef::Name(n) if n == "song" => Ok(Value::Song),
            ValueDef::Name(n) if n == "song_bank" => Ok(Value::SongBank),
            ValueDef::Name(n) => Err(format!(
                "unknown value \"{n}\" (expected a number, \"song\", \"song_bank\" or {{ ram = address }})"
            )),
            ValueDef::Ram { ram } => Ok(Value::Ram(*ram)),
        }
    }
}

fn routine(addr: u16) -> Result<u16, String> {
    if RETURN_REGION.contains(&addr) {
        return Err(format!("${addr:04X} cannot be called"));
    }
    Ok(addr)
}

impl StepDef {
    /// The steps this one stands for, appended to `out`. `frames` counts the updates that
    /// `frames` steps run.
    fn build(&self, out: &mut Vec<Step>, frames: Option<&mut u32>) -> Result<(), String> {
        let kinds = [
            self.write.is_some(),
            self.call.is_some(),
            self.call_by.is_some(),
            self.frames.is_some(),
        ]
        .iter()
        .filter(|&&k| k)
        .count();
        let call = self.call.is_some() || self.call_by.is_some();
        if kinds > 1 || (kinds == 0 && self.bank.is_none()) {
            return Err("a step needs exactly one of write, call, call_by, frames or bank".into());
        }
        if self.bank.is_some() && kinds == 1 && !call {
            return Err("bank can only be combined with call or call_by".into());
        }
        let regs = [
            (Reg::A, &self.a, 8),
            (Reg::B, &self.b, 8),
            (Reg::C, &self.c, 8),
            (Reg::D, &self.d, 8),
            (Reg::E, &self.e, 8),
            (Reg::H, &self.h, 8),
            (Reg::L, &self.l, 8),
            (Reg::Bc, &self.bc, 16),
            (Reg::De, &self.de, 16),
            (Reg::Hl, &self.hl, 16),
        ];
        if !call && regs.iter().any(|(_, v, _)| v.is_some()) {
            return Err("registers can only be set for call or call_by".into());
        }
        if self.write.is_none() && self.value.is_some() {
            return Err("value belongs to write".into());
        }
        if self.call_by.is_none() && (self.cases.is_some() || self.default.is_some()) {
            return Err("cases and default belong to call_by".into());
        }

        if let Some(bank) = &self.bank {
            out.push(Step::Bank(bank.build(8)?));
        }
        if let Some(addr) = self.write {
            let value = self.value.as_ref().ok_or("write needs a value")?;
            out.push(Step::Write {
                addr,
                value: value.build(8)?,
            });
        }
        if let Some(n) = self.frames {
            let frames = frames.ok_or("updates cannot run frames")?;
            *frames += n as u32;
            if *frames > MAX_FRAMES {
                return Err(format!("more than {MAX_FRAMES} frames"));
            }
            out.push(Step::Frames(n));
        }
        let target = match (self.call, self.call_by) {
            (Some(addr), _) => Some(Target::Fixed(routine(addr)?)),
            (None, Some(ram)) => {
                let cases = self.cases.as_deref().unwrap_or_default();
                if cases.len() > MAX_CASES {
                    return Err(format!("more than {MAX_CASES} cases"));
                }
                Some(Target::ByRam {
                    ram,
                    cases: cases
                        .iter()
                        .map(|&(v, addr)| routine(addr).map(|a| (v, a)))
                        .collect::<Result<_, _>>()?,
                    default: routine(self.default.ok_or("call_by needs a default")?)?,
                })
            }
            (None, None) => None,
        };
        if let Some(target) = target {
            let regs = regs
                .iter()
                .filter_map(|(reg, v, bits)| v.as_ref().map(|v| v.build(*bits).map(|v| (*reg, v))))
                .collect::<Result<_, _>>()?;
            out.push(Step::Call { target, regs });
        }
        Ok(())
    }
}

fn build_steps(
    name: &str,
    defs: &[StepDef],
    mut frames: Option<&mut u32>,
) -> Result<Vec<Step>, String> {
    if defs.len() > MAX_STEPS {
        return Err(format!("{name}: more than {MAX_STEPS} steps"));
    }
    let mut steps = Vec::new();
    for (i, def) in defs.iter().enumerate() {
        def.build(&mut steps, frames.as_deref_mut())
            .map_err(|e| format!("{name} step {}: {e}", i + 1))?;
    }
    Ok(steps)
}

impl PlaybackDef {
    pub fn build(&self) -> Result<Playback, String> {
        let rate = match &self.rate {
            None => Rate::VBlank,
            Some(RateDef::Name(n)) if n == "vblank" => Rate::VBlank,
            Some(RateDef::Name(n)) => {
                return Err(format!(
                    "unknown rate \"{n}\" (expected \"vblank\" or {{ timer = {{ tac, tma }} }})"
                ))
            }
            Some(RateDef::Timer { timer }) => Rate::timer(timer.tac, timer.tma).ok_or(format!(
                "the timer must be on (TAC bit 2) and run at most {MAX_UPDATES_PER_FRAME} updates per frame"
            ))?,
        };
        let mut frames = 0;
        let setup = build_steps("setup", &self.setup, Some(&mut frames))?;
        let start = build_steps("start", &self.start, Some(&mut frames))?;
        let update = build_steps("update", &self.update, None)?;
        if start.is_empty() || update.is_empty() {
            return Err("start and update need at least one step".into());
        }
        Ok(Playback {
            rate,
            setup,
            start,
            update,
        })
    }
}

impl FieldDef {
    fn build(&self) -> Result<Field, String> {
        let field = match self.size.unwrap_or(1) {
            1 => Field::byte(self.at, self.step),
            2 => Field::word(self.at, self.step),
            s => return Err(format!("size must be 1 or 2, not {s}")),
        };
        Ok(match self.mask {
            Some(mask) => field.masked(mask),
            None => field,
        })
    }
}

impl ProbeDef {
    pub fn build(&self) -> Result<ChannelProbe, String> {
        let field = |name: &str, f: &FieldDef| f.build().map_err(|e| format!("probe {name}: {e}"));
        Ok(ChannelProbe {
            active: field("active", &self.active)?,
            address: field("address", &self.address)?,
            return_address: field("return", &self.return_address)?,
            loop_count: field("loop", &self.loop_count)?,
            flags: field("flags", &self.flags)?,
            note_timer: field("timer", &self.timer)?,
            check: self
                .check
                .map(|c| {
                    Ok::<_, String>(SongCheck {
                        song_id: field("check.song_id", &c.song_id)?,
                        all: c.all,
                    })
                })
                .transpose()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playback(toml_text: &str) -> Result<Playback, String> {
        toml::from_str::<PlaybackDef>(toml_text)
            .map_err(|e| e.to_string())?
            .build()
    }

    #[test]
    fn steps_build_in_order() {
        let p = playback(
            r#"
            start = [
                { write = 0xFF26, value = 0x80 },
                { bank = "song_bank", call = 0x4000, a = "song", de = 0x1234 },
                { frames = 2 },
            ]
            update = [{ call_by = 0xC0EF, cases = [[2, 0x4000], [8, 0x455F]], default = 0x4417 }]
            "#,
        )
        .unwrap();
        assert_eq!(
            p.start,
            [
                Step::Write {
                    addr: 0xFF26,
                    value: Value::Const(0x80)
                },
                Step::Bank(Value::SongBank),
                Step::Call {
                    target: Target::Fixed(0x4000),
                    regs: vec![(Reg::A, Value::Song), (Reg::De, Value::Const(0x1234))],
                },
                Step::Frames(2),
            ]
        );
        assert_eq!(
            p.update,
            [Step::Call {
                target: Target::ByRam {
                    ram: 0xC0EF,
                    cases: vec![(2, 0x4000), (8, 0x455F)],
                    default: 0x4417
                },
                regs: Vec::new(),
            }]
        );
    }

    #[test]
    fn bad_steps_are_refused() {
        let update = "update = [{ call = 0x4100 }]\n";
        for (start, why) in [
            ("[]", "no start"),
            ("[{ write = 0xC000 }]", "write without value"),
            (
                "[{ write = 0xC000, value = 0x100 }]",
                "value too big for a byte",
            ),
            (
                "[{ write = 0xC000, value = 1, call = 0x4000 }]",
                "two kinds",
            ),
            (
                "[{ bank = 1, write = 0xC000, value = 1 }]",
                "bank with write",
            ),
            ("[{ a = 1 }]", "registers without a call"),
            ("[{ call = 0x4000, a = 0x100 }]", "too big for A"),
            ("[{ call = 0x4000, a = \"songs\" }]", "unknown value"),
            ("[{ call = 0xFEA0 }]", "call into the return region"),
            ("[{ call_by = 0xC000 }]", "call_by without default"),
            (
                "[{ call = 0x4000, default = 1 }]",
                "default without call_by",
            ),
            ("[{ frames = 301 }]", "too many frames"),
            (
                "[{ frames = 200 }, { frames = 101 }]",
                "too many frames together",
            ),
            ("[{ call = 0x4000, pc = 1 }]", "unknown field"),
            ("[{ call = 0x10000 }]", "address out of range"),
        ] {
            let text = format!("start = {start}\n{update}");
            assert!(playback(&text).is_err(), "{why}: {text}");
        }
        assert!(playback("start = [{ call = 0x4000 }]\nupdate = [{ frames = 1 }]").is_err());
        assert!(playback("start = [{ call = 0x4000 }]\nupdate = []").is_err());
        assert!(
            playback("rate = \"often\"\nstart = [{ call = 0x4000 }]\nupdate = [{ call = 1 }]")
                .is_err()
        );
        let many = vec!["{ call = 0x4000 }"; MAX_STEPS + 1].join(", ");
        assert!(playback(&format!("start = [{many}]\n{update}")).is_err());
    }

    #[test]
    fn rates() {
        let steps = "start = [{ call = 0x4000 }]
update = [{ call = 0x4100 }]
";
        let rate = |r: &str| {
            playback(&format!(
                "rate = {r}
{steps}"
            ))
            .map(|p| p.rate)
        };
        assert_eq!(rate("\"vblank\""), Ok(Rate::VBlank));
        assert_eq!(
            rate("{ timer = { tac = 0x04, tma = 0xC0 } }"),
            Ok(Rate::Timer { period: 64 * 1024 })
        );
        assert!(rate("{ timer = { tac = 0x00, tma = 0xC0 } }").is_err());
        assert!(rate("{ timer = { tac = 0x05, tma = 0xFF } }").is_err());
        assert!(rate("{ timer = { tac = 0x04 } }").is_err());
    }

    #[test]
    fn probe_fields() {
        let probe: ProbeDef = toml::from_str(
            r#"
            active = { at = 0xC104, step = 0x32, mask = 1 }
            address = { at = 0xC107, step = 0x32, size = 2 }
            return = { at = 0xC109, step = 0x32, size = 2 }
            loop = { at = 0xC119, step = 0x32 }
            flags = { at = 0xC104, step = 0x32, mask = 6 }
            timer = { at = 0xC116, step = 0x32 }
            check = { song_id = { at = 0xC101, step = 0x32, size = 2 } }
            "#,
        )
        .unwrap();
        let built = probe.build().unwrap();
        assert_eq!(built.active, Field::byte(0xC104, 0x32).masked(1));
        assert_eq!(built.address, Field::word(0xC107, 0x32));
        assert!(!built.check.unwrap().all);
        let bad = ProbeDef {
            active: FieldDef {
                size: Some(3),
                ..probe.active
            },
            ..probe
        };
        assert!(bad.build().is_err());
    }
}
