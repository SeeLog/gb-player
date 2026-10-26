//! Audio output: a cpal stream that pulls samples from the shared [`Player`].

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};
use gb_music::Player;

use crate::i18n::Text;

pub type SharedPlayer = Arc<Mutex<Option<Player>>>;

pub struct AudioOut {
    _stream: Stream,
    pub sample_rate: u32,
}

impl AudioOut {
    pub fn start(player: SharedPlayer, t: &Text) -> Result<AudioOut, String> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or(t.get("err_no_device"))?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let format = supported.sample_format();
        let config: StreamConfig = supported.into();
        let sample_rate = config.sample_rate;
        let stream = match format {
            SampleFormat::F32 => build::<f32>(&device, config, player),
            SampleFormat::I16 => build::<i16>(&device, config, player),
            SampleFormat::I32 => build::<i32>(&device, config, player),
            SampleFormat::U16 => build::<u16>(&device, config, player),
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
                match player.lock().ok().as_deref_mut().and_then(|p| p.as_mut()) {
                    Some(p) => p.render(&mut stereo),
                    None => stereo.fill(0.0),
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
