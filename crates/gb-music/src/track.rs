//! Audio files (WAV, FLAC, MP3, Ogg Vorbis, AAC/ALAC in MP4, AIFF), played like the songs of a
//! ROM: a folder of them is opened like a ROM, and [`TrackDecoder`] turns one into stereo samples at the
//! output's rate, for the same effects as the APU's output.

use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use symphonia::core::codecs::audio::{AudioCodecParameters, AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTag, Tag};
use symphonia::core::units::{Duration, Time, TimeBase, Timestamp};

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
    /// How long it plays, in frames: up to its loop and then the loop, or to its end;
    /// [`SongLength::Unknown`] when the file does not say.
    pub length: SongLength,
    /// The loop written in the file, if any.
    pub looping: Option<Loop>,
}

/// A part of a file that repeats, in samples from its start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Loop {
    pub start: u64,
    /// The first sample after the loop.
    pub end: u64,
}

impl Loop {
    /// `start..end`, if it is a loop within the file's `n_frames` (an end past them is the
    /// file's end).
    fn new(start: u64, end: u64, n_frames: Option<u64>) -> Option<Loop> {
        let end = n_frames.map_or(end, |n| end.min(n));
        (start < end).then_some(Loop { start, end })
    }
}

/// Read the tags and the length of an audio file.
pub fn probe(path: &Path) -> Result<Track, TrackError> {
    let (mut format, tags) = open(path)?;
    let (track, n_frames, time_base, sample_rate) = {
        let (t, params) = default_track(format.as_ref())?;
        (t.id, t.num_frames, t.time_base, params.sample_rate)
    };
    let n_frames = match n_frames {
        Some(n) => Some(n),
        // Count the frames of every packet, without decoding them.
        None => {
            let mut n = 0;
            loop {
                match format.next_packet() {
                    Ok(Some(p)) if p.track_id == track => n += p.dur.get(),
                    Ok(Some(_)) => {}
                    Ok(None) | Err(_) => break,
                }
            }
            (n > 0).then_some(n)
        }
    };
    let secs = match (time_base, sample_rate, n_frames) {
        (Some(tb), _, Some(n)) => Some(tb.calc_duration_saturating(Duration::new(n)).as_secs_f64()),
        (None, Some(rate), Some(n)) => Some(n as f64 / rate as f64),
        _ => None,
    };
    let tags: Vec<&Tag> = tags
        .iter()
        .flat_map(|r| {
            let tracks = r.per_track.iter().flat_map(|t| &t.metadata.tags);
            r.media.tags.iter().chain(tracks)
        })
        .collect();
    // The value of the first tag of a kind.
    macro_rules! tag {
        ($kind:ident) => {
            tags.iter().find_map(|t| match &t.std {
                Some(StandardTag::$kind(value)) => Some(value),
                _ => None,
            })
        };
    }
    let text = |s: Option<&Arc<String>>| {
        let s = s?.trim();
        (!s.is_empty()).then(|| s.to_string())
    };
    let custom = |name: &str| {
        tags.iter()
            .find(|t| match t.raw.sub_fields.as_deref() {
                // ID3v2's user text frames have their name in a description.
                Some(fields) if t.raw.key == "TXXX" => fields
                    .iter()
                    .any(|f| f.value.to_string().eq_ignore_ascii_case(name)),
                _ => t.raw.key.eq_ignore_ascii_case(name),
            })
            .and_then(|t| t.raw.value.to_string().trim().parse::<u64>().ok())
    };
    // RPG Maker's tags, which other tools use too: LOOPSTART, and LOOPLENGTH or LOOPEND;
    // without either, the loop runs to the end of the file. A WAV file may have a sampler
    // loop instead.
    let looping = match custom("LOOPSTART") {
        Some(start) => {
            let end = match (custom("LOOPLENGTH"), custom("LOOPEND")) {
                (Some(len), _) => start.saturating_add(len),
                (None, Some(end)) => end,
                (None, None) => n_frames.unwrap_or(0),
            };
            Loop::new(start, end, n_frames)
        }
        None => wav_loop(path).and_then(|l| Loop::new(l.start, l.end, n_frames)),
    }
    .filter(|_| sample_rate.is_some());
    let length = match (looping, sample_rate, secs) {
        (Some(l), Some(rate), _) => {
            let frames = |samples: u64| (samples as f64 / rate as f64 * FRAME_RATE).round() as u32;
            let intro = frames(l.start);
            SongLength::Loops {
                intro,
                period: frames(l.end).saturating_sub(intro).max(1),
            }
        }
        (_, _, Some(s)) if s > 0.0 => SongLength::Ends {
            at: (s * FRAME_RATE).round() as u32,
        },
        _ => SongLength::Unknown,
    };
    let number = |n: Option<&u64>| u32::try_from(*n?).ok();
    Ok(Track {
        path: path.to_path_buf(),
        title: text(tag!(TrackTitle)).unwrap_or_else(|| {
            path.file_stem()
                .map_or_else(String::new, |s| s.to_string_lossy().into_owned())
        }),
        album: text(tag!(Album)),
        artist: text(tag!(Artist)).or_else(|| text(tag!(AlbumArtist))),
        number: number(tag!(TrackNumber)),
        disc: number(tag!(DiscNumber)),
        length,
        looping,
    })
}

