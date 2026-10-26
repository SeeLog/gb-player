//! Plays the music of Game Boy games by running their own sound engine on an emulated
//! machine, GBS-style: the game's "play song" routine is called once, then its per-frame
//! update routine is called 59.7 times per second while the APU renders the result.

pub mod driver;
mod names;
pub mod pattern;
pub mod player;

pub use driver::{Driver, Engine, Song};
pub use player::{analyze, frames_to_secs, LoadError, PlayState, Player, SongLength, FRAME_RATE};
