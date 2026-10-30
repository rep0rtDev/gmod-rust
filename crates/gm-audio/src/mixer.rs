//! A small mixer: one-shot voices with pitch (resampled linearly), volume, and a position
//! that is attenuated with distance and panned against the listener. Output goes through
//! cpal's default device; without one (headless) the mixer still accepts sounds and drops
//! them, so the game runs the same.

use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use glam::Vec3;

use crate::wav::Pcm;

/// Where sounds are heard from: eye position and the horizontal right vector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Listener {
    pub position: Vec3,
    pub right: Vec3,
}

impl Default for Listener {
    fn default() -> Self {
        Listener {
            position: Vec3::ZERO,
            right: Vec3::Y,
        }
    }
}

/// One sound to start.
#[derive(Debug, Clone)]
pub struct Play {
    pub pcm: Arc<Pcm>,
    /// Linear gain before distance.
    pub volume: f32,
    /// Percent: 100 is the recorded rate.
    pub pitch: f32,
    /// World position; `None` plays at the listener (the owner's own weapon).
    pub position: Option<Vec3>,
    /// Sound level in dB at the source (`SNDLVL_*`); 0 carries without attenuation.
    pub level: f32,
}

/// Distance gain of a sound with level `level` dB heard `dist` units away. The SDK's
/// `SNDLVL_TO_ATTN` (`20 / (level - 50)`, 4 under 50 dB) gives the attenuation; the gain
/// then falls linearly to silence at `1000 / attenuation` units (GoldSrc's law, which the
/// game's scripts were tuned against: gunfire at 140 dB carries 4500 units, 75 dB 1250).
/// Not measured against the game; the client cannot record its mix.
pub fn distance_gain(level: f32, dist: f32) -> f32 {
    if level <= 0.0 {
        return 1.0;
    }
    let attn = if level > 50.0 {
        20.0 / (level - 50.0)
    } else {
        4.0
    };
    (1.0 - dist * attn / 1000.0).clamp(0.0, 1.0)
}

struct Voice {
    pcm: Arc<Pcm>,
    cursor: f64,
    step: f64,
    gain: f32,
    position: Option<Vec3>,
    level: f32,
}

#[derive(Default)]
struct Shared {
    voices: Vec<Voice>,
    listener: Listener,
}

