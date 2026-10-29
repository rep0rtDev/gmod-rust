//! Valve Texture Format header (7.0 to 7.5). Image data decoding comes with
//! the content pipeline; this module reads geometry, formats and mip counts.

pub mod decode;

use crate::{FormatError, Reader, Result};

pub const SIGNATURE: &[u8; 4] = b"VTF\0";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum ImageFormat {
    None = -1,
    Rgba8888 = 0,
    Abgr8888 = 1,
    Rgb888 = 2,
    Bgr888 = 3,
    Rgb565 = 4,
    I8 = 5,
    Ia88 = 6,
    P8 = 7,
    A8 = 8,
    Rgb888Bluescreen = 9,
    Bgr888Bluescreen = 10,
    Argb8888 = 11,
    Bgra8888 = 12,
    Dxt1 = 13,
    Dxt3 = 14,
    Dxt5 = 15,
    Bgrx8888 = 16,
    Bgr565 = 17,
    Bgrx5551 = 18,
    Bgra4444 = 19,
    Dxt1OneBitAlpha = 20,
    Bgra5551 = 21,
    Uv88 = 22,
    Uvwq8888 = 23,
    Rgba16161616F = 24,
    Rgba16161616 = 25,
    Uvlx8888 = 26,
}

impl ImageFormat {
    pub fn from_raw(v: i32) -> Option<Self> {
        use ImageFormat::*;
        Some(match v {
            -1 => None,
            0 => Rgba8888,
            1 => Abgr8888,
            2 => Rgb888,
            3 => Bgr888,
            4 => Rgb565,
            5 => I8,
            6 => Ia88,
            7 => P8,
            8 => A8,
            9 => Rgb888Bluescreen,
            10 => Bgr888Bluescreen,
            11 => Argb8888,
            12 => Bgra8888,
            13 => Dxt1,
            14 => Dxt3,
            15 => Dxt5,
            16 => Bgrx8888,
            17 => Bgr565,
            18 => Bgrx5551,
            19 => Bgra4444,
            20 => Dxt1OneBitAlpha,
            21 => Bgra5551,
            22 => Uv88,
            23 => Uvwq8888,
            24 => Rgba16161616F,
            25 => Rgba16161616,
            26 => Uvlx8888,
            _ => return Option::None,
        })
    }

    /// Bits per pixel for uncompressed formats; block-compressed formats report
    /// their effective rate (DXT1 4, DXT3/5 8).
    pub fn bits_per_pixel(self) -> u32 {
        use ImageFormat::*;
        match self {
            None => 0,
            Rgba8888 | Abgr8888 | Argb8888 | Bgra8888 | Bgrx8888 | Uvwq8888 | Uvlx8888 => 32,
            Rgb888 | Bgr888 | Rgb888Bluescreen | Bgr888Bluescreen => 24,
            Rgb565 | Ia88 | Bgr565 | Bgrx5551 | Bgra4444 | Bgra5551 | Uv88 => 16,
            I8 | P8 | A8 => 8,
            Dxt1 | Dxt1OneBitAlpha => 4,
            Dxt3 | Dxt5 => 8,
            Rgba16161616F | Rgba16161616 => 64,
        }
    }

    pub fn is_compressed(self) -> bool {
        matches!(
            self,
            ImageFormat::Dxt1
                | ImageFormat::Dxt1OneBitAlpha
                | ImageFormat::Dxt3
                | ImageFormat::Dxt5
        )
    }

