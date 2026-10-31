//! Sandbox server core, measured against the real gamemode (`tools/parity/scenarios/sandbox`):
//! where `gm_spawn` puts a prop, how `gmod_undo` and `gmod_cleanup` walk the per-player
//! stacks and how `sbox_maxprops` is enforced. No GLua is executed; the behaviour is
//! re-implemented and checked by the `sandbox_*` harnesses.

use glam::{Quat, Vec3};
use gm_collision::{World, DIST_EPSILON, MASK_SOLID};
use std::collections::BTreeMap;

/// Model bounds the game uses for placement (`OBBMins`/`OBBMaxs`, the studio hull).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelBounds {
    pub min: Vec3,
    pub max: Vec3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Prop {
    pub id: u32,
    pub owner: u32,
    pub model: String,
    pub pos: Vec3,
    /// Source angles: pitch, yaw, roll in degrees.
    pub angles: Vec3,
    /// `PhysObj:EnableMotion(false)`.
    pub frozen: bool,
    /// The gravity property (`PhysObj:EnableGravity`); on by default.
    pub gravity: bool,
    /// The collision property: off puts the prop in `COLLISION_GROUP_WORLD`.
    pub collide: bool,
    /// The keep-upright property: the angles the prop is held at.
    pub upright: Option<Vec3>,
    /// `prop_data` health (0: unbreakable); fire counts it down, zero breaks the prop.
    pub health: i32,
    /// `Entity:GetSkin`: the skin family drawn (the `skin` property picks it).
    pub skin: u8,
    /// `Entity:GetBodyGroups` digits, one per body part (empty: all 0).
    pub body: String,
    /// The `entityflame` on the prop (`Ignite`), in server ticks.
    pub fire: Option<Fire>,
}

/// A flame on a prop: damage thinks every 13 ticks until `until`, then the flame lingers
/// 0.5 s (`out_at`) before it is gone (measured `lua_fire`).
#[derive(Clone, Debug, PartialEq)]
pub struct Fire {
    pub until: u64,
    pub next_think: u64,
    pub out_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Undo {
    pub id: u32,
    pub name: String,
    pub text: String,
    /// `undo.SetCustomUndoText`, what the client prints instead of "Undone <name>".
    pub custom: Option<String>,
    pub entities: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// `achievements.SpawnedProp()` is sent to the player; a prop exists now.
    Spawned { prop: u32 },
    /// `Player:LimitHit(name)`: the spawn was refused by `sbox_max<name>`.
    LimitHit { limit: String },
    /// `gmod_undo` removed `removed` entities of undo `id` (0 when the entry was already empty).
    Undone { id: u32, removed: usize },
    /// `gmod_cleanup` without arguments: `hook.Run('OnCleanup','all')`.
    Cleanup { kind: String },
    /// An entity left the table (`EntityRemoved`), one per entity, before the Undone/Cleanup.
    Removed { prop: u32 },
}

/// `sbox_*` limits (measured defaults: props 200, constraints 2000, ropeconstraints 1000);
/// negative means unlimited, as in `Player:CheckLimit`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    pub max_props: i32,
    pub max_constraints: i32,
    pub max_ropeconstraints: i32,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_props: 200,
            max_constraints: 2000,
            max_ropeconstraints: 1000,
        }
    }
}

impl Limits {
    fn get(&self, kind: &str) -> i32 {
        match kind {
            "props" => self.max_props,
            "constraints" => self.max_constraints,
            "ropeconstraints" => self.max_ropeconstraints,
            _ => -1,
        }
    }
}

/// A player's `gm_spawn`: eye position, view angles (pitch, yaw, roll) and the model.
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnRequest<'a> {
    pub eye: Vec3,
    pub eye_angles: Vec3,
    pub model: &'a str,
    pub bounds: ModelBounds,
}

/// Where the spawn ray hit and the pose the gamemode computed from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub hit: Vec3,
    pub normal: Vec3,
    /// The prop the spawn ray hit, `None` for the world.
    pub hit_ent: Option<u32>,
    pub pos: Vec3,
    pub angles: Vec3,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConstraintKind {
    /// `constraint.Weld`: `phys_constraint`.
    Weld { forcelimit: f32, nocollide: bool },
    /// `constraint.NoCollide`: `logic_collision_pair` with collisions disabled.
    NoCollide,
    /// `constraint.Axis`: `phys_hinge` at `lpos1` of the first object, axis towards `local_axis`.
    Axis {
        lpos1: Vec3,
        lpos2: Vec3,
        local_axis: Vec3,
        forcelimit: f32,
        torquelimit: f32,
        friction: f32,
        nocollide: bool,
    },
    /// `constraint.Ballsocket`: `phys_ballsocket` at `lpos` of the second object (its frame, world
    /// space for the world); `forcelimit` / `torquelimit` are stored, not simulated.
    Ballsocket {
        lpos: Vec3,
        forcelimit: f32,
        torquelimit: f32,
        nocollide: bool,
    },
    /// `constraint.Slider`: `phys_slideconstraint` along the axis from `lpos1` to `lpos2`, with a
    /// keyframe rope of `width` drawn between them.
    Slider {
        lpos1: Vec3,
        lpos2: Vec3,
        width: f32,
        material: String,
        /// The hydraulic or muscle this slider belongs to (`fixed`): it has no type of its own
        /// and goes when its owner goes (`DeleteOnRemove`).
        part_of: Option<u32>,
    },
    /// `constraint.Hydraulic`: an Elastic whose rest length a `gmod_winch_controller` drives
    /// between `length1` and `length2` at `speed` u/s off numpad `key`; `fixed` adds a slider
    /// between the same points. Spring constants from the lighter end's mass (`CalcElasticConsts`).
    Hydraulic {
        lpos1: Vec3,
        lpos2: Vec3,
        length1: f32,
        length2: f32,
        width: f32,
        material: String,
        key: i32,
        fixed: bool,
        speed: f32,
        toggle: bool,
        constant: f32,
        damping: f32,
        controller: LengthController,
    },
    /// `constraint.Winch`: a stretch-only Elastic with no upper length and a floor of 1 u, wound
    /// in and out by two numpad keys at their own speeds.
    Winch {
        lpos1: Vec3,
        lpos2: Vec3,
        width: f32,
        material: String,
        fwd_key: i32,
        bwd_key: i32,
        fwd_speed: f32,
        bwd_speed: f32,
        toggle: bool,
        constant: f32,
        damping: f32,
        controller: LengthController,
    },
    /// `constraint.Muscle`: an Elastic whose rest length swings sinusoidally between `length1`
    /// and `length1 + 2 * amplitude` with `period`, switched by numpad `key`.
    Muscle {
        lpos1: Vec3,
        lpos2: Vec3,
        length1: f32,
        length2: f32,
        width: f32,
        material: String,
        key: i32,
        fixed: bool,
        period: f32,
        amplitude: f32,
        starton: bool,
        constant: f32,
        damping: f32,
        controller: LengthController,
    },
    /// `constraint.Pulley`: `phys_pulleyconstraint` from `lpos1` on the first entity over the
    /// world points `wpos2` and `wpos3` to `lpos4` on the second (`ent2`); three visual ropes.
    Pulley {
        lpos1: Vec3,
        lpos4: Vec3,
        wpos2: Vec3,
        wpos3: Vec3,
        forcelimit: f32,
        rigid: bool,
        width: f32,
        material: String,
        /// Slack beyond the distance at creation (`WPos3 - WPos4`), as the game stores it.
        addlength: f32,
    },
    /// `constraint.Elastic`: `phys_spring` between `lpos1` and `lpos2` with `F = -constant * x -
    /// damping * v` about `length` (their distance when made); `stretchonly` is slack when shorter.
    Elastic {
        lpos1: Vec3,
        lpos2: Vec3,
        constant: f32,
        damping: f32,
        rdamping: f32,
        width: f32,
        material: String,
        length: f32,
        stretchonly: bool,
    },
    /// `constraint.Rope`: `phys_lengthconstraint`; local attachment points are in the entity
    /// frame (world position for the world).
    Rope {
        lpos1: Vec3,
        lpos2: Vec3,
        length: f32,
        addlength: f32,
        width: f32,
        material: String,
        rigid: bool,
    },
}

/// A constraint entity; `None` stands for the world.
impl ConstraintKind {
    /// The length controller of a hydraulic, winch or muscle.
    pub fn controller(&self) -> Option<&LengthController> {
        match self {
            ConstraintKind::Hydraulic { controller, .. }
            | ConstraintKind::Winch { controller, .. }
            | ConstraintKind::Muscle { controller, .. } => Some(controller),
            _ => None,
        }
    }

    pub fn controller_mut(&mut self) -> Option<&mut LengthController> {
        match self {
            ConstraintKind::Hydraulic { controller, .. }
            | ConstraintKind::Winch { controller, .. }
            | ConstraintKind::Muscle { controller, .. } => Some(controller),
            _ => None,
        }
    }
}

/// `gmod_winch_controller`: drives the rest length of a hydraulic, winch or muscle spring. Each
/// tick (`Think` every 0.01 s, so every server tick) a running controller moves `length` by
/// `speed * dt` in `direction` (normal) or along a sine (muscle), clamps it to `[min, max]`,
/// and a toggle controller stops at the limit it reached.
#[derive(Clone, Debug, PartialEq)]
pub struct LengthController {
    pub muscle: bool,
    pub length: f32,
    pub min: f32,
    pub max: Option<f32>,
    pub fwd_speed: f32,
    pub bwd_speed: f32,
    pub toggle: bool,
    /// 1 lengthens, -1 shortens, 0 holds (`DIR_FORWARD` / `DIR_BACKWARD` / `DIR_NONE`).
    pub direction: i8,
    pub expanded: bool,
    pub ctime: f32,
    pub period: f32,
    pub amplitude: f32,
}

