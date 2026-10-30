//! Studio models: `.mdl` (header, bones, body parts, textures), `.vvd`
//! (vertices) and `.vtx` (triangle strips per LOD). Versions 44 to 49, as
//! shipped with Half-Life 2 and Garry's Mod. Layouts follow the public
//! "MDL (Source)", "VVD" and "VTX" documentation.

use crate::{FormatError, Reader, Result};

pub const MDL_ID: &[u8; 4] = b"IDST";
pub const VVD_ID: &[u8; 4] = b"IDSV";

fn vec3(r: &mut Reader<'_>) -> Result<[f32; 3]> {
    Ok([r.f32()?, r.f32()?, r.f32()?])
}

fn cstr_at(data: &[u8], pos: usize) -> Result<&str> {
    let mut r = Reader::at(data, pos);
    r.cstr()
}

fn offset(base: usize, rel: i32) -> usize {
    (base as i64 + rel as i64).max(0) as usize
}

#[derive(Clone, Debug)]
pub struct Header {
    pub version: i32,
    pub checksum: i32,
    pub name: String,
    pub length: i32,
    pub eye_position: [f32; 3],
    pub illum_position: [f32; 3],
    pub hull_min: [f32; 3],
    pub hull_max: [f32; 3],
    pub view_bbmin: [f32; 3],
    pub view_bbmax: [f32; 3],
    pub flags: i32,
    pub bone_count: i32,
    pub bone_offset: i32,
    pub hitbox_set_count: i32,
    pub hitbox_set_offset: i32,
    pub local_anim_count: i32,
    pub local_anim_offset: i32,
    pub local_seq_count: i32,
    pub local_seq_offset: i32,
    pub local_pose_param_count: i32,
    pub local_pose_param_offset: i32,
    pub texture_count: i32,
    pub texture_offset: i32,
    pub texture_dir_count: i32,
    pub texture_dir_offset: i32,
    pub skin_reference_count: i32,
    pub skin_family_count: i32,
    pub skin_reference_offset: i32,
    pub body_part_count: i32,
    pub body_part_offset: i32,
    pub attachment_count: i32,
    pub attachment_offset: i32,
    pub surface_prop_offset: i32,
    pub key_value_offset: i32,
    pub key_value_count: i32,
    pub mass: f32,
    pub contents: i32,
    pub include_model_count: i32,
    pub include_model_offset: i32,
    pub root_lod: u8,
    pub allowed_root_lods: u8,
}

impl Header {
    pub const SIZE: usize = 408;

    pub fn parse(data: &[u8]) -> Result<Self> {
        let mut r = Reader::new(data);
        let id = r.array::<4>()?;
        if &id != MDL_ID {
            return Err(FormatError::BadMagic {
                expected: "IDST",
                found: id.to_vec(),
            });
        }
        let version = r.i32()?;
        if !(44..=49).contains(&version) {
            return Err(FormatError::UnsupportedVersion {
                what: "MDL",
                version: version as u32,
            });
        }
        let checksum = r.i32()?;
        let name_bytes = r.bytes(64)?;
        let name = String::from_utf8_lossy(name_bytes.split(|&b| b == 0).next().unwrap_or(&[]))
            .into_owned();
        let length = r.i32()?;
        let eye_position = vec3(&mut r)?;
        let illum_position = vec3(&mut r)?;
        let hull_min = vec3(&mut r)?;
        let hull_max = vec3(&mut r)?;
        let view_bbmin = vec3(&mut r)?;
        let view_bbmax = vec3(&mut r)?;
        let flags = r.i32()?;
        let bone_count = r.i32()?;
        let bone_offset = r.i32()?;
        r.skip(8)?; // bone controllers
        let hitbox_set_count = r.i32()?;
        let hitbox_set_offset = r.i32()?;
        let local_anim_count = r.i32()?;
        let local_anim_offset = r.i32()?;
        let local_seq_count = r.i32()?;
        let local_seq_offset = r.i32()?;
        r.skip(8)?; // activity list version, events indexed
        let texture_count = r.i32()?;
        let texture_offset = r.i32()?;
        let texture_dir_count = r.i32()?;
        let texture_dir_offset = r.i32()?;
        let skin_reference_count = r.i32()?;
        let skin_family_count = r.i32()?;
        let skin_reference_offset = r.i32()?;
        let body_part_count = r.i32()?;
        let body_part_offset = r.i32()?;
        let attachment_count = r.i32()?;
        let attachment_offset = r.i32()?;
        r.skip(12)?; // local nodes
        r.skip(8)?; // flex descs
        r.skip(8)?; // flex controllers
        r.skip(8)?; // flex rules
        r.skip(8)?; // ik chains
        r.skip(8)?; // mouths
        let local_pose_param_count = r.i32()?;
        let local_pose_param_offset = r.i32()?;
        let surface_prop_offset = r.i32()?;
        let key_value_offset = r.i32()?;
        let key_value_count = r.i32()?;
        r.skip(8)?; // ik locks
        let mass = r.f32()?;
        let contents = r.i32()?;
        let include_model_count = r.i32()?;
        let include_model_offset = r.i32()?;
        r.skip(4)?; // virtual model ptr
        r.skip(16)?; // anim blocks
        r.skip(4)?; // bone table name index
        r.skip(8)?; // vertex base, offset2
        r.skip(1)?; // directional dot product
        let root_lod = r.u8()?;
        let allowed_root_lods = r.u8()?;
        Ok(Header {
            version,
            checksum,
            name,
            length,
            eye_position,
            illum_position,
            hull_min,
            hull_max,
            view_bbmin,
            view_bbmax,
            flags,
            bone_count,
            bone_offset,
            hitbox_set_count,
            hitbox_set_offset,
            local_anim_count,
            local_anim_offset,
            local_seq_count,
            local_seq_offset,
            local_pose_param_count,
            local_pose_param_offset,
            texture_count,
            texture_offset,
            texture_dir_count,
            texture_dir_offset,
            skin_reference_count,
            skin_family_count,
            skin_reference_offset,
            body_part_count,
            body_part_offset,
            attachment_count,
            attachment_offset,
            surface_prop_offset,
            key_value_offset,
            key_value_count,
            mass,
            contents,
            include_model_count,
            include_model_offset,
            root_lod,
            allowed_root_lods,
        })
    }

