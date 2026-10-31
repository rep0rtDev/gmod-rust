//! Model lighting data of a BSP: the world lights (`LUMP_WORLDLIGHTS` 15, HDR 54), the per-leaf
//! ambient cubes (`LUMP_LEAF_AMBIENT_INDEX` 52 / 51 and `LUMP_LEAF_AMBIENT_LIGHTING` 56 / 55, or
//! the 24 bytes inline in a version 0 leaf) and the static prop vertex lighting vrad bakes into
//! `sp_<n>.vhv` (`sp_hdr_<n>.vhv`) files in the pakfile lump. Layouts are the public BSP format.

use std::io::Read;

use super::lumps::ColorRgbExp32;
use super::{Bsp, Lump};
use crate::reader::Reader;
use crate::{FormatError, Result};

/// `emittype_t` of a world light.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmitType {
    Surface,
    Point,
    Spotlight,
    Skylight,
    QuakeLight,
    SkyAmbient,
    Unknown(i32),
}

impl EmitType {
    fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Surface,
            1 => Self::Point,
            2 => Self::Spotlight,
            3 => Self::Skylight,
            4 => Self::QuakeLight,
            5 => Self::SkyAmbient,
            o => Self::Unknown(o),
        }
    }
}

/// `dworldlight_t` (version 0, 88 bytes; version 1 adds `shadow_cast_offset`, 100 bytes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldLight {
    pub origin: [f32; 3],
    /// Linear light colour times brightness.
    pub intensity: [f32; 3],
    pub normal: [f32; 3],
    pub shadow_cast_offset: [f32; 3],
    pub cluster: i32,
    pub emit: EmitType,
    pub style: i32,
    /// Spotlights: cos of the inner / outer cone half angle.
    pub stopdot: f32,
    pub stopdot2: f32,
    pub exponent: f32,
    /// Cutoff distance (0 = none).
    pub radius: f32,
    pub constant_attn: f32,
    pub linear_attn: f32,
    pub quadratic_attn: f32,
    pub flags: i32,
    pub texinfo: i32,
    pub owner: i32,
}

impl WorldLight {
    fn read(r: &mut Reader<'_>, version: u32) -> Result<Self> {
        let v3 = |r: &mut Reader<'_>| -> Result<[f32; 3]> { Ok([r.f32()?, r.f32()?, r.f32()?]) };
        let origin = v3(r)?;
        let intensity = v3(r)?;
        let normal = v3(r)?;
        let shadow_cast_offset = if version >= 1 { v3(r)? } else { [0.0; 3] };
        Ok(WorldLight {
            origin,
            intensity,
            normal,
            shadow_cast_offset,
            cluster: r.i32()?,
            emit: EmitType::from_i32(r.i32()?),
            style: r.i32()?,
            stopdot: r.f32()?,
            stopdot2: r.f32()?,
            exponent: r.f32()?,
            radius: r.f32()?,
            constant_attn: r.f32()?,
            linear_attn: r.f32()?,
            quadratic_attn: r.f32()?,
            flags: r.i32()?,
            texinfo: r.i32()?,
            owner: r.i32()?,
        })
    }
}

/// World lights of the LDR (`hdr = false`) or HDR lump; the HDR lump falls back to LDR when
/// the map has none.
pub fn world_lights(bsp: &Bsp<'_>, hdr: bool) -> Result<Vec<WorldLight>> {
    let id = if hdr && bsp.header.lump(Lump::WorldLightsHdr).length > 0 {
        Lump::WorldLightsHdr
    } else {
        Lump::WorldLights
    };
    let info = bsp.header.lump(id);
    let data = bsp.header.lump_data(bsp.data, id)?;
    let size = if info.version >= 1 { 100 } else { 88 };
    if !data.len().is_multiple_of(size) {
        return Err(FormatError::Malformed {
            what: "BSP worldlights",
            detail: format!("lump length {} not a multiple of {size}", data.len()),
        });
    }
    let mut r = Reader::new(data);
    (0..data.len() / size)
        .map(|_| WorldLight::read(&mut r, info.version))
        .collect()
}

/// One ambient sample of a leaf: the light cube (+x, -x, +y, -y, +z, -z) at a point given as
/// fractions of the leaf box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AmbientSample {
    pub cube: [ColorRgbExp32; 6],
    pub pos: [u8; 3],
}

impl AmbientSample {
    pub fn position(&self, mins: [f32; 3], maxs: [f32; 3]) -> [f32; 3] {
        let f = |k: usize| mins[k] + (maxs[k] - mins[k]) * self.pos[k] as f32 / 255.0;
        [f(0), f(1), f(2)]
    }
}

fn read_cube(r: &mut Reader<'_>) -> Result<[ColorRgbExp32; 6]> {
    let mut cube = [ColorRgbExp32 {
        r: 0,
        g: 0,
        b: 0,
        exponent: 0,
    }; 6];
    for c in &mut cube {
        *c = ColorRgbExp32 {
            r: r.u8()?,
            g: r.u8()?,
            b: r.u8()?,
            exponent: r.u8()? as i8,
        };
    }
    Ok(cube)
}

/// Ambient cubes per leaf.
#[derive(Clone, Debug, Default)]
pub struct LeafAmbient {
    /// Per leaf: its samples (empty when the map has none for it).
    pub leaves: Vec<Vec<AmbientSample>>,
}