impl LengthController {
    pub fn new(
        length: f32,
        min: f32,
        max: Option<f32>,
        fwd_speed: f32,
        bwd_speed: f32,
        toggle: bool,
    ) -> Self {
        LengthController {
            muscle: false,
            length,
            min,
            max,
            fwd_speed,
            bwd_speed,
            toggle,
            direction: 0,
            expanded: max.is_some_and(|m| length >= m),
            ctime: 0.0,
            period: 0.0,
            amplitude: 0.0,
        }
    }

    /// A muscle controller starts its clock where the sine gives the current length.
    pub fn muscle(
        length: f32,
        min: f32,
        max: f32,
        period: f32,
        amplitude: f32,
        starton: bool,
    ) -> Self {
        let spos = ((length - min) / (amplitude * 2.0) - 1.0).clamp(-1.0, 1.0);
        LengthController {
            muscle: true,
            length,
            min,
            max: Some(max),
            fwd_speed: 0.0,
            bwd_speed: 0.0,
            toggle: true,
            direction: i8::from(starton),
            expanded: length >= max,
            ctime: spos.asin() / (period * std::f32::consts::PI),
            period,
            amplitude,
        }
    }

    /// One `Think`; returns the rest length to set, `None` while the controller holds.
    pub fn tick(&mut self, dt: f32) -> Option<f32> {
        if self.direction == 0 {
            return None;
        }
        if self.muscle {
            let spos = ((self.ctime * std::f32::consts::PI * self.period).sin() + 1.0)
                * (self.amplitude / 2.0);
            self.length = self.min + spos.clamp(0.0, self.amplitude);
            self.ctime += dt;
        } else {
            let speed = if self.direction > 0 {
                self.fwd_speed
            } else {
                -self.bwd_speed
            };
            self.length += speed * dt;
            if self.length < self.min {
                self.length = self.min;
                if self.toggle {
                    self.direction = 0;
                }
            }
            if let Some(max) = self.max {
                if self.length > max {
                    self.length = max;
                    self.expanded = true;
                    if self.toggle {
                        self.direction = 0;
                    }
                }
            }
        }
        Some(self.length)
    }

    /// `HydraulicToggle`: a held one reverses, a stopped one leaves the end it is at.
    pub fn hydraulic_toggle(&mut self) {
        self.direction = match self.direction {
            0 if self.expanded => -1,
            0 => 1,
            d => -d,
        };
    }

    /// `WinchOn` / `WinchOff` / `HydraulicDir`.
    pub fn set_direction(&mut self, d: i8) {
        self.direction = d;
    }

    /// `WinchToggle`: the same key again stops, the other reverses.
    pub fn winch_toggle(&mut self, d: i8) {
        self.direction = if self.direction == d { 0 } else { d };
    }

    /// `MuscleToggle`: running stops, stopped runs.
    pub fn muscle_toggle(&mut self) {
        self.direction = if self.direction == 0 { 1 } else { 0 };
    }
}

/// `CalcElasticConsts`: spring constant and damping off the lighter end's mass (the world does
/// not count); a fixed (slider-guided) hydraulic/muscle is stiffer and better damped.
pub fn elastic_consts(mass1: Option<f32>, mass2: Option<f32>, fixed: bool) -> (f32, f32) {
    let min_mass = match (mass1, mass2) {
        (None, Some(m)) | (Some(m), None) => m,
        (Some(a), Some(b)) => a.min(b),
        (None, None) => 0.0,
    };
    if fixed {
        (min_mass * 100.0, min_mass * 100.0 * 0.2)
    } else {
        (min_mass * 50.0, min_mass * 50.0 * 0.1)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Constraint {
    pub id: u32,
    pub owner: u32,
    pub ent1: Option<u32>,
    pub ent2: Option<u32>,
    pub kind: ConstraintKind,
}

/// One object the tool gun has selected (`ToolObj:SetObject`): position and normal are kept
/// in the entity frame so they stay valid when it moves; for the world they are world space.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolObject {
    pub ent: Option<u32>,
    pub pos: Vec3,
    pub normal: Vec3,
    pub bone: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToolGun {
    pub objects: Vec<ToolObject>,
    pub stage: u32,
}

/// Client tool settings the stools read with `GetClientNumber`/`GetClientInfo`.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolSettings {
    pub axis_forcelimit: f32,
    pub axis_torquelimit: f32,
    pub axis_friction: f32,
    pub axis_nocollide: bool,
    pub weld_forcelimit: f32,
    pub weld_nocollide: bool,
    pub rope_forcelimit: f32,
    pub rope_addlength: f32,
    pub rope_width: f32,
    pub rope_material: String,
    pub rope_rigid: bool,
    pub ballsocket_forcelimit: f32,
    pub ballsocket_nocollide: bool,
    pub slider_width: f32,
    pub slider_material: String,
    pub elastic_constant: f32,
    pub elastic_damping: f32,
    pub elastic_rdamping: f32,
    pub elastic_width: f32,
    pub elastic_material: String,
    pub elastic_stretchonly: bool,
    pub hydraulic_width: f32,
    pub hydraulic_addlength: f32,
    pub hydraulic_fixed: bool,
    pub hydraulic_speed: f32,
    pub hydraulic_toggle: bool,
    pub hydraulic_material: String,
    pub hydraulic_key: i32,
    pub winch_width: f32,
    pub winch_fwd_speed: f32,
    pub winch_bwd_speed: f32,
    pub winch_fwd_key: i32,
    pub winch_bwd_key: i32,
    pub winch_material: String,
    pub winch_toggle: bool,
    pub muscle_width: f32,
    pub muscle_addlength: f32,
    pub muscle_fixed: bool,
    pub muscle_period: f32,
    pub muscle_starton: bool,
    pub muscle_material: String,
    pub muscle_key: i32,
    pub pulley_width: f32,
    pub pulley_forcelimit: f32,
    pub pulley_rigid: bool,
    pub pulley_material: String,
}

impl Default for ToolSettings {
    /// A player without the convars (a bot) reads 0 and "" everywhere.
    fn default() -> Self {
        ToolSettings {
            axis_forcelimit: 0.0,
            axis_torquelimit: 0.0,
            axis_friction: 0.0,
            axis_nocollide: false,
            weld_forcelimit: 0.0,
            weld_nocollide: false,
            rope_forcelimit: 0.0,
            rope_addlength: 0.0,
            rope_width: 0.0,
            rope_material: String::new(),
            rope_rigid: false,
            ballsocket_forcelimit: 0.0,
            ballsocket_nocollide: false,
            slider_width: 1.5,
            slider_material: "cable/cable".into(),
            elastic_constant: 500.0,
            elastic_damping: 3.0,
            elastic_rdamping: 0.01,
            elastic_width: 2.0,
            elastic_material: "cable/cable".into(),
            elastic_stretchonly: true,
            hydraulic_width: 3.0,
            hydraulic_addlength: 100.0,
            hydraulic_fixed: true,
            hydraulic_speed: 64.0,
            hydraulic_toggle: true,
            hydraulic_material: "cable/rope".into(),
            hydraulic_key: 37,
            winch_width: 3.0,
            winch_fwd_speed: 64.0,
            winch_bwd_speed: 64.0,
            winch_fwd_key: 44,
            winch_bwd_key: 41,
            winch_material: "cable/rope".into(),
            winch_toggle: false,
            muscle_width: 2.0,
            muscle_addlength: 100.0,
            muscle_fixed: true,
            muscle_period: 1.0,
            muscle_starton: false,
            muscle_material: "cable/rope".into(),
            muscle_key: 37,
            pulley_width: 3.0,
            pulley_forcelimit: 0.0,
            pulley_rigid: false,
            pulley_material: "cable/cable".into(),
        }
    }
}

/// A prop as the tool trace sees it: pose plus collision hulls as outward planes (entity frame).
#[derive(Clone, Debug, PartialEq)]
pub struct PropBody {
    pub id: u32,
    pub pos: Vec3,
    pub angles: Vec3,
    pub hulls: Vec<Vec<(Vec3, f32)>>,
}

/// What the tool gun's trace hit (`SWEP:DoToolTrace`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolHit {
    pub pos: Vec3,
    pub normal: Vec3,
    /// `None` is the world.
    pub ent: Option<u32>,
    pub fraction: f32,
}

/// `MAX_TRACE_LENGTH` of the engine, the reach of `util.GetPlayerTrace`.
pub const MAX_TRACE_LENGTH: f32 = 56756.0;

/// Outward planes of one convex from its triangle soup (PHY ledges).
pub fn hull_planes(points: &[Vec3], triangles: &[[u16; 3]]) -> Vec<(Vec3, f32)> {
    if points.is_empty() {
        return Vec::new();
    }
    let centre = points.iter().sum::<Vec3>() / points.len() as f32;
    let mut planes: Vec<(Vec3, f32)> = Vec::new();
    for t in triangles {
        let (a, b, c) = (
            points[t[0] as usize],
            points[t[1] as usize],
            points[t[2] as usize],
        );
        let mut n = (b - a).cross(c - a);
        if n.length_squared() < 1e-10 {
            continue;
        }
        n = n.normalize();
        let mut d = n.dot(a);
        if n.dot(centre) - d > 0.0 {
            n = -n;
            d = -d;
        }
        if planes
            .iter()
            .any(|(pn, pd)| pn.dot(n) > 0.9999 && (pd - d).abs() < 1e-3)
        {
            continue;
        }
        planes.push((n, d));
    }
    planes
}

