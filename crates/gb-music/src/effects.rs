//! Effects on the APU's output, between the chip and the fade and the volume: what happens to
//! the sound on its way out of the console. A [`Profile`] picks where it comes out (the line
//! out as the APU makes it, or the speaker of a real console) and builds the [`Chain`] for it.
//!
//! The GBC speaker was measured on a real Game Boy Color: songs of Pokémon Crystal and Red
//! recorded from its speaker with a microphone, against the same songs rendered here. Its filters
//! fit the ratio of the two within about 1.2 dB from 300 Hz to 14 kHz (below 300 Hz the speaker
//! is lost in the room noise), and its interference tones are those that showed up in a
//! recording of the console switched on and silent, but not with it switched off.

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
}

/// One stage of a [`Chain`].
pub trait Effect: Send {
    /// Process interleaved stereo samples in place.
    fn process(&mut self, samples: &mut [f32]);
    /// Forget the sound so far, so that nothing of it rings on into what comes next.
    fn reset(&mut self) {}
}

/// The effects of a [`Profile`], applied in order.
pub struct Chain {
    effects: Vec<Box<dyn Effect>>,
}

impl Chain {
    /// The chain of `profile` at `sample_rate`; `noise` adds the console's own hum and whine.
    pub fn new(profile: Profile, sample_rate: u32, noise: bool) -> Chain {
        let fs = sample_rate.max(1) as f64;
        let mut effects: Vec<Box<dyn Effect>> = Vec::new();
        match profile {
            Profile::Line => {}
            Profile::GbcSpeaker => {
                effects.push(Box::new(Mono));
                for &(kind, hz, q, db) in GBC_SPEAKER_FILTERS {
                    effects.push(Box::new(Biquad::new(kind, hz, q, db, fs)));
                }
                if noise {
                    effects.push(Box::new(Interference::new(GBC_INTERFERENCE, fs)));
                }
                effects.push(Box::new(Gain(db_to_gain(GBC_SPEAKER_GAIN_DB))));
            }
        }
        Chain { effects }
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        for e in &mut self.effects {
            e.process(samples);
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

/// The speaker's response, fitted to the measurement: (kind, Hz, Q, dB).
const GBC_SPEAKER_FILTERS: &[(FilterKind, f64, f64, f64)] = &[
    (FilterKind::HighPass, 904.8, 1.574, 0.0),
    (FilterKind::HighPass, 181.1, 0.400, 0.0),
    (FilterKind::Peak, 1150.0, 0.400, 6.70),
    (FilterKind::Peak, 1901.9, 2.909, -5.68),
    (FilterKind::Peak, 2169.1, 4.000, 4.16),
    (FilterKind::Peak, 3624.0, 2.977, 15.0),
    (FilterKind::Peak, 6468.5, 4.000, 3.15),
    (FilterKind::Peak, 7973.8, 4.000, 5.44),
    (FilterKind::LowPass, 9456.4, 1.906, 0.0),
];

/// Keeps the loudest moments about as loud as the line out's: the speaker's resonance at
/// 3.5 kHz makes the peaks stand out, so it sounds a little quieter than the line out overall.
const GBC_SPEAKER_GAIN_DB: f64 = -9.5;

/// The tones heard from the GBC's speaker with nothing playing, at their level relative to the
/// filtered music (before [`GBC_SPEAKER_GAIN_DB`]). Most are harmonics of the 59.7 Hz frame rate,
/// gathered around the LCD's line rate (154 lines a frame, 9198 Hz), which is by far the loudest
/// of them.
const GBC_INTERFERENCE: &[Tone] = &[
    harmonic(11, 2.02e-3),
    harmonic(22, 7.36e-4),
    harmonic(58, 3.37e-4),
    harmonic(62, 4.39e-4),
    harmonic(101, 3.27e-4),
    harmonic(103, 5.40e-4),
    harmonic(104, 6.47e-4),
    harmonic(105, 3.65e-4),
    harmonic(108, 2.95e-4),
    harmonic(125, 3.18e-4),
    harmonic(126, 3.18e-4),
    harmonic(127, 3.92e-4),
    harmonic(130, 4.75e-4),
    harmonic(131, 3.70e-4),
    harmonic(134, 3.95e-4),
    harmonic(139, 3.40e-4),
    harmonic(142, 4.02e-4),
    harmonic(144, 4.07e-4),
    harmonic(145, 4.89e-4),
    harmonic(146, 3.39e-4),
    harmonic(148, 4.19e-4),
    harmonic(149, 2.97e-4),
    harmonic(150, 3.57e-4),
    harmonic(151, 3.91e-4),
    harmonic(152, 6.32e-4),
    harmonic(153, 6.20e-4),
    harmonic(154, 5.26e-3),
    harmonic(155, 3.15e-4),
    harmonic(157, 4.66e-4),
    harmonic(158, 4.93e-4),
    harmonic(159, 3.11e-4),
    harmonic(160, 4.14e-4),
    harmonic(161, 6.16e-4),
    harmonic(162, 3.49e-4),
    harmonic(164, 4.90e-4),
    harmonic(165, 3.88e-4),
    harmonic(308, 5.03e-4),
    // Not harmonics of the frame rate; the loudest, 584 Hz, perhaps from the power supply.
    tone(584.0, 1.38e-2),
    tone(3800.0, 7.00e-4),
    tone(3859.8, 1.25e-3),
    tone(4569.2, 6.74e-4),
    tone(4629.0, 6.46e-4),
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
    fn speaker_follows_the_measured_response() {
        let db = |hz| {
            let mut chain = Chain::new(Profile::GbcSpeaker, RATE, false);
            // The left channel alone reaches the speaker at half level.
            20.0 * (sine_gain(&mut chain, hz) * 2.0).log10()
        };
        let mid = db(1500.0);
        // Measured, relative to 1-2 kHz: about -19 dB at 400 Hz, +12 dB at 3.7 kHz, -11 dB at
        // 12.5 kHz; and nothing to speak of in the bass.
        assert!((db(400.0) - mid + 19.0).abs() < 4.0, "{}", db(400.0) - mid);
        assert!(
            (db(3700.0) - mid - 12.0).abs() < 4.0,
            "{}",
            db(3700.0) - mid
        );
        assert!(
            (db(12500.0) - mid + 11.0).abs() < 4.0,
            "{}",
            db(12500.0) - mid
        );
        assert!(db(100.0) - mid < -40.0);
    }

    #[test]
    fn noise_is_the_lines_whine_and_the_hum() {
        let mut chain = Chain::new(Profile::GbcSpeaker, RATE, true);
        let mut s = vec![0.0f32; RATE as usize * 2];
        chain.process(&mut s);
        let left: Vec<f32> = s.iter().step_by(2).copied().collect();
        // The level of one frequency, by correlating with it over the whole second.
        let level = |hz: f64| {
            let (mut c, mut si) = (0.0f64, 0.0f64);
            for (i, &v) in left.iter().enumerate() {
                let w = TAU * hz * i as f64 / RATE as f64;
                c += v as f64 * w.cos();
                si += v as f64 * w.sin();
            }
            2.0 * (c * c + si * si).sqrt() / left.len() as f64
        };
        let gain = db_to_gain(GBC_SPEAKER_GAIN_DB) as f64;
        assert!((level(154.0 * FRAME_HZ) - 5.26e-3 * gain).abs() < 5e-4 * gain);
        assert!((level(584.0) - 1.38e-2 * gain).abs() < 1.4e-3 * gain);
        assert!(level(1000.0) < 1e-4);
        // Quiet: well under the music.
        assert!(left.iter().all(|v| v.abs() < 0.05));
    }

    #[test]
    fn noise_can_be_left_out() {
        let mut chain = Chain::new(Profile::GbcSpeaker, RATE, false);
        let mut s = vec![0.0f32; 4800];
        chain.process(&mut s);
        assert!(s.iter().all(|&v| v == 0.0));
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
        let mut chain = Chain::new(Profile::GbcSpeaker, 8_000, true);
        let mut s: Vec<f32> = (0..16_000).map(|i| ((i / 7) % 2) as f32 - 0.5).collect();
        chain.process(&mut s);
        assert!(s.iter().all(|v| v.is_finite() && v.abs() < 10.0));
    }
}
