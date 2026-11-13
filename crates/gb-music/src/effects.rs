//! Effects on the APU's output: what happens to the sound on its way out of the console. A
//! [`Profile`] picks where it comes out (the line out as the APU makes it, or a real console's
//! headphone jack or speaker) and builds the [`Chain`] for it. The filters come before the fade
//! and the volume; the console's own noise comes after them, as it does after the console's
//! volume wheel: it is as loud whatever the volume.
//!
//! The GBC was measured on a real Game Boy Color. A test ROM's sine steps from the headphone jack,
//! against the same ROM rendered here, give the chip's side: a first-order high-pass at 323 Hz,
//! within 0.7 dB. Songs of Pokémon Crystal and Red recorded from the speaker with a microphone,
//! less that high-pass, give the speaker's: its filters fit within about 1 dB from 280 Hz to
//! 14 kHz (below, the speaker is lost in the room noise). The noise is the tones that a silent
//! console added to the room's own noise, at the jack and at the speaker.

use std::f64::consts::TAU;

use serde::{Deserialize, Serialize};

/// The machine clock over the T-cycles of a frame: the LCD's refresh rate.
const FRAME_HZ: f64 = 4_194_304.0 / 70_224.0;

/// Where the sound comes out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Profile {
    /// The APU's output as it is.
    #[default]
    Line,
    /// The speaker of a Game Boy Color.
    GbcSpeaker,
    /// The headphone jack of a Game Boy Color.
    GbcHeadphones,
}

/// One stage of a [`Chain`].
pub trait Effect: Send {
    /// Process interleaved stereo samples in place.
    fn process(&mut self, samples: &mut [f32]);
    /// Forget the sound so far, so that nothing of it rings on into what comes next.
    fn reset(&mut self) {}
}

/// The effects of a [`Profile`]: the filters, applied in order, and the console's noise.
pub struct Chain {
    effects: Vec<Box<dyn Effect>>,
    noise: Option<Interference>,
}

impl Chain {
    /// The chain of `profile` at `sample_rate`; `noise` adds the console's own hum and whine.
    pub fn new(profile: Profile, sample_rate: u32, noise: bool) -> Chain {
        let fs = sample_rate.max(1) as f64;
        let mut effects: Vec<Box<dyn Effect>> = Vec::new();
        let tones = match profile {
            Profile::Line => &[][..],
            Profile::GbcSpeaker => {
                effects.push(Box::new(Biquad::new(
                    FilterKind::HighPass1,
                    GBC_CHIP_HIGHPASS_HZ,
                    0.0,
                    0.0,
                    fs,
                )));
                effects.push(Box::new(Mono));
                for &(kind, hz, q, db) in GBC_SPEAKER_FILTERS {
                    effects.push(Box::new(Biquad::new(kind, hz, q, db, fs)));
                }
                effects.push(Box::new(Gain(db_to_gain(GBC_SPEAKER_GAIN_DB))));
                GBC_SPEAKER_NOISE
            }
            Profile::GbcHeadphones => {
                effects.push(Box::new(Biquad::new(
                    FilterKind::HighPass1,
                    GBC_CHIP_HIGHPASS_HZ,
                    0.0,
                    0.0,
                    fs,
                )));
                GBC_HEADPHONE_NOISE
            }
        };
        let noise = (noise && !tones.is_empty()).then(|| Interference::new(tones, fs));
        Chain { effects, noise }
    }

    /// The filters, before the fade and the volume.
    pub fn process(&mut self, samples: &mut [f32]) {
        for e in &mut self.effects {
            e.process(samples);
        }
    }

    /// Add the console's noise, after the fade and the volume.
    pub fn add_noise(&mut self, samples: &mut [f32]) {
        if let Some(n) = &mut self.noise {
            n.process(samples);
        }
    }

    pub fn reset(&mut self) {
        for e in &mut self.effects {
            e.reset();
        }
    }
}