    pub fn is_static_prop(&self) -> bool {
        self.flags & 0x10 != 0
    }
}

#[derive(Clone, Debug)]
pub struct Bone {
    pub name: String,
    pub parent: i32,
    pub pos: [f32; 3],
    pub quat: [f32; 4],
    pub rot: [f32; 3],
    /// Per-axis scale of run-length encoded position / rotation animation values.
    pub pos_scale: [f32; 3],
    pub rot_scale: [f32; 3],
    /// Row-major 3x4 pose-to-bone matrix.
    pub pose_to_bone: [[f32; 4]; 3],
    pub flags: i32,
    pub physics_bone: i32,
    pub surface_prop: String,
    pub contents: i32,
}

#[derive(Clone, Debug)]
pub struct Mesh {
    pub material: i32,
    pub model_index: i32,
    pub num_vertices: i32,
    pub vertex_offset: i32,
    pub mesh_id: i32,
    pub center: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct Model {
    pub name: String,
    pub bounding_radius: f32,
    pub meshes: Vec<Mesh>,
    pub num_vertices: i32,
    /// Byte offset of this model's first vertex in the VVD vertex block.
    pub vertex_index: i32,
}

#[derive(Clone, Debug)]
pub struct BodyPart {
    pub name: String,
    pub base: i32,
    pub models: Vec<Model>,
}

#[derive(Clone, Debug)]
pub struct Texture {
    pub name: String,
    pub flags: i32,
}

/// Parsed `.mdl`.
#[derive(Clone, Debug)]
pub struct Mdl {
    pub header: Header,
    pub bones: Vec<Bone>,
    pub body_parts: Vec<BodyPart>,
    pub textures: Vec<Texture>,
    pub texture_dirs: Vec<String>,
    /// `skins[family][reference]` = texture index.
    pub skins: Vec<Vec<i16>>,
    pub surface_prop: String,
    pub key_values: String,
}

/// One hit box of the model's default hit box set (`mstudiobbox_t`): an AABB in its bone's
/// space, with the hit group the game reports (1 head, 2 chest, 3 stomach, 4/5 arms, 6/7 legs).
#[derive(Debug, Clone, PartialEq)]
pub struct HitBox {
    pub bone: i32,
    pub group: i32,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
}

/// The default hit box set (set 0) of a model, empty when the model has none.
pub fn hitboxes(data: &[u8], header: &Header) -> Vec<HitBox> {
    let mut out = Vec::new();
    if header.hitbox_set_count <= 0 || header.hitbox_set_offset <= 0 {
        return out;
    }
    let i32_at = |o: usize| -> Option<i32> {
        data.get(o..o + 4)
            .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
    };
    let f32_at = |o: usize| -> Option<f32> {
        data.get(o..o + 4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
    };
    let so = header.hitbox_set_offset as usize;
    let (Some(n), Some(off)) = (i32_at(so + 4), i32_at(so + 8)) else {
        return out;
    };
    for i in 0..n.max(0) as usize {
        let o = so + off as usize + i * 68;
        let (Some(bone), Some(group)) = (i32_at(o), i32_at(o + 4)) else {
            break;
        };
        let v = |k: usize| -> Option<[f32; 3]> {
            Some([f32_at(o + k)?, f32_at(o + k + 4)?, f32_at(o + k + 8)?])
        };
        let (Some(mins), Some(maxs)) = (v(8), v(20)) else {
            break;
        };
        out.push(HitBox {
            bone,
            group,
            mins,
            maxs,
        });
    }
    out
}

/// A sequence of the model (`mstudioseqdesc_t`, 212 bytes): its label, the first animation
/// of its blend grid and the layers it adds on top.
#[derive(Debug, Clone, PartialEq)]
pub struct Sequence {
    pub name: String,
    pub anim: i16,
    pub autolayers: Vec<AutoLayer>,
    /// Blend grid: `group_size[0]` columns by `group_size[1]` rows of animation indices, row
    /// major in `blends`; `anim` is `blends[0]`.
    pub group_size: [i32; 2],
    pub blends: Vec<i16>,
    /// Pose parameter driving each grid axis (-1 = none) and the parameter range it spans.
    pub param: [i32; 2],
    pub param_start: [f32; 2],
    pub param_end: [f32; 2],
}

impl Sequence {
    /// Animation index at grid cell (`x`, `y`).
    pub fn blend(&self, x: usize, y: usize) -> Option<i16> {
        self.blends
            .get(y * self.group_size[0].max(1) as usize + x)
            .copied()
    }

    /// Where pose value `value` of axis `axis` lands on the grid: the lower cell and the
    /// fraction towards the next one.
    pub fn grid_pos(&self, axis: usize, value: f32) -> (usize, f32) {
        let n = self.group_size[axis].max(1) as usize;
        if n < 2 || self.param_end[axis] == self.param_start[axis] {
            return (0, 0.0);
        }
        let t = ((value - self.param_start[axis])
            / (self.param_end[axis] - self.param_start[axis]))
            .clamp(0.0, 1.0)
            * (n - 1) as f32;
        let i = (t.floor() as usize).min(n - 2);
        (i, t - i as f32)
    }
}

/// A pose parameter (`mstudioposeparamdesc_t`, 20 bytes): a named value in `start..end` the
/// blend grids are driven by; `loop_range` > 0 wraps it (angles).
#[derive(Debug, Clone, PartialEq)]
pub struct PoseParam {
    pub name: String,
    pub flags: i32,
    pub start: f32,
    pub end: f32,
    pub loop_range: f32,
}

/// The pose parameters of a model.
pub fn pose_params(data: &[u8], header: &Header) -> Vec<PoseParam> {
    let f = |p: usize| f32::from_bits(i32_at(data, p).unwrap_or(0) as u32);
    (0..header.local_pose_param_count.max(0) as usize)
        .filter_map(|i| {
            let o = header.local_pose_param_offset as usize + i * 20;
            Some(PoseParam {
                name: name_at(data, o.checked_add_signed(i32_at(data, o)? as isize)?)?,
                flags: i32_at(data, o + 4)?,
                start: f(o + 8),
                end: f(o + 12),
                loop_range: f(o + 16),
            })
        })
        .collect()
}

/// A layer a sequence blends in (`mstudioautolayer_t`, 24 bytes): another sequence (usually a
/// delta animation) whose weight ramps 0 → 1 over `start..peak` of the cycle, holds 1 to
/// `tail` and fades to 0 at `end`; `flags` 0x4000 marks a pose-parameter layer.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoLayer {
    pub sequence: i16,
    pub pose: i16,
    pub flags: i32,
    pub start: f32,
    pub peak: f32,
    pub tail: f32,
    pub end: f32,
}

impl AutoLayer {
    /// Blend weight at `cycle` (0..1 through the host sequence).
    pub fn weight(&self, cycle: f32) -> f32 {
        if self.start == self.end {
            return 1.0;
        }
        if cycle < self.start || cycle > self.end {
            0.0
        } else if cycle < self.peak && self.peak > self.start {
            (cycle - self.start) / (self.peak - self.start)
        } else if cycle > self.tail && self.end > self.tail {
            (self.end - cycle) / (self.end - self.tail)
        } else {
            1.0
        }
    }
}

/// An animation (`mstudioanimdesc_t`, 100 bytes).
#[derive(Debug, Clone, PartialEq)]
pub struct AnimDesc {
    pub name: String,
    pub fps: f32,
    pub flags: i32,
    pub frames: i32,
    /// External animation block (0 = the data is in the `.mdl`).
    pub block: i32,
    pub index: i32,
    /// Byte offset of this descriptor in the `.mdl`; `index` and the section table are
    /// relative to it.
    pub offset: usize,
    /// Long animations are cut into sections of `section_frames` frames, each with its own
    /// (block, index) pair at `section_index`.
    pub section_index: i32,
    pub section_frames: i32,
}

impl AnimDesc {
    /// Where frame `frame` of this animation lives: `(block, offset, frame)`. Block 0 is the
    /// `.mdl` itself with `offset` from the file start; another block is a range of the `.ani`
    /// (see [`anim_blocks`]) with `offset` from the block start. Sectioned animations go
    /// through their section table in `data` (the `.mdl`).
    pub fn frame_block(&self, data: &[u8], frame: usize) -> Option<(i32, usize, usize)> {
        if self.section_frames <= 0 {
            let offset = if self.block == 0 {
                self.offset.checked_add_signed(self.index as isize)?
            } else {
                usize::try_from(self.index).ok()?
            };
            return Some((self.block, offset, frame));
        }
        let section = frame / self.section_frames as usize;
        let p = self.offset + self.section_index as usize + section * 8;
        let block = i32_at(data, p)?;
        let index = i32_at(data, p + 4)?;
        let offset = if block == 0 {
            self.offset.checked_add_signed(index as isize)?
        } else {
            usize::try_from(index).ok()?
        };
        Some((block, offset, frame % self.section_frames as usize))
    }

