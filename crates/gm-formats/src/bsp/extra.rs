//! Static props (game lump `sprp`), displacements and PVS.

use super::lumps::{parse_vec3, read_all, Record};
use super::{Bsp, Lump};
use crate::{FormatError, Reader, Result};

pub const GAME_LUMP_STATIC_PROPS: u32 = 0x7370_7270; // 'sprp'

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameLumpInfo {
    pub id: u32,
    pub flags: u16,
    pub version: u16,
    pub offset: i32,
    pub length: i32,
}

pub fn game_lumps(bsp: &Bsp<'_>) -> Result<Vec<GameLumpInfo>> {
    let data = bsp.header.lump_data(bsp.data, Lump::GameLump)?;
    if data.is_empty() {
        return Ok(Vec::new());
    }
    let mut r = Reader::new(data);
    let n = r.i32()?.max(0);
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        out.push(GameLumpInfo {
            id: r.u32()?,
            flags: r.u16()?,
            version: r.u16()?,
            offset: r.i32()?,
            length: r.i32()?,
        });
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq)]
pub struct StaticProp {
    pub origin: [f32; 3],
    /// Pitch, yaw, roll in degrees.
    pub angles: [f32; 3],
    pub model: u16,
    pub first_leaf: u16,
    pub leaf_count: u16,
    pub solid: u8,
    pub flags: u8,
    pub skin: i32,
    pub fade_min: f32,
    pub fade_max: f32,
    pub lighting_origin: [f32; 3],
    pub forced_fade_scale: f32,
    pub diffuse_modulation: [u8; 4],
    pub uniform_scale: f32,
}

#[derive(Clone, Debug, Default)]
pub struct StaticProps {
    pub version: u16,
    pub models: Vec<String>,
    pub leaves: Vec<u16>,
    pub props: Vec<StaticProp>,
}

impl StaticProps {
    pub fn record_size(version: u16) -> Option<usize> {
        Some(match version {
            4 => 56,
            5 => 60,
            6 => 64,
            7 | 8 => 68,
            9 => 72,
            10 => 76,
            11 => 80,
            _ => return None,
        })
    }