fn ray_convex(start: Vec3, end: Vec3, planes: &[(Vec3, f32)]) -> Option<(f32, Vec3)> {
    let (mut t_in, mut t_out) = (0.0f32, 1.0f32);
    let mut normal = Vec3::ZERO;
    for &(n, d) in planes {
        let d1 = n.dot(start) - d;
        let d2 = n.dot(end) - d;
        if d1 > 0.0 && d2 > 0.0 {
            return None;
        }
        if d1 <= 0.0 && d2 <= 0.0 {
            continue;
        }
        if d1 > d2 {
            // the engine stops the ray 1/32 u in front of the surface, like the brush trace
            let t = (d1 - DIST_EPSILON) / (d1 - d2);
            if t > t_in {
                t_in = t;
                normal = n;
            }
        } else {
            let t = (d1 + DIST_EPSILON) / (d1 - d2);
            if t < t_out {
                t_out = t;
            }
        }
    }
    (t_in <= t_out && normal != Vec3::ZERO).then_some((t_in, normal))
}

/// A line trace against the world (brushes and displacements, `MASK_SOLID`) and the props'
/// collision hulls; the nearest hit wins. `None` entity is the world.
pub fn trace_line(world: &World, props: &[PropBody], start: Vec3, end: Vec3) -> Option<ToolHit> {
    let tr = world.trace_ray(start, end, MASK_SOLID);
    let mut best = tr.hit().then_some(ToolHit {
        pos: tr.end,
        normal: tr.normal,
        ent: None,
        fraction: tr.fraction,
    });
    for p in props {
        let rot = rotation(p.angles);
        let inv = rot.inverse();
        let (ls, le) = (inv * (start - p.pos), inv * (end - p.pos));
        for hull in &p.hulls {
            if let Some((t, n)) = ray_convex(ls, le, hull) {
                if best.is_none_or(|b| t < b.fraction) {
                    best = Some(ToolHit {
                        pos: start + (end - start) * t,
                        normal: rot * n,
                        ent: Some(p.id),
                        fraction: t,
                    });
                }
            }
        }
    }
    best
}

/// The tool gun's line trace from the eye (`SWEP:DoToolTrace`).
pub fn tool_trace(world: &World, props: &[PropBody], eye: Vec3, dir: Vec3) -> Option<ToolHit> {
    trace_line(world, props, eye, eye + dir * MAX_TRACE_LENGTH)
}

#[derive(Default)]
pub struct Sandbox {
    pub limits: Limits,
    pub tool_settings: ToolSettings,
    next_id: u32,
    props: Vec<Prop>,
    constraints: Vec<Constraint>,
    tools: BTreeMap<u32, ToolGun>,
    undo: BTreeMap<u32, Vec<Undo>>,
    cleanup: BTreeMap<u32, BTreeMap<String, Vec<u32>>>,
    counts: BTreeMap<u32, BTreeMap<String, Vec<u32>>>,
    /// Prop masses (kg) the owner of the physics tells us; `CalcElasticConsts` needs them.
    masses: BTreeMap<u32, f32>,
    /// Numpad bindings per (player, key): which controllers a press and a release drive.
    numpad: BTreeMap<(u32, i32), Vec<NumpadBind>>,
    /// Keys currently held per player (`numpad.Activate` ignores a repeat while held).
    numpad_held: BTreeMap<u32, Vec<i32>>,
}

/// What a numpad key does to one constraint's controller (`numpad.OnDown` / `OnUp`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NumpadBind {
    HydraulicToggle(u32),
    /// Down sets +1, up sets -1 (a non-toggle hydraulic).
    HydraulicDir(u32),
    /// Down sets the direction, up stops (a non-toggle winch).
    WinchDir(u32, i8),
    WinchToggle(u32, i8),
    MuscleToggle(u32),
}

impl NumpadBind {
    fn constraint(self) -> u32 {
        match self {
            NumpadBind::HydraulicToggle(c)
            | NumpadBind::HydraulicDir(c)
            | NumpadBind::WinchDir(c, _)
            | NumpadBind::WinchToggle(c, _)
            | NumpadBind::MuscleToggle(c) => c,
        }
    }
}

/// Source `AngleVectors` forward for pitch/yaw in degrees.
pub fn forward(pitch: f32, yaw: f32) -> Vec3 {
    let (sp, cp) = pitch.to_radians().sin_cos();
    let (sy, cy) = yaw.to_radians().sin_cos();
    Vec3::new(cp * cy, cp * sy, -sp)
}

/// Rotation of Source angles (pitch about +y, yaw about +z, roll about +x), applied roll first.
pub fn rotation(angles: Vec3) -> Quat {
    Quat::from_rotation_z(angles.y.to_radians())
        * Quat::from_rotation_y(angles.x.to_radians())
        * Quat::from_rotation_x(angles.z.to_radians())
}

/// `Vector:Angle()`: pitch and yaw of a direction, roll 0.
pub fn angle_of(dir: Vec3) -> Vec3 {
    // straight up or down: the engine's VectorAngles keeps yaw 0 (atan2(-0, -0) would say 180)
    let yaw = if dir.x == 0.0 && dir.y == 0.0 {
        0.0
    } else {
        dir.y.atan2(dir.x).to_degrees()
    };
    let pitch = (-dir.z)
        .atan2((dir.x * dir.x + dir.y * dir.y).sqrt())
        .to_degrees();
    Vec3::new(pitch, yaw, 0.0)
}

/// Source angles of a rotation (`MatrixAngles`).
pub fn angles_of(rot: Quat) -> Vec3 {
    let forward = rot * Vec3::X;
    let left = rot * Vec3::Y;
    let up = rot * Vec3::Z;
    let xy = (forward.x * forward.x + forward.y * forward.y).sqrt();
    if xy > 0.001 {
        Vec3::new(
            (-forward.z).atan2(xy).to_degrees(),
            forward.y.atan2(forward.x).to_degrees(),
            left.z.atan2(up.z).to_degrees(),
        )
    } else {
        Vec3::new(
            (-forward.z).atan2(xy).to_degrees(),
            (-left.x).atan2(left.y).to_degrees(),
            0.0,
        )
    }
}

/// `PhysObj:AlignAngles(from, to)`: the object's current rotation turned by the rotation that
/// takes `from` to `to`.
pub fn align_angles(current: Vec3, from: Vec3, to: Vec3) -> Vec3 {
    let delta = rotation(to) * rotation(from).inverse();
    angles_of(delta * rotation(current))
}

fn normalize_angle(a: f32) -> f32 {
    let mut a = a % 360.0;
    if a > 180.0 {
        a -= 360.0;
    } else if a <= -180.0 {
        a += 360.0;
    }
    a
}

/// `Entity:NearestPoint` on the model box at `pos`/`rot`.
fn nearest_point(pos: Vec3, rot: Quat, b: &ModelBounds, q: Vec3) -> Vec3 {
    let local = rot.inverse() * (q - pos);
    pos + rot * local.clamp(b.min, b.max)
}

/// `DoPlayerEntitySpawn` + `TryFixPropPosition` for a `prop_physics`: the entity is placed at
/// the hit point facing the player (yaw + 180, no pitch or roll), then moved so the point of
/// its box nearest to 512 u behind the surface sits on the hit point, then nudged out of walls
/// along each of its axes.
pub fn place_prop(
    world: &World,
    props: &[PropBody],
    eye: Vec3,
    eye_angles: Vec3,
    bounds: &ModelBounds,
) -> Placement {
    let dir = forward(eye_angles.x, eye_angles.y);
    let end = eye + dir * 2048.0;
    let (hit, normal, hit_ent) = match trace_line(world, props, eye, end) {
        Some(h) => (h.pos, h.normal, h.ent),
        None => (end, Vec3::ZERO, None),
    };
    let angles = Vec3::new(0.0, normalize_angle(eye_angles.y + 180.0), 0.0);
    let rot = rotation(angles);
    let far = hit - normal * 512.0;
    let nearest = nearest_point(hit, rot, bounds, far);
    let mut pos = hit + (hit - nearest);
    for axis in 0..3 {
        let mut lo = Vec3::ZERO;
        let mut hi = Vec3::ZERO;
        lo[axis] = bounds.min[axis];
        hi[axis] = bounds.max[axis];
        let end_d = pos + rot * lo;
        let end_u = pos + rot * hi;
        let tr_d = trace_line(world, props, pos, end_d);
        let tr_u = trace_line(world, props, pos, end_u);
        if tr_d.is_some() && tr_u.is_some() {
            continue;
        }
        if let Some(t) = tr_d {
            pos += t.pos - end_d;
        }
        if let Some(t) = tr_u {
            pos += t.pos - end_u;
        }
    }
    Placement {
        hit,
        normal,
        hit_ent,
        pos,
        angles,
    }
}

impl Sandbox {
    pub fn new(limits: Limits) -> Self {
        Sandbox {
            limits,
            next_id: 1,
            ..Default::default()
        }
    }