    /// [`frame_block`](Self::frame_block) for data in the `.mdl` only: `(offset, frame)`.
    pub fn frame_data(&self, data: &[u8], frame: usize) -> Option<(usize, usize)> {
        let (block, offset, frame) = self.frame_block(data, frame)?;
        (block == 0).then_some((offset, frame))
    }
}

/// One bone of a static (single-frame) pose read from an animation.
#[derive(Debug, Clone, PartialEq)]
pub struct BonePose {
    pub bone: u8,
    pub pos: Option<[f32; 3]>,
    pub quat: Option<[f32; 4]>,
}

fn i32_at(data: &[u8], o: usize) -> Option<i32> {
    data.get(o..o + 4)
        .map(|b| i32::from_le_bytes(b.try_into().unwrap()))
}

fn i16_at(data: &[u8], o: usize) -> Option<i16> {
    data.get(o..o + 2)
        .map(|b| i16::from_le_bytes(b.try_into().unwrap()))
}

fn name_at(data: &[u8], o: usize) -> Option<String> {
    cstr_at(data, o).ok().map(str::to_owned)
}

/// The sequences of a model.
pub fn sequences(data: &[u8], header: &Header) -> Vec<Sequence> {
    let mut out = Vec::new();
    for i in 0..header.local_seq_count.max(0) as usize {
        let o = header.local_seq_offset as usize + i * 212;
        let (Some(name_off), Some(anim_idx)) = (i32_at(data, o + 4), i32_at(data, o + 60)) else {
            break;
        };
        let (Some(name), Some(anim)) = (
            name_at(data, o + name_off as usize),
            i16_at(data, o + anim_idx as usize),
        ) else {
            break;
        };
        let layers = i32_at(data, o + 148).unwrap_or(0).max(0) as usize;
        let layer_off = i32_at(data, o + 152).unwrap_or(0);
        let f = |p: usize| f32::from_bits(i32_at(data, p).unwrap_or(0) as u32);
        let autolayers = (0..layers)
            .filter_map(|l| {
                let p = o.checked_add_signed(layer_off as isize)? + l * 24;
                Some(AutoLayer {
                    sequence: i16_at(data, p)?,
                    pose: i16_at(data, p + 2)?,
                    flags: i32_at(data, p + 4)?,
                    start: f(p + 8),
                    peak: f(p + 12),
                    tail: f(p + 16),
                    end: f(p + 20),
                })
            })
            .collect();
        let group_size = [
            i32_at(data, o + 68).unwrap_or(1).max(1),
            i32_at(data, o + 72).unwrap_or(1).max(1),
        ];
        let blends = (0..(group_size[0] * group_size[1]) as usize)
            .filter_map(|i| i16_at(data, o + anim_idx as usize + i * 2))
            .collect();
        let f = |p: usize| f32::from_bits(i32_at(data, p).unwrap_or(0) as u32);
        out.push(Sequence {
            name,
            anim,
            autolayers,
            group_size,
            blends,
            param: [
                i32_at(data, o + 76).unwrap_or(-1),
                i32_at(data, o + 80).unwrap_or(-1),
            ],
            param_start: [f(o + 84), f(o + 88)],
            param_end: [f(o + 92), f(o + 96)],
        });
    }
    out
}

/// Models whose animations this one includes (`mstudiomodelgroup_t`: label, file name).
pub fn include_models(data: &[u8], header: &Header) -> Vec<String> {
    (0..header.include_model_count.max(0) as usize)
        .filter_map(|i| {
            let o = header.include_model_offset as usize + i * 8;
            name_at(data, o + i32_at(data, o + 4)? as usize)
        })
        .collect()
}

/// The animations of a model.
pub fn anim_descs(data: &[u8], header: &Header) -> Vec<AnimDesc> {
    let mut out = Vec::new();
    for i in 0..header.local_anim_count.max(0) as usize {
        let o = header.local_anim_offset as usize + i * 100;
        let Some(name_off) = i32_at(data, o + 4) else {
            break;
        };
        let Some(name) = name_at(data, o + name_off as usize) else {
            break;
        };
        let fps = f32::from_bits(i32_at(data, o + 8).unwrap_or(0) as u32);
        out.push(AnimDesc {
            name,
            fps,
            flags: i32_at(data, o + 12).unwrap_or(0),
            frames: i32_at(data, o + 16).unwrap_or(0),
            block: i32_at(data, o + 52).unwrap_or(0),
            index: i32_at(data, o + 56).unwrap_or(0),
            offset: o,
            section_index: i32_at(data, o + 80).unwrap_or(0),
            section_frames: i32_at(data, o + 84).unwrap_or(0),
        });
    }
    out
}

/// The external animation block file (`.ani`) name and the `(start, end)` byte ranges of the
/// blocks in it. The header triple sits at byte 0x15C (`szanimblocknameindex`, `numanimblocks`,
/// `animblockindex`), after the fields `Header` reads.
pub fn anim_blocks(data: &[u8]) -> (String, Vec<(usize, usize)>) {
    let name = i32_at(data, 0x15c)
        .and_then(|o| name_at(data, o as usize))
        .unwrap_or_default();
    let n = i32_at(data, 0x160).unwrap_or(0).max(0) as usize;
    let off = i32_at(data, 0x164).unwrap_or(0) as usize;
    let blocks = (0..n)
        .filter_map(|b| {
            Some((
                i32_at(data, off + b * 8)? as usize,
                i32_at(data, off + b * 8 + 4)? as usize,
            ))
        })
        .collect();
    (name, blocks)
}

fn f16_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((h >> 10) & 0x1f) as i32;
    let mant = (h & 0x3ff) as f32;
    match exp {
        0 => sign * mant * 2f32.powi(-24),
        31 => {
            if mant == 0.0 {
                sign * f32::INFINITY
            } else {
                f32::NAN
            }
        }
        _ => sign * (1.0 + mant / 1024.0) * 2f32.powi(exp - 15),
    }
}

