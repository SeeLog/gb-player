//! Audio files (WAV, FLAC, MP3, Ogg Vorbis, AAC/ALAC in MP4, AIFF), played like the songs of a
//! ROM: a folder of them is opened like a ROM, and [`TrackDecoder`] turns one into stereo samples at the
//! output's rate, for the same effects as the APU's output.

use std::fmt;
use std::fs::File;
use std::path::{Path, PathBuf};

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTagKey};
use symphonia::core::probe::Hint;
use symphonia::core::units::{Time, TimeBase};

use crate::length::{SongLength, FRAME_RATE};

/// The extensions of the files opened as audio, in lower case.
pub const EXTENSIONS: &[&str] = &[
    "wav", "flac", "mp3", "ogg", "oga", "m4a", "mp4", "aac", "aif", "aiff",
];

/// Whether `path` is named like an audio file.
pub fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackError(String);

impl fmt::Display for TrackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TrackError {}

impl From<Error> for TrackError {
    fn from(e: Error) -> Self {
        TrackError(e.to_string())
    }
}

impl From<std::io::Error> for TrackError {
    fn from(e: std::io::Error) -> Self {
        TrackError(e.to_string())
    }
}

/// An audio file and what its tags say about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub path: PathBuf,
    /// The title tag, or else the file name.
    pub title: String,
    pub album: Option<String>,
    pub artist: Option<String>,
    number: Option<u32>,
    disc: Option<u32>,
    /// How long it plays, in frames; [`SongLength::Unknown`] when the file does not say.
    pub length: SongLength,
}

/// Read the tags and the length of an audio file.
pub fn probe(path: &Path) -> Result<Track, TrackError> {
    let (mut format, tags) = open(path)?;
    let (track, params) = {
        let t = default_track(format.as_ref())?;
        (t.id, t.codec_params.clone())
    };
    let n_frames = match params.n_frames {
        Some(n) => Some(n),
        // Count the frames of every packet, without decoding them.
        None => {
            let mut n = 0;
            loop {
                match format.next_packet() {
                    Ok(p) if p.track_id() == track => n += p.dur,
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            (n > 0).then_some(n)
        }
    };
    let secs = match (params.time_base, params.sample_rate, n_frames) {
        (Some(tb), _, Some(n)) => Some(secs_of(tb.calc_time(n))),
        (None, Some(rate), Some(n)) => Some(n as f64 / rate as f64),
        _ => None,
    };
    let tag = |key: StandardTagKey| {
        tags.iter()
            .flat_map(|r| r.tags())
            .find(|t| t.std_key == Some(key))
            .map(|t| t.value.to_string().trim().to_string())
            .filter(|s| !s.is_empty())
    };
    // "3/12" and the like: the number before the total.
    let number = |key| {
        let s = tag(key)?;
        let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    };
    Ok(Track {
        path: path.to_path_buf(),
        title: tag(StandardTagKey::TrackTitle).unwrap_or_else(|| {
            path.file_stem()
                .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
        }),
        album: tag(StandardTagKey::Album),
        artist: tag(StandardTagKey::Artist).or_else(|| tag(StandardTagKey::AlbumArtist)),
        number: number(StandardTagKey::TrackNumber),
        disc: number(StandardTagKey::DiscNumber),
        length: match secs {
            Some(s) if s > 0.0 => SongLength::Ends {
                at: (s * FRAME_RATE).round() as u32,
            },
            _ => SongLength::Unknown,
        },
    })
}

/// The audio files of a folder.
pub struct Folder {
    /// In album order: disc, track number, then file name.
    pub tracks: Vec<Track>,
    /// The audio files left out, and why.
    pub unreadable: Vec<(PathBuf, TrackError)>,
}

/// Read the audio files in `dir`.
pub fn folder(dir: &Path) -> Result<Folder, TrackError> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && is_audio(p))
        .collect();
    paths.sort();
    let mut tracks = Vec::new();
    let mut unreadable = Vec::new();
    for path in paths {
        match probe(&path) {
            Ok(t) => tracks.push(t),
            Err(e) => unreadable.push((path, e)),
        }
    }
    tracks.sort_by_key(|t| (t.disc.unwrap_or(1), t.number.unwrap_or(u32::MAX)));
    Ok(Folder { tracks, unreadable })
}

fn secs_of(t: Time) -> f64 {
    t.seconds as f64 + t.frac
}

/// The file's container, and the tags found in it and in front of it (ID3v2).
fn open(path: &Path) -> Result<(Box<dyn FormatReader>, Vec<MetadataRevision>), TrackError> {
    let source = MediaSourceStream::new(Box::new(File::open(path)?), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let options = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };
    let mut probed = symphonia::default::get_probe().format(
        &hint,
        source,
        &options,
        &MetadataOptions::default(),
    )?;
    let mut tags = Vec::new();
    if let Some(r) = probed.metadata.get().as_ref().and_then(|m| m.current()) {
        tags.push(r.clone());
    }
    if let Some(r) = probed.format.metadata().current() {
        tags.push(r.clone());
    }
    Ok((probed.format, tags))
}