fn db_to_gain(db: f64) -> f32 {
    10f64.powf(db / 20.0) as f32
}

/// Both channels the same: what a single speaker plays.
struct Mono;

impl Effect for Mono {
    fn process(&mut self, samples: &mut [f32]) {
        for f in samples.as_chunks_mut::<2>().0 {
            let m = (f[0] + f[1]) * 0.5;
            f[0] = m;
            f[1] = m;
        }
    }
}

struct Gain(f32);

impl Effect for Gain {
    fn process(&mut self, samples: &mut [f32]) {
        for s in samples {
            *s *= self.0;
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum FilterKind {
    /// First order: its Q and gain are not used.
    HighPass1,
    HighPass,
    LowPass,
    /// A bell that raises or lowers the band around its frequency.
    Peak,
}

/// A second-order filter (the RBJ cookbook's), on both channels.
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    /// Transposed direct form II state, per channel.
    z: [[f32; 2]; 2],
}

impl Biquad {
    fn new(kind: FilterKind, hz: f64, q: f64, db: f64, fs: f64) -> Biquad {
        // A filter must stay under the Nyquist frequency, however low the output rate.
        let w = TAU * hz.min(fs * 0.45) / fs;
        let (cw, sw) = (w.cos(), w.sin());
        let alpha = sw / (2.0 * q);
        let amp = 10f64.powf(db / 40.0);
        let (b, a) = match kind {
            // By the bilinear transform.
            FilterKind::HighPass1 => {
                let k = (w / 2.0).tan();
                ([1.0, -1.0, 0.0], [1.0 + k, k - 1.0, 0.0])
            }
            FilterKind::HighPass => (
                [(1.0 + cw) / 2.0, -(1.0 + cw), (1.0 + cw) / 2.0],
                [1.0 + alpha, -2.0 * cw, 1.0 - alpha],
            ),
            FilterKind::LowPass => (
                [(1.0 - cw) / 2.0, 1.0 - cw, (1.0 - cw) / 2.0],
                [1.0 + alpha, -2.0 * cw, 1.0 - alpha],
            ),
            FilterKind::Peak => (
                [1.0 + alpha * amp, -2.0 * cw, 1.0 - alpha * amp],
                [1.0 + alpha / amp, -2.0 * cw, 1.0 - alpha / amp],
            ),
        };
        Biquad {
            b: b.map(|v| (v / a[0]) as f32),
            a: [(a[1] / a[0]) as f32, (a[2] / a[0]) as f32],
            z: [[0.0; 2]; 2],
        }
    }
}

impl Effect for Biquad {
    fn process(&mut self, samples: &mut [f32]) {
        for f in samples.as_chunks_mut::<2>().0 {
            for (s, z) in f.iter_mut().zip(&mut self.z) {
                let x = *s;
                let y = self.b[0] * x + z[0];
                z[0] = self.b[1] * x - self.a[0] * y + z[1];
                z[1] = self.b[2] * x - self.a[1] * y;
                *s = y;
            }
        }
    }

    fn reset(&mut self) {
        self.z = [[0.0; 2]; 2];
    }
}

/// A tone of the console's interference: a harmonic of the frame rate, or a frequency of its
/// own when `harmonic` is 0.
struct Tone {
    harmonic: u32,
    hz: f64,
    amplitude: f32,
}

const fn harmonic(harmonic: u32, amplitude: f32) -> Tone {
    Tone {
        harmonic,
        hz: 0.0,
        amplitude,
    }
}

const fn tone(hz: f64, amplitude: f32) -> Tone {
    Tone {
        harmonic: 0,
        hz,
        amplitude,
    }
}

/// Steady tones mixed into both channels, as the console's electronics leak into its speaker.
struct Interference {
    /// Each tone as a rotating phasor: (cos, sin) now, and the step per sample.
    phasors: Vec<([f64; 2], [f64; 2], f32)>,
    /// Samples since the phasors were last put back on the unit circle.
    since_norm: u32,
}

impl Interference {
    fn new(tones: &[Tone], fs: f64) -> Interference {
        let phasors = tones
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                let hz = if t.harmonic > 0 {
                    t.harmonic as f64 * FRAME_HZ
                } else {
                    t.hz
                };
                (hz < fs * 0.45).then(|| {
                    // Fixed but unrelated starting phases, so the tones do not line up into a
                    // click once a frame.
                    let phase = (i as f64 * 0.618_034).fract() * TAU;
                    let step = TAU * hz / fs;
                    (
                        [phase.cos(), phase.sin()],
                        [step.cos(), step.sin()],
                        t.amplitude,
                    )
                })
            })
            .collect();
        Interference {
            phasors,
            since_norm: 0,
        }
    }
}

