//! Rendering songs to WAV files, for `--wav` and the GUI's export: the mix, and optionally one
//! file per APU channel (stems) that line up sample for sample and add up to the mix.

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gb_music::{
    analyze, is_audio, read_rom, track, PlayState, Player, Profile, Song, SongLength, Track,
    ANALYSIS_FRAMES, FRAME_RATE,
};

pub const RATE: u32 = 44_100;

/// Longest export of a song.
const MAX_SECS: u32 = 60 * 12;

/// Export of a song whose loop was not found.
const UNKNOWN_SECS: u32 = 180;

/// The loop count of an export when the player loops forever.
pub const DEFAULT_LOOPS: u32 = 2;

/// Where the songs of an export come from: a ROM (or GBS file) read once, or an audio file.
pub enum Source {
    Rom(Arc<[u8]>),
    Audio(Box<Track>),
}

impl Source {
    pub fn open(path: &Path) -> Result<Source, String> {
        if is_audio(path) {
            let t = track::probe(path).map_err(|e| format!("{}: {e}", path.display()))?;
            return Ok(Source::Audio(Box::new(t)));
        }
        let rom = read_rom(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Source::Rom(rom.into()))
    }

    /// A fresh player of the songs.
    pub fn player(&self) -> Result<Player, String> {
        match self {
            Source::Rom(rom) => Player::new(rom.to_vec(), RATE).map_err(|e| e.to_string()),
            Source::Audio(t) => Ok(Player::files(vec![(**t).clone()], RATE)),
        }
    }
}

/// The length of `song`: analysed for a ROM, as the file says for an audio file.
pub fn length_of(player: &Player, song: &Song) -> Result<SongLength, String> {
    match (player.rom(), player.driver()) {
        (Some(rom), Some(driver)) => {
            analyze(&rom, driver, song, ANALYSIS_FRAMES).map_err(|e| e.to_string())
        }
        _ => Ok(song.length.unwrap_or(SongLength::Unknown)),
    }
}

/// How the songs sound in the files.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub loops: u32,
    pub profile: Profile,
    /// The console's own noise, with the GBC speaker.
    pub noise: bool,
}

/// What one file of a song holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Mix,
    /// One APU channel (pulse 1, pulse 2, wave, noise), the others muted.
    Channel(usize),
    /// The console's noise alone, which belongs to no channel: with the channels it adds up to
    /// the mix.
    Console,
}

impl Part {
    /// The parts of a song with stems: the mix, the four channels and, when the mix has it, the
    /// console's noise.
    pub fn with_stems(options: &Options) -> Vec<Part> {
        let mut parts = vec![Part::Mix];
        parts.extend((0..4).map(Part::Channel));
        if options.profile == Profile::GbcSpeaker && options.noise {
            parts.push(Part::Console);
        }
        parts
    }

    /// The key of its name in the UI text, after the song's; `None` for the mix.
    pub fn text_key(self) -> Option<&'static str> {
        match self {
            Part::Mix => None,
            Part::Channel(c) => Some(
                [
                    "channel_pulse1",
                    "channel_pulse2",
                    "channel_wave",
                    "channel_noise",
                ][c],
            ),
            Part::Console => Some("export_console_noise"),
        }
    }
}

/// `01 Title.wav`, or `01 Title - Wave.wav` for a part: the number padded to `width` digits,
/// and characters that cannot be in a file name made safe.
pub fn file_name(number: usize, width: usize, title: &str, part: Option<&str>) -> String {
    let safe = |s: &str| -> String {
        let s: String = s
            .chars()
            .map(|c| match c {
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
                c if c.is_control() => '_',
                c => c,
            })
            .collect();
        // Long names stay well under the 255 a file name may have.
        let s: String = s.chars().take(120).collect();
        // Windows drops dots and spaces at the end of a name.
        s.trim().trim_end_matches('.').trim_end().to_string()
    };
    let words: Vec<String> = [Some(safe(title)), part.map(safe)]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect();
    if words.is_empty() {
        format!("{number:0width$}.wav")
    } else {
        format!("{number:0width$} {}.wav", words.join(" - "))
    }
}

#[derive(Debug, PartialEq)]
pub enum Error {
    Cancelled,
    Failed(String),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Failed(e.to_string())
    }
}

/// A rendered song.
#[derive(Debug, PartialEq)]
pub struct Rendered {
    pub secs: f64,
}

/// About how many seconds the song plays for with `loops`, for the progress.
fn expected_secs(length: SongLength, loops: u32, fade_secs: f32) -> f64 {
    let secs = match length {
        SongLength::Loops { intro, period } => {
            (intro as f64 + period as f64 * loops.max(1) as f64) / FRAME_RATE + fade_secs as f64
        }
        SongLength::Ends { at } => at as f64 / FRAME_RATE + 1.0,
        SongLength::Unknown => UNKNOWN_SECS as f64,
    };
    secs.clamp(1.0, MAX_SECS as f64)
}

