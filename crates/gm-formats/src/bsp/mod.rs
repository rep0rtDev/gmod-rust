//! Source BSP (`VBSP`) header and lump directory, versions 19 to 21.
//!
//! Lump payload parsers live in submodules as they are implemented; this module
//! only locates them. Layout per the public VDC "Source BSP File Format" page.

pub mod lumps;

use crate::{FormatError, Reader, Result};
use lumps::{read_all, Record};

pub const IDENT: &[u8; 4] = b"VBSP";
pub const HEADER_LUMPS: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LumpInfo {
    pub offset: u32,
    pub length: u32,
    pub version: u32,
    pub four_cc: [u8; 4],
}

impl LumpInfo {
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Uncompressed length when the lump is LZMA compressed (fourCC holds the size).
    pub fn uncompressed_len(&self) -> Option<u32> {
        let n = u32::from_le_bytes(self.four_cc);
        (n != 0).then_some(n)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub version: i32,
    pub lumps: [LumpInfo; HEADER_LUMPS],
    pub map_revision: i32,
}

/// Lump ids used by gmod-rust. Numbers are the public lump indices.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Lump {
    Entities = 0,
    Planes = 1,
    TexData = 2,
    Vertexes = 3,
    Visibility = 4,
    Nodes = 5,
    TexInfo = 6,
    Faces = 7,
    Lighting = 8,
    Occlusion = 9,
    Leafs = 10,
    FaceIds = 11,
    Edges = 12,
    SurfEdges = 13,
    Models = 14,
    WorldLights = 15,
    LeafFaces = 16,
    LeafBrushes = 17,
    Brushes = 18,
    BrushSides = 19,
    Areas = 20,
    AreaPortals = 21,
    DispInfo = 26,
    OriginalFaces = 27,
    PhysDisp = 28,
    PhysCollide = 29,
    VertNormals = 30,
    VertNormalIndices = 31,
    DispLightmapAlphas = 32,
    DispVerts = 33,
    DispLightmapSamplePositions = 34,
    GameLump = 35,
    LeafWaterData = 36,
    Primitives = 37,
    PrimVerts = 38,
    PrimIndices = 39,
    PakFile = 40,
    ClipPortalVerts = 41,
    Cubemaps = 42,
    TexDataStringData = 43,
    TexDataStringTable = 44,
    Overlays = 45,
    LeafMinDistToWater = 46,
    FaceMacroTextureInfo = 47,
    DispTris = 48,
    PhysCollideSurface = 49,
    WaterOverlays = 50,
    LeafAmbientIndexHdr = 51,
    LeafAmbientIndex = 52,
    LightingHdr = 53,
    WorldLightsHdr = 54,
    LeafAmbientLightingHdr = 55,
    LeafAmbientLighting = 56,
    XzipPakFile = 57,
    FacesHdr = 58,
    MapFlags = 59,
    OverlayFades = 60,
}

impl Header {
    pub const SIZE: usize = 4 + 4 + HEADER_LUMPS * 16 + 4;

    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let ident = r.array::<4>()?;
        if &ident != IDENT {
            return Err(FormatError::BadMagic {
                expected: "VBSP",
                found: ident.to_vec(),
            });
        }
        let version = r.i32()?;
        if !(19..=21).contains(&version) {
            return Err(FormatError::UnsupportedVersion {
                what: "BSP",
                version: version as u32,
            });
        }
        let mut lumps = [LumpInfo::default(); HEADER_LUMPS];
        for l in lumps.iter_mut() {
            l.offset = r.u32()?;
            l.length = r.u32()?;
            l.version = r.u32()?;
            l.four_cc = r.array::<4>()?;
        }
        let map_revision = r.i32()?;
        Ok(Header {
            version,
            lumps,
            map_revision,
        })
    }

    pub fn lump(&self, id: Lump) -> &LumpInfo {
        &self.lumps[id as usize]
    }

    /// Raw lump bytes. Compressed lumps are returned as stored; see `uncompressed_len`.
    pub fn lump_data<'a>(&self, data: &'a [u8], id: Lump) -> Result<&'a [u8]> {
        let l = self.lump(id);
        let start = l.offset as usize;
        let end = start + l.length as usize;
        data.get(start..end).ok_or(FormatError::Eof {
            offset: start,
            needed: l.length as usize,
        })
    }
}

/// Parsed world geometry lumps of a map. Game lump, displacements, PVS and
/// physics data are read by their own modules as they are implemented.
pub struct Bsp<'a> {
    pub data: &'a [u8],
    pub header: Header,
    pub planes: Vec<lumps::Plane>,
    pub vertexes: Vec<lumps::Vertex>,
    pub edges: Vec<lumps::Edge>,
    pub surf_edges: Vec<lumps::SurfEdge>,
    pub faces: Vec<lumps::Face>,
    pub tex_infos: Vec<lumps::TexInfo>,
    pub tex_datas: Vec<lumps::TexData>,
    pub tex_names: Vec<String>,
    pub models: Vec<lumps::Model>,
    pub nodes: Vec<lumps::Node>,
    pub leafs: Vec<lumps::Leaf>,
    pub leaf_faces: Vec<u16>,
    pub leaf_brushes: Vec<u16>,
    pub brushes: Vec<lumps::Brush>,
    pub brush_sides: Vec<lumps::BrushSide>,
    pub entities: Vec<lumps::Entity>,
}

