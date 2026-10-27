//! The data sent to the web UI; `ui/src/lib/types.ts` mirrors these shapes.

use std::collections::HashMap;

use gb_music::{frames_to_secs, Profile, SongLength};
use serde::{Deserialize, Serialize};

use crate::settings::Shuffle;
use crate::themes::Theme;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LengthView {
    Loops { intro: f64, period: f64 },
    Ends { at: f64 },
    Unknown,
}

impl From<SongLength> for LengthView {
    fn from(l: SongLength) -> LengthView {
        match l {
            SongLength::Loops { intro, period } => LengthView::Loops {
                intro: frames_to_secs(intro),
                period: frames_to_secs(period),
            },
            SongLength::Ends { at } => LengthView::Ends {
                at: frames_to_secs(at),
            },
            SongLength::Unknown => LengthView::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SongView {
    pub index: usize,
    pub title: String,
    /// `None` while the song is still being analysed.
    pub length: Option<LengthView>,
}

/// An open ROM and its songs.
#[derive(Debug, Clone, Serialize)]
pub struct RomView {
    /// The ROM's [hash](super::playlists::hash), which stays the same between runs.
    pub id: String,
    pub game: String,
    pub title: String,
    pub path: String,
    pub songs: Vec<SongView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LanguageView {
    pub code: &'static str,
    pub name: &'static str,
}

/// A song of a playlist, with the names it had when it was added (or the current ones when
/// its ROM is loaded).
#[derive(Debug, Clone, Serialize)]
pub struct EntryView {
    pub game: String,
    pub title: String,
    pub length: Option<LengthView>,
    pub path: String,
    /// Whether the ROM file is not where the playlist says.
    pub missing: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaylistView {
    pub id: String,
    pub name: String,
    pub entries: Vec<EntryView>,
}

/// The playlist being played and the position in it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QueueView {
    pub playlist: String,
    pub position: usize,
}

/// Everything about the open ROMs and the settings, sent when any of it changes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryView {
    pub language: String,
    pub languages: Vec<LanguageView>,
    pub text: HashMap<String, String>,
    /// In the order they were opened.
    pub roms: Vec<RomView>,
    /// The ROM whose songs were last shown (on startup, the one shown when the app was closed).
    pub shown_rom: Option<String>,
    pub loop_count: Option<u32>,
    pub auto_next: bool,
    pub shuffle: Shuffle,
    pub volume: f32,
    pub profile: Profile,
    pub speaker_noise: bool,
    /// The volume of each channel, from 0 to 1.
    pub channel_volumes: [f32; 4],
    pub muted: [bool; 4],
    pub theme: String,
    pub themes: Vec<Theme>,
    pub playlists: Vec<PlaylistView>,
    /// The playlist that was shown when the app was last closed, to show it again.
    pub open_playlist: Option<String>,
    /// Whether the window is the mini player.
    pub mini: bool,
    /// Whether the mini player stays above other windows.
    pub mini_on_top: bool,
    pub error: Option<String>,
}

/// The playback state, sent on every tick.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickView {
    /// The id of the ROM the current song is from.
    pub rom: Option<String>,
    pub current: Option<usize>,
    /// Set while a playlist is playing.
    pub queue: Option<QueueView>,
    /// `none` (no ROM), `stopped`, `playing`, `paused`, `finished` or `failed`.
    pub state: &'static str,
    pub failure: Option<String>,
    /// Seconds since the song started.
    pub position: f64,
    pub length: LengthView,
    pub levels: [f32; 4],
    /// The latest output samples, oldest first.
    pub scope: Vec<f32>,
}