/// The first loop of a WAV file's sampler chunk (`smpl`).
fn wav_loop(path: &Path) -> Option<Loop> {
    if !path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
    {
        return None;
    }
    let mut f = File::open(path).ok()?;
    let mut riff = [0u8; 12];
    f.read_exact(&mut riff).ok()?;
    if &riff[..4] != b"RIFF" || &riff[8..] != b"WAVE" {
        return None;
    }
    let u32_at = |b: &[u8], at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
    };
    loop {
        let mut head = [0u8; 8];
        f.read_exact(&mut head).ok()?;
        let size = u32_at(&head, 4)?;
        if &head[..4] != b"smpl" {
            // Chunks are padded to an even size.
            f.seek(SeekFrom::Current(size as i64 + (size & 1) as i64))
                .ok()?;
            continue;
        }
        let mut chunk = vec![0u8; size.min(4096) as usize];
        f.read_exact(&mut chunk).ok()?;
        // Nine fields, the number of loops among them, then the loops: an id, a type, the
        // first and the last sample, ...
        if u32_at(&chunk, 28)? == 0 {
            return None;
        }
        let (start, last) = (u32_at(&chunk, 44)?, u32_at(&chunk, 48)?);
        return Some(Loop {
            start: start as u64,
            end: last as u64 + 1,
        });
    }
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

/// The time stamp `secs` seconds in.
fn stamp_of(tb: TimeBase, secs: f64) -> Option<u64> {
    let ts = tb.calc_timestamp(Time::try_from_secs_f64(secs)?)?;
    Some(ts.get().max(0) as u64)
}