fn quat64(b: &[u8]) -> [f32; 4] {
    let v = u64::from_le_bytes(b[..8].try_into().unwrap());
    let c = |bits: u64| (bits as f32 - 1_048_576.0) * (1.0 / 1_048_576.5);
    let x = c(v & 0x1f_ffff);
    let y = c((v >> 21) & 0x1f_ffff);
    let z = c((v >> 42) & 0x1f_ffff);
    let w = (1.0 - x * x - y * y - z * z).max(0.0).sqrt();
    [x, y, z, if v >> 63 != 0 { -w } else { w }]
}

fn quat48(b: &[u8]) -> [f32; 4] {
    let u = |i: usize| u16::from_le_bytes(b[i..i + 2].try_into().unwrap());
    let c = |v: u16| (v as f32 - 32768.0) * (1.0 / 32768.0);
    let (x, y) = (c(u(0)), c(u(2)));
    let zw = u(4);
    let z = c(zw & 0x7fff);
    let w = (1.0 - x * x - y * y - z * z).max(0.0).sqrt();
    [x, y, z, if zw & 0x8000 != 0 { -w } else { w }]
}

/// The per-bone values of an animation whose bones are all raw (single-frame) encodings:
/// flags 0x01 raw position (three 16-bit floats), 0x02 48-bit quaternion, 0x20 64-bit
/// quaternion. Returns `None` on a run-length encoded bone (flags 0x04 / 0x08), which this does
/// not decode. `data` is the `.mdl` for block 0, else the `.ani`; `start` is the block start
/// plus the animation's `index`.
pub fn static_pose(data: &[u8], start: usize) -> Option<Vec<BonePose>> {
    let mut out = Vec::new();
    let mut p = start;
    loop {
        let bone = *data.get(p)?;
        let flags = *data.get(p + 1)?;
        let next = i16_at(data, p + 2)?;
        if flags & 0x0c != 0 {
            return None;
        }
        let mut q = p + 4;
        let mut quat = None;
        if flags & 0x20 != 0 {
            quat = Some(quat64(data.get(q..q + 8)?));
            q += 8;
        } else if flags & 0x02 != 0 {
            quat = Some(quat48(data.get(q..q + 6)?));
            q += 6;
        }
        let mut pos = None;
        if flags & 0x01 != 0 {
            let b = data.get(q..q + 6)?;
            let h = |i: usize| f16_to_f32(u16::from_le_bytes(b[i..i + 2].try_into().unwrap()));
            pos = Some([h(0), h(2), h(4)]);
        }
        out.push(BonePose { bone, pos, quat });
        if next == 0 {
            break;
        }
        p += next as usize;
    }
    Some(out)
}

