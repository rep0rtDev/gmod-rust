//! Locating and decoding VTF image data to RGBA8.
//!
//! Data layout after the header: the low-resolution thumbnail, then the
//! high-resolution mips from smallest to largest, each mip holding
//! `frames * faces * depth` images. Version 7.3+ stores a resource table whose
//! `0x30` entry gives the high-res offset instead.

use super::{Header, ImageFormat};
use crate::{FormatError, Reader, Result};

pub const RES_LOW_RES: [u8; 3] = [0x01, 0, 0];
pub const RES_HIGH_RES: [u8; 3] = [0x30, 0, 0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resource {
    pub tag: [u8; 3],
    pub flags: u8,
    pub offset: u32,
}

pub fn resources(h: &Header, data: &[u8]) -> Result<Vec<Resource>> {
    if h.version.1 < 3 {
        return Ok(Vec::new());
    }
    let mut r = Reader::at(data, 80);
    let mut out = Vec::with_capacity(h.resource_count as usize);
    for _ in 0..h.resource_count {
        let tag = r.array::<3>()?;
        let flags = r.u8()?;
        let offset = r.u32()?;
        out.push(Resource { tag, flags, offset });
    }
    Ok(out)
}

/// Byte offset of the high-resolution image block.
pub fn high_res_offset(h: &Header, data: &[u8]) -> Result<usize> {
    if h.version.1 >= 3 {
        let res = resources(h, data)?;
        if let Some(r) = res.iter().find(|r| r.tag == RES_HIGH_RES) {
            return Ok(r.offset as usize);
        }
        return Err(FormatError::Malformed {
            what: "VTF",
            detail: "no high-res image resource".into(),
        });
    }
    let thumb = if h.low_res_format == ImageFormat::None {
        0
    } else {
        h.low_res_format
            .image_size(h.low_res_width as u32, h.low_res_height as u32) as usize
    };
    Ok(h.header_size as usize + thumb)
}

/// Slice of the raw data for one mip level (0 = largest), frame, face and z slice.
pub fn mip_slice<'a>(
    h: &Header,
    data: &'a [u8],
    mip: u32,
    frame: u32,
    face: u32,
    slice: u32,
) -> Result<&'a [u8]> {
    if mip >= h.mipmap_count as u32 {
        return Err(FormatError::Malformed {
            what: "VTF",
            detail: format!("mip {mip} out of {}", h.mipmap_count),
        });
    }
    let faces = h.faces();
    let base = high_res_offset(h, data)?;
    let mut offset = base;
    // smaller mips come first
    for m in (mip + 1..h.mipmap_count as u32).rev() {
        offset += mip_block_size(h, m) as usize;
    }
    let (w, hgt, d) = mip_dims(h, mip);
    let img = h.high_res_format.image_size(w, hgt) as usize;
    offset += ((frame * faces + face) * d + slice) as usize * img;
    data.get(offset..offset + img).ok_or(FormatError::Eof {
        offset,
        needed: img,
    })
}

pub fn mip_dims(h: &Header, mip: u32) -> (u32, u32, u32) {
    (
        (h.width as u32 >> mip).max(1),
        (h.height as u32 >> mip).max(1),
        (h.depth as u32 >> mip).max(1),
    )
}

fn mip_block_size(h: &Header, mip: u32) -> u32 {
    let (w, hgt, d) = mip_dims(h, mip);
    h.high_res_format.image_size(w, hgt) * d * h.frames as u32 * h.faces()
}

