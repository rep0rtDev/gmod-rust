//! Fixed-layout BSP lump records. Sizes and field order follow the public
//! "Source BSP File Format" documentation; every struct has a `SIZE` and a
//! `read` so lumps can be sliced into arrays and checked for exact division.

use crate::{FormatError, Reader, Result};

pub trait Record: Sized {
    const SIZE: usize;
    fn read(r: &mut Reader<'_>) -> Result<Self>;
}

pub fn read_all<T: Record>(what: &'static str, data: &[u8]) -> Result<Vec<T>> {
    if !data.len().is_multiple_of(T::SIZE) {
        return Err(FormatError::Malformed {
            what,
            detail: format!(
                "lump length {} is not a multiple of record size {}",
                data.len(),
                T::SIZE
            ),
        });
    }
    let mut r = Reader::new(data);
    let n = data.len() / T::SIZE;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(T::read(&mut r)?);
    }
    Ok(out)
}

fn vec3(r: &mut Reader<'_>) -> Result<[f32; 3]> {
    Ok([r.f32()?, r.f32()?, r.f32()?])
}

fn i16x3(r: &mut Reader<'_>) -> Result<[i16; 3]> {
    Ok([r.u16()? as i16, r.u16()? as i16, r.u16()? as i16])
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Plane {
    pub normal: [f32; 3],
    pub dist: f32,
    pub kind: i32,
}

impl Record for Plane {
    const SIZE: usize = 20;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Plane {
            normal: vec3(r)?,
            dist: r.f32()?,
            kind: r.i32()?,
        })
    }
}

pub type Vertex = [f32; 3];

impl Record for [f32; 3] {
    const SIZE: usize = 12;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        vec3(r)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub v: [u16; 2],
}

impl Record for Edge {
    const SIZE: usize = 4;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Edge {
            v: [r.u16()?, r.u16()?],
        })
    }
}

/// Signed edge index: negative means the edge is walked in reverse.
pub type SurfEdge = i32;

impl Record for i32 {
    const SIZE: usize = 4;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        r.i32()
    }
}

impl Record for u16 {
    const SIZE: usize = 2;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        r.u16()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Face {
    pub plane_num: u16,
    pub side: u8,
    pub on_node: u8,
    pub first_edge: i32,
    pub num_edges: i16,
    pub tex_info: i16,
    pub disp_info: i16,
    pub surface_fog_volume_id: i16,
    pub styles: [u8; 4],
    pub light_ofs: i32,
    pub area: f32,
    pub lightmap_mins: [i32; 2],
    pub lightmap_size: [i32; 2],
    pub orig_face: i32,
    pub num_prims: u16,
    pub first_prim_id: u16,
    pub smoothing_groups: u32,
}

impl Record for Face {
    const SIZE: usize = 56;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Face {
            plane_num: r.u16()?,
            side: r.u8()?,
            on_node: r.u8()?,
            first_edge: r.i32()?,
            num_edges: r.u16()? as i16,
            tex_info: r.u16()? as i16,
            disp_info: r.u16()? as i16,
            surface_fog_volume_id: r.u16()? as i16,
            styles: r.array::<4>()?,
            light_ofs: r.i32()?,
            area: r.f32()?,
            lightmap_mins: [r.i32()?, r.i32()?],
            lightmap_size: [r.i32()?, r.i32()?],
            orig_face: r.i32()?,
            num_prims: r.u16()?,
            first_prim_id: r.u16()?,
            smoothing_groups: r.u32()?,
        })
    }
}

impl Face {
    /// Luxel grid dimensions: size is the number of luxels minus one per axis.
    pub fn lightmap_dims(&self) -> (u32, u32) {
        (
            (self.lightmap_size[0] + 1) as u32,
            (self.lightmap_size[1] + 1) as u32,
        )
    }