/// Value `frame` of a run-length encoded track (`mstudioanimvalue_t`): chunks of a
/// `{valid, total}` header and `valid` shorts, the last short held for the rest of the chunk.
fn rle_value(data: &[u8], mut p: usize, mut frame: usize) -> Option<i16> {
    loop {
        let valid = *data.get(p)? as usize;
        let total = *data.get(p + 1)? as usize;
        if total == 0 {
            return None;
        }
        if frame < total {
            let i = if frame < valid { frame + 1 } else { valid };
            return i16_at(data, p + i * 2);
        }
        frame -= total;
        p += (1 + valid) * 2;
    }
}

/// Quaternion of Source radian Euler angles (x, y, z).
pub fn quat_from_euler(e: [f32; 3]) -> [f32; 4] {
    let (sx, cx) = (e[0] * 0.5).sin_cos();
    let (sy, cy) = (e[1] * 0.5).sin_cos();
    let (sz, cz) = (e[2] * 0.5).sin_cos();
    [
        sx * cy * cz - cx * sy * sz,
        cx * sy * cz + sx * cy * sz,
        cx * cy * sz - sx * sy * cz,
        cx * cy * cz + sx * sy * sz,
    ]
}

/// Local (position, quaternion) of every bone at `frame` of an animation: raw values (flags
/// 0x01 / 0x02 / 0x20) or run-length encoded tracks (0x04 position, 0x08 rotation) scaled by
/// the bone's `pos_scale` / `rot_scale` and added to the bind pose. For a `delta` animation
/// (`AnimDesc::flags` 0x04, layered onto another pose) the values are offsets: zero / identity
/// for untouched bones and for the base of each track. `data` / `start` as in [`static_pose`].
pub fn anim_pose(
    data: &[u8],
    start: usize,
    bones: &[Bone],
    frame: usize,
    delta: bool,
) -> Option<Vec<([f32; 3], [f32; 4])>> {
    let mut out: Vec<_> = bones
        .iter()
        .map(|b| {
            if delta {
                ([0.0; 3], [0.0, 0.0, 0.0, 1.0])
            } else {
                (b.pos, b.quat)
            }
        })
        .collect();
    let mut p = start;
    loop {
        let bone = *data.get(p)? as usize;
        let flags = *data.get(p + 1)?;
        let next = i16_at(data, p + 2)?;
        let Some(b) = bones.get(bone) else {
            break;
        };
        let delta = delta || flags & 0x10 != 0;
        let base = |v: [f32; 3]| if delta { [0.0; 3] } else { v };
        let mut q = p + 4;
        let (mut pos, mut quat) = out[bone];
        if flags & 0x20 != 0 {
            quat = quat64(data.get(q..q + 8)?);
            q += 8;
        } else if flags & 0x02 != 0 {
            quat = quat48(data.get(q..q + 6)?);
            q += 6;
        } else if flags & 0x08 != 0 {
            let mut e = base(b.rot);
            for (i, v) in e.iter_mut().enumerate() {
                let off = i16_at(data, q + i * 2)?;
                if off != 0 {
                    *v += rle_value(data, q + off as usize, frame)? as f32 * b.rot_scale[i];
                }
            }
            quat = quat_from_euler(e);
            q += 6;
        }
        if flags & 0x01 != 0 {
            let bytes = data.get(q..q + 6)?;
            let h = |i: usize| f16_to_f32(u16::from_le_bytes(bytes[i..i + 2].try_into().unwrap()));
            pos = [h(0), h(2), h(4)];
        } else if flags & 0x04 != 0 {
            pos = base(b.pos);
            for (i, v) in pos.iter_mut().enumerate() {
                let off = i16_at(data, q + i * 2)?;
                if off != 0 {
                    *v += rle_value(data, q + off as usize, frame)? as f32 * b.pos_scale[i];
                }
            }
        }
        out[bone] = (pos, quat);
        if next == 0 {
            break;
        }
        p += next as usize;
    }
    Some(out)
}