impl Effect for Interference {
    fn process(&mut self, samples: &mut [f32]) {
        for f in samples.as_chunks_mut::<2>().0 {
            let mut n = 0.0f32;
            for (p, step, amp) in &mut self.phasors {
                n += p[1] as f32 * *amp;
                *p = [
                    p[0] * step[0] - p[1] * step[1],
                    p[0] * step[1] + p[1] * step[0],
                ];
            }
            f[0] += n;
            f[1] += n;
        }
        // Rounding slowly changes the phasors' length; keep it at 1.
        self.since_norm += (samples.len() / 2) as u32;
        if self.since_norm >= 4096 {
            self.since_norm = 0;
            for (p, _, _) in &mut self.phasors {
                let len = (p[0] * p[0] + p[1] * p[1]).sqrt();
                *p = [p[0] / len, p[1] / len];
            }
        }
    }
}

/// The chip's own high-pass, measured at the headphone jack (where the APU here, as a DMG's, has
/// one at 28 Hz).
const GBC_CHIP_HIGHPASS_HZ: f64 = 323.0;

/// The speaker's response, after the chip's high-pass: (kind, Hz, Q, dB).
/// `tools/speaker_measure.py fit --chip-hp 323` prints these.
const GBC_SPEAKER_FILTERS: &[(FilterKind, f64, f64, f64)] = &[
    (FilterKind::HighPass, 755.0, 2.000, 0.0),
    (FilterKind::HighPass, 180.0, 1.000, 0.0),
    (FilterKind::Peak, 1121.3, 1.370, 7.18),
    (FilterKind::Peak, 1839.2, 4.000, -2.82),
    (FilterKind::Peak, 2702.5, 0.693, 9.26),
    (FilterKind::Peak, 3673.3, 3.650, 15.0),
    (FilterKind::Peak, 6430.0, 4.000, 5.34),
    (FilterKind::Peak, 8003.9, 4.000, 8.18),
    (FilterKind::LowPass, 10113.4, 2.257, 0.0),
];

/// Keeps the loudest moments about as loud as the line out's: the speaker's resonance at
/// 3.7 kHz makes the peaks stand out, so it sounds a little quieter than the line out overall.
const GBC_SPEAKER_GAIN_DB: f64 = -13.5;

