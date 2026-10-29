//! Source BSP (`VBSP`) header and lump directory, versions 19 to 21.
//!
//! Lump payload parsers live in submodules as they are implemented; this module
//! only locates them. Layout per the public VDC "Source BSP File Format" page.

use crate::{FormatError, Reader, Result};

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
