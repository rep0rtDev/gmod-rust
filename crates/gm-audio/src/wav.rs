//! RIFF WAVE decode for the game's sound files: PCM 8-bit unsigned and 16-bit signed,
//! mono or stereo (stereo is folded to mono: the game spatializes every emitted sound,
//! so a stereo source only matters as its mix).

use anyhow::{anyhow, bail, Result};

/// Mono samples in [-1, 1] at `rate` Hz.
#[derive(Debug, Clone, PartialEq)]
pub struct Pcm {
    pub rate: u32,
    pub samples: Vec<f32>,
}

impl Pcm {
    pub fn duration(&self) -> f32 {
        self.samples.len() as f32 / self.rate.max(1) as f32
    }
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?]))
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *b.get(i)?,
        *b.get(i + 1)?,
        *b.get(i + 2)?,
        *b.get(i + 3)?,
    ]))
}

pub fn parse(bytes: &[u8]) -> Result<Pcm> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        bail!("not a RIFF WAVE file");
    }
    let mut fmt: Option<(u16, u16, u32, u16)> = None;
    let mut data: Option<&[u8]> = None;
    let mut i = 12;
    while i + 8 <= bytes.len() {
        let id = &bytes[i..i + 4];
        let n = u32_at(bytes, i + 4).unwrap() as usize;
        let body = bytes
            .get(i + 8..(i + 8 + n).min(bytes.len()))
            .ok_or_else(|| anyhow!("truncated chunk"))?;
        match id {
            b"fmt " => {
                let tag = u16_at(body, 0).ok_or_else(|| anyhow!("short fmt"))?;
                let channels = u16_at(body, 2).ok_or_else(|| anyhow!("short fmt"))?;
                let rate = u32_at(body, 4).ok_or_else(|| anyhow!("short fmt"))?;
                let bits = u16_at(body, 14).ok_or_else(|| anyhow!("short fmt"))?;
                fmt = Some((tag, channels, rate, bits));
            }
            b"data" => data = Some(body),
            _ => {}
        }
        i += 8 + n + (n & 1);
    }
    let (tag, channels, rate, bits) = fmt.ok_or_else(|| anyhow!("no fmt chunk"))?;
    let data = data.ok_or_else(|| anyhow!("no data chunk"))?;
    if tag != 1 {
        bail!("unsupported WAVE format tag {tag} (only PCM)");
    }
    if channels == 0 || channels > 2 {
        bail!("unsupported channel count {channels}");
    }
    let ch = channels as usize;
    let samples = match bits {
        8 => data
            .chunks_exact(ch)
            .map(|f| f.iter().map(|&s| (s as f32 - 128.0) / 128.0).sum::<f32>() / ch as f32)
            .collect(),
        16 => data
            .chunks_exact(2 * ch)
            .map(|f| {
                f.as_chunks::<2>()
                    .0
                    .iter()
                    .map(|s| i16::from_le_bytes(*s) as f32 / 32768.0)
                    .sum::<f32>()
                    / ch as f32
            })
            .collect(),
        _ => bail!("unsupported bit depth {bits}"),
    };
    Ok(Pcm { rate, samples })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(channels: u16, bits: u16, rate: u32, data: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend(b"RIFF");
        b.extend((36 + data.len() as u32).to_le_bytes());
        b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(channels.to_le_bytes());
        b.extend(rate.to_le_bytes());
        b.extend((rate * channels as u32 * bits as u32 / 8).to_le_bytes());
        b.extend((channels * bits / 8).to_le_bytes());
        b.extend(bits.to_le_bytes());
        b.extend(b"data");
        b.extend((data.len() as u32).to_le_bytes());
        b.extend(data);
        b
    }

    #[test]
    fn decodes_8_bit_mono_and_16_bit_stereo() {
        let p = parse(&wav(1, 8, 22050, &[128, 255, 0])).unwrap();
        assert_eq!(p.rate, 22050);
        assert_eq!(p.samples, vec![0.0, 127.0 / 128.0, -1.0]);
        let mut d = Vec::new();
        for s in [16384i16, -16384, 32767, 32767] {
            d.extend(s.to_le_bytes());
        }
        let p = parse(&wav(2, 16, 44100, &d)).unwrap();
        assert_eq!(p.samples.len(), 2);
        assert!(p.samples[0].abs() < 1e-6, "stereo folds to its mix");
        assert!((p.samples[1] - 32767.0 / 32768.0).abs() < 1e-6);
        assert!((p.duration() - 2.0 / 44100.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_non_pcm() {
        let mut b = wav(1, 16, 44100, &[0, 0]);
        b[20] = 2;
        assert!(parse(&b).is_err());
        assert!(parse(b"RIFX").is_err());
    }
}
