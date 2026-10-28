//! Plays the music of Game Boy games by running their own sound engine on an emulated
//! machine, GBS-style: the game's "play song" routine is called once, then its per-frame
//! update routine is called 59.7 times per second while the APU renders the result.

pub mod analysis;
pub mod config;
pub mod driver;
pub mod effects;
pub mod gbs;
pub mod length;
pub mod pattern;
pub mod playback;
pub mod player;
pub mod probe;
pub mod songdb;
pub mod track;

pub use analysis::{analyze, ANALYSIS_FRAMES};
pub use driver::{Driver, Engine, Song};
pub use effects::Profile;
pub use gb_core::read_rom;
pub use length::{frames_to_secs, SongLength, FRAME_RATE};
pub use player::{Failure, LoadError, PlayState, Player, Seek};
pub use track::{is_audio, Track};