impl Mdl {
    pub fn parse(data: &[u8]) -> Result<Self> {
        let header = Header::parse(data)?;
        let mut bones = Vec::with_capacity(header.bone_count.max(0) as usize);
        for i in 0..header.bone_count.max(0) as usize {
            let base = header.bone_offset as usize + i * 216;
            let mut r = Reader::at(data, base);
            let name_idx = r.i32()?;
            let parent = r.i32()?;
            r.skip(24)?; // bone controllers
            let pos = vec3(&mut r)?;
            let quat = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];
            let rot = vec3(&mut r)?;
            let pos_scale = vec3(&mut r)?;
            let rot_scale = vec3(&mut r)?;
            let mut m = [[0f32; 4]; 3];
            for row in m.iter_mut() {
                for v in row.iter_mut() {
                    *v = r.f32()?;
                }
            }
            r.skip(16)?; // alignment quat
            let flags = r.i32()?;
            r.skip(8)?; // proc type/index
            let physics_bone = r.i32()?;
            let sp_idx = r.i32()?;
            let contents = r.i32()?;
            bones.push(Bone {
                name: cstr_at(data, offset(base, name_idx))?.to_string(),
                parent,
                pos,
                quat,
                rot,
                pos_scale,
                rot_scale,
                pose_to_bone: m,
                flags,
                physics_bone,
                surface_prop: if sp_idx != 0 {
                    cstr_at(data, offset(base, sp_idx))?.to_string()
                } else {
                    String::new()
                },
                contents,
            });
        }

        let mut body_parts = Vec::new();
        for i in 0..header.body_part_count.max(0) as usize {
            let base = header.body_part_offset as usize + i * 16;
            let mut r = Reader::at(data, base);
            let name_idx = r.i32()?;
            let num_models = r.i32()?;
            let bp_base = r.i32()?;
            let model_index = r.i32()?;
            let mut models = Vec::new();
            for m in 0..num_models.max(0) as usize {
                let mbase = offset(base, model_index) + m * 148;
                let mut mr = Reader::at(data, mbase);
                let name_bytes = mr.bytes(64)?;
                let name =
                    String::from_utf8_lossy(name_bytes.split(|&b| b == 0).next().unwrap_or(&[]))
                        .into_owned();
                mr.skip(4)?; // type
                let bounding_radius = mr.f32()?;
                let num_meshes = mr.i32()?;
                let mesh_index = mr.i32()?;
                let num_vertices = mr.i32()?;
                let vertex_index = mr.i32()?;
                let mut meshes = Vec::new();
                for k in 0..num_meshes.max(0) as usize {
                    let kbase = offset(mbase, mesh_index) + k * 116;
                    let mut kr = Reader::at(data, kbase);
                    let material = kr.i32()?;
                    let model_index = kr.i32()?;
                    let num_vertices = kr.i32()?;
                    let vertex_offset = kr.i32()?;
                    kr.skip(8)?; // flexes
                    kr.skip(8)?; // material type/param
                    let mesh_id = kr.i32()?;
                    let center = vec3(&mut kr)?;
                    meshes.push(Mesh {
                        material,
                        model_index,
                        num_vertices,
                        vertex_offset,
                        mesh_id,
                        center,
                    });
                }
                models.push(Model {
                    name,
                    bounding_radius,
                    meshes,
                    num_vertices,
                    vertex_index,
                });
            }
            body_parts.push(BodyPart {
                name: cstr_at(data, offset(base, name_idx))?.to_string(),
                base: bp_base,
                models,
            });
        }

