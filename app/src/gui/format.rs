//! Text shown for playback positions and song lengths.

use gb_music::{frames_to_secs, SongLength};

pub fn fmt_time(frames: u32) -> String {
    let s = frames_to_secs(frames);
    format!("{}:{:04.1}", (s / 60.0) as u32, s % 60.0)
}

pub fn describe(length: Option<&SongLength>) -> String {
    match length {
        None => "解析中…".into(),
        Some(SongLength::Loops { intro, period }) => {
            format!(
                "イントロ {} / ループ {}",
                fmt_time(*intro),
                fmt_time(*period)
            )
        }
        Some(SongLength::Ends { at }) => format!("{} で終了", fmt_time(*at)),
        Some(SongLength::Unknown) => "ループ不明".into(),
    }
}