impl<'a> Bsp<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        let header = Header::parse(data)?;
        let lump = |id: Lump| header.lump_data(data, id);
        let tex_datas: Vec<lumps::TexData> = read_all("texdata", lump(Lump::TexData)?)?;
        let string_data = lump(Lump::TexDataStringData)?;
        let offsets: Vec<i32> = read_all("texdata string table", lump(Lump::TexDataStringTable)?)?;
        let mut tex_names = Vec::with_capacity(offsets.len());
        for off in offsets {
            let mut r = Reader::at(string_data, off as usize);
            tex_names.push(r.cstr()?.to_ascii_lowercase());
        }
        let leaf_info = header.lump(Lump::Leafs);
        Ok(Bsp {
            data,
            planes: read_all("planes", lump(Lump::Planes)?)?,
            vertexes: read_all("vertexes", lump(Lump::Vertexes)?)?,
            edges: read_all("edges", lump(Lump::Edges)?)?,
            surf_edges: read_all("surfedges", lump(Lump::SurfEdges)?)?,
            faces: read_all("faces", lump(Lump::Faces)?)?,
            tex_infos: read_all("texinfo", lump(Lump::TexInfo)?)?,
            tex_datas,
            tex_names,
            models: read_all("models", lump(Lump::Models)?)?,
            nodes: read_all("nodes", lump(Lump::Nodes)?)?,
            leafs: lumps::Leaf::read_lump(leaf_info.version, lump(Lump::Leafs)?)?,
            leaf_faces: read_all("leaffaces", lump(Lump::LeafFaces)?)?,
            leaf_brushes: read_all("leafbrushes", lump(Lump::LeafBrushes)?)?,
            brushes: read_all("brushes", lump(Lump::Brushes)?)?,
            brush_sides: read_all("brushsides", lump(Lump::BrushSides)?)?,
            entities: lumps::parse_entities(lump(Lump::Entities)?)?,
            header,
        })
    }

    /// Material name (`concrete/concretefloor001a`) for a face, if any.
    pub fn face_material(&self, face: &lumps::Face) -> Option<&str> {
        let ti = self.tex_infos.get(usize::try_from(face.tex_info).ok()?)?;
        let td = self.tex_datas.get(usize::try_from(ti.tex_data).ok()?)?;
        self.tex_names
            .get(usize::try_from(td.name_string_table_id).ok()?)
            .map(String::as_str)
    }

    /// Vertex positions of a face polygon, in winding order.
    pub fn face_vertices(&self, face: &lumps::Face) -> Vec<lumps::Vertex> {
        let start = face.first_edge as usize;
        let n = face.num_edges.max(0) as usize;
        let mut out = Vec::with_capacity(n);
        for se in self.surf_edges.iter().skip(start).take(n) {
            let edge = &self.edges[se.unsigned_abs() as usize];
            let v = if *se >= 0 { edge.v[0] } else { edge.v[1] };
            out.push(self.vertexes[v as usize]);
        }
        out
    }

    /// LDR lightmap samples for a face: `styles * w * h` entries, style-major.
    pub fn face_lightmap(&self, face: &lumps::Face) -> Result<Vec<lumps::ColorRgbExp32>> {
        if face.light_ofs < 0 {
            return Ok(Vec::new());
        }
        let (w, h) = face.lightmap_dims();
        let count = (w * h) as usize * face.light_styles();
        let lighting = self.header.lump_data(self.data, Lump::Lighting)?;
        let start = face.light_ofs as usize;
        let bytes = lighting
            .get(start..start + count * lumps::ColorRgbExp32::SIZE)
            .ok_or(FormatError::Eof {
                offset: start,
                needed: count * 4,
            })?;
        read_all("lighting", bytes)
    }

    pub fn worldspawn(&self) -> Option<&lumps::Entity> {
        self.entities.iter().find(|e| e.classname() == "worldspawn")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic(version: i32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(IDENT);
        v.extend_from_slice(&version.to_le_bytes());
        for i in 0..HEADER_LUMPS as u32 {
            let (off, len): (u32, u32) = if i == Lump::Entities as u32 {
                (Header::SIZE as u32, 5)
            } else {
                (0, 0)
            };
            v.extend_from_slice(&off.to_le_bytes());
            v.extend_from_slice(&len.to_le_bytes());
            v.extend_from_slice(&0u32.to_le_bytes());
            v.extend_from_slice(&[0, 0, 0, 0]);
        }
        v.extend_from_slice(&1234i32.to_le_bytes());
        v.extend_from_slice(b"hello");
        v
    }

    #[test]
    fn parses_header_and_lump() {
        let d = synthetic(20);
        let h = Header::parse(&d).unwrap();
        assert_eq!(h.version, 20);
        assert_eq!(h.map_revision, 1234);
        assert_eq!(h.lump_data(&d, Lump::Entities).unwrap(), b"hello");
        assert!(h.lump(Lump::Planes).is_empty());
    }

    #[test]
    fn rejects_unknown_version() {
        assert!(matches!(
            Header::parse(&synthetic(17)).unwrap_err(),
            FormatError::UnsupportedVersion { .. }
        ));
    }
}
