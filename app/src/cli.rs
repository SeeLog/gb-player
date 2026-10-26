//! Command-line helpers for checking a ROM without the GUI.
//!
//!   gb-player --info <rom>                      list detected songs and their loop points
//!   gb-player --wav <rom> <song#> <out.wav> [loops]   render a song to a WAV file
//!
//! `--lang <code>` before the command (handled in main.rs) picks the language of the output
//! and the song names; the default is the OS language.

use std::ffi::OsString;
use std::io::Write;

use gb_music::songdb::Names;
use gb_music::{analyze, frames_to_secs, PlayState, Player, SongLength, ANALYSIS_FRAMES};

use crate::i18n::Text;

const RATE: u32 = 44_100;

pub fn run(args: &[OsString], language: &str) -> i32 {
    let a: Vec<String> = args
        .iter()
        .map(|s| s.to_string_lossy().into_owned())
        .collect();
    let t = Text::new(language);
    let result = match a.first().map(String::as_str) {
        Some("--info") if a.len() == 2 => info(&t, &a[1]),
        Some("--wav") if a.len() >= 4 => wav(&t, &a[1], &a[2], &a[3], a.get(4).map(String::as_str)),
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

/// Load `path` and look up its names in the output language.
fn load(t: &Text, path: &str) -> Result<(Player, Names), String> {
    let rom = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let player = Player::new(rom, RATE).map_err(|e| e.to_string())?;
    let (names, warnings) = Names::new(player.driver(), t.language());
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
    let (player, names) = load(t, path)?;
    println!("{} ({})", names.game, player.title());
    println!("engine: {:04X?}", player.driver().engine);
    let rom = player.rom();
    for song in player.songs() {
        let length = analyze(&rom, player.driver(), song, ANALYSIS_FRAMES);
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

fn wav(t: &Text, path: &str, song: &str, out: &str, loops: Option<&str>) -> Result<(), String> {
    let (mut player, names) = load(t, path)?;
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
    let length =
        analyze(&player.rom(), player.driver(), &s, ANALYSIS_FRAMES).map_err(|e| e.to_string())?;
    player.play(index);
    player.loop_count = Some(
        loops
            .map_or(Ok(2), str::parse)
            .map_err(|_| t.get("err_loops"))?,
    );
    player.set_length(index, length);
    if length == SongLength::Unknown {
        eprintln!("{}", t.get("wav_no_loop"));
    }
    let max_samples = RATE as usize * 60 * 12;
    let mut samples = Vec::new();
    let mut chunk = vec![0.0f32; 4096];
    while player.state() == PlayState::Playing && samples.len() / 2 < max_samples {
        if length == SongLength::Unknown && samples.len() / 2 >= RATE as usize * 180 {
            break;
        }
        player.render(&mut chunk);
        samples.extend_from_slice(&chunk);
    }
    if let PlayState::Failed(e) = player.state() {
        return Err(t.fmt("err_playback", &[("error", &e)]));
    }
    write_wav(out, &samples).map_err(|e| e.to_string())?;
    let secs = format!("{:.1}", samples.len() as f64 / 2.0 / RATE as f64);
    println!(
        "{}",
        t.fmt(
            "wav_written",
            &[
                ("title", &names.song(index)),
                ("secs", &secs),
                ("path", &out)
            ]
        )
    );
    Ok(())
}

fn write_wav(path: &str, samples: &[f32]) -> std::io::Result<()> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    let data_len = (samples.len() * 2) as u32;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&2u16.to_le_bytes())?; // stereo
    f.write_all(&RATE.to_le_bytes())?;
    f.write_all(&(RATE * 4).to_le_bytes())?;
    f.write_all(&4u16.to_le_bytes())?;
    f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    for s in samples {
        f.write_all(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
    }
    f.flush()
}