/// The tones heard from the GBC's speaker with nothing playing, at their level at the output
/// with the music at full volume; `tools/speaker_measure.py noise` prints them. Most are
/// harmonics of the 59.7 Hz frame rate, gathered around the LCD's line rate (154 lines a frame,
/// 9198 Hz), which is by far the loudest of them.
const GBC_SPEAKER_NOISE: &[Tone] = &[
    harmonic(11, 6.65e-4),
    harmonic(22, 2.42e-4),
    harmonic(58, 1.11e-4),
    harmonic(62, 1.45e-4),
    harmonic(101, 1.08e-4),
    harmonic(103, 1.78e-4),
    harmonic(104, 2.13e-4),
    harmonic(105, 1.20e-4),
    harmonic(108, 9.70e-5),
    harmonic(125, 1.05e-4),
    harmonic(126, 1.05e-4),
    harmonic(127, 1.29e-4),
    harmonic(130, 1.56e-4),
    harmonic(131, 1.22e-4),
    harmonic(134, 1.30e-4),
    harmonic(139, 1.12e-4),
    harmonic(142, 1.33e-4),
    harmonic(144, 1.34e-4),
    harmonic(145, 1.61e-4),
    harmonic(146, 1.12e-4),
    harmonic(148, 1.38e-4),
    harmonic(149, 9.77e-5),
    harmonic(150, 1.18e-4),
    harmonic(151, 1.29e-4),
    harmonic(152, 2.08e-4),
    harmonic(153, 2.04e-4),
    harmonic(154, 1.73e-3),
    harmonic(155, 1.04e-4),
    harmonic(157, 1.54e-4),
    harmonic(158, 1.62e-4),
    harmonic(159, 1.02e-4),
    harmonic(160, 1.37e-4),
    harmonic(161, 2.03e-4),
    harmonic(162, 1.15e-4),
    harmonic(164, 1.61e-4),
    harmonic(165, 1.28e-4),
    harmonic(308, 1.66e-4),
    // Not harmonics of the frame rate. They came with the game cartridges (Crystal's and Red's)
    // and not with a test ROM on a flash cart: the game's, not the console's.
    tone(584.0, 4.55e-3),
    tone(3800.0, 2.31e-4),
    tone(3859.8, 4.12e-4),
    tone(4569.2, 2.22e-4),
    tone(4629.0, 2.13e-4),
];