/// Per-channel gains for a voice against the listener (constant-power pan on the
/// horizontal right vector; sounds at the listener are centered).
fn pan(listener: &Listener, position: Option<Vec3>, level: f32) -> (f32, f32) {
    let Some(p) = position else {
        return (0.707, 0.707);
    };
    let d = p - listener.position;
    let dist = d.length();
    let g = distance_gain(level, dist);
    let side = if dist > 1.0 {
        (d / dist).dot(listener.right).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    // side -1 (left) .. 1 (right) -> angle 0 .. pi/2
    let a = (side + 1.0) * 0.25 * std::f32::consts::PI;
    (g * a.cos(), g * a.sin())
}

fn mix(shared: &Mutex<Shared>, out: &mut [f32], channels: usize, out_rate: f64) {
    out.fill(0.0);
    let Ok(mut s) = shared.lock() else { return };
    let listener = s.listener;
    let _ = out_rate;
    for v in &mut s.voices {
        let (gl, gr) = pan(&listener, v.position, v.level);
        let n = v.pcm.samples.len();
        for frame in out.chunks_exact_mut(channels) {
            let i = v.cursor as usize;
            if i + 1 >= n {
                v.cursor = n as f64;
                break;
            }
            let f = (v.cursor - i as f64) as f32;
            let sample = (v.pcm.samples[i] * (1.0 - f) + v.pcm.samples[i + 1] * f) * v.gain;
            match channels {
                1 => frame[0] += sample * (gl + gr) * 0.707,
                _ => {
                    frame[0] += sample * gl;
                    frame[1] += sample * gr;
                }
            }
            v.cursor += v.step;
        }
    }
    s.voices
        .retain(|v| (v.cursor as usize) + 1 < v.pcm.samples.len());
    for x in out.iter_mut() {
        *x = x.clamp(-1.0, 1.0);
    }
}

/// The output device and the voices playing on it.
pub struct Mixer {
    shared: Arc<Mutex<Shared>>,
    out_rate: f64,
    _stream: Option<cpal::Stream>,
    /// Device name, or why there is none.
    pub device: String,
}

impl Mixer {
    /// Open the default output device; a mixer without one plays silently.
    pub fn open() -> Mixer {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let host = cpal::default_host();
        let Some(device) = host.default_output_device() else {
            return Mixer::silent(shared, "no output device".into());
        };
        let name = device.name().unwrap_or_else(|_| "?".into());
        let cfg = match device.default_output_config() {
            Ok(c) => c,
            Err(e) => return Mixer::silent(shared, format!("{name}: {e}")),
        };
        let channels = cfg.channels() as usize;
        let out_rate = cfg.sample_rate().0 as f64;
        let s = shared.clone();
        let stream = device.build_output_stream(
            &cfg.config(),
            move |out: &mut [f32], _| mix(&s, out, channels, out_rate),
            |e| eprintln!("audio stream error: {e}"),
            None,
        );
        match stream {
            Ok(stream) => {
                if let Err(e) = stream.play() {
                    return Mixer::silent(shared, format!("{name}: {e}"));
                }
                Mixer {
                    shared,
                    out_rate,
                    _stream: Some(stream),
                    device: format!("{name} ({out_rate} Hz, {channels} ch)"),
                }
            }
            Err(e) => Mixer::silent(shared, format!("{name}: {e}")),
        }
    }

    fn silent(shared: Arc<Mutex<Shared>>, why: String) -> Mixer {
        Mixer {
            shared,
            out_rate: 44100.0,
            _stream: None,
            device: why,
        }
    }

    pub fn is_live(&self) -> bool {
        self._stream.is_some()
    }

    pub fn set_listener(&self, listener: Listener) {
        if let Ok(mut s) = self.shared.lock() {
            s.listener = listener;
        }
    }

    pub fn play(&self, p: Play) {
        if !self.is_live() || p.pcm.samples.len() < 2 {
            return;
        }
        let step = p.pcm.rate as f64 * (p.pitch.max(1.0) as f64 / 100.0) / self.out_rate;
        if let Ok(mut s) = self.shared.lock() {
            s.voices.push(Voice {
                pcm: p.pcm,
                cursor: 0.0,
                step,
                gain: p.volume.clamp(0.0, 1.0),
                position: p.position,
                level: p.level,
            });
        }
    }

    pub fn voices(&self) -> usize {
        self.shared.lock().map(|s| s.voices.len()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_falls_linearly_to_the_level_range() {
        assert_eq!(distance_gain(140.0, 0.0), 1.0);
        assert!((distance_gain(140.0, 2250.0) - 0.5).abs() < 1e-4);
        assert_eq!(distance_gain(140.0, 5000.0), 0.0);
        assert!((distance_gain(75.0, 625.0) - 0.5).abs() < 1e-4);
        assert_eq!(distance_gain(0.0, 1e6), 1.0);
        assert_eq!(
            distance_gain(40.0, 250.0),
            0.0,
            "under 50 dB: attenuation 4"
        );
    }

    #[test]
    fn pan_follows_the_listener_right_vector() {
        let l = Listener {
            position: Vec3::ZERO,
            right: Vec3::Y,
        };
        let (gl, gr) = pan(&l, Some(Vec3::new(0.0, 100.0, 0.0)), 140.0);
        assert!(
            gl.abs() < 1e-5 && gr > 0.9,
            "right of the listener: {gl} {gr}"
        );
        let (gl, gr) = pan(&l, Some(Vec3::new(100.0, 0.0, 0.0)), 140.0);
        assert!((gl - gr).abs() < 1e-5, "ahead: centered");
        let (gl, gr) = pan(&l, None, 140.0);
        assert!((gl - 0.707).abs() < 1e-3 && (gr - 0.707).abs() < 1e-3);
    }

    #[test]
    fn mixes_voices_with_pitch_and_drops_them_when_done() {
        let shared = Mutex::new(Shared::default());
        let pcm = Arc::new(Pcm {
            rate: 100,
            samples: vec![0.5; 10],
        });
        shared.lock().unwrap().voices.push(Voice {
            pcm,
            cursor: 0.0,
            step: 2.0,
            gain: 1.0,
            position: None,
            level: 0.0,
        });
        let mut out = vec![0.0f32; 8];
        mix(&shared, &mut out, 2, 100.0);
        assert!((out[0] - 0.5 * 0.707).abs() < 1e-3);
        assert_eq!(shared.lock().unwrap().voices.len(), 1);
        mix(&shared, &mut out, 2, 100.0);
        assert_eq!(
            shared.lock().unwrap().voices.len(),
            0,
            "10 samples at step 2 end"
        );
        mix(&shared, &mut out, 2, 100.0);
        assert!(out.iter().all(|&x| x == 0.0));
    }
}
