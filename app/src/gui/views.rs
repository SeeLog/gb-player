//! The data sent to the web UI; `ui/src/lib/types.ts` mirrors these shapes.

use std::collections::HashMap;

use gb_music::{frames_to_secs, SongLength};
use serde::Serialize;

use crate::themes::Theme;

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
pub struct LanguageView {
    pub code: &'static str,
    pub name: &'static str,
}

/// Everything about the loaded ROM and the settings, sent when any of it changes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryView {
    pub language: String,
    pub languages: Vec<LanguageView>,
    pub text: HashMap<String, String>,
    pub game: String,
    pub title: String,
    pub rom_path: Option<String>,
    pub songs: Vec<SongView>,
    pub loop_count: Option<u32>,
    pub auto_next: bool,
    pub volume: f32,
    pub muted: [bool; 4],
    pub theme: String,
    pub themes: Vec<Theme>,
    pub error: Option<String>,
}

/// The playback state, sent on every tick.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickView {
    pub current: Option<usize>,
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
