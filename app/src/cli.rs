//! Command-line helpers for checking a ROM without the GUI.
//!
//!   gb-player --info <rom>                      list detected songs and their loop points
//!   gb-player --wav <rom> <song#> <out.wav> [loops]   render a song to a WAV file

use std::ffi::OsString;
use std::io::Write;

use gb_music::{analyze, frames_to_secs, PlayState, Player, SongLength, FRAME_RATE};

const RATE: u32 = 44_100;

pub fn run(args: &[OsString]) -> i32 {
    let a: Vec<String> = args
        .iter()
        .map(|s| s.to_string_lossy().into_owned())
        .collect();
    let result = match a.first().map(String::as_str) {
        Some("--info") if a.len() == 2 => info(&a[1]),
        Some("--wav") if a.len() >= 4 => wav(&a[1], &a[2], &a[3], a.get(4).map(String::as_str)),
        _ => Err("usage:\n  gb-player [rom]\n  gb-player --info <rom>\n  gb-player --wav <rom> <song#> <out.wav> [loops]".into()),
    };
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn load(path: &str) -> Result<Player, String> {
    let rom = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    Player::new(rom, RATE).map_err(|e| e.to_string())
}

fn info(path: &str) -> Result<(), String> {
    let player = load(path)?;
    println!("{} ({})", player.game(), player.title());
    println!("engine: {:04X?}", player.driver().engine);
    let rom = player.rom();
    for song in player.songs() {
        let length = analyze(&rom, player.driver(), song, (FRAME_RATE * 600.0) as u32);
        let desc = match length {
            Ok(SongLength::Loops { intro, period }) => {
                format!(
                    "intro {:.2}s, loop {:.2}s",
                    frames_to_secs(intro),
                    frames_to_secs(period)
                )
            }
            Ok(SongLength::Ends { at }) => format!("ends at {:.2}s", frames_to_secs(at)),
            Ok(SongLength::Unknown) => "no loop found".into(),
            Err(e) => format!("error: {e}"),
        };
        println!(
            "{:3}  id=${:02X} bank=${:02X}  {:32} {}",
            song.index + 1,
            song.id,
            song.bank,
            song.title,
            desc
        );
    }
    Ok(())
}

fn wav(path: &str, song: &str, out: &str, loops: Option<&str>) -> Result<(), String> {
    let mut player = load(path)?;
    let index = song
        .parse::<usize>()
        .map_err(|_| "song# must be a number")?
        .checked_sub(1)
        .ok_or("song# starts at 1")?;
    let s = player.songs().get(index).ok_or("no such song")?.clone();
    let length = analyze(
        &player.rom(),
        player.driver(),
        &s,
        (FRAME_RATE * 600.0) as u32,
    )
    .map_err(|e| e.to_string())?;
    player.play(index);
    player.loop_count = Some(
        loops
            .map_or(Ok(2), str::parse)
            .map_err(|_| "loops must be a number")?,
    );
    player.set_length(index, length);
    if length == SongLength::Unknown {
        eprintln!("loop not found; rendering 3 minutes");
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
        return Err(format!("playback failed: {e}"));
    }
    write_wav(out, &samples).map_err(|e| e.to_string())?;
    println!(
        "{}: {:.1}s written to {out}",
        s.title,
        samples.len() as f64 / 2.0 / RATE as f64
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