    /// A prop record straight from a save: no placement, limits or undo entry; the server
    /// builds its body on the next physics sync.
    pub fn restore_prop(
        &mut self,
        owner: u32,
        model: &str,
        pos: Vec3,
        angles: Vec3,
        frozen: bool,
    ) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.props.push(Prop {
            id,
            owner,
            model: model.to_string(),
            pos,
            angles,
            frozen,
            gravity: true,
            collide: true,
            upright: None,
            health: 0,
            skin: 0,
            body: String::new(),
            fire: None,
        });
        id
    }

    /// A constraint record straight from a save (no limits, undo entry or duplicate check).
    pub fn restore_constraint(
        &mut self,
        owner: u32,
        ent1: Option<u32>,
        ent2: Option<u32>,
        kind: ConstraintKind,
    ) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.constraints.push(Constraint {
            id,
            owner,
            ent1,
            ent2,
            kind,
        });
        id
    }

    pub fn props(&self) -> &[Prop] {
        &self.props
    }

    /// `Player:GetCount("props")`: live props owned by the player.
    pub fn count(&self, player: u32) -> usize {
        self.props.iter().filter(|p| p.owner == player).count()
    }

    /// `Player:GetCount(kind)` for the counted kinds (`props`, `constraints`, `ropeconstraints`):
    /// entries whose entity is gone do not count.
    pub fn count_of(&self, player: u32, kind: &str) -> usize {
        if kind == "props" {
            return self.count(player);
        }
        self.counts
            .get(&player)
            .and_then(|m| m.get(kind))
            .map(|v| v.iter().filter(|&&id| self.exists(id)).count())
            .unwrap_or(0)
    }

    fn exists(&self, id: u32) -> bool {
        self.props.iter().any(|p| p.id == id) || self.constraints.iter().any(|c| c.id == id)
    }

    pub fn constraints(&self) -> &[Constraint] {
        &self.constraints
    }

    pub fn tool(&self, player: u32) -> ToolGun {
        self.tools.get(&player).cloned().unwrap_or_default()
    }

    pub fn undos(&self, player: u32) -> &[Undo] {
        self.undo.get(&player).map(Vec::as_slice).unwrap_or(&[])
    }

    /// `Player:CheckLimit(kind)`: refused once `count > max - 1`; negative max is unlimited.
    pub fn check_limit(&self, player: u32, kind: &str, events: &mut Vec<Event>) -> bool {
        let max = self.limits.get(kind);
        if max < 0 {
            return true;
        }
        if self.count_of(player, kind) as i32 > max - 1 {
            events.push(Event::LimitHit { limit: kind.into() });
            return false;
        }
        true
    }

    pub fn push_undo(
        &mut self,
        player: u32,
        name: &str,
        text: String,
        custom: Option<String>,
        entities: Vec<u32>,
    ) {
        let undos = self.undo.entry(player).or_default();
        let undo_id = undos.last().map(|u| u.id + 1).unwrap_or(1);
        undos.push(Undo {
            id: undo_id,
            name: name.into(),
            text,
            custom,
            entities,
        });
    }

    pub fn add_cleanup(&mut self, player: u32, kind: &str, id: u32) {
        self.cleanup
            .entry(player)
            .or_default()
            .entry(kind.into())
            .or_default()
            .push(id);
    }

    pub fn add_count(&mut self, player: u32, kind: &str, id: u32) {
        self.counts
            .entry(player)
            .or_default()
            .entry(kind.into())
            .or_default()
            .push(id);
    }

    /// `gm_spawn <model>` for a prop model.
    pub fn spawn_prop(
        &mut self,
        world: &World,
        props: &[PropBody],
        player: u32,
        req: &SpawnRequest<'_>,
        events: &mut Vec<Event>,
    ) -> Option<u32> {
        if !self.check_limit(player, "props", events) {
            return None;
        }
        let model = req.model;
        let placement = place_prop(world, props, req.eye, req.eye_angles, &req.bounds);
        let id = self.next_id;
        self.next_id += 1;
        self.props.push(Prop {
            id,
            owner: player,
            model: model.to_string(),
            pos: placement.pos,
            angles: placement.angles,
            frozen: false,
            gravity: true,
            collide: true,
            upright: None,
            health: 0,
            skin: 0,
            body: String::new(),
            fire: None,
        });
        events.push(Event::Spawned { prop: id });
        self.push_undo(
            player,
            "prop_physics",
            format!("#prop_physics ({model})"),
            None,
            vec![id],
        );
        self.add_cleanup(player, "props", id);
        Some(id)
    }

    /// Props that exist before the player does anything (placed by a scenario or a map): they
    /// get ids and poses but no undo entries or counts.
    pub fn place_existing(&mut self, owner: u32, bodies: &[PropBody], models: &[String]) {
        for (b, model) in bodies.iter().zip(models) {
            self.props.push(Prop {
                id: b.id,
                owner,
                model: model.clone(),
                pos: b.pos,
                angles: b.angles,
                frozen: false,
                gravity: true,
                collide: true,
                upright: None,
                health: 0,
                skin: 0,
                body: String::new(),
                fire: None,
            });
            self.next_id = self.next_id.max(b.id + 1);
        }
    }

    /// Removes a prop or constraint by id (`Entity:Remove`): constraints attached to a removed
    /// prop go with it, undo entries keep their ids (they skip dead entities when run).
    pub fn remove_entity(&mut self, id: u32) {
        let was_prop = self.props.iter().any(|p| p.id == id);
        self.props.retain(|p| p.id != id);
        self.constraints
            .retain(|c| c.id != id && !(was_prop && (c.ent1 == Some(id) || c.ent2 == Some(id))));
    }

    /// `PhysObj:EnableMotion(!frozen)`.
    pub fn set_gravity(&mut self, id: u32, on: bool) {
        if let Some(p) = self.props.iter_mut().find(|p| p.id == id) {
            p.gravity = on;
        }
    }

    pub fn set_collide(&mut self, id: u32, on: bool) {
        if let Some(p) = self.props.iter_mut().find(|p| p.id == id) {
            p.collide = on;
        }
    }

    pub fn set_upright(&mut self, id: u32, angles: Option<Vec3>) {
        if let Some(p) = self.props.iter_mut().find(|p| p.id == id) {
            p.upright = angles;
        }
    }

    pub fn set_health(&mut self, id: u32, health: i32) {
        if let Some(p) = self.props.iter_mut().find(|p| p.id == id) {
            p.health = health;
        }
    }

    pub fn set_skin(&mut self, id: u32, skin: u8) {
        if let Some(p) = self.props.iter_mut().find(|p| p.id == id) {
            p.skin = skin;
        }
    }

    pub fn set_body(&mut self, id: u32, body: String) {
        if let Some(p) = self.props.iter_mut().find(|p| p.id == id) {
            p.body = body;
        }
    }

    pub fn set_fire(&mut self, id: u32, fire: Option<Fire>) {
        if let Some(p) = self.props.iter_mut().find(|p| p.id == id) {
            p.fire = fire;
        }
    }

    pub fn set_frozen(&mut self, id: u32, frozen: bool) {
        if let Some(p) = self.props.iter_mut().find(|p| p.id == id) {
            p.frozen = frozen;
        }
    }

    /// Moves a prop (physics or a recording telling where it is now).
    pub fn set_prop_pose(&mut self, id: u32, pos: Vec3, angles: Vec3) {
        if let Some(p) = self.props.iter_mut().find(|p| p.id == id) {
            p.pos = pos;
            p.angles = angles;
        }
    }

    /// Entity frame of a prop (`PhysObj:LocalToWorld` / `WorldToLocal`).
    fn prop_pose(&self, id: u32) -> Option<(Vec3, Quat)> {
        self.props
            .iter()
            .find(|p| p.id == id)
            .map(|p| (p.pos, rotation(p.angles)))
    }

    pub fn object_world_pos(&self, o: &ToolObject) -> Vec3 {
        match o.ent.and_then(|e| self.prop_pose(e)) {
            Some((pos, rot)) => pos + rot * o.pos,
            None => o.pos,
        }
    }

    /// `ToolObj:GetNormal`: the stored normal point back in world space, minus the position.
    pub fn object_world_normal(&self, o: &ToolObject) -> Vec3 {
        match o.ent.and_then(|e| self.prop_pose(e)) {
            Some((pos, rot)) => (pos + rot * o.normal) - self.object_world_pos(o),
            None => o.normal,
        }
    }

    /// Right click of the weld tool: grab a prop, place it with the grabbed face against the
    /// second hit (frozen), weld it to the second object on the third click.
    pub fn tool_right_click(
        &mut self,
        player: u32,
        tool: &str,
        hit: &ToolHit,
        events: &mut Vec<Event>,
    ) -> bool {
        if tool != "weld" {
            return false;
        }
        let n = self.tool(player).objects.len();
        if n == 0 && hit.ent.is_none() {
            return false;
        }
        let n = self.set_object(player, hit);
        if n == 1 {
            self.tools.entry(player).or_default().stage = 1;
            return true;
        }
        let objs = self.tool(player).objects;
        if n == 2 {
            let (o1, o2) = (&objs[0], &objs[1]);
            let Some(ent1) = o1.ent else {
                self.clear_objects(player);
                return false;
            };
            let norm1 = self.object_world_normal(o1);
            let norm2 = self.object_world_normal(o2);
            let wpos2 = self.object_world_pos(o2);
            let (centre, _) = self.prop_pose(ent1).unwrap_or_default();
            let current = self
                .props
                .iter()
                .find(|p| p.id == ent1)
                .map(|p| p.angles)
                .unwrap_or_default();
            let target = align_angles(current, angle_of(norm1), angle_of(-norm2));
            self.set_prop_pose(ent1, centre, target);
            let grabbed = self.object_world_pos(o1);
            let target_pos = wpos2 + (centre - grabbed);
            self.set_prop_pose(ent1, target_pos, target);
            if let Some(p) = self.props.iter_mut().find(|p| p.id == ent1) {
                p.frozen = true;
            }
            self.tools.entry(player).or_default().stage = 2;
            return true;
        }
        if !self.check_limit(player, "constraints", events) {
            self.clear_objects(player);
            return false;
        }
        let (o1, o2) = (objs[0].clone(), objs[1].clone());
        let s = self.tool_settings.clone();
        if let Some(id) = self.weld(player, o1.ent, o2.ent, s.weld_forcelimit, s.weld_nocollide) {
            if let Some(p) = self.props.iter_mut().find(|p| Some(p.id) == o1.ent) {
                p.frozen = false;
            }
            self.push_undo(
                player,
                "Weld",
                "#tool.weld.name".into(),
                Some("Undone #tool.weld.name".into()),
                vec![id],
            );
            self.add_count(player, "constraints", id);
            self.add_cleanup(player, "constraints", id);
        }
        self.clear_objects(player);
        true
    }

    /// `constraint.NoCollide`: nothing for the same object or an existing pair.
    pub fn nocollide(&mut self, owner: u32, ent1: Option<u32>, ent2: Option<u32>) -> Option<u32> {
        if ent1 == ent2 {
            return None;
        }
        let exists = self.constraints.iter().any(|c| {
            matches!(c.kind, ConstraintKind::NoCollide)
                && ((c.ent1 == ent1 && c.ent2 == ent2) || (c.ent1 == ent2 && c.ent2 == ent1))
        });
        if exists {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.constraints.push(Constraint {
            id,
            owner,
            ent1,
            ent2,
            kind: ConstraintKind::NoCollide,
        });
        Some(id)
    }

    /// `ToolObj:SetObject`: the hit is stored in the entity frame (world space for the world).
    pub fn set_object(&mut self, player: u32, hit: &ToolHit) -> usize {
        let obj = match hit.ent.and_then(|e| self.prop_pose(e)) {
            Some((pos, rot)) => {
                let inv = rot.inverse();
                ToolObject {
                    ent: hit.ent,
                    pos: inv * (hit.pos - pos),
                    normal: inv * (hit.normal + hit.pos - pos),
                    bone: 0,
                }
            }
            None => ToolObject {
                ent: None,
                pos: hit.pos,
                normal: hit.normal,
                bone: 0,
            },
        };
        let tool = self.tools.entry(player).or_default();
        tool.objects.push(obj);
        tool.objects.len()
    }

    pub fn clear_objects(&mut self, player: u32) {
        let tool = self.tools.entry(player).or_default();
        tool.objects.clear();
        tool.stage = 0;
    }

    /// `TOOL:Reload` of the constraint tools: forget the selected objects.
    /// `TOOL:Reload`: the constraint stools remove every constraint of their type touching the
    /// hit entity (`constraint.RemoveConstraints`), true when any went; the world has none.
    pub fn tool_reload(&mut self, player: u32, tool: &str, hit: Option<&ToolHit>) -> bool {
        self.clear_objects(player);
        let Some(ent) = hit.and_then(|h| h.ent) else {
            return false;
        };
        let is_type = |k: &ConstraintKind| match tool {
            "ballsocket" => matches!(k, ConstraintKind::Ballsocket { .. }),
            "slider" => matches!(k, ConstraintKind::Slider { .. }),
            "elastic" => matches!(k, ConstraintKind::Elastic { .. }),
            "hydraulic" => matches!(k, ConstraintKind::Hydraulic { .. }),
            "winch" => matches!(k, ConstraintKind::Winch { .. }),
            "muscle" => matches!(k, ConstraintKind::Muscle { .. }),
            "pulley" => matches!(k, ConstraintKind::Pulley { .. }),
            "axis" => matches!(k, ConstraintKind::Axis { .. }),
            "weld" => matches!(k, ConstraintKind::Weld { .. }),
            "rope" => matches!(k, ConstraintKind::Rope { .. }),
            "nocollide" => matches!(k, ConstraintKind::NoCollide),
            _ => false,
        };
        let before = self.constraints.len();
        self.constraints
            .retain(|c| !(is_type(&c.kind) && (c.ent1 == Some(ent) || c.ent2 == Some(ent))));
        self.drop_parts();
        self.constraints.len() != before
    }

    /// Left click of `weld` or `rope` on what the tool trace hit. Returns what `TOOL:LeftClick`
    /// returns (whether the gun fires its effect).
    /// Tool stage of a player (scripted stools set it like the Rust ones).
    pub fn set_stage(&mut self, player: u32, stage: u32) {
        self.tools.entry(player).or_default().stage = stage;
    }

    /// Whether the Rust sandbox implements this stool (others may come from scripts).
    pub fn knows_tool(tool: &str) -> bool {
        matches!(
            tool,
            "weld"
                | "rope"
                | "axis"
                | "nocollide"
                | "duplicator"
                | "ballsocket"
                | "slider"
                | "hydraulic"
                | "winch"
                | "muscle"
                | "pulley"
                | "elastic"
        )
    }

    /// The rope stool's second click on the two stored objects (the Rust rules; scripted stools
    /// call it through the `rope` tool op).
    pub fn finish_rope(&mut self, player: u32, events: &mut Vec<Event>) -> bool {
        let objs = self.tool(player).objects;
        if objs.len() < 2 {
            return false;
        }
        let (o1, o2) = (&objs[0], &objs[1]);

        if !self.check_limit(player, "ropeconstraints", events) {
            self.clear_objects(player);
            return false;
        }
        let s = self.tool_settings.clone();
        let (w1, w2) = (self.object_world_pos(o1), self.object_world_pos(o2));
        let length = (w1 - w2).length();
        let id = self.rope(player, o1.ent, o2.ent, o1.pos, o2.pos, length, &s);
        if let Some(id) = id {
            self.push_undo(
                player,
                "Rope",
                "#tool.rope.name".into(),
                Some("Undone #tool.rope.name".into()),
                vec![id],
            );
            self.add_count(player, "ropeconstraints", id);
            self.add_cleanup(player, "ropeconstraints", id);
        }
        self.clear_objects(player);
        true
    }

    /// The axis stool's second click on the two stored objects (the Rust rules; scripted stools
    /// call it through the `axis` tool op).
    /// Second click of the ballsocket, slider and elastic stools (measured, `toolgun_ballsocket`
    /// / `_slider` / `_elastic`): the same entity twice makes nothing but still returns true; the
    /// ballsocket counts as a constraint, the other two as rope constraints with their rope.
    pub fn finish_two_point(&mut self, player: u32, tool: &str, events: &mut Vec<Event>) -> bool {
        let objs = self.tool(player).objects;
        if objs.len() < 2 {
            return false;
        }
        let (o1, o2) = (objs[0].clone(), objs[1].clone());
        let count = if tool == "ballsocket" {
            "constraints"
        } else {
            "ropeconstraints"
        };
        if !self.check_limit(player, count, events) {
            self.clear_objects(player);
            return false;
        }
        if o1.ent != o2.ent {
            let s = self.tool_settings.clone();
            let (wpos1, wpos2) = (self.object_world_pos(&o1), self.object_world_pos(&o2));
            let (kind, name, text, rope) = match tool {
                "ballsocket" => (
                    ConstraintKind::Ballsocket {
                        lpos: o2.pos,
                        forcelimit: s.ballsocket_forcelimit,
                        torquelimit: 0.0,
                        nocollide: s.ballsocket_nocollide,
                    },
                    "BallSocket",
                    "ballsocket",
                    false,
                ),
                "slider" => (
                    ConstraintKind::Slider {
                        lpos1: o1.pos,
                        lpos2: o2.pos,
                        width: s.slider_width,
                        material: s.slider_material.clone(),
                        part_of: None,
                    },
                    "Slider",
                    "slider",
                    s.slider_width > 0.0,
                ),
                _ => (
                    ConstraintKind::Elastic {
                        lpos1: o1.pos,
                        lpos2: o2.pos,
                        constant: s.elastic_constant,
                        damping: s.elastic_damping,
                        rdamping: s.elastic_rdamping,
                        width: s.elastic_width,
                        material: s.elastic_material.clone(),
                        length: wpos1.distance(wpos2),
                        stretchonly: s.elastic_stretchonly,
                    },
                    "Elastic",
                    "elastic",
                    s.elastic_width > 0.0,
                ),
            };
            let id = self.next_id;
            self.next_id += 1;
            self.constraints.push(Constraint {
                id,
                owner: player,
                ent1: o1.ent,
                ent2: o2.ent,
                kind,
            });
            // the keyframe rope is its own entity in the undo; none for width <= 0
            let mut undo_ents = vec![id];
            if rope {
                undo_ents.push(self.next_id);
                self.next_id += 1;
            }
            self.push_undo(
                player,
                name,
                format!("#tool.{text}.name"),
                Some(format!("Undone #tool.{text}.name")),
                undo_ents,
            );
            self.add_count(player, count, id);
            self.add_cleanup(player, count, id);
        }
        self.clear_objects(player);
        true
    }

    /// Second click of `hydraulic`, `winch` or `muscle`: the spring with its controller (and
    /// the slider of a fixed one) between the two selected points. The undo lists the
    /// constraint, the rope when drawn, the controller and the slider, as the game's does.
    pub fn finish_powered(&mut self, player: u32, tool: &str, events: &mut Vec<Event>) -> bool {
        let objs = self.tool(player).objects;
        if objs.len() < 2 {
            return false;
        }
        let (o1, o2) = (objs[0].clone(), objs[1].clone());
        if !self.check_limit(player, "ropeconstraints", events) {
            self.clear_objects(player);
            return false;
        }
        if o1.ent != o2.ent {
            let s = self.tool_settings.clone();
            let (wpos1, wpos2) = (self.object_world_pos(&o1), self.object_world_pos(&o2));
            let length = wpos1.distance(wpos2);
            let (m1, m2) = (self.mass_of(o1.ent), self.mass_of(o2.ent));
            let id = self.next_id;
            self.next_id += 1;
            let mut binds: Vec<(i32, NumpadBind)> = Vec::new();
            let (kind, name, text, width, fixed) = match tool {
                "hydraulic" => {
                    let (constant, damping) = elastic_consts(m1, m2, s.hydraulic_fixed);
                    let length2 = length + s.hydraulic_addlength;
                    if s.hydraulic_toggle {
                        binds.push((s.hydraulic_key, NumpadBind::HydraulicToggle(id)));
                    } else {
                        binds.push((s.hydraulic_key, NumpadBind::HydraulicDir(id)));
                    }
                    (
                        ConstraintKind::Hydraulic {
                            lpos1: o1.pos,
                            lpos2: o2.pos,
                            length1: length,
                            length2,
                            width: s.hydraulic_width,
                            material: s.hydraulic_material.clone(),
                            key: s.hydraulic_key,
                            fixed: s.hydraulic_fixed,
                            speed: s.hydraulic_speed,
                            toggle: s.hydraulic_toggle,
                            constant,
                            damping,
                            controller: LengthController::new(
                                length,
                                length,
                                Some(length2),
                                s.hydraulic_speed,
                                s.hydraulic_speed,
                                s.hydraulic_toggle,
                            ),
                        },
                        "Hydraulic",
                        "hydraulic",
                        s.hydraulic_width,
                        s.hydraulic_fixed,
                    )
                }
                "winch" => {
                    let (constant, damping) = elastic_consts(m1, m2, false);
                    if s.winch_toggle {
                        binds.push((s.winch_fwd_key, NumpadBind::WinchToggle(id, 1)));
                        binds.push((s.winch_bwd_key, NumpadBind::WinchToggle(id, -1)));
                    } else {
                        binds.push((s.winch_fwd_key, NumpadBind::WinchDir(id, 1)));
                        binds.push((s.winch_bwd_key, NumpadBind::WinchDir(id, -1)));
                    }
                    (
                        ConstraintKind::Winch {
                            lpos1: o1.pos,
                            lpos2: o2.pos,
                            width: s.winch_width,
                            material: s.winch_material.clone(),
                            fwd_key: s.winch_fwd_key,
                            bwd_key: s.winch_bwd_key,
                            fwd_speed: s.winch_fwd_speed,
                            bwd_speed: s.winch_bwd_speed,
                            toggle: s.winch_toggle,
                            constant,
                            damping,
                            controller: LengthController::new(
                                length,
                                1.0,
                                None,
                                s.winch_fwd_speed,
                                s.winch_bwd_speed,
                                s.winch_toggle,
                            ),
                        },
                        "Winch",
                        "winch",
                        s.winch_width,
                        false,
                    )
                }
                _ => {
                    let (constant, damping) = elastic_consts(m1, m2, s.muscle_fixed);
                    // the stool hands the whole added length to the controller as amplitude
                    let amplitude = s.muscle_addlength;
                    let length2 = length + s.muscle_addlength;
                    binds.push((s.muscle_key, NumpadBind::MuscleToggle(id)));
                    (
                        ConstraintKind::Muscle {
                            lpos1: o1.pos,
                            lpos2: o2.pos,
                            length1: length,
                            length2,
                            width: s.muscle_width,
                            material: s.muscle_material.clone(),
                            key: s.muscle_key,
                            fixed: s.muscle_fixed,
                            period: s.muscle_period,
                            amplitude,
                            starton: s.muscle_starton,
                            constant,
                            damping,
                            controller: LengthController::muscle(
                                length,
                                length,
                                length2,
                                s.muscle_period,
                                amplitude,
                                s.muscle_starton,
                            ),
                        },
                        "Muscle",
                        "muscle",
                        s.muscle_width,
                        s.muscle_fixed,
                    )
                }
            };
            self.constraints.push(Constraint {
                id,
                owner: player,
                ent1: o1.ent,
                ent2: o2.ent,
                kind,
            });
            for (key, bind) in binds {
                self.numpad.entry((player, key)).or_default().push(bind);
            }
            let mut undo_ents = vec![id];
            if width > 0.0 {
                undo_ents.push(self.next_id);
                self.next_id += 1;
            }
            // the controller is an entity of its own in the undo
            undo_ents.push(self.next_id);
            self.next_id += 1;
            if fixed {
                let slider = self.next_id;
                self.next_id += 1;
                self.constraints.push(Constraint {
                    id: slider,
                    owner: player,
                    ent1: o1.ent,
                    ent2: o2.ent,
                    kind: ConstraintKind::Slider {
                        lpos1: o1.pos,
                        lpos2: o2.pos,
                        width: 0.0,
                        material: String::new(),
                        part_of: Some(id),
                    },
                });
                undo_ents.push(slider);
            }
            self.push_undo(
                player,
                name,
                format!("#tool.{text}.name"),
                Some(format!("Undone #tool.{text}.name")),
                undo_ents,
            );
            self.add_count(player, "ropeconstraints", id);
            self.add_cleanup(player, "ropeconstraints", id);
        }
        self.clear_objects(player);
        true
    }

    /// Fourth click of `pulley`: entity, two world points, entity.
    pub fn finish_pulley(&mut self, player: u32, events: &mut Vec<Event>) -> bool {
        let objs = self.tool(player).objects;
        if objs.len() < 4 {
            return false;
        }
        let (o1, o2, o3, o4) = (
            objs[0].clone(),
            objs[1].clone(),
            objs[2].clone(),
            objs[3].clone(),
        );
        if !self.check_limit(player, "ropeconstraints", events) {
            self.clear_objects(player);
            return false;
        }
        if o1.ent != o4.ent {
            let s = self.tool_settings.clone();
            let (wpos2, wpos3, wpos4) = (
                self.object_world_pos(&o2),
                self.object_world_pos(&o3),
                self.object_world_pos(&o4),
            );
            let id = self.next_id;
            self.next_id += 1;
            self.constraints.push(Constraint {
                id,
                owner: player,
                ent1: o1.ent,
                ent2: o4.ent,
                kind: ConstraintKind::Pulley {
                    lpos1: o1.pos,
                    lpos4: o4.pos,
                    wpos2,
                    wpos3,
                    forcelimit: s.pulley_forcelimit,
                    rigid: s.pulley_rigid,
                    width: s.pulley_width,
                    material: s.pulley_material.clone(),
                    addlength: wpos3.distance(wpos4),
                },
            });
            let mut undo_ents = vec![id];
            if s.pulley_width > 0.0 {
                for _ in 0..3 {
                    undo_ents.push(self.next_id);
                    self.next_id += 1;
                }
            }
            self.push_undo(
                player,
                "Pulley",
                "#tool.pulley.name".into(),
                Some("Undone #tool.pulley.name".into()),
                undo_ents,
            );
            self.add_count(player, "ropeconstraints", id);
            self.add_cleanup(player, "ropeconstraints", id);
        }
        self.clear_objects(player);
        true
    }

    pub fn finish_axis(&mut self, player: u32, events: &mut Vec<Event>) -> bool {
        let objs = self.tool(player).objects;
        if objs.len() < 2 {
            return false;
        }
        let (o1, o2) = (&objs[0], &objs[1]);

        if !self.check_limit(player, "constraints", events) {
            self.clear_objects(player);
            return false;
        }
        let (o1, o2) = (o1.clone(), o2.clone());
        let s = self.tool_settings.clone();
        if let Some(ent1) = o1.ent {
            // turn the first object so its hit face meets the second surface, put the hit
            // point 0.2 u in front of the second hit, hinge axis along the second normal
            let norm1 = self.object_world_normal(&o1);
            let norm2 = self.object_world_normal(&o2);
            let wpos2 = self.object_world_pos(&o2);
            let (centre, _) = self.prop_pose(ent1).unwrap_or_default();
            let current = self
                .props
                .iter()
                .find(|p| p.id == ent1)
                .map(|p| p.angles)
                .unwrap_or_default();
            let target = align_angles(current, angle_of(norm1), angle_of(-norm2));
            self.set_prop_pose(ent1, centre, target);
            let grabbed = self.object_world_pos(&o1);
            let target_pos = wpos2 + (centre - grabbed) + norm2 * 0.2;
            self.set_prop_pose(ent1, target_pos, target);
            let (pos1, rot1) = self.prop_pose(ent1).unwrap_or_default();
            let lpos1 = rot1.inverse() * (wpos2 + norm2 - pos1);
            if o2.ent != o1.ent {
                let id = self.next_id;
                self.next_id += 1;
                self.constraints.push(Constraint {
                    id,
                    owner: player,
                    ent1: o1.ent,
                    ent2: o2.ent,
                    kind: ConstraintKind::Axis {
                        lpos1,
                        lpos2: o2.pos,
                        local_axis: lpos1,
                        forcelimit: s.axis_forcelimit,
                        torquelimit: s.axis_torquelimit,
                        friction: s.axis_friction,
                        nocollide: s.axis_nocollide,
                    },
                });
                self.push_undo(
                    player,
                    "Axis",
                    "#tool.axis.name".into(),
                    Some("Undone #tool.axis.name".into()),
                    vec![id],
                );
                self.add_count(player, "constraints", id);
                self.add_cleanup(player, "constraints", id);
            }
        }
        self.clear_objects(player);
        true
    }

    pub fn tool_left_click(
        &mut self,
        player: u32,
        tool: &str,
        hit: &ToolHit,
        events: &mut Vec<Event>,
    ) -> bool {
        if (tool == "nocollide" || tool == "axis")
            && hit.ent.is_none()
            && self.tool(player).objects.is_empty()
        {
            return false;
        }
        // a pulley's first and last click must be entities; the two pulley points are anywhere
        if tool == "pulley"
            && hit.ent.is_none()
            && !matches!(self.tool(player).objects.len(), 1 | 2)
        {
            return false;
        }
        let n = self.set_object(player, hit);
        if n == 1 {
            self.tools.entry(player).or_default().stage = 1;
            return true;
        }
        if tool == "pulley" {
            if n < 4 {
                self.tools.entry(player).or_default().stage = n as u32;
                return true;
            }
            return self.finish_pulley(player, events);
        }
        let objs = self.tool(player).objects;
        let (o1, o2) = (&objs[0], &objs[1]);
        match tool {
            "weld" => {
                if !self.check_limit(player, "constraints", events) {
                    self.clear_objects(player);
                    return false;
                }
                let s = self.tool_settings.clone();
                if let Some(id) =
                    self.weld(player, o1.ent, o2.ent, s.weld_forcelimit, s.weld_nocollide)
                {
                    self.push_undo(
                        player,
                        "Weld",
                        "#tool.weld.name".into(),
                        Some("Undone #tool.weld.name".into()),
                        vec![id],
                    );
                    self.add_count(player, "constraints", id);
                    self.add_cleanup(player, "constraints", id);
                }
                self.clear_objects(player);
                true
            }
            "rope" => self.finish_rope(player, events),
            "axis" => self.finish_axis(player, events),
            "ballsocket" | "slider" | "elastic" => self.finish_two_point(player, tool, events),
            "hydraulic" | "winch" | "muscle" => self.finish_powered(player, tool, events),
            "nocollide" => {
                if !self.check_limit(player, "constraints", events) {
                    self.clear_objects(player);
                    return false;
                }
                if let Some(id) = self.nocollide(player, o1.ent, o2.ent) {
                    self.push_undo(
                        player,
                        "NoCollide",
                        "#tool.nocollide.name".into(),
                        Some("Undone #tool.nocollide.name".into()),
                        vec![id],
                    );
                    self.add_count(player, "constraints", id);
                    self.add_cleanup(player, "nocollide", id);
                }
                self.clear_objects(player);
                true
            }
            _ => {
                self.clear_objects(player);
                false
            }
        }
    }

    /// `constraint.Weld`: nothing for the same physics object, nothing when the pair is
    /// already welded, the world always goes second.
    pub fn weld(
        &mut self,
        owner: u32,
        ent1: Option<u32>,
        ent2: Option<u32>,
        forcelimit: f32,
        nocollide: bool,
    ) -> Option<u32> {
        if ent1 == ent2 {
            return None;
        }
        let exists = self.constraints.iter().any(|c| {
            matches!(c.kind, ConstraintKind::Weld { .. })
                && ((c.ent1 == ent1 && c.ent2 == ent2) || (c.ent1 == ent2 && c.ent2 == ent1))
        });
        if exists {
            return None;
        }
        let (ent1, ent2) = if ent1.is_none() {
            (ent2, ent1)
        } else {
            (ent1, ent2)
        };
        let id = self.next_id;
        self.next_id += 1;
        self.constraints.push(Constraint {
            id,
            owner,
            ent1,
            ent2,
            kind: ConstraintKind::Weld {
                forcelimit,
                nocollide,
            },
        });
        Some(id)
    }

    /// `constraint.Rope`: a `phys_lengthconstraint` of the measured length plus `addlength`
    /// between two different physics objects; the same object twice makes no constraint (and at
    /// width 0 no rope either).
    #[allow(clippy::too_many_arguments)]
    pub fn rope(
        &mut self,
        owner: u32,
        ent1: Option<u32>,
        ent2: Option<u32>,
        lpos1: Vec3,
        lpos2: Vec3,
        length: f32,
        s: &ToolSettings,
    ) -> Option<u32> {
        if ent1 == ent2 {
            return None;
        }
        let addlength = s.rope_addlength.clamp(-MAX_TRACE_LENGTH, MAX_TRACE_LENGTH);
        let id = self.next_id;
        self.next_id += 1;
        self.constraints.push(Constraint {
            id,
            owner,
            ent1,
            ent2,
            kind: ConstraintKind::Rope {
                lpos1,
                lpos2,
                length,
                addlength,
                width: s.rope_width,
                material: s.rope_material.clone(),
                rigid: s.rope_rigid,
            },
        });
        Some(id)
    }

    /// Removes an entity; a prop takes its constraints with it.
    fn remove(&mut self, id: u32) -> bool {
        let props_before = self.props.len();
        self.props.retain(|p| p.id != id);
        if self.props.len() != props_before {
            self.constraints
                .retain(|c| c.ent1 != Some(id) && c.ent2 != Some(id));
            return true;
        }
        let before = self.constraints.len();
        self.constraints.retain(|c| c.id != id);
        self.drop_parts();
        self.constraints.len() != before
    }

    /// `DeleteOnRemove`: a fixed hydraulic's or muscle's slider goes with its owner.
    fn drop_parts(&mut self) {
        let ids: Vec<u32> = self.constraints.iter().map(|c| c.id).collect();
        self.constraints.retain(|c| match &c.kind {
            ConstraintKind::Slider {
                part_of: Some(p), ..
            } => ids.contains(p),
            _ => true,
        });
    }

    /// Mass of a prop (kg) for the elastic constants; the physics owner reports it.
    pub fn set_mass(&mut self, ent: u32, mass: f32) {
        self.masses.insert(ent, mass);
    }

    fn mass_of(&self, ent: Option<u32>) -> Option<f32> {
        ent.map(|e| self.masses.get(&e).copied().unwrap_or(1.0))
    }

    /// `numpad.Activate` / `Deactivate`: a key edge of a player drives every controller bound
    /// to it; a repeat while held does nothing, bindings of gone constraints are dropped.
    pub fn numpad(&mut self, player: u32, key: i32, down: bool) {
        let held = self.numpad_held.entry(player).or_default();
        if held.contains(&key) == down {
            return;
        }
        if down {
            held.push(key);
        } else {
            held.retain(|k| *k != key);
        }
        let Some(binds) = self.numpad.get_mut(&(player, key)) else {
            return;
        };
        let live: Vec<u32> = self.constraints.iter().map(|c| c.id).collect();
        binds.retain(|b| live.contains(&b.constraint()));
        let binds = binds.clone();
        for b in binds {
            let Some(c) = self.constraints.iter_mut().find(|c| c.id == b.constraint()) else {
                continue;
            };
            let Some(ctrl) = c.kind.controller_mut() else {
                continue;
            };
            match b {
                NumpadBind::HydraulicToggle(_) if down => ctrl.hydraulic_toggle(),
                NumpadBind::HydraulicDir(_) => ctrl.set_direction(if down { 1 } else { -1 }),
                NumpadBind::WinchDir(_, d) => ctrl.set_direction(if down { d } else { 0 }),
                NumpadBind::WinchToggle(_, d) if down => ctrl.winch_toggle(d),
                NumpadBind::MuscleToggle(_) if down => ctrl.muscle_toggle(),
                _ => {}
            }
        }
    }

    /// Keys a player holds (`numpad` state), for snapshots.
    pub fn numpad_held(&self, player: u32) -> &[i32] {
        self.numpad_held.get(&player).map_or(&[], Vec::as_slice)
    }

    /// One tick of every length controller; returns the constraints whose spring rest length
    /// changed and the new length.
    pub fn tick_controllers(&mut self, dt: f32) -> Vec<(u32, f32)> {
        let mut out = Vec::new();
        for c in &mut self.constraints {
            if let Some(ctrl) = c.kind.controller_mut() {
                if let Some(len) = ctrl.tick(dt) {
                    out.push((c.id, len));
                }
            }
        }
        out
    }

    /// `gmod_undo`: pops the player's last undo; entries whose entities are already gone are
    /// popped too until one removes something.
    pub fn undo(&mut self, player: u32, events: &mut Vec<Event>) {
        loop {
            let Some(u) = self.undo.get_mut(&player).and_then(Vec::pop) else {
                return;
            };
            let mut removed = 0;
            for &e in &u.entities {
                if self.remove(e) {
                    removed += 1;
                    events.push(Event::Removed { prop: e });
                }
            }
            events.push(Event::Undone { id: u.id, removed });
            if removed > 0 {
                return;
            }
        }
    }

    /// `gmod_cleanup` without a type: everything the player spawned goes; undo entries stay.
    pub fn cleanup(&mut self, player: u32, events: &mut Vec<Event>) {
        if let Some(lists) = self.cleanup.remove(&player) {
            for id in lists.into_values().flatten() {
                if self.remove(id) {
                    events.push(Event::Removed { prop: id });
                }
            }
        }
        events.push(Event::Cleanup { kind: "all".into() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn floor() -> World {
        World::from_boxes(&[(
            Vec3::new(-4096.0, -4096.0, -200.0),
            Vec3::new(4096.0, 4096.0, -144.0),
        )])
    }

    fn crate_bounds() -> ModelBounds {
        ModelBounds {
            min: Vec3::new(-20.308, -20.150, -20.227),
            max: Vec3::new(20.231, 20.162, 20.011),
        }
    }

    #[test]
    fn floor_spawn_sits_flush_facing_the_player() {
        let p = place_prop(
            &floor(),
            &[],
            Vec3::new(823.235, -500.0, -79.0),
            Vec3::new(30.0, 0.0, 0.0),
            &crate_bounds(),
        );
        assert!((p.hit.z + 144.0).abs() < 0.05);
        assert!((p.pos.z - (p.hit.z + 20.227)).abs() < 1e-3);
        assert!((p.pos.x - p.hit.x).abs() < 1e-3);
        assert_eq!(p.angles, Vec3::new(0.0, 180.0, 0.0));
    }

    #[test]
    fn limits_and_undo() {
        let w = floor();
        let mut sb = Sandbox::new(Limits {
            max_props: 2,
            ..Limits::default()
        });
        let mut ev = Vec::new();
        let eye = Vec3::new(0.0, 0.0, -80.0);
        let b = crate_bounds();
        let req = |yaw: f32| SpawnRequest {
            eye,
            eye_angles: Vec3::new(30.0, yaw, 0.0),
            model: "m",
            bounds: b,
        };
        for yaw in [0.0, 45.0, 90.0] {
            sb.spawn_prop(&w, &[], 1, &req(yaw), &mut ev);
        }
        assert_eq!(sb.count(1), 2);
        assert!(matches!(ev.last(), Some(Event::LimitHit { .. })));
        assert_eq!(sb.undos(1).iter().map(|u| u.id).collect::<Vec<_>>(), [1, 2]);
        sb.undo(1, &mut ev);
        assert_eq!(sb.count(1), 1);
        sb.spawn_prop(&w, &[], 1, &req(135.0), &mut ev);
        assert_eq!(sb.undos(1).iter().map(|u| u.id).collect::<Vec<_>>(), [1, 2]);
        sb.cleanup(1, &mut ev);
        assert_eq!(sb.count(1), 0);
        assert_eq!(sb.undos(1).len(), 2);
        sb.undo(1, &mut ev);
        assert!(sb.undos(1).is_empty());
    }

    #[test]
    fn weld_and_rope_through_the_tool() {
        let w = floor();
        let mut sb = Sandbox::new(Limits::default());
        let mut ev = Vec::new();
        let eye = Vec3::new(823.235, -500.0, -79.0);
        let b = crate_bounds();
        let a = sb
            .spawn_prop(
                &w,
                &[],
                1,
                &SpawnRequest {
                    eye,
                    eye_angles: Vec3::new(30.0, 0.0, 0.0),
                    model: "m",
                    bounds: b,
                },
                &mut ev,
            )
            .unwrap();
        let c = sb
            .spawn_prop(
                &w,
                &[],
                1,
                &SpawnRequest {
                    eye,
                    eye_angles: Vec3::new(30.0, 40.0, 0.0),
                    model: "m",
                    bounds: b,
                },
                &mut ev,
            )
            .unwrap();
        let hit = |ent: Option<u32>, pos: Vec3| ToolHit {
            pos,
            normal: Vec3::Z,
            ent,
            fraction: 0.5,
        };
        assert!(sb.tool_left_click(
            1,
            "weld",
            &hit(Some(a), Vec3::new(935.0, -500.0, -104.0)),
            &mut ev
        ));
        assert_eq!(sb.tool(1).stage, 1);
        assert!(sb.tool_left_click(
            1,
            "weld",
            &hit(Some(c), Vec3::new(900.0, -440.0, -104.0)),
            &mut ev
        ));
        assert_eq!(sb.tool(1).stage, 0);
        assert_eq!(sb.count_of(1, "constraints"), 1);
        // same pair again: no second weld
        sb.tool_left_click(1, "weld", &hit(Some(c), Vec3::ZERO), &mut ev);
        sb.tool_left_click(1, "weld", &hit(Some(a), Vec3::ZERO), &mut ev);
        assert_eq!(sb.count_of(1, "constraints"), 1);
        // rope to the world: length between the attachment points
        sb.tool_left_click(
            1,
            "rope",
            &hit(Some(a), Vec3::new(935.0, -500.0, -104.0)),
            &mut ev,
        );
        sb.tool_left_click(
            1,
            "rope",
            &hit(None, Vec3::new(935.0, -500.0, -144.0)),
            &mut ev,
        );
        let ConstraintKind::Rope { length, .. } = &sb.constraints().last().unwrap().kind else {
            panic!()
        };
        assert!((length - 40.0).abs() < 1e-3);
        assert_eq!(
            sb.undos(1)
                .iter()
                .map(|u| u.name.as_str())
                .collect::<Vec<_>>(),
            ["prop_physics", "prop_physics", "Weld", "Rope"]
        );
        sb.undo(1, &mut ev);
        assert_eq!(sb.count_of(1, "ropeconstraints"), 0);
        // removing a prop takes its weld along
        sb.undo(1, &mut ev);
        sb.undo(1, &mut ev);
        assert_eq!(sb.count_of(1, "constraints"), 0);
    }

    /// `gmod_winch_controller` as measured: 64 u/s from the tick after the key, a toggle stops at
    /// its limit and the next press returns; a winch floors at 1 u; a muscle rides a sine from
    /// its start length with the stool's whole added length as amplitude.
    #[test]
    fn length_controllers() {
        let dt = 0.015;
        let mut h = LengthController::new(21.4, 21.4, Some(41.4), 64.0, 64.0, true);
        assert_eq!(h.tick(dt), None);
        h.hydraulic_toggle();
        assert_eq!(h.direction, 1);
        assert!((h.tick(dt).unwrap() - 22.36).abs() < 1e-3);
        for _ in 0..40 {
            h.tick(dt);
        }
        assert_eq!((h.length, h.direction, h.expanded), (41.4, 0, true));
        h.hydraulic_toggle();
        assert_eq!(h.direction, -1);
        for _ in 0..40 {
            h.tick(dt);
        }
        assert_eq!((h.length, h.direction), (21.4, 0));

        let mut w = LengthController::new(21.4, 1.0, None, 64.0, 64.0, false);
        w.set_direction(-1);
        for _ in 0..40 {
            w.tick(dt);
        }
        assert_eq!((w.length, w.direction), (1.0, -1));

        let mut m = LengthController::muscle(44.785, 44.785, 144.785, 1.0, 100.0, false);
        assert!((m.ctime + 0.5).abs() < 1e-4);
        m.muscle_toggle();
        let mut last = 0.0;
        for _ in 0..60 {
            last = m.tick(dt).unwrap();
        }
        assert!((last - 141.5575).abs() < 1e-2, "{last}");
        m.muscle_toggle();
        assert_eq!(m.tick(dt), None);
    }

    /// Hydraulic stool: spring + slider + controller + rope in one undo, the key drives it,
    /// reload on an end removes the slider with it.
    #[test]
    fn hydraulic_stool() {
        let mut sb = Sandbox::new(Limits::default());
        let mut ev = Vec::new();
        let crate_ = "models/props_junk/wood_crate001a.mdl";
        let a = sb.restore_prop(1, crate_, Vec3::ZERO, Vec3::ZERO, false);
        let b = sb.restore_prop(1, crate_, Vec3::new(0.0, 0.0, 60.0), Vec3::ZERO, true);
        sb.set_mass(a, 30.0);
        sb.set_mass(b, 30.0);
        let hit = |ent, z| ToolHit {
            ent: Some(ent),
            pos: Vec3::new(0.0, 0.0, z),
            normal: Vec3::Z,
            fraction: 0.5,
        };
        assert!(sb.tool_left_click(1, "hydraulic", &hit(a, 19.0), &mut ev));
        assert_eq!(sb.tool(1).stage, 1);
        assert!(sb.tool_left_click(1, "hydraulic", &hit(b, 41.0), &mut ev));
        assert_eq!(sb.constraints().len(), 2);
        assert_eq!(sb.undos(1)[0].entities.len(), 4);
        assert_eq!(sb.count_of(1, "ropeconstraints"), 1);
        let (k, c) = match &sb.constraints()[0].kind {
            ConstraintKind::Hydraulic {
                constant,
                damping,
                length1,
                length2,
                ..
            } => {
                assert_eq!((*length1, *length2), (22.0, 122.0));
                (*constant, *damping)
            }
            k => panic!("{k:?}"),
        };
        assert_eq!((k, c), (3000.0, 600.0));
        sb.numpad(1, 37, true);
        sb.numpad(1, 37, true);
        assert_eq!(
            sb.tick_controllers(0.015),
            vec![(sb.constraints()[0].id, 22.96)]
        );
        sb.numpad(1, 37, false);
        sb.numpad(1, 37, true);
        assert_eq!(sb.constraints()[0].kind.controller().unwrap().direction, -1);
        assert!(sb.tool_reload(1, "hydraulic", Some(&hit(b, 41.0))));
        assert!(sb.constraints().is_empty());
        sb.numpad(1, 37, false);
        assert!(sb.tick_controllers(0.015).is_empty());
    }
}