/// The file's container, and the tags found in it and in front of it (ID3v2).
fn open(path: &Path) -> Result<(Box<dyn FormatReader>, Vec<MetadataRevision>), TrackError> {
    let source = MediaSourceStream::new(Box::new(File::open(path)?), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe().probe(
        &hint,
        source,
        FormatOptions::default(),
        MetadataOptions::default(),
    )?;
    let mut metadata = format.metadata();
    let mut tags: Vec<_> = std::iter::from_fn(|| metadata.pop()).collect();
    tags.extend(metadata.current().cloned());
    Ok((format, tags))
}

fn default_track(
    format: &dyn FormatReader,
) -> Result<(&symphonia::core::formats::Track, &AudioCodecParameters), TrackError> {
    format
        .default_track(TrackType::Audio)
        .and_then(|t| Some((t, t.codec_params.as_ref()?.audio()?)))
        .ok_or_else(|| TrackError("no audio track".into()))
}

/// Decodes an audio file into interleaved stereo samples at the output's rate.
pub struct TrackDecoder {
    path: PathBuf,
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track: u32,
    time_base: Option<TimeBase>,
    sample_rate: Option<u32>,
    out_rate: u32,
    /// The loop, in time stamps: from its end, decoding goes on from its start.
    looping: Option<Loop>,
    /// Frames decoded since the start, or since the last jump back to the loop.
    since_jump: u64,
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
        let (t, params) = default_track(format.as_ref())?;
        let (track, time_base, sample_rate) = (t.id, t.time_base, params.sample_rate);
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(params, &AudioDecoderOptions::default())?;
        Ok(TrackDecoder {
            path: path.to_path_buf(),
            format,
            decoder,
            track,
            time_base,
            sample_rate,
            out_rate: out_rate.max(1),
            looping: None,
            since_jump: 0,
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

    /// Repeat `looping` (in samples) over and over, instead of ending.
    pub fn set_loop(&mut self, looping: Option<Loop>) {
        let ts = |samples: u64| match (self.time_base, self.sample_rate) {
            (Some(tb), Some(rate)) if (tb.numer.get(), tb.denom.get()) != (1, rate) => {
                stamp_of(tb, samples as f64 / rate as f64).unwrap_or(samples)
            }
            _ => samples,
        };
        self.looping = looping.map(|l| Loop {
            start: ts(l.start),
            end: ts(l.end),
        });
    }

    /// Continue from `secs` seconds in.
    pub fn seek(&mut self, secs: f64) -> Result<(), TrackError> {
        let secs = secs.max(0.0);
        let mut ts = self
            .time_base
            .and_then(|tb| stamp_of(tb, secs))
            .unwrap_or(0);
        // Past the loop (by rounding, or the loops played): the same point of the loop.
        if let Some(l) = self.looping.filter(|l| ts >= l.end) {
            ts = l.start + (ts - l.start) % (l.end - l.start);
        }
        self.go_to(ts)?;
        self.resampler = None;
        self.ready.clear();
        self.ended = false;
        Ok(())
    }

    /// Decode on from the time stamp `ts`; what was decoded before stays in the resampler.
    fn go_to(&mut self, ts: u64) -> Result<(), TrackError> {
        let to = SeekTo::Timestamp {
            ts: Timestamp::try_from(ts).unwrap_or(Timestamp::MAX),
            track_id: self.track,
        };
        match self.format.seek(SeekMode::Accurate, to) {
            Ok(seeked) => self.skip_to = seeked.required_ts.get().max(0) as u64,
            // A file that cannot seek is decoded again from its start, dropping what comes
            // before.
            Err(_) => {
                let fresh = TrackDecoder::open(&self.path, self.out_rate)?;
                (self.format, self.decoder) = (fresh.format, fresh.decoder);
                self.skip_to = ts;
            }
        }
        self.decoder.reset();
        self.since_jump = 0;
        Ok(())
    }

    /// Go back to the start of the loop, if there is one and it played a little since the
    /// last time (a loop that plays nothing would never end); returns whether it did.
    fn jump_back(&mut self) -> bool {
        match self.looping {
            Some(l) if self.since_jump > 0 => self.go_to(l.start).is_ok(),
            _ => false,
        }
    }

    /// The resampler has the last of the track: let it ring out.
    fn end(&mut self) {
        self.ended = true;
        if let Some(r) = &mut self.resampler {
            r.finish(&mut self.ready);
        }
    }

    /// Decode the next packet into `ready`; at the end of the file, let the resampler ring out.
    fn decode_packet(&mut self) {
        let packet = match self.format.next_packet() {
            Ok(Some(p)) => p,
            // The end of the file, a read error, or a new stream that would need a new decoder
            // (chained Ogg streams): the track ends here, unless it loops.
            Ok(None) | Err(_) => {
                if !self.jump_back() {
                    self.end();
                }
                return;
            }
        };
        if packet.track_id != self.track {
            return;
        }
        let decoded = match self.decoder.decode(&packet) {
            Ok(d) => d,
            // A damaged packet is passed over.
            Err(Error::DecodeError(_)) => return,
            Err(_) => {
                self.end();
                return;
            }
        };
        let rate = decoded.spec().rate();
        let channels = decoded.spec().channels().count().max(1);
        let frames = decoded.frames();
        if frames == 0 {
            return;
        }
        let mut samples = Vec::<f32>::new();
        decoded.copy_to_vec_interleaved(&mut samples);
        // A time stamp before 0 is the encoder's delay, which the decoder has left out.
        let ts = packet.pts.get().max(0) as u64;
        let drop = self.skip_to.saturating_sub(ts).min(frames as u64) as usize;
        // Up to the end of the loop, when the packet reaches it.
        let (first, kept) = (ts + drop as u64, (frames - drop) as u64);
        let (take, wrap) = match self.looping {
            Some(l) if first + kept >= l.end => (l.end.saturating_sub(first).min(kept), true),
            _ => (kept, false),
        };
        self.since_jump += take;
        let resampler = match &mut self.resampler {
            Some(r) if r.from == rate => r,
            r => r.insert(Resampler::new(rate, self.out_rate)),
        };
        for f in samples.chunks(channels).skip(drop).take(take as usize) {
            // Mono plays on both sides; beyond two channels, only the front left and right.
            let (l, r) = if channels == 1 {
                (f[0], f[0])
            } else {
                (f[0], f[1])
            };
            resampler.push(l, r);
        }
        resampler.drain(&mut self.ready);
        if wrap && !self.jump_back() {
            self.end();
        }
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
