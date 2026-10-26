//! Audio output: a cpal stream that pulls samples from the shared [`Player`].

use std::collections::VecDeque;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Mutex, PoisonError};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};
use gb_music::Player;

use crate::i18n::Text;

pub type SharedPlayer = Arc<Mutex<Option<Player>>>;

/// The most recent output samples as mono, for the oscilloscope.
pub type Scope = Arc<Mutex<VecDeque<f32>>>;

/// Number of samples kept in a [`Scope`].
pub const SCOPE_LEN: usize = 2048;

pub struct AudioOut {
    _stream: Stream,
    pub sample_rate: u32,
}

impl AudioOut {
    pub fn start(player: SharedPlayer, scope: Scope, t: &Text) -> Result<AudioOut, String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or(t.get("err_no_device"))?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let sample_rate = config.sample_rate;
        let stream = match format {
            SampleFormat::F32 => build::<f32>(&device, config, player, scope),
            SampleFormat::I16 => build::<i16>(&device, config, player, scope),
            SampleFormat::I32 => build::<i32>(&device, config, player, scope),
            SampleFormat::U16 => build::<u16>(&device, config, player, scope),
            f => return Err(t.fmt("err_sample_format", &[("format", &format!("{f:?}"))])),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(AudioOut {
            _stream: stream,
            sample_rate,
        })
    }
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    player: SharedPlayer,
    scope: Scope,
) -> Result<Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let mut stereo: Vec<f32> = Vec::new();
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                let frames = data.len() / channels;
                stereo.resize(frames * 2, 0.0);
                let mut guard = player.lock().unwrap_or_else(PoisonError::into_inner);
                match guard.as_mut() {
                    Some(p) => {
                        // A bug in the emulation must not take the audio thread (and with it
                        // the player lock) down; stop the song and play silence instead.
                        let rendered =
                            panic::catch_unwind(AssertUnwindSafe(|| p.render(&mut stereo)));
                        if rendered.is_err() {
                            p.stop();
                            stereo.fill(0.0);
                        }
                    }
                    None => stereo.fill(0.0),
                }
                drop(guard);
                // Never make the audio thread wait for the UI.
                if let Ok(mut s) = scope.try_lock() {
                    s.extend(stereo.chunks(2).map(|f| (f[0] + f[1]) * 0.5));
                    let excess = s.len().saturating_sub(SCOPE_LEN);
                    s.drain(..excess);
                }
                for (out, s) in data.chunks_mut(channels).zip(stereo.chunks(2)) {
                    let (l, r) = (s[0].clamp(-1.0, 1.0), s[1].clamp(-1.0, 1.0));
                    match out.len() {
                        1 => out[0] = T::from_sample((l + r) * 0.5),
                        _ => {
                            out[0] = T::from_sample(l);
                            out[1] = T::from_sample(r);
                            for o in &mut out[2..] {
                                *o = T::from_sample(0.0);
                            }
                        }
                    }
                }
            },
            |err| eprintln!("audio stream error: {err}"),
            None,
        )
        .map_err(|e| e.to_string())
}