        let mut textures = Vec::new();
        for i in 0..header.texture_count.max(0) as usize {
            let base = header.texture_offset as usize + i * 64;
            let mut r = Reader::at(data, base);
            let name_idx = r.i32()?;
            let flags = r.i32()?;
            textures.push(Texture {
                name: cstr_at(data, offset(base, name_idx))?
                    .replace('\\', "/")
                    .to_ascii_lowercase(),
                flags,
            });
        }
        let mut texture_dirs = Vec::new();
        for i in 0..header.texture_dir_count.max(0) as usize {
            let mut r = Reader::at(data, header.texture_dir_offset as usize + i * 4);
            let off = r.i32()?;
            texture_dirs.push(
                cstr_at(data, off as usize)?
                    .replace('\\', "/")
                    .to_ascii_lowercase(),
            );
        }
        let mut skins = Vec::new();
        let mut r = Reader::at(data, header.skin_reference_offset as usize);
        for _ in 0..header.skin_family_count.max(0) {
            let mut fam = Vec::new();
            for _ in 0..header.skin_reference_count.max(0) {
                fam.push(r.u16()? as i16);
            }
            skins.push(fam);
        }
        let surface_prop = if header.surface_prop_offset > 0 {
            cstr_at(data, header.surface_prop_offset as usize)?.to_string()
        } else {
            String::new()
        };
        let key_values = if header.key_value_count > 1 {
            let s = header.key_value_offset as usize;
            String::from_utf8_lossy(
                &data[s..(s + header.key_value_count as usize - 1).min(data.len())],
            )
            .into_owned()
        } else {
            String::new()
        };
        Ok(Mdl {
            header,
            bones,
            body_parts,
            textures,
            texture_dirs,
            skins,
            surface_prop,
            key_values,
        })
    }

    /// Material path candidates for a texture: `materials/<dir><name>.vmt` for each directory.
    pub fn material_paths(&self, texture: usize) -> Vec<String> {
        let Some(t) = self.textures.get(texture) else {
            return Vec::new();
        };
        self.texture_dirs
            .iter()
            .map(|d| {
                let d = d.trim_matches('/');
                if d.is_empty() {
                    format!("materials/{}.vmt", t.name)
                } else {
                    format!("materials/{d}/{}.vmt", t.name)
                }
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub weights: [f32; 3],
    pub bones: [u8; 3],
    pub num_bones: u8,
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

/// Parsed `.vvd` with fixups applied for the root LOD.
#[derive(Clone, Debug)]
pub struct Vvd {
    pub checksum: i32,
    pub num_lods: i32,
    pub lod_vertex_counts: [i32; 8],
    pub vertices: Vec<Vertex>,
    pub tangents: Vec<[f32; 4]>,
}

impl Vvd {
    pub fn parse(data: &[u8], lod: usize) -> Result<Self> {
        let mut r = Reader::new(data);
        let id = r.array::<4>()?;
        if &id != VVD_ID {
            return Err(FormatError::BadMagic {
                expected: "IDSV",
                found: id.to_vec(),
            });
        }
        let version = r.i32()?;
        if version != 4 {
            return Err(FormatError::UnsupportedVersion {
                what: "VVD",
                version: version as u32,
            });
        }
        let checksum = r.i32()?;
        let num_lods = r.i32()?;
        let mut lod_vertex_counts = [0i32; 8];
        for c in lod_vertex_counts.iter_mut() {
            *c = r.i32()?;
        }
        let num_fixups = r.i32()?;
        let fixup_start = r.i32()? as usize;
        let vertex_start = r.i32()? as usize;
        let tangent_start = r.i32()? as usize;
        let total = lod_vertex_counts[0].max(0) as usize;
        let read_vertex = |i: usize| -> Result<Vertex> {
            let mut vr = Reader::at(data, vertex_start + i * 48);
            let weights = [vr.f32()?, vr.f32()?, vr.f32()?];
            let bones = vr.array::<3>()?;
            let num_bones = vr.u8()?;
            let position = vec3(&mut vr)?;
            let normal = vec3(&mut vr)?;
            let uv = [vr.f32()?, vr.f32()?];
            Ok(Vertex {
                weights,
                bones,
                num_bones,
                position,
                normal,
                uv,
            })
        };
        let read_tangent = |i: usize| -> Result<[f32; 4]> {
            let mut tr = Reader::at(data, tangent_start + i * 16);
            Ok([tr.f32()?, tr.f32()?, tr.f32()?, tr.f32()?])
        };
        let has_tangents = tangent_start != 0 && tangent_start + total * 16 <= data.len();
        let mut vertices = Vec::new();
        let mut tangents = Vec::new();
        if num_fixups > 0 {
            let mut fr = Reader::at(data, fixup_start);
            for _ in 0..num_fixups {
                let f_lod = fr.i32()?;
                let src = fr.i32()? as usize;
                let n = fr.i32()? as usize;
                if f_lod < lod as i32 {
                    continue;
                }
                for i in src..src + n {
                    vertices.push(read_vertex(i)?);
                    if has_tangents {
                        tangents.push(read_tangent(i)?);
                    }
                }
            }
        } else {
            let n = lod_vertex_counts.get(lod).copied().unwrap_or(0).max(0) as usize;
            for i in 0..n {
                vertices.push(read_vertex(i)?);
                if has_tangents {
                    tangents.push(read_tangent(i)?);
                }
            }
        }
        Ok(Vvd {
            checksum,
            num_lods,
            lod_vertex_counts,
            vertices,
            tangents,
        })
    }
}

/// One drawable mesh for one LOD: indices into the VVD vertex list (after fixups).
#[derive(Clone, Debug, Default)]
pub struct VtxMesh {
    pub flags: u8,
    /// Triangle list; each index is `vertex_index` into `Vvd::vertices` for the
    /// owning model (mesh vertex offset and model vertex base already applied).
    pub indices: Vec<u32>,
}

#[derive(Clone, Debug, Default)]
pub struct VtxLod {
    pub switch_point: f32,
    pub meshes: Vec<VtxMesh>,
}

#[derive(Clone, Debug, Default)]
pub struct VtxModel {
    pub lods: Vec<VtxLod>,
}

#[derive(Clone, Debug, Default)]
pub struct VtxBodyPart {
    pub models: Vec<VtxModel>,
}

#[derive(Clone, Debug)]
pub struct Vtx {
    pub version: i32,
    pub checksum: i32,
    pub num_lods: i32,
    pub body_parts: Vec<VtxBodyPart>,
}

impl Vtx {
    /// `mdl` is needed to resolve each mesh's vertex offset into the shared VVD list.
    pub fn parse(data: &[u8], mdl: &Mdl) -> Result<Self> {
        let mut r = Reader::new(data);
        let version = r.i32()?;
        if version != 7 {
            return Err(FormatError::UnsupportedVersion {
                what: "VTX",
                version: version as u32,
            });
        }
        r.skip(4)?; // vert cache size
        r.skip(4)?; // max bones per strip, per tri
        r.skip(4)?; // max bones per vert
        let checksum = r.i32()?;
        let num_lods = r.i32()?;
        r.skip(4)?; // material replacement list offset
        let num_body_parts = r.i32()?;
        let body_part_offset = r.i32()?;
        let v49 = mdl.header.version >= 49;
        let strip_group_size = if v49 { 33 } else { 25 };
        let strip_size = if v49 { 35 } else { 27 };

        let mut body_parts = Vec::new();
        for bp in 0..num_body_parts.max(0) as usize {
            let bp_base = body_part_offset as usize + bp * 8;
            let mut br = Reader::at(data, bp_base);
            let num_models = br.i32()?;
            let model_offset = br.i32()?;
            let mdl_bp = mdl.body_parts.get(bp);
            let mut models = Vec::new();
            for m in 0..num_models.max(0) as usize {
                let m_base = offset(bp_base, model_offset) + m * 8;
                let mut mr = Reader::at(data, m_base);
                let num_lods_m = mr.i32()?;
                let lod_offset = mr.i32()?;
                let mdl_model = mdl_bp.and_then(|b| b.models.get(m));
                let mut lods = Vec::new();
                for l in 0..num_lods_m.max(0) as usize {
                    let l_base = offset(m_base, lod_offset) + l * 12;
                    let mut lr = Reader::at(data, l_base);
                    let num_meshes = lr.i32()?;
                    let mesh_offset = lr.i32()?;
                    let switch_point = lr.f32()?;
                    let mut meshes = Vec::new();
                    for k in 0..num_meshes.max(0) as usize {
                        let k_base = offset(l_base, mesh_offset) + k * 9;
                        let mut kr = Reader::at(data, k_base);
                        let num_sg = kr.i32()?;
                        let sg_offset = kr.i32()?;
                        let flags = kr.u8()?;
                        let mesh_vertex_base = mdl_model
                            .and_then(|mm| mm.meshes.get(k))
                            .map(|me| me.vertex_offset)
                            .unwrap_or(0)
                            .max(0) as u32;
                        let mut indices = Vec::new();
                        for s in 0..num_sg.max(0) as usize {
                            let s_base = offset(k_base, sg_offset) + s * strip_group_size;
                            let mut sr = Reader::at(data, s_base);
                            let num_verts = sr.i32()?;
                            let vert_offset = sr.i32()?;
                            let _num_indices = sr.i32()?;
                            let index_offset = sr.i32()?;
                            let num_strips = sr.i32()?;
                            let strip_offset = sr.i32()?;
                            let _sg_flags = sr.u8()?;
                            // vertex table: orig mesh vertex id per strip-group vertex
                            let mut orig = Vec::with_capacity(num_verts.max(0) as usize);
                            for v in 0..num_verts.max(0) as usize {
                                let mut vr = Reader::at(data, offset(s_base, vert_offset) + v * 9);
                                vr.skip(4)?; // bone weight indices, num bones
                                orig.push(vr.u16()?);
                            }
                            let idx_base = offset(s_base, index_offset);
                            for st in 0..num_strips.max(0) as usize {
                                let mut tr = Reader::at(
                                    data,
                                    offset(s_base, strip_offset) + st * strip_size,
                                );
                                let n_idx = tr.i32()?;
                                let i_off = tr.i32()?;
                                tr.skip(8)?; // num verts, vert offset
                                tr.skip(2)?; // num bones
                                let s_flags = tr.u8()?;
                                let is_trilist = s_flags & 0x1 != 0;
                                let mut ir = Reader::at(data, idx_base + i_off as usize * 2);
                                let mut strip_idx = Vec::with_capacity(n_idx.max(0) as usize);
                                for _ in 0..n_idx.max(0) {
                                    let i = ir.u16()? as usize;
                                    let o = *orig.get(i).ok_or_else(|| FormatError::Malformed {
                                        what: "VTX",
                                        detail: format!(
                                            "strip index {i} out of {} verts",
                                            orig.len()
                                        ),
                                    })?;
                                    strip_idx.push(mesh_vertex_base + o as u32);
                                }
                                if is_trilist || s_flags & 0x2 == 0 {
                                    indices.extend_from_slice(&strip_idx);
                                } else {
                                    for (t, w) in strip_idx.windows(3).enumerate() {
                                        if t % 2 == 0 {
                                            indices.extend_from_slice(&[w[0], w[1], w[2]]);
                                        } else {
                                            indices.extend_from_slice(&[w[1], w[0], w[2]]);
                                        }
                                    }
                                }
                            }
                        }
                        meshes.push(VtxMesh { flags, indices });
                    }
                    lods.push(VtxLod {
                        switch_point,
                        meshes,
                    });
                }
                models.push(VtxModel { lods });
            }
            body_parts.push(VtxBodyPart { models });
        }
        Ok(Vtx {
            version,
            checksum,
            num_lods,
            body_parts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_wrong_magic_and_version() {
        assert!(matches!(
            Header::parse(b"NOPE").unwrap_err(),
            FormatError::BadMagic { .. }
        ));
        let mut v = Vec::new();
        v.extend_from_slice(MDL_ID);
        v.extend_from_slice(&10i32.to_le_bytes());
        v.extend_from_slice(&[0u8; 400]);
        assert!(matches!(
            Header::parse(&v).unwrap_err(),
            FormatError::UnsupportedVersion { .. }
        ));
    }
}