/// Render song `index` into a file per part, from players made by `player`: one for each part,
/// played side by side so that every file has the same length. Each file is written next to
/// its path and moved there once complete; on an error or a cancel none is left behind.
/// `progress` gets the fraction done, from 0 to 1.
pub fn render(
    player: &dyn Fn() -> Result<Player, String>,
    index: usize,
    length: SongLength,
    options: &Options,
    files: &[(Part, PathBuf)],
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f64),
) -> Result<Rendered, Error> {
    let mut players = Vec::new();
    for &(part, _) in files {
        let mut p = player().map_err(Error::Failed)?;
        if part != Part::Mix && p.driver().is_none() {
            return Err(Error::Failed("an audio file has no channels".into()));
        }
        // The channels without the console's noise, which has a file of its own.
        p.set_profile(
            options.profile,
            options.noise && !matches!(part, Part::Channel(_)),
        );
        for c in 0..4 {
            p.set_muted(c, !matches!(part, Part::Mix) && part != Part::Channel(c));
        }
        p.play(index);
        p.loop_count = Some(options.loops);
        p.set_length(index, length);
        players.push(p);
    }
    let temp: Vec<PathBuf> = files.iter().map(|(_, p)| part_path(p)).collect();
    let result = render_into(&mut players, &temp, length, options, cancel, progress);
    match &result {
        Ok(_) => {
            for (t, (_, path)) in temp.iter().zip(files) {
                if let Err(e) = std::fs::rename(t, path) {
                    remove(&temp);
                    return Err(Error::Failed(format!("{}: {e}", path.display())));
                }
            }
        }
        Err(_) => remove(&temp),
    }
    result
}

fn part_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

fn remove(paths: &[PathBuf]) {
    for p in paths {
        let _ = std::fs::remove_file(p);
    }
}

fn render_into(
    players: &mut [Player],
    paths: &[PathBuf],
    length: SongLength,
    options: &Options,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f64),
) -> Result<Rendered, Error> {
    let mut writers = paths
        .iter()
        .map(|p| WavWriter::create(p))
        .collect::<Result<Vec<_>, _>>()?;
    let no_loop = length == SongLength::Unknown;
    let limit = RATE as usize * if no_loop { UNKNOWN_SECS } else { MAX_SECS } as usize;
    let expected = expected_secs(length, options.loops, players[0].fade_secs);
    let mut frames = 0;
    let mut chunk = vec![0.0f32; 4096];
    // The first player sets the length; the others play the same song.
    while players[0].state() == PlayState::Playing && frames < limit {
        if cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        for (p, w) in players.iter_mut().zip(&mut writers) {
            p.render(&mut chunk);
            w.write(&chunk)?;
        }
        frames += chunk.len() / 2;
        progress((frames as f64 / RATE as f64 / expected).min(1.0));
    }
    if let Some(PlayState::Failed(e)) = players
        .iter()
        .map(Player::state)
        .find(|s| matches!(s, PlayState::Failed(_)))
    {
        return Err(Error::Failed(e.to_string()));
    }
    for w in writers {
        w.finish()?;
    }
    Ok(Rendered {
        secs: frames as f64 / RATE as f64,
    })
}

/// A 16-bit stereo WAV file written as the samples come; the sizes go into the header at the
/// end.
struct WavWriter {
    file: BufWriter<File>,
    bytes: u32,
}

impl WavWriter {
    fn create(path: &Path) -> std::io::Result<WavWriter> {
        let mut file = BufWriter::new(File::create(path)?);
        file.write_all(&header(0))?;
        Ok(WavWriter { file, bytes: 0 })
    }

    fn write(&mut self, samples: &[f32]) -> std::io::Result<()> {
        for s in samples {
            self.file
                .write_all(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())?;
        }
        self.bytes += samples.len() as u32 * 2;
        Ok(())
    }

    fn finish(mut self) -> std::io::Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&header(self.bytes))?;
        self.file.flush()
    }
}

fn header(data_len: u32) -> Vec<u8> {
    let mut h = Vec::with_capacity(44);
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&(36 + data_len).to_le_bytes());
    h.extend_from_slice(b"WAVEfmt ");
    h.extend_from_slice(&16u32.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes()); // PCM
    h.extend_from_slice(&2u16.to_le_bytes()); // stereo
    h.extend_from_slice(&RATE.to_le_bytes());
    h.extend_from_slice(&(RATE * 4).to_le_bytes());
    h.extend_from_slice(&4u16.to_le_bytes());
    h.extend_from_slice(&16u16.to_le_bytes());
    h.extend_from_slice(b"data");
    h.extend_from_slice(&data_len.to_le_bytes());
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_numbered_and_safe() {
        assert_eq!(file_name(1, 2, "Title", None), "01 Title.wav");
        assert_eq!(file_name(7, 3, "a/b: c?", None), "007 a_b_ c_.wav");
        assert_eq!(
            file_name(12, 2, "Route 1", Some("Wave")),
            "12 Route 1 - Wave.wav"
        );
        assert_eq!(file_name(3, 2, "  end... ", None), "03 end.wav");
        assert_eq!(file_name(3, 2, "", Some("Noise")), "03 Noise.wav");
        assert_eq!(file_name(3, 2, "", None), "03.wav");
        assert_eq!(
            file_name(4, 2, "ポケモン\tセンター", None),
            "04 ポケモン_センター.wav"
        );
    }

    #[test]
    fn the_header_counts_the_data() {
        let path = std::env::temp_dir().join(format!("gb-player-wav-{}.wav", std::process::id()));
        let mut w = WavWriter::create(&path).unwrap();
        w.write(&[0.5, -0.5, 1.5, 0.0]).unwrap();
        w.finish().unwrap();
        let bytes = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(bytes.len(), 44 + 8);
        assert_eq!(&bytes[4..8], &(36u32 + 8).to_le_bytes());
        assert_eq!(&bytes[40..44], &8u32.to_le_bytes());
        assert_eq!(&bytes[48..50], &32767i16.to_le_bytes());
    }
}