    pub fn light_styles(&self) -> usize {
        self.styles.iter().take_while(|&&s| s != 255).count()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TexInfo {
    /// `[s|t][x, y, z, offset]`: texel = dot(pos, xyz) + offset.
    pub texture_vecs: [[f32; 4]; 2],
    pub lightmap_vecs: [[f32; 4]; 2],
    pub flags: i32,
    pub tex_data: i32,
}

impl Record for TexInfo {
    const SIZE: usize = 72;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let mut tv = [[0f32; 4]; 2];
        let mut lv = [[0f32; 4]; 2];
        for row in tv.iter_mut() {
            for v in row.iter_mut() {
                *v = r.f32()?;
            }
        }
        for row in lv.iter_mut() {
            for v in row.iter_mut() {
                *v = r.f32()?;
            }
        }
        Ok(TexInfo {
            texture_vecs: tv,
            lightmap_vecs: lv,
            flags: r.i32()?,
            tex_data: r.i32()?,
        })
    }
}

pub mod surf {
    pub const LIGHT: i32 = 0x1;
    pub const SKY2D: i32 = 0x2;
    pub const SKY: i32 = 0x4;
    pub const WARP: i32 = 0x8;
    pub const TRANS: i32 = 0x10;
    pub const NOPORTAL: i32 = 0x20;
    pub const TRIGGER: i32 = 0x40;
    pub const NODRAW: i32 = 0x80;
    pub const HINT: i32 = 0x100;
    pub const SKIP: i32 = 0x200;
    pub const NOLIGHT: i32 = 0x400;
    pub const BUMPLIGHT: i32 = 0x800;
    pub const NOSHADOWS: i32 = 0x1000;
    pub const NODECALS: i32 = 0x2000;
    pub const NOCHOP: i32 = 0x4000;
    pub const HITBOX: i32 = 0x8000;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TexData {
    pub reflectivity: [f32; 3],
    pub name_string_table_id: i32,
    pub width: i32,
    pub height: i32,
    pub view_width: i32,
    pub view_height: i32,
}

impl Record for TexData {
    const SIZE: usize = 32;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(TexData {
            reflectivity: vec3(r)?,
            name_string_table_id: r.i32()?,
            width: r.i32()?,
            height: r.i32()?,
            view_width: r.i32()?,
            view_height: r.i32()?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Model {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub origin: [f32; 3],
    pub head_node: i32,
    pub first_face: i32,
    pub num_faces: i32,
}

impl Record for Model {
    const SIZE: usize = 48;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Model {
            mins: vec3(r)?,
            maxs: vec3(r)?,
            origin: vec3(r)?,
            head_node: r.i32()?,
            first_face: r.i32()?,
            num_faces: r.i32()?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Node {
    pub plane_num: i32,
    /// Negative child = leaf index `-(child + 1)`.
    pub children: [i32; 2],
    pub mins: [i16; 3],
    pub maxs: [i16; 3],
    pub first_face: u16,
    pub num_faces: u16,
    pub area: i16,
}

impl Record for Node {
    const SIZE: usize = 32;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let n = Node {
            plane_num: r.i32()?,
            children: [r.i32()?, r.i32()?],
            mins: i16x3(r)?,
            maxs: i16x3(r)?,
            first_face: r.u16()?,
            num_faces: r.u16()?,
            area: r.u16()? as i16,
        };
        r.skip(2)?;
        Ok(n)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Leaf {
    pub contents: i32,
    pub cluster: i16,
    /// 9 bits area, 7 bits flags, packed as in the file.
    pub area_flags: i16,
    pub mins: [i16; 3],
    pub maxs: [i16; 3],
    pub first_leaf_face: u16,
    pub num_leaf_faces: u16,
    pub first_leaf_brush: u16,
    pub num_leaf_brushes: u16,
    pub leaf_water_data_id: i16,
}

impl Leaf {
    pub fn area(&self) -> i16 {
        self.area_flags & 0x1ff
    }
    pub fn flags(&self) -> i16 {
        (self.area_flags >> 9) & 0x7f
    }

    /// Lump version 0 stored 24 bytes of ambient lighting inline (56-byte record);
    /// version 1 moved it to its own lump (32-byte record).
    pub fn read_lump(version: u32, data: &[u8]) -> Result<Vec<Leaf>> {
        let size = if version == 0 { 56 } else { 32 };
        if !data.len().is_multiple_of(size) {
            return Err(FormatError::Malformed {
                what: "BSP leafs",
                detail: format!("lump length {} not a multiple of {size}", data.len()),
            });
        }
        let mut r = Reader::new(data);
        let mut out = Vec::with_capacity(data.len() / size);
        for _ in 0..data.len() / size {
            let l = Leaf {
                contents: r.i32()?,
                cluster: r.u16()? as i16,
                area_flags: r.u16()? as i16,
                mins: i16x3(&mut r)?,
                maxs: i16x3(&mut r)?,
                first_leaf_face: r.u16()?,
                num_leaf_faces: r.u16()?,
                first_leaf_brush: r.u16()?,
                num_leaf_brushes: r.u16()?,
                leaf_water_data_id: r.u16()? as i16,
            };
            if version == 0 {
                r.skip(24)?;
            }
            r.skip(2)?;
            out.push(l);
        }
        Ok(out)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Brush {
    pub first_side: i32,
    pub num_sides: i32,
    pub contents: i32,
}

impl Record for Brush {
    const SIZE: usize = 12;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(Brush {
            first_side: r.i32()?,
            num_sides: r.i32()?,
            contents: r.i32()?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrushSide {
    pub plane_num: u16,
    pub tex_info: i16,
    pub disp_info: i16,
    pub bevel: i16,
}

impl Record for BrushSide {
    const SIZE: usize = 8;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        Ok(BrushSide {
            plane_num: r.u16()?,
            tex_info: r.u16()? as i16,
            disp_info: r.u16()? as i16,
            bevel: r.u16()? as i16,
        })
    }
}

pub mod contents {
    pub const EMPTY: i32 = 0;
    pub const SOLID: i32 = 0x1;
    pub const WINDOW: i32 = 0x2;
    pub const AUX: i32 = 0x4;
    pub const GRATE: i32 = 0x8;
    pub const SLIME: i32 = 0x10;
    pub const WATER: i32 = 0x20;
    pub const MIST: i32 = 0x40;
    pub const OPAQUE: i32 = 0x80;
    pub const TESTFOGVOLUME: i32 = 0x100;
    pub const TEAM1: i32 = 0x800;
    pub const TEAM2: i32 = 0x1000;
    pub const IGNORE_NODRAW_OPAQUE: i32 = 0x2000;
    pub const MOVEABLE: i32 = 0x4000;
    pub const AREAPORTAL: i32 = 0x8000;
    pub const PLAYERCLIP: i32 = 0x10000;
    pub const MONSTERCLIP: i32 = 0x20000;
    pub const ORIGIN: i32 = 0x1000000;
    pub const MONSTER: i32 = 0x2000000;
    pub const DEBRIS: i32 = 0x4000000;
    pub const DETAIL: i32 = 0x8000000;
    pub const TRANSLUCENT: i32 = 0x10000000;
    pub const LADDER: i32 = 0x20000000;
    pub const HITBOX: i32 = 0x40000000;
}

/// Lightmap sample: RGB with a shared exponent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorRgbExp32 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub exponent: i8,
}

impl Record for ColorRgbExp32 {
    const SIZE: usize = 4;
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let [cr, cg, cb, e] = r.array::<4>()?;
        Ok(ColorRgbExp32 {
            r: cr,
            g: cg,
            b: cb,
            exponent: e as i8,
        })
    }
}

impl ColorRgbExp32 {
    /// Linear RGB: `c * 2^exponent / 255`.
    pub fn to_linear(self) -> [f32; 3] {
        let s = (self.exponent as f32).exp2() / 255.0;
        [self.r as f32 * s, self.g as f32 * s, self.b as f32 * s]
    }
}

/// Entity lump: a sequence of `{ "key" "value" ... }` blocks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entity {
    pub pairs: Vec<(String, String)>,
}

impl Entity {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    pub fn classname(&self) -> &str {
        self.get("classname").unwrap_or("")
    }

    pub fn origin(&self) -> Option<[f32; 3]> {
        parse_vec3(self.get("origin")?)
    }

    pub fn angles(&self) -> Option<[f32; 3]> {
        parse_vec3(self.get("angles")?)
    }
}

pub fn parse_vec3(s: &str) -> Option<[f32; 3]> {
    let mut it = s.split_ascii_whitespace().map(|t| t.parse::<f32>().ok());
    Some([it.next()??, it.next()??, it.next()??])
}

pub fn parse_entities(data: &[u8]) -> Result<Vec<Entity>> {
    let text = std::str::from_utf8(data).map_err(|_| FormatError::Utf8 { offset: 0 })?;
    let text = text.trim_end_matches('\0');
    let mut out = Vec::new();
    let mut cur: Option<Entity> = None;
    for (ln, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line == "{" {
            if cur.is_some() {
                return Err(FormatError::Malformed {
                    what: "BSP entities",
                    detail: format!("nested block at line {}", ln + 1),
                });
            }
            cur = Some(Entity::default());
        } else if line == "}" {
            out.push(cur.take().ok_or_else(|| FormatError::Malformed {
                what: "BSP entities",
                detail: format!("stray '}}' at line {}", ln + 1),
            })?);
        } else if let Some(e) = cur.as_mut() {
            let mut parts = Vec::with_capacity(2);
            let mut rest = line;
            while let Some(start) = rest.find('"') {
                let after = &rest[start + 1..];
                let Some(end) = after.find('"') else { break };
                parts.push(&after[..end]);
                rest = &after[end + 1..];
            }
            if parts.len() >= 2 {
                e.pairs.push((parts[0].to_string(), parts[1].to_string()));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities_parse() {
        let src = b"{\n\"classname\" \"worldspawn\"\n\"skyname\" \"painted\"\n}\n{\n\"origin\" \"1 2.5 -3\"\n\"classname\" \"info_player_start\"\n\"angles\" \"0 90 0\"\n}\n\0";
        let ents = parse_entities(src).unwrap();
        assert_eq!(ents.len(), 2);
        assert_eq!(ents[0].classname(), "worldspawn");
        assert_eq!(ents[0].get("SKYNAME"), Some("painted"));
        assert_eq!(ents[1].origin(), Some([1.0, 2.5, -3.0]));
        assert_eq!(ents[1].angles(), Some([0.0, 90.0, 0.0]));
    }

    #[test]
    fn record_sizes_divide() {
        let bytes = vec![0u8; Face::SIZE * 3];
        assert_eq!(read_all::<Face>("faces", &bytes).unwrap().len(), 3);
        assert!(read_all::<Face>("faces", &bytes[..Face::SIZE + 1]).is_err());
        let leafs = Leaf::read_lump(1, &[0u8; 64]).unwrap();
        assert_eq!(leafs.len(), 2);
        assert_eq!(Leaf::read_lump(0, &[0u8; 56]).unwrap().len(), 1);
    }

    #[test]
    fn rgbexp() {
        let c = ColorRgbExp32 {
            r: 255,
            g: 0,
            b: 128,
            exponent: -1,
        };
        let l = c.to_linear();
        assert!((l[0] - 0.5).abs() < 1e-3);
        assert_eq!(l[1], 0.0);
    }
}
