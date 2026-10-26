//! Text shown for playback positions and song lengths.

use gb_music::{frames_to_secs, SongLength};

use crate::i18n::Text;

pub fn fmt_time(frames: u32) -> String {
    let s = frames_to_secs(frames);
    format!("{}:{:04.1}", (s / 60.0) as u32, s % 60.0)
}

pub fn describe(t: &Text, length: Option<&SongLength>) -> String {
    match length {
        None => t.get("analyzing").to_string(),
        Some(SongLength::Loops { intro, period }) => t.fmt(
            "length_loops",
            &[("intro", &fmt_time(*intro)), ("loop", &fmt_time(*period))],
        ),
        Some(SongLength::Ends { at }) => t.fmt("length_ends", &[("at", &fmt_time(*at))]),
        Some(SongLength::Unknown) => t.get("length_unknown").to_string(),
    }
}