fn default_track(
    format: &dyn FormatReader,
) -> Result<&symphonia::core::formats::Track, TrackError> {
    format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| TrackError("no audio track".into()))
}

/// Decodes an audio file into interleaved stereo samples at the output's rate.
pub struct TrackDecoder {
    path: PathBuf,
    format: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    track: u32,
    time_base: Option<TimeBase>,
    out_rate: u32,
    resampler: Option<Resampler>,
    /// Resampled samples not read yet.
    ready: Vec<f32>,
    /// Decoded frames before this time stamp are dropped: a seek lands on the packet holding it.
    skip_to: u64,
    ended: bool,
}

impl TrackDecoder {
    pub fn open(path: &Path, out_rate: u32) -> Result<TrackDecoder, TrackError> {
        let (format, _) = open(path)?;
        let (track, params) = {
            let t = default_track(format.as_ref())?;
            (t.id, t.codec_params.clone())
        };
        let decoder = symphonia::default::get_codecs().make(&params, &DecoderOptions::default())?;
        Ok(TrackDecoder {
            path: path.to_path_buf(),
            format,
            decoder,
            track,
            time_base: params.time_base,
            out_rate: out_rate.max(1),
            resampler: None,
            ready: Vec::new(),
            skip_to: 0,
            ended: false,
        })
    }

    /// Append up to `frames` stereo frames to `out`; returns how many, fewer only at the end.
    pub fn read(&mut self, out: &mut Vec<f32>, frames: usize) -> usize {
        while self.ready.len() < frames * 2 && !self.ended {
            self.decode_packet();
        }
        let n = self.ready.len().min(frames * 2);
        out.extend(self.ready.drain(..n));
        n / 2
    }

    /// Continue from `secs` seconds in.
    pub fn seek(&mut self, secs: f64) -> Result<(), TrackError> {
        let time = Time::from(secs.max(0.0));
        let to = SeekTo::Time {
            time,
            track_id: Some(self.track),
        };
        match self.format.seek(SeekMode::Accurate, to) {
            Ok(seeked) => self.skip_to = seeked.required_ts,
            // A file that cannot seek is decoded again from its start, dropping what comes
            // before.
            Err(_) => {
                *self = TrackDecoder::open(&self.path, self.out_rate)?;
                self.skip_to = self.time_base.map_or(0, |tb| tb.calc_timestamp(time));
            }
        }
        self.decoder.reset();
        self.resampler = None;
        self.ready.clear();
        self.ended = false;
        Ok(())
    }

    /// Decode the next packet into `ready`; at the end of the file, let the resampler ring out.
    fn decode_packet(&mut self) {
        let packet = match self.format.next_packet() {
            Ok(p) => p,
            // The end of the file, a read error, or a new stream that would need a new decoder
            // (chained Ogg streams): the track ends here.
            Err(_) => {
                self.ended = true;
                if let Some(r) = &mut self.resampler {
                    r.finish(&mut self.ready);
                }
                return;
            }
        };
        if packet.track_id() != self.track {
            return;
        }
        let decoded = match self.decoder.decode(&packet) {
            Ok(d) => d,
            // A damaged packet is passed over.
            Err(Error::DecodeError(_)) => return,
            Err(_) => {
                self.ended = true;
                return;
            }
        };
        let spec = *decoded.spec();
        let frames = decoded.frames();
        if frames == 0 {
            return;
        }
        let mut samples = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        samples.copy_interleaved_ref(decoded);
        let channels = spec.channels.count().max(1);
        let drop = self.skip_to.saturating_sub(packet.ts()).min(frames as u64) as usize;
        let resampler = match &mut self.resampler {
            Some(r) if r.from == spec.rate => r,
            r => r.insert(Resampler::new(spec.rate, self.out_rate)),
        };
        for f in samples.samples().chunks(channels).skip(drop) {
            // Mono plays on both sides; beyond two channels, only the front left and right.
            let (l, r) = if channels == 1 {
                (f[0], f[0])
            } else {
                (f[0], f[1])
            };
            resampler.push(l, r);
        }
        resampler.drain(&mut self.ready);
    }
}

/// Input frames on each side of an output frame that it is made from.
const HALF_TAPS: usize = 32;

/// Points of the kernel per input frame; it is interpolated between them.
const PHASES: usize = 512;

/// A windowed-sinc resampler for interleaved stereo.
struct Resampler {
    from: u32,
    /// Input frames per output frame; the kernel is empty when the rates are the same.
    step: f64,
    /// The kernel from 0 to [`HALF_TAPS`] input frames away, [`PHASES`] points per frame.
    kernel: Vec<f32>,
    input: Vec<[f32; 2]>,
    /// Where the next output frame falls, in input frames from the start of `input`.
    pos: f64,
}

