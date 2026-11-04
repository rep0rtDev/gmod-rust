//! The engine's compiled node graph (`maps/graphs/<map>.ain`, AI_GRAPH_VERSION 37): the
//! `info_node*` positions with their per-hull ground offsets, type and zone, and the links
//! between them with the movement capabilities each hull needs to cross (bits_CAP_MOVE_*).

use std::fmt;

/// Number of hull types the graph stores offsets and link capabilities for (HULL_HUMAN ..
/// HULL_MEDIUM_TALL).
pub const NUM_HULLS: usize = 10;

pub const NODE_GROUND: u8 = 2;
pub const NODE_AIR: u8 = 3;
pub const NODE_CLIMB: u8 = 4;
pub const NODE_WATER: u8 = 5;

pub const CAP_MOVE_GROUND: u8 = 0x01;
pub const CAP_MOVE_JUMP: u8 = 0x02;
pub const CAP_MOVE_FLY: u8 = 0x04;
pub const CAP_MOVE_CLIMB: u8 = 0x08;

#[derive(Debug, Clone, PartialEq)]
pub struct AinNode {
    pub pos: [f32; 3],
    pub yaw: f32,
    /// Height each hull stands at over `pos` on this node.
    pub offsets: [f32; NUM_HULLS],
    pub node_type: u8,
    pub info: u16,
    pub zone: i16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AinLink {
    pub src: u16,
    pub dst: u16,
    /// bits_CAP_MOVE_* a hull needs to travel the link (0: impassable for that hull).
    pub moves: [u8; NUM_HULLS],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ain {
    pub version: i32,
    pub map_version: i32,
    pub nodes: Vec<AinNode>,
    pub links: Vec<AinLink>,
    /// Hammer node id of each node (the lookup table at the file's end).
    pub wc_ids: Vec<i32>,
}

#[derive(Debug)]
pub struct AinError(&'static str);

impl fmt::Display for AinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ain: {}", self.0)
    }
}

impl std::error::Error for AinError {}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], AinError> {
        let end = self.pos.checked_add(n).ok_or(AinError("overflow"))?;
        let s = self.data.get(self.pos..end).ok_or(AinError("truncated"))?;
        self.pos = end;
        Ok(s)
    }
    fn i32(&mut self) -> Result<i32, AinError> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i16(&mut self) -> Result<i16, AinError> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u16(&mut self) -> Result<u16, AinError> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u8(&mut self) -> Result<u8, AinError> {
        Ok(self.take(1)?[0])
    }
    fn f32(&mut self) -> Result<f32, AinError> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
}

impl Ain {
    pub fn parse(data: &[u8]) -> Result<Self, AinError> {
        let mut r = Reader { data, pos: 0 };
        let version = r.i32()?;
        if version != 37 {
            return Err(AinError("unsupported graph version"));
        }
        let map_version = r.i32()?;
        let n = usize::try_from(r.i32()?).map_err(|_| AinError("node count"))?;
        let mut nodes = Vec::with_capacity(n.min(1 << 16));
        for _ in 0..n {
            let pos = [r.f32()?, r.f32()?, r.f32()?];
            let yaw = r.f32()?;
            let mut offsets = [0.0; NUM_HULLS];
            for o in &mut offsets {
                *o = r.f32()?;
            }
            let node_type = r.u8()?;
            let info = r.u16()?;
            let zone = r.i16()?;
            nodes.push(AinNode {
                pos,
                yaw,
                offsets,
                node_type,
                info,
                zone,
            });
        }
        let l = usize::try_from(r.i32()?).map_err(|_| AinError("link count"))?;
        let mut links = Vec::with_capacity(l.min(1 << 18));
        for _ in 0..l {
            let src = r.i16()?;
            let dst = r.i16()?;
            let mut moves = [0u8; NUM_HULLS];
            for m in &mut moves {
                *m = r.u8()?;
            }
            if src < 0 || dst < 0 || src as usize >= n || dst as usize >= n {
                return Err(AinError("link out of range"));
            }
            links.push(AinLink {
                src: src as u16,
                dst: dst as u16,
                moves,
            });
        }
        let mut wc_ids = Vec::with_capacity(n);
        for _ in 0..n {
            wc_ids.push(r.i32()?);
        }
        Ok(Ain {
            version,
            map_version,
            nodes,
            links,
            wc_ids,
        })
    }
}