    pub fn parse(bsp: &Bsp<'_>) -> Result<Self> {
        let Some(info) = game_lumps(bsp)?
            .into_iter()
            .find(|g| g.id == GAME_LUMP_STATIC_PROPS)
        else {
            return Ok(StaticProps::default());
        };
        let start = info.offset as usize;
        let data = bsp
            .data
            .get(start..start + info.length.max(0) as usize)
            .ok_or(FormatError::Eof {
                offset: start,
                needed: info.length.max(0) as usize,
            })?;
        let mut r = Reader::new(data);
        let n = r.i32()?.max(0) as usize;
        let mut models = Vec::with_capacity(n);
        for _ in 0..n {
            let raw = r.bytes(128)?;
            let s = raw.split(|&b| b == 0).next().unwrap_or(&[]);
            models.push(
                String::from_utf8_lossy(s)
                    .replace('\\', "/")
                    .to_ascii_lowercase(),
            );
        }
        let nl = r.i32()?.max(0) as usize;
        let mut leaves = Vec::with_capacity(nl);
        for _ in 0..nl {
            leaves.push(r.u16()?);
        }
        let count = r.i32()?.max(0) as usize;
        let remaining = r.remaining();
        let expected = Self::record_size(info.version);
        let size = match expected {
            Some(s) if count == 0 || s * count == remaining => s,
            _ if count > 0 && remaining.is_multiple_of(count) => remaining / count,
            _ => {
                return Err(FormatError::Malformed {
                    what: "static props",
                    detail: format!(
                        "version {} with {remaining} bytes for {count} props",
                        info.version
                    ),
                })
            }
        };
        let mut props = Vec::with_capacity(count);
        for i in 0..count {
            let base = r.pos() + i * size;
            let mut p = Reader::at(data, base);
            let origin = [p.f32()?, p.f32()?, p.f32()?];
            let angles = [p.f32()?, p.f32()?, p.f32()?];
            let model = p.u16()?;
            let first_leaf = p.u16()?;
            let leaf_count = p.u16()?;
            let solid = p.u8()?;
            let flags = p.u8()?;
            let skin = p.i32()?;
            let fade_min = p.f32()?;
            let fade_max = p.f32()?;
            let lighting_origin = [p.f32()?, p.f32()?, p.f32()?];
            let forced_fade_scale = if size >= 60 { p.f32()? } else { 1.0 };
            let mut diffuse_modulation = [255u8; 4];
            let mut uniform_scale = 1.0;
            match info.version {
                6 => {
                    p.skip(4)?; // min/max dx level
                }
                7 => {
                    p.skip(4)?;
                    diffuse_modulation = p.array::<4>()?;
                }
                8..=11 => {
                    p.skip(4)?; // cpu/gpu levels
                    diffuse_modulation = p.array::<4>()?;
                    if info.version >= 9 {
                        p.skip(4)?; // disable x360
                    }
                    if info.version >= 10 {
                        p.skip(4)?; // flags ex
                    }
                    if info.version >= 11 {
                        uniform_scale = p.f32()?;
                    }
                }
                _ => {}
            }
            props.push(StaticProp {
                origin,
                angles,
                model,
                first_leaf,
                leaf_count,
                solid,
                flags,
                skin,
                fade_min,
                fade_max,
                lighting_origin,
                forced_fade_scale,
                diffuse_modulation,
                uniform_scale,
            });
        }
        Ok(StaticProps {
            version: info.version,
            models,
            leaves,
            props,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DispInfo {
    pub start_position: [f32; 3],
    pub disp_vert_start: i32,
    pub disp_tri_start: i32,
    pub power: i32,
    pub min_tess: i32,
    pub smoothing_angle: f32,
    pub contents: i32,
    pub map_face: u16,
    pub lightmap_alpha_start: i32,
    pub lightmap_sample_position_start: i32,
}

impl Record for DispInfo {
    const SIZE: usize = 176;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let d = DispInfo {
            start_position: [r.f32()?, r.f32()?, r.f32()?],
            disp_vert_start: r.i32()?,
            disp_tri_start: r.i32()?,
            power: r.i32()?,
            min_tess: r.i32()?,
            smoothing_angle: r.f32()?,
            contents: r.i32()?,
            map_face: r.u16()?,
            lightmap_alpha_start: {
                r.skip(2)?;
                r.i32()?
            },
            lightmap_sample_position_start: r.i32()?,
        };
        r.skip(48 + 40 + 40)?; // edge neighbours, corner neighbours, allowed verts
        Ok(d)
    }
}

impl DispInfo {
    pub fn side_verts(&self) -> usize {
        (1usize << self.power) + 1
    }
    pub fn vert_count(&self) -> usize {
        self.side_verts() * self.side_verts()
    }
    pub fn tri_count(&self) -> usize {
        2 * (1usize << self.power) * (1usize << self.power)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DispVert {
    pub vec: [f32; 3],
    pub dist: f32,
    pub alpha: f32,
}

impl Record for DispVert {
    const SIZE: usize = 20;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(DispVert {
            vec: [r.f32()?, r.f32()?, r.f32()?],
            dist: r.f32()?,
            alpha: r.f32()?,
        })
    }
}

/// Generated displacement surface: positions, alphas and a triangle list.
#[derive(Clone, Debug, Default)]
pub struct DispMesh {
    pub positions: Vec<[f32; 3]>,
    pub alphas: Vec<f32>,
    pub indices: Vec<u32>,
}

pub fn disp_infos(bsp: &Bsp<'_>) -> Result<Vec<DispInfo>> {
    read_all("dispinfo", bsp.header.lump_data(bsp.data, Lump::DispInfo)?)
}

pub fn disp_verts(bsp: &Bsp<'_>) -> Result<Vec<DispVert>> {
    read_all(
        "dispverts",
        bsp.header.lump_data(bsp.data, Lump::DispVerts)?,
    )
}

/// Build the subdivided surface for one displacement. The base face must be a
/// quad; the corner closest to `start_position` is taken as the origin.
pub fn build_disp_mesh(bsp: &Bsp<'_>, disp: &DispInfo, verts: &[DispVert]) -> Result<DispMesh> {
    let face = bsp
        .faces
        .get(disp.map_face as usize)
        .ok_or_else(|| FormatError::Malformed {
            what: "dispinfo",
            detail: format!("map_face {} out of range", disp.map_face),
        })?;
    let corners = bsp.face_vertices(face);
    if corners.len() != 4 {
        return Err(FormatError::Malformed {
            what: "dispinfo",
            detail: format!("base face has {} vertices", corners.len()),
        });
    }
    let dist2 = |a: [f32; 3], b: [f32; 3]| {
        (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
    };
    let start = (0..4)
        .min_by(|&a, &b| {
            dist2(corners[a], disp.start_position)
                .partial_cmp(&dist2(corners[b], disp.start_position))
                .unwrap()
        })
        .unwrap();
    let c = |i: usize| corners[(start + i) % 4];
    let (c0, c1, c2, c3) = (c(0), c(1), c(2), c(3));
    let side = disp.side_verts();
    let mut mesh = DispMesh::default();
    let vstart = disp.disp_vert_start.max(0) as usize;
    for y in 0..side {
        let ty = y as f32 / (side - 1) as f32;
        for x in 0..side {
            let tx = x as f32 / (side - 1) as f32;
            let dv = verts.get(vstart + y * side + x).ok_or(FormatError::Eof {
                offset: vstart + y * side + x,
                needed: 1,
            })?;
            let mut p = [0f32; 3];
            for k in 0..3 {
                let left = c0[k] + (c1[k] - c0[k]) * ty;
                let right = c3[k] + (c2[k] - c3[k]) * ty;
                p[k] = left + (right - left) * tx + dv.vec[k] * dv.dist;
            }
            mesh.positions.push(p);
            mesh.alphas.push(dv.alpha);
        }
    }
    for y in 0..side - 1 {
        for x in 0..side - 1 {
            let i = (y * side + x) as u32;
            let s = side as u32;
            if (x + y) % 2 == 0 {
                mesh.indices
                    .extend_from_slice(&[i, i + s, i + s + 1, i, i + s + 1, i + 1]);
            } else {
                mesh.indices
                    .extend_from_slice(&[i, i + s, i + 1, i + 1, i + s, i + s + 1]);
            }
        }
    }
    Ok(mesh)
}

/// Potentially visible set: one bit per cluster, run-length encoded zeros.
#[derive(Clone, Debug, Default)]
pub struct Visibility {
    pub num_clusters: usize,
    /// `offsets[cluster] = (pvs, pas)` byte offsets into the lump.
    pub offsets: Vec<(i32, i32)>,
}

impl Visibility {
    pub fn parse(bsp: &Bsp<'_>) -> Result<Self> {
        let data = bsp.header.lump_data(bsp.data, Lump::Visibility)?;
        if data.is_empty() {
            return Ok(Visibility::default());
        }
        let mut r = Reader::new(data);
        let n = r.i32()?.max(0) as usize;
        let mut offsets = Vec::with_capacity(n);
        for _ in 0..n {
            offsets.push((r.i32()?, r.i32()?));
        }
        Ok(Visibility {
            num_clusters: n,
            offsets,
        })
    }

    /// Decompress the PVS of `cluster` into a bit vector of `num_clusters` bits.
    pub fn pvs(&self, bsp: &Bsp<'_>, cluster: usize) -> Result<Vec<bool>> {
        let data = bsp.header.lump_data(bsp.data, Lump::Visibility)?;
        let mut out = vec![false; self.num_clusters];
        let Some(&(off, _)) = self.offsets.get(cluster) else {
            return Ok(out);
        };
        let mut pos = off as usize;
        let mut c = 0usize;
        while c < self.num_clusters {
            let b = *data.get(pos).ok_or(FormatError::Eof {
                offset: pos,
                needed: 1,
            })?;
            pos += 1;
            if b == 0 {
                let run = *data.get(pos).ok_or(FormatError::Eof {
                    offset: pos,
                    needed: 1,
                })? as usize;
                pos += 1;
                c += 8 * run.max(1);
                continue;
            }
            for bit in 0..8 {
                if c + bit < self.num_clusters && b & (1 << bit) != 0 {
                    out[c + bit] = true;
                }
            }
            c += 8;
        }
        Ok(out)
    }
}

impl Bsp<'_> {
    /// Leaf containing `point`, by walking the BSP tree from model 0.
    pub fn leaf_at(&self, point: [f32; 3]) -> usize {
        let mut node = self.models.first().map(|m| m.head_node).unwrap_or(0);
        while node >= 0 {
            let n = &self.nodes[node as usize];
            let p = &self.planes[n.plane_num as usize];
            let d =
                p.normal[0] * point[0] + p.normal[1] * point[1] + p.normal[2] * point[2] - p.dist;
            node = if d >= 0.0 {
                n.children[0]
            } else {
                n.children[1]
            };
        }
        (-node - 1) as usize
    }
}

/// `"x y z"` entity origin parsing re-exported for callers building prop lists.
pub fn parse_origin(s: &str) -> Option<[f32; 3]> {
    parse_vec3(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_sizes() {
        assert_eq!(StaticProps::record_size(6), Some(64));
        assert_eq!(StaticProps::record_size(11), Some(80));
        assert_eq!(StaticProps::record_size(3), None);
        assert_eq!(DispInfo::SIZE, 176);
        let d = DispInfo {
            start_position: [0.0; 3],
            disp_vert_start: 0,
            disp_tri_start: 0,
            power: 3,
            min_tess: 0,
            smoothing_angle: 0.0,
            contents: 0,
            map_face: 0,
            lightmap_alpha_start: 0,
            lightmap_sample_position_start: 0,
        };
        assert_eq!(d.vert_count(), 81);
        assert_eq!(d.tri_count(), 128);
    }
}
