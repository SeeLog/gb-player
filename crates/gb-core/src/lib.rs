//! A minimal Game Boy / Game Boy Color core: enough of the machine to run a game's sound
//! engine and render its output.

pub mod apu;
pub mod cartridge;
pub mod cpu;
pub mod machine;

pub use apu::{Apu, CPU_CLOCK};
pub use cartridge::{Cartridge, CartridgeError, Header, MbcKind};
pub use cpu::{Bus, Cpu};
pub use machine::{CallError, Machine, CYCLES_PER_FRAME};