/// The tones at the GBC's headphone jack with nothing playing, likewise. The frame rate's low
/// harmonics, which the speaker cannot play, are its buzz.
const GBC_HEADPHONE_NOISE: &[Tone] = &[
    harmonic(6, 7.37e-4),
    harmonic(7, 5.98e-4),
    harmonic(8, 4.81e-4),
    harmonic(9, 3.89e-4),
    harmonic(10, 3.01e-4),
    harmonic(11, 2.36e-4),
    harmonic(12, 1.71e-4),
    harmonic(13, 1.22e-4),
    harmonic(14, 8.72e-5),
    harmonic(15, 4.97e-5),
    harmonic(16, 1.79e-5),
    harmonic(18, 1.86e-5),
    harmonic(19, 3.63e-5),
    harmonic(20, 4.30e-5),
    harmonic(21, 4.78e-5),
    harmonic(22, 5.53e-5),
    harmonic(23, 5.52e-5),
    harmonic(24, 5.24e-5),
    harmonic(25, 4.88e-5),
    harmonic(26, 4.76e-5),
    harmonic(27, 3.90e-5),
    harmonic(28, 3.41e-5),
    harmonic(29, 2.83e-5),
    harmonic(30, 2.36e-5),
    harmonic(31, 1.39e-5),
    harmonic(35, 1.22e-5),
    harmonic(36, 1.28e-5),
    harmonic(37, 1.99e-5),
    harmonic(38, 2.01e-5),
    harmonic(39, 2.31e-5),
    harmonic(40, 2.36e-5),
    harmonic(41, 2.09e-5),
    harmonic(42, 2.45e-5),
    harmonic(43, 2.10e-5),
    harmonic(44, 1.83e-5),
    harmonic(45, 2.15e-5),
    harmonic(46, 1.38e-5),
    harmonic(47, 1.24e-5),
    harmonic(48, 1.36e-5),
    harmonic(57, 2.38e-5),
    harmonic(91, 2.28e-5),
    harmonic(145, 3.68e-5),
    harmonic(146, 3.54e-5),
    harmonic(147, 5.19e-5),
    harmonic(148, 4.33e-5),
    harmonic(149, 5.88e-5),
    harmonic(150, 6.04e-5),
    harmonic(151, 6.23e-5),
    harmonic(152, 5.76e-5),
    harmonic(153, 5.79e-5),
    harmonic(154, 1.27e-3),
    harmonic(155, 6.26e-5),
    harmonic(156, 6.06e-5),
    harmonic(157, 7.15e-5),
    harmonic(158, 6.72e-5),
    harmonic(159, 5.55e-5),
    harmonic(160, 5.74e-5),
    harmonic(161, 4.68e-5),
    harmonic(162, 4.52e-5),
    harmonic(163, 3.61e-5),
    harmonic(164, 3.39e-5),
    harmonic(165, 3.02e-5),
    harmonic(166, 2.67e-5),
    harmonic(167, 2.25e-5),
    harmonic(168, 1.80e-5),
    harmonic(177, 1.96e-5),
    harmonic(178, 1.31e-5),
    harmonic(179, 1.42e-5),
    harmonic(181, 1.41e-5),
    harmonic(182, 1.03e-5),
    harmonic(194, 1.07e-5),
    harmonic(197, 1.05e-5),
    harmonic(308, 1.66e-4),
    tone(507.8, 2.15e-5),
    tone(13707.8, 1.74e-5),
    tone(13767.5, 5.70e-5),
    tone(13827.2, 5.54e-5),
    tone(13887.0, 1.66e-5),
];

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// Peak level of `chain`'s output for a sine of `hz` on the left channel only, after it
    /// settles.
    fn sine_gain(chain: &mut Chain, hz: f64) -> f32 {
        let n = RATE as usize;
        let mut s: Vec<f32> = (0..n)
            .flat_map(|i| [(TAU * hz * i as f64 / RATE as f64).sin() as f32, 0.0])
            .collect();
        chain.process(&mut s);
        s[n..].iter().fold(0.0f32, |m, v| m.max(v.abs()))
    }

    #[test]
    fn line_leaves_the_sound_alone() {
        let mut chain = Chain::new(Profile::Line, RATE, true);
        let mut s = vec![0.25, -0.5, 0.0, 1.0];
        chain.process(&mut s);
        assert_eq!(s, [0.25, -0.5, 0.0, 1.0]);
    }

    /// The level of `hz` in the left channel of `s`, by correlating with it.
    fn level(s: &[f32], hz: f64) -> f64 {
        let (mut c, mut si) = (0.0f64, 0.0f64);
        let left: Vec<f32> = s.iter().step_by(2).copied().collect();
        for (i, &v) in left.iter().enumerate() {
            let w = TAU * hz * i as f64 / RATE as f64;
            c += v as f64 * w.cos();
            si += v as f64 * w.sin();
        }
        2.0 * (c * c + si * si).sqrt() / left.len() as f64
    }

    #[test]
    fn speaker_plays_both_channels_as_one() {
        let mut chain = Chain::new(Profile::GbcSpeaker, RATE, false);
        let mut s: Vec<f32> = (0..2000)
            .flat_map(|i| [((i / 20) % 2) as f32, 0.0])
            .collect();
        chain.process(&mut s);
        assert!(s.chunks(2).all(|f| f[0] == f[1]));
        assert!(s.iter().any(|&v| v != 0.0));
    }

    #[test]
    fn headphones_keep_the_two_channels_apart() {
        let mut chain = Chain::new(Profile::GbcHeadphones, RATE, false);
        let mut s: Vec<f32> = (0..2000)
            .flat_map(|i| [((i / 20) % 2) as f32, 0.0])
            .collect();
        chain.process(&mut s);
        assert!(s.chunks(2).all(|f| f[1] == 0.0));
        assert!(s.chunks(2).any(|f| f[0] != 0.0));
    }

    #[test]
    fn speaker_follows_the_measured_response() {
        let db = |hz| {
            let mut chain = Chain::new(Profile::GbcSpeaker, RATE, false);
            // The left channel alone reaches the speaker at half level.
            20.0 * (sine_gain(&mut chain, hz) * 2.0).log10()
        };
        let mid = db(1500.0);
        // Measured, relative to 1-2 kHz: about -20 dB at 400 Hz, +12 dB at 3.7 kHz, -11 dB at
        // 12.5 kHz; and nothing to speak of in the bass.
        for (hz, want) in [(400.0, -20.0), (3700.0, 12.0), (12500.0, -11.0)] {
            assert!(
                (db(hz) - mid - want).abs() < 4.0,
                "{hz} Hz: {}",
                db(hz) - mid
            );
        }
        assert!(db(100.0) - mid < -40.0);
    }

    #[test]
    fn headphones_lose_the_bass_to_the_chips_high_pass() {
        let db = |hz| {
            let mut chain = Chain::new(Profile::GbcHeadphones, RATE, false);
            20.0 * sine_gain(&mut chain, hz).log10()
        };
        // Measured at the jack: -9.8 dB at 100 Hz, -1.2 dB at 400 Hz.
        assert!((db(100.0) - db(2000.0) + 9.8).abs() < 1.0);
        assert!((db(400.0) - db(2000.0) + 1.2).abs() < 1.0);
    }

    #[test]
    fn noise_is_the_lines_whine_and_the_hum() {
        let mut chain = Chain::new(Profile::GbcSpeaker, RATE, true);
        let mut s = vec![0.0f32; RATE as usize * 2];
        chain.add_noise(&mut s);
        assert!((level(&s, 154.0 * FRAME_HZ) - 1.73e-3).abs() < 2e-4);
        assert!((level(&s, 584.0) - 4.55e-3).abs() < 5e-4);
        assert!(level(&s, 1000.0) < 1e-4);
        // Quiet: well under the music.
        assert!(s.iter().all(|v| v.abs() < 0.05));
    }

    #[test]
    fn headphones_buzz_at_the_frame_rate() {
        let mut chain = Chain::new(Profile::GbcHeadphones, RATE, true);
        let mut s = vec![0.0f32; RATE as usize * 2];
        chain.add_noise(&mut s);
        assert!((level(&s, 6.0 * FRAME_HZ) - 7.37e-4).abs() < 1e-4);
        assert!((level(&s, 154.0 * FRAME_HZ) - 1.27e-3).abs() < 2e-4);
        // Both sides alike.
        assert!(s.chunks(2).all(|f| f[0] == f[1]));
    }

    #[test]
    fn noise_comes_only_when_asked_and_not_with_the_filters() {
        for profile in [Profile::Line, Profile::GbcSpeaker, Profile::GbcHeadphones] {
            let mut s = vec![0.0f32; 4800];
            let mut chain = Chain::new(profile, RATE, false);
            chain.process(&mut s);
            chain.add_noise(&mut s);
            assert!(s.iter().all(|&v| v == 0.0), "{profile:?} without noise");
            let mut chain = Chain::new(profile, RATE, true);
            chain.process(&mut s);
            assert!(s.iter().all(|&v| v == 0.0), "{profile:?}'s filters");
        }
    }

    #[test]
    fn reset_stops_the_ringing() {
        let mut chain = Chain::new(Profile::GbcSpeaker, RATE, false);
        let mut s: Vec<f32> = (0..4800)
            .map(|i| if i % 64 < 32 { 0.5 } else { -0.5 })
            .collect();
        chain.process(&mut s);
        chain.reset();
        let mut quiet = vec![0.0f32; 64];
        chain.process(&mut quiet);
        assert!(quiet.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn low_output_rates_stay_stable() {
        for profile in [Profile::GbcSpeaker, Profile::GbcHeadphones] {
            let mut chain = Chain::new(profile, 8_000, true);
            let mut s: Vec<f32> = (0..16_000).map(|i| ((i / 7) % 2) as f32 - 0.5).collect();
            chain.process(&mut s);
            chain.add_noise(&mut s);
            assert!(s.iter().all(|v| v.is_finite() && v.abs() < 10.0));
        }
    }
}
