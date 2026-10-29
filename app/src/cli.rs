//! Command-line helpers for checking a ROM without the GUI.
//!
//!   gb-player --info <rom>                      list detected songs and their loop points
//!   gb-player --wav <rom> <song#> <out.wav> [loops] [line|gbc-speaker] [stems]
//!                                               render a song to a WAV file, from the line out
//!                                               (the default) or a GBC's speaker; `stems`
//!                                               adds a file per channel
//!
//! An audio file can stand in for the ROM, as a ROM with that one song.
//!
//! `--lang <code>` before the command (handled in main.rs) picks the language of the output
//! and the song names; the default is the OS language.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use gb_music::songdb::Names;
use gb_music::{frames_to_secs, Player, Profile, SongLength, DEFAULT_FADE_SECS};

use crate::export::{self, length_of, Options, Part, Source};
use crate::i18n::Text;

pub fn run(args: &[OsString], language: &str) -> i32 {
    let a: Vec<String> = args
        .iter()
        .map(|s| s.to_string_lossy().into_owned())
        .collect();
    let t = Text::new(language);
    let result = match a.first().map(String::as_str) {
        Some("--info") if a.len() == 2 => info(&t, &a[1]),
        Some("--wav") if (4..=7).contains(&a.len()) => wav(&t, &a[1], &a[2], &a[3], &a[4..]),
        _ => Err(t.get("usage").to_string()),
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

/// A player of `source`, and its names in the output language.
fn load(t: &Text, source: &Source) -> Result<(Player, Names), String> {
    let player = source.player()?;
    if let Source::Audio(track) = source {
        let names = Names {
            game: track.album.clone().unwrap_or_default(),
            songs: vec![track.title.clone()],
        };
        return Ok((player, names));
    }
    let driver = player.driver().expect("a ROM's player has a driver");
    let (names, warnings) = Names::new(driver, t.language());
    for warning in warnings {
        eprintln!("{warning}");
    }
    Ok((player, names))
}

/// Pad `s` to `width` terminal columns, counting East Asian wide characters as two.
fn pad(s: &str, width: usize) -> String {
    let wide = |c: char| {
        matches!(c as u32,
            0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF
            | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6)
    };
    let used: usize = s.chars().map(|c| if wide(c) { 2 } else { 1 }).sum();
    format!("{s}{}", " ".repeat(width.saturating_sub(used)))
}

fn secs(frames: u32) -> String {
    format!("{:.2}", frames_to_secs(frames))
}

fn info(t: &Text, path: &str) -> Result<(), String> {
    let (player, names) = load(t, &Source::open(Path::new(path))?)?;
    println!("{} ({})", names.game, player.title().unwrap_or(path));
    if let Some(driver) = player.driver() {
        println!("engine: {:04X?}", driver.engine);
    }
    for song in player.songs() {
        let length = length_of(&player, song);
        let desc = match length {
            Ok(SongLength::Loops { intro, period }) => t.fmt(
                "info_loops",
                &[("intro", &secs(intro)), ("loop", &secs(period))],
            ),
            Ok(SongLength::Ends { at }) => t.fmt("info_ends", &[("at", &secs(at))]),
            Ok(SongLength::Unknown) => t.get("info_unknown").to_string(),
            Err(e) => t.fmt("info_error", &[("error", &e)]),
        };
        println!(
            "{:3}  id=${:02X} bank=${:02X}  {} {}",
            song.index + 1,
            song.id,
            song.bank,
            pad(names.song(song.index), 32),
            desc
        );
    }
    Ok(())
}

fn wav(t: &Text, path: &str, song: &str, out: &str, rest: &[String]) -> Result<(), String> {
    let mut options = Options {
        loops: 2,
        profile: Profile::Line,
        noise: true,
        fade_secs: DEFAULT_FADE_SECS,
    };
    let mut stems = false;
    for arg in rest {
        match arg.as_str() {
            "line" => options.profile = Profile::Line,
            "gbc-speaker" => options.profile = Profile::GbcSpeaker,
            "stems" => stems = true,
            n if n.starts_with(|c: char| c.is_ascii_digit()) => {
                options.loops = n.parse().map_err(|_| t.get("err_loops"))?
            }
            _ => return Err(t.get("err_wav_arg").to_string()),
        }
    }
    let source = Source::open(Path::new(path))?;
    let (player, names) = load(t, &source)?;
    let index = song
        .parse::<usize>()
        .map_err(|_| t.get("err_song_number"))?
        .checked_sub(1)
        .ok_or(t.get("err_song_start"))?;
    let s = player
        .songs()
        .get(index)
        .ok_or(t.get("err_no_song"))?
        .clone();
    if stems && player.driver().is_none() {
        return Err(t.get("err_stems_audio").to_string());
    }
    let length = length_of(&player, &s)?;
    let parts = if stems {
        Part::with_stems(&options)
    } else {
        vec![Part::Mix]
    };
    // The stems go next to the mix: `out - Pulse 1.wav`.
    let out = PathBuf::from(out);
    let stem = out.file_stem().unwrap_or_default().to_string_lossy();
    let files: Vec<(Part, PathBuf)> = parts
        .iter()
        .map(|&part| match part.text_key() {
            None => (part, out.clone()),
            Some(key) => (
                part,
                out.with_file_name(format!("{stem} - {}.wav", t.get(key))),
            ),
        })
        .collect();
    if length == SongLength::Unknown {
        eprintln!("{}", t.get("wav_no_loop"));
    }
    let rendered = export::render(
        &|| source.player(),
        index,
        length,
        &options,
        &files,
        &AtomicBool::new(false),
        &mut |_| {},
    )
    .map_err(|e| match e {
        export::Error::Failed(e) => t.fmt("err_playback", &[("error", &e)]),
        export::Error::Cancelled => String::new(),
    })?;
    let secs = format!("{:.1}", rendered.secs);
    for (_, path) in &files {
        println!(
            "{}",
            t.fmt(
                "wav_written",
                &[
                    ("title", &names.song(index)),
                    ("secs", &secs),
                    ("path", &path.display())
                ]
            )
        );
    }
    Ok(())
}