impl Resampler {
    fn new(from: u32, to: u32) -> Resampler {
        let step = from as f64 / to.max(1) as f64;
        let mut r = Resampler {
            from,
            step,
            kernel: Vec::new(),
            // Silence before the start, so that the first frame has input on both sides.
            input: vec![[0.0; 2]; HALF_TAPS],
            pos: HALF_TAPS as f64,
        };
        if from == to {
            r.input.clear();
            return r;
        }
        // Cut off a little below the lower of the two Nyquist frequencies, as a fraction of
        // the input rate, so that the transition band stays above what can be heard.
        let cutoff = 0.5 * (1.0 / step).min(1.0) * 0.95;
        r.kernel = (0..=HALF_TAPS * PHASES + 1)
            .map(|i| {
                let d = i as f64 / PHASES as f64;
                if d >= HALF_TAPS as f64 {
                    return 0.0;
                }
                let x = std::f64::consts::PI * 2.0 * cutoff * d;
                let sinc = if x == 0.0 { 1.0 } else { x.sin() / x };
                let w = std::f64::consts::PI * d / HALF_TAPS as f64;
                let blackman = 0.42 + 0.5 * w.cos() + 0.08 * (2.0 * w).cos();
                (2.0 * cutoff * sinc * blackman) as f32
            })
            .collect();
        r
    }

    fn push(&mut self, l: f32, r: f32) {
        self.input.push([l, r]);
    }

    /// Append every output frame that the input so far makes.
    fn drain(&mut self, out: &mut Vec<f32>) {
        if self.kernel.is_empty() {
            out.extend(self.input.drain(..).flatten());
            return;
        }
        while self.pos as usize + HALF_TAPS < self.input.len() {
            let base = self.pos as usize;
            let (mut l, mut r) = (0.0f32, 0.0f32);
            for k in base + 1 - HALF_TAPS..=base + HALF_TAPS {
                let at = (k as f64 - self.pos).abs() * PHASES as f64;
                let i = at as usize;
                let frac = (at - i as f64) as f32;
                let h = self.kernel[i] + (self.kernel[i + 1] - self.kernel[i]) * frac;
                l += self.input[k][0] * h;
                r += self.input[k][1] * h;
            }
            out.push(l);
            out.push(r);
            self.pos += self.step;
        }
        // Keep what the next output frame needs.
        let used = (self.pos as usize + 1).saturating_sub(HALF_TAPS);
        self.input.drain(..used);
        self.pos -= used as f64;
    }

    /// Let the last input ring out into `out`.
    fn finish(&mut self, out: &mut Vec<f32>) {
        if self.kernel.is_empty() {
            return;
        }
        for _ in 0..HALF_TAPS {
            self.push(0.0, 0.0);
        }
        self.drain(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
        let mut r = Resampler::new(from, to);
        let mut out = Vec::new();
        for &s in input {
            r.push(s, -s);
            r.drain(&mut out);
        }
        r.finish(&mut out);
        out
    }

    #[test]
    fn the_same_rate_passes_samples_through() {
        let input: Vec<f32> = (0..100).map(|i| (i as f32 * 0.1).sin()).collect();
        let out = resample(&input, 48_000, 48_000);
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        assert_eq!(left, input);
    }

    #[test]
    fn resampling_keeps_the_length_and_a_tone() {
        let (from, to) = (44_100, 48_000);
        let hz = 1000.0;
        let input: Vec<f32> = (0..from)
            .map(|i| (std::f64::consts::TAU * hz * i as f64 / from as f64).sin() as f32 * 0.5)
            .collect();
        let out = resample(&input, from, to);
        let frames = out.len() / 2;
        assert!((frames as i64 - to as i64).abs() <= 1, "{frames}");
        // Away from the edges, the output is the same tone at the new rate, on both sides.
        for i in 1000..frames - 1000 {
            let want = (std::f64::consts::TAU * hz * i as f64 / to as f64).sin() as f32 * 0.5;
            assert!(
                (out[2 * i] - want).abs() < 2e-3,
                "{i}: {} vs {want}",
                out[2 * i]
            );
            assert_eq!(out[2 * i + 1], -out[2 * i]);
        }
    }

    #[test]
    fn downsampling_removes_what_the_new_rate_cannot_hold() {
        let (from, to) = (96_000, 48_000);
        // 30 kHz would fold back to 18 kHz.
        let input: Vec<f32> = (0..from)
            .map(|i| (std::f64::consts::TAU * 30_000.0 * i as f64 / from as f64).sin() as f32)
            .collect();
        let out = resample(&input, from, to);
        let peak = out[2000..out.len() - 2000]
            .iter()
            .fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak < 1e-3, "{peak}");
    }

    #[test]
    fn audio_files_are_told_by_their_extension() {
        assert!(is_audio(Path::new("a/b.MP3")));
        assert!(is_audio(Path::new("song.flac")));
        assert!(!is_audio(Path::new("game.gb")));
        assert!(!is_audio(Path::new("noext")));
    }
}