/// Decode one image to tightly packed RGBA8.
pub fn decode_rgba8(fmt: ImageFormat, w: u32, h: u32, src: &[u8]) -> Result<Vec<u8>> {
    let need = fmt.image_size(w, h) as usize;
    if src.len() < need {
        return Err(FormatError::Eof {
            offset: 0,
            needed: need,
        });
    }
    let n = (w * h) as usize;
    let mut out = vec![0u8; n * 4];
    use ImageFormat::*;
    match fmt {
        Rgba8888 => out.copy_from_slice(&src[..n * 4]),
        Abgr8888 => {
            for i in 0..n {
                let p = &src[i * 4..i * 4 + 4];
                out[i * 4..i * 4 + 4].copy_from_slice(&[p[3], p[2], p[1], p[0]]);
            }
        }
        Argb8888 => {
            for i in 0..n {
                let p = &src[i * 4..i * 4 + 4];
                out[i * 4..i * 4 + 4].copy_from_slice(&[p[1], p[2], p[3], p[0]]);
            }
        }
        Bgra8888 | Bgrx8888 => {
            for i in 0..n {
                let p = &src[i * 4..i * 4 + 4];
                let a = if fmt == Bgra8888 { p[3] } else { 255 };
                out[i * 4..i * 4 + 4].copy_from_slice(&[p[2], p[1], p[0], a]);
            }
        }
        Rgb888 | Rgb888Bluescreen => {
            for i in 0..n {
                let p = &src[i * 3..i * 3 + 3];
                let a = if fmt == Rgb888Bluescreen && p == [0, 0, 255] {
                    0
                } else {
                    255
                };
                out[i * 4..i * 4 + 4].copy_from_slice(&[p[0], p[1], p[2], a]);
            }
        }
        Bgr888 | Bgr888Bluescreen => {
            for i in 0..n {
                let p = &src[i * 3..i * 3 + 3];
                let a = if fmt == Bgr888Bluescreen && p == [255, 0, 0] {
                    0
                } else {
                    255
                };
                out[i * 4..i * 4 + 4].copy_from_slice(&[p[2], p[1], p[0], a]);
            }
        }
        I8 => {
            for i in 0..n {
                out[i * 4..i * 4 + 4].copy_from_slice(&[src[i], src[i], src[i], 255]);
            }
        }
        A8 => {
            for i in 0..n {
                out[i * 4..i * 4 + 4].copy_from_slice(&[255, 255, 255, src[i]]);
            }
        }
        Ia88 => {
            for i in 0..n {
                let (l, a) = (src[i * 2], src[i * 2 + 1]);
                out[i * 4..i * 4 + 4].copy_from_slice(&[l, l, l, a]);
            }
        }
        Rgb565 | Bgr565 => {
            for i in 0..n {
                let v = u16::from_le_bytes([src[i * 2], src[i * 2 + 1]]);
                let (r, g, b) = unpack565(v);
                let (r, b) = if fmt == Bgr565 { (b, r) } else { (r, b) };
                out[i * 4..i * 4 + 4].copy_from_slice(&[r, g, b, 255]);
            }
        }
        Uv88 => {
            for i in 0..n {
                out[i * 4..i * 4 + 4].copy_from_slice(&[src[i * 2], src[i * 2 + 1], 0, 255]);
            }
        }
        Rgba16161616F => {
            for i in 0..n {
                for c in 0..4 {
                    let hbits = u16::from_le_bytes([src[i * 8 + c * 2], src[i * 8 + c * 2 + 1]]);
                    let f = half_to_f32(hbits);
                    // HDR textures are stored linear with a 16x overbright range; clamp for RGBA8 preview.
                    out[i * 4 + c] = (f.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
            }
        }
        Rgba16161616 => {
            for i in 0..n {
                for c in 0..4 {
                    out[i * 4 + c] = src[i * 8 + c * 2 + 1];
                }
            }
        }
        Dxt1 | Dxt1OneBitAlpha => decode_dxt(src, w, h, false, &mut out),
        Dxt3 => decode_dxt3(src, w, h, &mut out),
        Dxt5 => decode_dxt(src, w, h, true, &mut out),
        other => {
            return Err(FormatError::Malformed {
                what: "VTF",
                detail: format!("decoding {other:?} not supported"),
            })
        }
    }
    Ok(out)
}

fn unpack565(v: u16) -> (u8, u8, u8) {
    let r = ((v >> 11) & 0x1f) as u32;
    let g = ((v >> 5) & 0x3f) as u32;
    let b = (v & 0x1f) as u32;
    (
        ((r * 255 + 15) / 31) as u8,
        ((g * 255 + 31) / 63) as u8,
        ((b * 255 + 15) / 31) as u8,
    )
}

fn half_to_f32(h: u16) -> f32 {
    let sign = ((h >> 15) & 1) as u32;
    let exp = ((h >> 10) & 0x1f) as u32;
    let frac = (h & 0x3ff) as u32;
    let bits = if exp == 0 {
        if frac == 0 {
            sign << 31
        } else {
            // subnormal
            let mut e = 127 - 15 + 1;
            let mut f = frac;
            while f & 0x400 == 0 {
                f <<= 1;
                e -= 1;
            }
            (sign << 31) | ((e as u32) << 23) | ((f & 0x3ff) << 13)
        }
    } else if exp == 31 {
        (sign << 31) | 0x7f80_0000 | (frac << 13)
    } else {
        (sign << 31) | ((exp + 127 - 15) << 23) | (frac << 13)
    };
    f32::from_bits(bits)
}

fn put(out: &mut [u8], w: u32, h: u32, x: u32, y: u32, rgba: [u8; 4]) {
    if x < w && y < h {
        let i = ((y * w + x) * 4) as usize;
        out[i..i + 4].copy_from_slice(&rgba);
    }
}

fn dxt_colors(block: &[u8], opaque_block_has_alpha: bool) -> [[u8; 4]; 4] {
    let c0 = u16::from_le_bytes([block[0], block[1]]);
    let c1 = u16::from_le_bytes([block[2], block[3]]);
    let (r0, g0, b0) = unpack565(c0);
    let (r1, g1, b1) = unpack565(c1);
    let mut c = [
        [r0, g0, b0, 255],
        [r1, g1, b1, 255],
        [0, 0, 0, 255],
        [0, 0, 0, 255],
    ];
    if c0 > c1 || !opaque_block_has_alpha {
        c[2] = [
            ((2 * r0 as u32 + r1 as u32) / 3) as u8,
            ((2 * g0 as u32 + g1 as u32) / 3) as u8,
            ((2 * b0 as u32 + b1 as u32) / 3) as u8,
            255,
        ];
        c[3] = [
            ((r0 as u32 + 2 * r1 as u32) / 3) as u8,
            ((g0 as u32 + 2 * g1 as u32) / 3) as u8,
            ((b0 as u32 + 2 * b1 as u32) / 3) as u8,
            255,
        ];
    } else {
        c[2] = [
            ((r0 as u32 + r1 as u32) / 2) as u8,
            ((g0 as u32 + g1 as u32) / 2) as u8,
            ((b0 as u32 + b1 as u32) / 2) as u8,
            255,
        ];
        c[3] = [0, 0, 0, 0];
    }
    c
}

fn decode_dxt(src: &[u8], w: u32, h: u32, dxt5: bool, out: &mut [u8]) {
    let bw = w.div_ceil(4);
    let bh = h.div_ceil(4);
    let bs = if dxt5 { 16 } else { 8 };
    for by in 0..bh {
        for bx in 0..bw {
            let block = &src[((by * bw + bx) * bs) as usize..][..bs as usize];
            let (alpha, color) = if dxt5 {
                (Some(&block[..8]), &block[8..])
            } else {
                (None, block)
            };
            let colors = dxt_colors(color, !dxt5);
            let idx = u32::from_le_bytes([color[4], color[5], color[6], color[7]]);
            let alphas = alpha.map(|a| {
                let a0 = a[0] as u32;
                let a1 = a[1] as u32;
                let mut table = [a0 as u8, a1 as u8, 0, 0, 0, 0, 0, 0];
                if a0 > a1 {
                    for i in 1..7u32 {
                        table[i as usize + 1] = (((7 - i) * a0 + i * a1) / 7) as u8;
                    }
                } else {
                    for i in 1..5u32 {
                        table[i as usize + 1] = (((5 - i) * a0 + i * a1) / 5) as u8;
                    }
                    table[6] = 0;
                    table[7] = 255;
                }
                let bits = u64::from_le_bytes([a[2], a[3], a[4], a[5], a[6], a[7], 0, 0]);
                (table, bits)
            });
            for py in 0..4 {
                for px in 0..4 {
                    let i = py * 4 + px;
                    let ci = ((idx >> (2 * i)) & 3) as usize;
                    let mut rgba = colors[ci];
                    if let Some((table, bits)) = alphas {
                        rgba[3] = table[((bits >> (3 * i)) & 7) as usize];
                    }
                    put(out, w, h, bx * 4 + px, by * 4 + py, rgba);
                }
            }
        }
    }
}

fn decode_dxt3(src: &[u8], w: u32, h: u32, out: &mut [u8]) {
    let bw = w.div_ceil(4);
    let bh = h.div_ceil(4);
    for by in 0..bh {
        for bx in 0..bw {
            let block = &src[((by * bw + bx) * 16) as usize..][..16];
            let colors = dxt_colors(&block[8..], false);
            let idx = u32::from_le_bytes([block[12], block[13], block[14], block[15]]);
            for py in 0..4 {
                for px in 0..4 {
                    let i = py * 4 + px;
                    let a4 = (u16::from_le_bytes([
                        block[(py * 2) as usize],
                        block[(py * 2 + 1) as usize],
                    ]) >> (4 * px))
                        & 0xf;
                    let mut rgba = colors[((idx >> (2 * i)) & 3) as usize];
                    rgba[3] = (a4 * 17) as u8;
                    put(out, w, h, bx * 4 + px, by * 4 + py, rgba);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dxt1_solid_block() {
        // c0 = pure red (0xF800), c1 = pure blue (0x001F), all indices 0 -> red
        let block = [0x00, 0xF8, 0x1F, 0x00, 0, 0, 0, 0];
        let out = decode_rgba8(ImageFormat::Dxt1, 4, 4, &block).unwrap();
        assert_eq!(&out[..4], &[255, 0, 0, 255]);
        assert_eq!(out.len(), 64);
    }

    #[test]
    fn dxt5_alpha_table() {
        let mut block = [0u8; 16];
        block[0] = 255; // a0
        block[1] = 0; // a1
                      // alpha indices all 1 -> a1 = 0
        block[2] = 0b0100_1001;
        block[3] = 0b1001_0010;
        block[4] = 0b0010_0100;
        block[5] = block[2];
        block[6] = block[3];
        block[7] = block[4];
        block[8] = 0xFF;
        block[9] = 0xFF; // white
        let out = decode_rgba8(ImageFormat::Dxt5, 4, 4, &block).unwrap();
        assert_eq!(&out[..4], &[255, 255, 255, 0]);
    }

    #[test]
    fn bgra_and_half() {
        let out = decode_rgba8(ImageFormat::Bgra8888, 1, 1, &[1, 2, 3, 4]).unwrap();
        assert_eq!(out, vec![3, 2, 1, 4]);
        assert_eq!(half_to_f32(0x3C00), 1.0);
        assert_eq!(half_to_f32(0x3800), 0.5);
        assert_eq!(half_to_f32(0x0000), 0.0);
        assert_eq!(unpack565(0xFFFF), (255, 255, 255));
    }
}