    /// Byte size of one image of `w` x `h` pixels in this format.
    pub fn image_size(self, w: u32, h: u32) -> u32 {
        if self.is_compressed() {
            let bw = w.div_ceil(4).max(1);
            let bh = h.div_ceil(4).max(1);
            let block = if self.bits_per_pixel() == 4 { 8 } else { 16 };
            bw * bh * block
        } else {
            w * h * self.bits_per_pixel() / 8
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Header {
    pub version: (u32, u32),
    pub header_size: u32,
    pub width: u16,
    pub height: u16,
    pub flags: u32,
    pub frames: u16,
    pub first_frame: u16,
    pub reflectivity: [f32; 3],
    pub bumpmap_scale: f32,
    pub high_res_format: ImageFormat,
    pub mipmap_count: u8,
    pub low_res_format: ImageFormat,
    pub low_res_width: u8,
    pub low_res_height: u8,
    /// 7.2+; 1 otherwise.
    pub depth: u16,
    /// 7.3+; 0 otherwise.
    pub resource_count: u32,
}

pub mod flags {
    pub const POINT_SAMPLE: u32 = 0x1;
    pub const TRILINEAR: u32 = 0x2;
    pub const CLAMP_S: u32 = 0x4;
    pub const CLAMP_T: u32 = 0x8;
    pub const ANISOTROPIC: u32 = 0x10;
    pub const HINT_DXT5: u32 = 0x20;
    pub const NORMAL: u32 = 0x80;
    pub const NO_MIP: u32 = 0x100;
    pub const NO_LOD: u32 = 0x200;
    pub const ALL_MIPS: u32 = 0x400;
    pub const PROCEDURAL: u32 = 0x800;
    pub const ONE_BIT_ALPHA: u32 = 0x1000;
    pub const EIGHT_BIT_ALPHA: u32 = 0x2000;
    pub const ENVMAP: u32 = 0x4000;
    pub const RENDER_TARGET: u32 = 0x8000;
    pub const DEPTH_RENDER_TARGET: u32 = 0x10000;
    pub const NO_DEBUG_OVERRIDE: u32 = 0x20000;
    pub const SINGLE_COPY: u32 = 0x40000;
    pub const NO_DEPTH_BUFFER: u32 = 0x800000;
    pub const CLAMP_U: u32 = 0x2000000;
    pub const VERTEX_TEXTURE: u32 = 0x4000000;
    pub const SSBUMP: u32 = 0x8000000;
    pub const BORDER: u32 = 0x20000000;
}

impl Header {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let sig = r.array::<4>()?;
        if &sig != SIGNATURE {
            return Err(FormatError::BadMagic {
                expected: "VTF",
                found: sig.to_vec(),
            });
        }
        let major = r.u32()?;
        let minor = r.u32()?;
        if major != 7 || minor > 5 {
            return Err(FormatError::UnsupportedVersion {
                what: "VTF",
                version: major * 10 + minor,
            });
        }
        let header_size = r.u32()?;
        let width = r.u16()?;
        let height = r.u16()?;
        let flags = r.u32()?;
        let frames = r.u16()?;
        let first_frame = r.u16()?;
        r.skip(4)?;
        let reflectivity = [r.f32()?, r.f32()?, r.f32()?];
        r.skip(4)?;
        let bumpmap_scale = r.f32()?;
        let hi = r.i32()?;
        let mipmap_count = r.u8()?;
        let lo = r.i32()?;
        let low_res_width = r.u8()?;
        let low_res_height = r.u8()?;
        let depth = if minor >= 2 { r.u16()? } else { 1 };
        let resource_count = if minor >= 3 {
            r.skip(3)?;
            r.u32()?
        } else {
            0
        };
        let fmt = |v: i32| {
            ImageFormat::from_raw(v).ok_or_else(|| FormatError::Malformed {
                what: "VTF",
                detail: format!("unknown image format {v}"),
            })
        };
        Ok(Header {
            version: (major, minor),
            header_size,
            width,
            height,
            flags,
            frames,
            first_frame,
            reflectivity,
            bumpmap_scale,
            high_res_format: fmt(hi)?,
            mipmap_count,
            low_res_format: fmt(lo)?,
            low_res_width,
            low_res_height,
            depth,
            resource_count,
        })
    }

    pub fn faces(&self) -> u32 {
        if self.flags & flags::ENVMAP != 0 {
            if self.version.1 < 5 && self.version != (7, 1) {
                7
            } else {
                6
            }
        } else {
            1
        }
    }

    /// Total bytes of the high-resolution image data across all mips, frames and faces.
    pub fn high_res_data_size(&self) -> u32 {
        let mut total = 0u32;
        for mip in 0..self.mipmap_count as u32 {
            let w = (self.width as u32 >> mip).max(1);
            let h = (self.height as u32 >> mip).max(1);
            let d = (self.depth as u32 >> mip).max(1);
            total += self.high_res_format.image_size(w, h) * d;
        }
        total * self.frames as u32 * self.faces()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_72() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(SIGNATURE);
        v.extend_from_slice(&7u32.to_le_bytes());
        v.extend_from_slice(&2u32.to_le_bytes());
        v.extend_from_slice(&80u32.to_le_bytes());
        v.extend_from_slice(&512u16.to_le_bytes());
        v.extend_from_slice(&256u16.to_le_bytes());
        v.extend_from_slice(&(flags::NO_LOD).to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes());
        v.extend_from_slice(&[0; 4]);
        for f in [0.5f32, 0.25, 0.125] {
            v.extend_from_slice(&f.to_le_bytes());
        }
        v.extend_from_slice(&[0; 4]);
        v.extend_from_slice(&1.0f32.to_le_bytes());
        v.extend_from_slice(&(ImageFormat::Dxt5 as i32).to_le_bytes());
        v.push(10);
        v.extend_from_slice(&(ImageFormat::Dxt1 as i32).to_le_bytes());
        v.push(16);
        v.push(8);
        v.extend_from_slice(&1u16.to_le_bytes());
        v
    }

    #[test]
    fn parses_72() {
        let h = Header::parse(&synthetic_72()).unwrap();
        assert_eq!(h.version, (7, 2));
        assert_eq!((h.width, h.height), (512, 256));
        assert_eq!(h.high_res_format, ImageFormat::Dxt5);
        assert_eq!(h.mipmap_count, 10);
        assert_eq!(h.depth, 1);
        assert_eq!(h.faces(), 1);
        // DXT5 512x256 = 128*64 blocks * 16 = 131072; sum of mips down to 1x1
        assert_eq!(
            h.high_res_data_size(),
            131072 + 32768 + 8192 + 2048 + 512 + 128 + 32 + 16 + 16 + 16
        );
    }

    #[test]
    fn image_sizes() {
        assert_eq!(ImageFormat::Dxt1.image_size(4, 4), 8);
        assert_eq!(ImageFormat::Dxt1.image_size(1, 1), 8);
        assert_eq!(ImageFormat::Bgra8888.image_size(2, 2), 16);
        assert_eq!(ImageFormat::Rgb888.image_size(3, 1), 9);
    }
}
