//! The `sheet` resource of a VTF (`mksheet`): animation sequences over a sprite atlas, used by
//! `render_animated_sprites`. Version 0 holds one texcoord rect per frame, version 1 four
//! (the second-texture / multi-texture rects) of which the first is the sprite.
use super::{decode, Header};
use crate::{FormatError, Reader, Result};

pub const RES_SHEET: [u8; 3] = [0x10, 0, 0];

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub time: f32,
    /// `[u_min, v_min, u_max, v_max]`.
    pub uv: [f32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sequence {
    pub id: u32,
    pub clamp: bool,
    pub total_time: f32,
    pub frames: Vec<Frame>,
}

impl Sequence {
    /// The frame rect at `t` (seconds into the sequence), looping unless clamped, and the
    /// next frame's rect with the blend weight towards it.
    pub fn sample(&self, t: f32) -> ([f32; 4], [f32; 4], f32) {
        if self.frames.is_empty() {
            return ([0.0, 0.0, 1.0, 1.0], [0.0, 0.0, 1.0, 1.0], 0.0);
        }
        let total = self.total_time.max(1e-6);
        let mut t = if self.clamp {
            t.clamp(0.0, total)
        } else {
            t.rem_euclid(total)
        };
        for (i, f) in self.frames.iter().enumerate() {
            let dur = f.time.max(1e-6);
            if t < dur || i + 1 == self.frames.len() {
                let next = if i + 1 < self.frames.len() {
                    &self.frames[i + 1]
                } else if self.clamp {
                    f
                } else {
                    &self.frames[0]
                };
                return (f.uv, next.uv, (t / dur).clamp(0.0, 1.0));
            }
            t -= dur;
        }
        let f = &self.frames[self.frames.len() - 1];
        (f.uv, f.uv, 0.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sheet {
    pub sequences: Vec<Sequence>,
}

impl Sheet {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let version = r.u32()?;
        if version > 1 {
            return Err(FormatError::UnsupportedVersion {
                what: "vtf sheet",
                version,
            });
        }
        let n_seq = r.u32()?;
        let mut sequences = Vec::with_capacity((n_seq as usize).min(256));
        for _ in 0..n_seq {
            let id = r.u32()?;
            let clamp = r.u32()? != 0;
            let n_frames = r.u32()?;
            let total_time = r.f32()?;
            let mut frames = Vec::with_capacity((n_frames as usize).min(1024));
            for _ in 0..n_frames {
                let time = r.f32()?;
                let uv = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];
                if version > 0 {
                    r.skip(3 * 16)?;
                }
                frames.push(Frame { time, uv });
            }
            sequences.push(Sequence {
                id,
                clamp,
                total_time,
                frames,
            });
        }
        Ok(Sheet { sequences })
    }

    pub fn sequence(&self, id: u32) -> Option<&Sequence> {
        self.sequences
            .iter()
            .find(|s| s.id == id)
            .or_else(|| self.sequences.first())
    }
}

/// The sheet of a VTF file, if it carries one (7.3+ resource `0x10`).
pub fn sheet_of(vtf: &[u8]) -> Result<Option<Sheet>> {
    let header = Header::parse(vtf)?;
    if header.version.1 < 3 {
        return Ok(None);
    }
    let Some(res) = decode::resources(&header, vtf)?
        .into_iter()
        .find(|r| r.tag == RES_SHEET)
    else {
        return Ok(None);
    };
    let start = res.offset as usize;
    if start + 4 > vtf.len() {
        return Err(FormatError::Eof {
            offset: start,
            needed: 4,
        });
    }
    let len = u32::from_le_bytes(vtf[start..start + 4].try_into().unwrap()) as usize;
    let body = vtf
        .get(start + 4..start + 4 + len)
        .ok_or(FormatError::Eof {
            offset: start + 4,
            needed: len,
        })?;
    Sheet::parse(body).map(Some)
}