impl LeafAmbient {
    /// From the ambient lumps (LDR or HDR, HDR falling back to LDR), or the inline cubes of a
    /// version 0 leaf lump (one sample at the leaf centre).
    pub fn parse(bsp: &Bsp<'_>, hdr: bool) -> Result<Self> {
        let (idx_id, light_id) = if hdr && bsp.header.lump(Lump::LeafAmbientLightingHdr).length > 0
        {
            (Lump::LeafAmbientIndexHdr, Lump::LeafAmbientLightingHdr)
        } else {
            (Lump::LeafAmbientIndex, Lump::LeafAmbientLighting)
        };
        let n = bsp.leafs.len();
        if bsp.header.lump(light_id).length == 0 {
            // version 0 leafs: the cube sits after the 32-byte record
            let leaf_data = bsp.header.lump_data(bsp.data, Lump::Leafs)?;
            if bsp.header.lump(Lump::Leafs).version == 0 && leaf_data.len() == n * 56 {
                let mut leaves = Vec::with_capacity(n);
                for i in 0..n {
                    let mut r = Reader::at(leaf_data, i * 56 + 32);
                    leaves.push(vec![AmbientSample {
                        cube: read_cube(&mut r)?,
                        pos: [128; 3],
                    }]);
                }
                return Ok(Self { leaves });
            }
            return Ok(Self {
                leaves: vec![Vec::new(); n],
            });
        }
        let idx = bsp.header.lump_data(bsp.data, idx_id)?;
        let light = bsp.header.lump_data(bsp.data, light_id)?;
        if !light.len().is_multiple_of(28) || idx.len() < n * 4 {
            return Err(FormatError::Malformed {
                what: "BSP leaf ambient lighting",
                detail: format!(
                    "index {} bytes for {n} leafs, lighting {} bytes",
                    idx.len(),
                    light.len()
                ),
            });
        }
        let mut samples = Vec::with_capacity(light.len() / 28);
        let mut r = Reader::new(light);
        for _ in 0..light.len() / 28 {
            let cube = read_cube(&mut r)?;
            let pos = [r.u8()?, r.u8()?, r.u8()?];
            r.u8()?;
            samples.push(AmbientSample { cube, pos });
        }
        let mut ir = Reader::new(idx);
        let mut leaves = Vec::with_capacity(n);
        for _ in 0..n {
            let count = ir.u16()? as usize;
            let first = ir.u16()? as usize;
            leaves.push(
                samples
                    .get(first..(first + count).min(samples.len()))
                    .unwrap_or(&[])
                    .to_vec(),
            );
        }
        Ok(Self { leaves })
    }
}

/// Vertex colours vrad baked for one static prop (`sp_<n>.vhv`): per mesh of LOD 0, RGBA8 per
/// vertex in the mesh's vertex order.
#[derive(Clone, Debug, Default)]
pub struct StaticPropVertexLight {
    /// Per mesh (LOD 0 only): the colours of its vertices.
    pub meshes: Vec<Vec<[u8; 4]>>,
}

/// The pakfile lump as a zip archive.
pub fn pakfile<'a>(bsp: &Bsp<'a>) -> Result<zip::ZipArchive<std::io::Cursor<&'a [u8]>>> {
    let data = bsp.header.lump_data(bsp.data, Lump::PakFile)?;
    zip::ZipArchive::new(std::io::Cursor::new(data)).map_err(|e| FormatError::Malformed {
        what: "BSP pakfile",
        detail: e.to_string(),
    })
}

/// Names of the pakfile entries.
pub fn pakfile_names(bsp: &Bsp<'_>) -> Result<Vec<String>> {
    let z = pakfile(bsp)?;
    Ok(z.file_names().map(str::to_string).collect())
}

/// `sp_<index>.vhv` (`sp_hdr_<index>.vhv` first when `hdr`) of a static prop, `None` when the
/// map has no baked vertex lighting for it. Layout: a 40-byte header (version, checksum,
/// vertex flags, vertex size, vertex count, mesh count, 4 unused), then per mesh a 28-byte
/// record (lod, vertex count, offset from file start, 4 unused), then the vertices.
pub fn static_prop_vertex_light(
    bsp: &Bsp<'_>,
    index: usize,
    hdr: bool,
) -> Result<Option<StaticPropVertexLight>> {
    let mut z = pakfile(bsp)?;
    let names = [format!("sp_hdr_{index}.vhv"), format!("sp_{index}.vhv")];
    let names: Vec<&str> = if hdr {
        names.iter().map(String::as_str).collect()
    } else {
        vec![&names[1]]
    };
    let mut bytes = Vec::new();
    let mut found = false;
    for name in names {
        if let Ok(mut f) = z.by_name(name) {
            f.read_to_end(&mut bytes)
                .map_err(|e| FormatError::Malformed {
                    what: "BSP static prop lighting",
                    detail: e.to_string(),
                })?;
            found = true;
            break;
        }
    }
    if !found {
        return Ok(None);
    }
    let mut r = Reader::new(&bytes);
    let _version = r.i32()?;
    let _checksum = r.u32()?;
    let _flags = r.u32()?;
    let vertex_size = r.u32()? as usize;
    let _vertex_count = r.u32()?;
    let mesh_count = r.i32()?.max(0) as usize;
    r.skip(16)?;
    let mut meshes = Vec::new();
    for _ in 0..mesh_count {
        let lod = r.u32()?;
        let count = r.u32()? as usize;
        let offset = r.u32()? as usize;
        r.skip(16)?;
        if lod != 0 {
            continue;
        }
        let mut mr = Reader::at(&bytes, offset);
        let mut colors = Vec::with_capacity(count);
        for _ in 0..count {
            let c = mr.array::<4>()?;
            if vertex_size > 4 {
                mr.skip(vertex_size - 4)?;
            }
            colors.push(c);
        }
        meshes.push(colors);
    }
    Ok(Some(StaticPropVertexLight { meshes }))
}
