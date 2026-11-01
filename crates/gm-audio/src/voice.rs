//! Voice chat codec and playback: Opus (VoIP mode) at 48 kHz mono, 20 ms frames (960
//! samples), one encoder per microphone and one decoder per speaker heard. Decoded frames
//! stream into a looping-less engine voice at the speaker's mouth through
//! [`crate::Mixer::feed`]; a lost packet is concealed by the decoder (`fec` off, PLC on).
//!
//! The game sends Steam voice / Speex at 24 kHz; ours is not wire-compatible (own
//! ecosystem), only the behaviour matches: push-to-talk, 3D at the speaker, `voice_scale`,
//! `voice_loopback`.

use anyhow::Result;

pub const RATE: u32 = 48_000;
/// Samples per packet (20 ms).
pub const FRAME: usize = 960;
/// Largest Opus packet we send.
pub const MAX_PACKET: usize = 400;
/// Target bitrate (bit/s): speech at good quality.
pub const BITRATE: i32 = 24_000;

pub struct Encoder {
    inner: opus::Encoder,
    pending: Vec<f32>,
    pub seq: u16,
}

impl Encoder {
    pub fn new() -> Result<Encoder> {
        let mut inner = opus::Encoder::new(RATE, opus::Channels::Mono, opus::Application::Voip)?;
        inner.set_bitrate(opus::Bitrate::Bits(BITRATE))?;
        Ok(Encoder {
            inner,
            pending: Vec::with_capacity(FRAME * 2),
            seq: 0,
        })
    }

    /// Queue captured mono samples at [`RATE`]; returns the packets of every whole frame
    /// now available, each with its sequence number.
    pub fn push(&mut self, samples: &[f32]) -> Vec<(u16, Vec<u8>)> {
        self.pending.extend_from_slice(samples);
        let mut out = Vec::new();
        while self.pending.len() >= FRAME {
            let frame: Vec<f32> = self.pending.drain(..FRAME).collect();
            if let Ok(p) = self.inner.encode_vec_float(&frame, MAX_PACKET) {
                out.push((self.seq, p));
                self.seq = self.seq.wrapping_add(1);
            }
        }
        out
    }

    /// Drop what has not filled a frame (key released).
    pub fn flush(&mut self) {
        self.pending.clear();
    }
}

pub struct Decoder {
    inner: opus::Decoder,
    pub last_seq: Option<u16>,
}

impl Decoder {
    pub fn new() -> Result<Decoder> {
        Ok(Decoder {
            inner: opus::Decoder::new(RATE, opus::Channels::Mono)?,
            last_seq: None,
        })
    }

    /// Decode one packet; a gap in `seq` first conceals the missing frames (at most two).
    pub fn decode(&mut self, seq: u16, packet: &[u8]) -> Vec<f32> {
        let mut out = Vec::with_capacity(FRAME * 3);
        if let Some(last) = self.last_seq {
            let gap = seq.wrapping_sub(last).wrapping_sub(1);
            for _ in 0..gap.min(2) {
                let mut buf = vec![0.0f32; FRAME];
                if let Ok(n) = self.inner.decode_float(&[], &mut buf, false) {
                    out.extend_from_slice(&buf[..n]);
                }
            }
        }
        self.last_seq = Some(seq);
        let mut buf = vec![0.0f32; FRAME];
        if let Ok(n) = self.inner.decode_float(packet, &mut buf, false) {
            out.extend_from_slice(&buf[..n]);
        }
        out
    }
}

/// The microphone: the default input device, whatever its rate and channel count, delivered
/// as mono at [`RATE`] through a channel. `None` when there is no input device (headless).
pub struct Capture {
    _stream: cpal::Stream,
    rx: std::sync::mpsc::Receiver<Vec<f32>>,
}

impl Capture {
    pub fn start() -> Option<Capture> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        let host = cpal::default_host();
        let device = host.default_input_device()?;
        let config = device.default_input_config().ok()?;
        let rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        let (tx, rx) = std::sync::mpsc::channel();
        let stream = device
            .build_input_stream(
                &config.into(),
                move |data: &[f32], _| {
                    let _ = tx.send(resample(data, rate, channels));
                },
                |e| eprintln!("voice: capture: {e}"),
                None,
            )
            .ok()?;
        stream.play().ok()?;
        Some(Capture {
            _stream: stream,
            rx,
        })
    }

    /// Everything captured since the last call.
    pub fn drain(&self) -> Vec<f32> {
        let mut out = Vec::new();
        while let Ok(chunk) = self.rx.try_recv() {
            out.extend(chunk);
        }
        out
    }
}

/// Mono samples at any rate to [`RATE`] (linear interpolation; microphones run at 44.1
/// or 48 kHz, this is not a quality path).
pub fn resample(input: &[f32], from: u32, channels: usize) -> Vec<f32> {
    let n = input.len() / channels.max(1);
    if n == 0 {
        return Vec::new();
    }
    let mono = |i: usize| -> f32 {
        let f = &input[i * channels..(i + 1) * channels];
        f.iter().sum::<f32>() / channels as f32
    };
    if from == RATE {
        return (0..n).map(mono).collect();
    }
    let step = from as f64 / RATE as f64;
    let count = ((n as f64) / step) as usize;
    (0..count)
        .map(|k| {
            let x = k as f64 * step;
            let i = x as usize;
            let f = (x - i as f64) as f32;
            let a = mono(i.min(n - 1));
            let b = mono((i + 1).min(n - 1));
            a * (1.0 - f) + b * f
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tone_survives_the_codec() {
        let mut enc = Encoder::new().unwrap();
        let mut dec = Decoder::new().unwrap();
        let tone: Vec<f32> = (0..FRAME * 10)
            .map(|i| (i as f32 * 300.0 / RATE as f32 * std::f32::consts::TAU).sin() * 0.4)
            .collect();
        let packets = enc.push(&tone);
        assert_eq!(packets.len(), 10);
        assert!(packets
            .iter()
            .all(|(_, p)| p.len() < MAX_PACKET && p.len() > 8));
        let mut decoded = Vec::new();
        for (seq, p) in &packets {
            decoded.extend(dec.decode(*seq, p));
        }
        assert_eq!(decoded.len(), FRAME * 10);
        // after the codec's warm-up the tone comes back at about its level
        let tail = &decoded[FRAME * 5..];
        let peak = tail.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!((0.25..0.6).contains(&peak), "peak {peak}");
    }

    #[test]
    fn a_lost_packet_is_concealed() {
        let mut enc = Encoder::new().unwrap();
        let mut dec = Decoder::new().unwrap();
        let tone: Vec<f32> = (0..FRAME * 4)
            .map(|i| ((i % 160) as f32 / 80.0 - 1.0) * 0.3)
            .collect();
        let packets = enc.push(&tone);
        let a = dec.decode(packets[0].0, &packets[0].1);
        let c = dec.decode(packets[2].0, &packets[2].1);
        assert_eq!(a.len(), FRAME);
        assert_eq!(c.len(), FRAME * 2, "the gap is filled before the packet");
    }

    #[test]
    fn resampling_keeps_duration() {
        let stereo_44k: Vec<f32> = vec![0.1; 44100 * 2];
        let out = resample(&stereo_44k, 44100, 2);
        assert!((out.len() as i64 - 48000).abs() <= 2, "{}", out.len());
        assert!((out[100] - 0.1).abs() < 1e-6);
        assert_eq!(resample(&[0.5; 960], 48000, 1).len(), 960);
    }
}
