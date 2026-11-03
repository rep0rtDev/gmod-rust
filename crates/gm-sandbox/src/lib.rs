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
    /// `Entity:SetMaterial` override (the material stool), empty for the model's own
    pub material: String,
    /// `Entity:SetColor` r, g, b, a
    pub color: [u8; 4],
    /// `RENDERMODE_*` 0..9 and `renderfx` (the colour stool)
    pub render_mode: u8,
    pub render_fx: u8,
    /// `PhysObj:SetMaterial` surfaceprop override (the physprop stool), empty for the PHY's
    pub phys_material: String,
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
    Undone {
        id: u32,
        removed: usize,
        name: String,
    },
    /// `gmod_cleanup` without arguments: `hook.Run('OnCleanup','all')`.
    Cleanup { kind: String },
    /// An entity left the table (`EntityRemoved`), one per entity, before the Undone/Cleanup.
    Removed { prop: u32 },
    /// paint stool: `util.Decal(name, pos1, pos2)` for the clients.
    Decal {
        name: String,
        pos1: Vec3,
        pos2: Vec3,
    },
    /// `Player:SendLua` from a stool (the remover's achievement pokes).
    Lua { code: String },
}

/// `sbox_*` limits (measured defaults: props 200, constraints 2000, ropeconstraints 1000);
/// negative means unlimited, as in `Player:CheckLimit`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    pub max_props: i32,
    pub max_constraints: i32,
    pub max_ropeconstraints: i32,
    pub max_thrusters: i32,
    pub max_hoverballs: i32,
    pub max_balloons: i32,
    pub max_wheels: i32,
    pub max_buttons: i32,
    pub max_dynamite: i32,
    pub max_lamps: i32,
    pub max_lights: i32,
    pub max_emitters: i32,
    pub max_cameras: i32,
    pub max_ragdolls: i32,
    pub max_npcs: i32,
    pub max_vehicles: i32,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_props: 200,
            max_constraints: 2000,
            max_ropeconstraints: 1000,
            max_thrusters: 30,
            max_hoverballs: 50,
            max_balloons: 50,
            max_wheels: 50,
            max_buttons: 50,
            max_dynamite: 20,
            max_lamps: 3,
            max_lights: 5,
            max_emitters: 20,
            max_cameras: 10,
            max_ragdolls: 10,
            max_npcs: 10,
            max_vehicles: 4,
        }
    }
}

impl Limits {
    pub fn get(&self, kind: &str) -> i32 {
        match kind {
            "props" => self.max_props,
            "ragdolls" => self.max_ragdolls,
            "npcs" => self.max_npcs,
            "vehicles" => self.max_vehicles,
            "constraints" => self.max_constraints,
            "ropeconstraints" => self.max_ropeconstraints,
            "thrusters" => self.max_thrusters,
            "hoverballs" => self.max_hoverballs,
            "balloons" => self.max_balloons,
            "wheels" => self.max_wheels,
            "buttons" => self.max_buttons,
            "dynamite" => self.max_dynamite,
            "lamps" => self.max_lamps,
            "lights" => self.max_lights,
            "emitters" => self.max_emitters,
            "cameras" => self.max_cameras,
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

/// What `place_ragdoll` needs of a ragdoll model beyond `SpawnRequest`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RagdollSpawn {
    /// Root bone (physics object 0) position in model space in the model's ragdoll pose.
    pub root_local: Vec3,
    /// The entity's collision bounds (hull box widened by the root solid), unrotated about the
    /// root.
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

/// Every member of the rigid assembly `start` belongs to through weld constraints (transitive;
/// the game's weld is a rigid IVP constraint, so a force on one member moves them all as one body).
pub fn weld_group<T: Copy + PartialEq>(welds: &[(T, T)], start: T) -> Vec<T> {
    let mut group = vec![start];
    let mut i = 0;
    while i < group.len() {
        let cur = group[i];
        i += 1;
        for &(a, b) in welds {
            let other = if a == cur {
                b
            } else if b == cur {
                a
            } else {
                continue;
            };
            if !group.contains(&other) {
                group.push(other);
            }
        }
    }
    group
}

#[derive(Clone, Debug, PartialEq)]
pub struct Constraint {
    pub id: u32,
    pub owner: u32,
    pub ent1: Option<u32>,
    pub ent2: Option<u32>,
    pub kind: ConstraintKind,
    /// Colour of its rope (`keyframe_rope`), from the tool's `*_color_r/g/b`; white otherwise.
    pub color: [u8; 3],
}

/// A tool's colour convars (0..255 each) as the rope colour.
pub fn color_u8(r: f32, g: f32, b: f32) -> [u8; 3] {
    [r, g, b].map(|c| c.clamp(0.0, 255.0).round() as u8)
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
    /// The stool that selected `objects` (the game keeps one object list per tool mode; we
    /// keep the current mode's and drop it when another stool starts selecting).
    pub objects_tool: String,
    pub stage: u32,
    /// `TOOL:SetOperation`: an NWInt on the toolgun itself, so shared across modes (the weld's
    /// left click sets 2, which makes the axis' left click refuse until something sets 1).
    pub operation: u32,
}

/// Client tool settings the stools read with `GetClientNumber`/`GetClientInfo`.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolSettings {
    pub axis_forcelimit: f32,
    pub axis_torquelimit: f32,
    pub axis_hingefriction: f32,
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
    pub elastic_stretch_only: bool,
    pub hydraulic_width: f32,
    pub hydraulic_addlength: f32,
    pub hydraulic_fixed: bool,
    pub hydraulic_speed: f32,
    pub hydraulic_toggle: bool,
    pub hydraulic_material: String,
    pub hydraulic_group: i32,
    pub winch_rope_width: f32,
    pub winch_fwd_speed: f32,
    pub winch_bwd_speed: f32,
    pub winch_fwd_group: i32,
    pub winch_bwd_group: i32,
    pub winch_rope_material: String,
    pub winch_toggle: bool,
    pub muscle_width: f32,
    pub muscle_addlength: f32,
    pub muscle_fixed: bool,
    pub muscle_period: f32,
    pub muscle_starton: bool,
    pub muscle_material: String,
    pub muscle_group: i32,
    pub pulley_width: f32,
    pub pulley_forcelimit: f32,
    pub pulley_rigid: bool,
    pub pulley_material: String,
    pub thruster_force: f32,
    pub thruster_model: String,
    pub thruster_keygroup: i32,
    pub thruster_keygroup_back: i32,
    pub thruster_toggle: bool,
    pub thruster_collision: bool,
    /// Soundscript of the loop while thrusting; one of [`THRUSTER_SOUNDS`] or nothing.
    pub thruster_soundname: String,
    pub rope_color_r: f32,
    pub rope_color_g: f32,
    pub rope_color_b: f32,
    pub elastic_color_r: f32,
    pub elastic_color_g: f32,
    pub elastic_color_b: f32,
    pub hydraulic_color_r: f32,
    pub hydraulic_color_g: f32,
    pub hydraulic_color_b: f32,
    pub muscle_color_r: f32,
    pub muscle_color_g: f32,
    pub muscle_color_b: f32,
    pub pulley_color_r: f32,
    pub pulley_color_g: f32,
    pub pulley_color_b: f32,
    pub slider_color_r: f32,
    pub slider_color_g: f32,
    pub slider_color_b: f32,
    pub winch_color_r: f32,
    pub winch_color_g: f32,
    pub winch_color_b: f32,
    pub balloon_r: f32,
    pub balloon_g: f32,
    pub balloon_b: f32,
    pub ballsocket_torquelimit: f32,
    pub thruster_effect: String,
    pub thruster_damageable: bool,
    pub button_description: String,
    pub hoverball_keyup: i32,
    pub hoverball_keydn: i32,
    pub hoverball_keyon: i32,
    pub hoverball_speed: f32,
    pub hoverball_resistance: f32,
    pub hoverball_strength: f32,
    pub hoverball_model: String,
    pub balloon_ropelength: f32,
    pub balloon_force: f32,
    pub balloon_model: String,
    pub wheel_torque: f32,
    pub wheel_friction: f32,
    pub wheel_nocollide: bool,
    pub wheel_forcelimit: f32,
    pub wheel_fwd: i32,
    pub wheel_bck: i32,
    pub wheel_toggle: bool,
    pub wheel_model: String,
    pub wheel_rx: f32,
    pub wheel_ry: f32,
    pub wheel_rz: f32,
    pub motor_torque: f32,
    pub motor_friction: f32,
    pub motor_forcelimit: f32,
    pub motor_forcetime: f32,
    pub motor_nocollide: bool,
    pub motor_toggle: bool,
    pub motor_fwd: i32,
    pub motor_bwd: i32,
    pub button_model: String,
    pub button_keygroup: i32,
    pub button_toggle: bool,
    pub dynamite_group: i32,
    pub dynamite_damage: f32,
    pub dynamite_delay: f32,
    pub dynamite_model: String,
    pub lamp_r: f32,
    pub lamp_g: f32,
    pub lamp_b: f32,
    pub lamp_fov: f32,
    pub lamp_distance: f32,
    pub lamp_brightness: f32,
    pub lamp_toggle: bool,
    pub lamp_texture: String,
    pub lamp_model: String,
    pub lamp_key: i32,
    pub light_r: f32,
    pub light_g: f32,
    pub light_b: f32,
    pub light_brightness: f32,
    pub light_size: f32,
    pub light_ropelength: f32,
    pub light_toggle: bool,
    pub light_ropematerial: String,
    pub light_key: i32,
    pub material_override: String,
    pub colour_r: f32,
    pub colour_g: f32,
    pub colour_b: f32,
    pub colour_a: f32,
    pub colour_mode: f32,
    pub colour_fx: f32,
    pub physprop_gravity_toggle: bool,
    pub physprop_material: String,
    pub trails_r: f32,
    pub trails_g: f32,
    pub trails_b: f32,
    pub trails_a: f32,
    pub trails_length: f32,
    pub trails_startsize: f32,
    pub trails_endsize: f32,
    pub trails_material: String,
    pub emitter_key: i32,
    pub emitter_delay: f32,
    pub emitter_scale: f32,
    pub emitter_toggle: bool,
    pub emitter_starton: bool,
    pub emitter_effect: String,
    pub camera_key: i32,
    pub camera_toggle: bool,
    pub camera_locked: bool,
    pub dynamite_remove: bool,
    /// eyeposer sliders `x` / `y` (0..1 -> -1..1) and `strabismus` (-1..1).
    pub eyeposer_x: f32,
    pub eyeposer_y: f32,
    pub eyeposer_strabismus: f32,
    /// faceposer `flex<i>` sliders (in the controller's own range) and `scale`.
    pub faceposer_flex: Vec<f32>,
    pub faceposer_scale: f32,
    /// finger poser `finger_<i>` = "x y" per finger part (15 per hand).
    pub finger: Vec<[f32; 2]>,
    /// paint stool `decal` name.
    pub paint_decal: String,
    /// creator stool `type` (0 SENT, 1 vehicle, 2 NPC, 3 weapon, 4 prop) and `name`.
    pub creator_type: i32,
    pub creator_name: String,
}

impl ToolSettings {
    /// `tool <name> <key> <value>` from a client's ControlPanel: the stools' ClientConVars by
    /// field name; false for an unknown key or an unparsable number.
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        macro_rules! fields {
            ($($f32s:ident),* ; $($bools:ident),* ; $($strs:ident),* ; $($ints:ident),*) => {
                match key {
                    $(stringify!($f32s) => match value.trim().parse::<f32>() {
                        Ok(v) => { self.$f32s = v; true }
                        Err(_) => false,
                    })*
                    $(stringify!($bools) => { self.$bools = matches!(value.trim(), "1" | "true" | "on"); true })*
                    $(stringify!($strs) => { self.$strs = value.to_string(); true })*
                    $(stringify!($ints) => match value.trim().parse::<f32>() {
                        Ok(v) => { self.$ints = v as i32; true }
                        Err(_) => false,
                    })*
                    _ => self.set_indexed(key, value),
                }
            };
        }
        fields!(
            faceposer_scale, axis_forcelimit, axis_torquelimit, axis_hingefriction, weld_forcelimit, rope_forcelimit,
            rope_addlength, rope_width, ballsocket_forcelimit, slider_width, elastic_constant,
            elastic_damping, elastic_rdamping, elastic_width, hydraulic_width, hydraulic_addlength,
            hydraulic_speed, winch_rope_width, winch_fwd_speed, winch_bwd_speed, muscle_width,
            muscle_addlength, muscle_period, pulley_width, pulley_forcelimit, thruster_force,
            hoverball_speed, hoverball_resistance, hoverball_strength, balloon_ropelength,
            balloon_force, wheel_torque, wheel_friction, wheel_forcelimit, wheel_rx, wheel_ry,
            wheel_rz, motor_torque, motor_friction, motor_forcelimit, motor_forcetime,
            dynamite_damage, dynamite_delay, lamp_r, lamp_g, lamp_b, lamp_fov, lamp_distance,
            lamp_brightness, light_r, light_g, light_b, light_brightness, light_size,
            light_ropelength, colour_r, colour_g, colour_b, colour_a, colour_mode, colour_fx,
            trails_r, trails_g, trails_b, trails_a, trails_length, trails_startsize, trails_endsize,
            emitter_delay, emitter_scale, rope_color_r, rope_color_g, rope_color_b, elastic_color_r, elastic_color_g, elastic_color_b, hydraulic_color_r, hydraulic_color_g, hydraulic_color_b, muscle_color_r, muscle_color_g, muscle_color_b, pulley_color_r, pulley_color_g, pulley_color_b, slider_color_r, slider_color_g, slider_color_b, winch_color_r, winch_color_g, winch_color_b, balloon_r, balloon_g, balloon_b, ballsocket_torquelimit;
            axis_nocollide, weld_nocollide, rope_rigid, ballsocket_nocollide, elastic_stretch_only,
            hydraulic_fixed, hydraulic_toggle, winch_toggle, muscle_fixed, muscle_starton,
            pulley_rigid, thruster_toggle, thruster_collision, wheel_nocollide, wheel_toggle,
            motor_nocollide, motor_toggle,
            button_toggle, dynamite_remove, lamp_toggle, light_toggle, physprop_gravity_toggle,
            emitter_toggle, emitter_starton, camera_toggle, camera_locked, thruster_damageable;
            rope_material, slider_material, elastic_material, hydraulic_material, winch_rope_material,
            muscle_material, pulley_material, thruster_model, thruster_soundname, hoverball_model,
            balloon_model,
            wheel_model, button_model, dynamite_model, lamp_texture, lamp_model, light_ropematerial,
            material_override, physprop_material, trails_material, emitter_effect, thruster_effect, button_description;
            hydraulic_group, winch_fwd_group, winch_bwd_group, muscle_group, thruster_keygroup,
            thruster_keygroup_back, hoverball_keyup, hoverball_keydn, hoverball_keyon, wheel_fwd,
            wheel_bck, motor_fwd, motor_bwd, button_keygroup, dynamite_group, lamp_key, light_key,
            emitter_key, camera_key
        )
    }

    /// The posers' indexed convars: `faceposer_flex<i>` (a flex weight), `finger_<i>` ("x y"
    /// degrees of a finger part) and the native panel's `finger_<i>_x` / `finger_<i>_y` sliders.
    fn set_indexed(&mut self, key: &str, value: &str) -> bool {
        let num = |s: &str| s.trim().parse::<f32>().ok();
        if let Some(i) = key
            .strip_prefix("faceposer_flex")
            .and_then(|i| i.parse::<usize>().ok())
        {
            return match (self.faceposer_flex.get_mut(i), num(value)) {
                (Some(slot), Some(v)) => {
                    *slot = v;
                    true
                }
                _ => false,
            };
        }
        let Some(rest) = key.strip_prefix("finger_") else {
            return false;
        };
        let (idx, axis) = match (rest.strip_suffix("_x"), rest.strip_suffix("_y")) {
            (Some(i), _) => (i, Some(0)),
            (_, Some(i)) => (i, Some(1)),
            _ => (rest, None),
        };
        let Some(slot) = idx
            .parse::<usize>()
            .ok()
            .and_then(|i| self.finger.get_mut(i))
        else {
            return false;
        };
        match axis {
            Some(a) => match num(value) {
                Some(v) => {
                    slot[a] = v;
                    true
                }
                None => false,
            },
            None => {
                let mut it = value.split_whitespace().map(num);
                match (it.next().flatten(), it.next().flatten()) {
                    (Some(x), Some(y)) => {
                        *slot = [x, y];
                        true
                    }
                    _ => false,
                }
            }
        }
    }
}

impl Default for ToolSettings {
    /// A player without the convars (a bot) reads 0 and "" everywhere.
    fn default() -> Self {
        ToolSettings {
            axis_forcelimit: 0.0,
            axis_torquelimit: 0.0,
            axis_hingefriction: 0.0,
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
            elastic_stretch_only: true,
            hydraulic_width: 3.0,
            hydraulic_addlength: 100.0,
            hydraulic_fixed: true,
            hydraulic_speed: 64.0,
            hydraulic_toggle: true,
            hydraulic_material: "cable/rope".into(),
            hydraulic_group: 37,
            winch_rope_width: 3.0,
            winch_fwd_speed: 64.0,
            winch_bwd_speed: 64.0,
            winch_fwd_group: 44,
            winch_bwd_group: 41,
            winch_rope_material: "cable/rope".into(),
            winch_toggle: false,
            muscle_width: 2.0,
            muscle_addlength: 100.0,
            muscle_fixed: true,
            muscle_period: 1.0,
            muscle_starton: false,
            muscle_material: "cable/rope".into(),
            muscle_group: 37,
            pulley_width: 3.0,
            pulley_forcelimit: 0.0,
            pulley_rigid: false,
            pulley_material: "cable/cable".into(),
            thruster_force: 1500.0,
            thruster_model: "models/props_phx2/garbage_metalcan001a.mdl".into(),
            thruster_keygroup: 45,
            thruster_keygroup_back: 42,
            thruster_toggle: false,
            thruster_soundname: "PhysicsCannister.ThrusterLoop".into(),
            rope_color_r: 255.0,
            rope_color_g: 255.0,
            rope_color_b: 255.0,
            elastic_color_r: 255.0,
            elastic_color_g: 255.0,
            elastic_color_b: 255.0,
            hydraulic_color_r: 255.0,
            hydraulic_color_g: 255.0,
            hydraulic_color_b: 255.0,
            muscle_color_r: 255.0,
            muscle_color_g: 255.0,
            muscle_color_b: 255.0,
            pulley_color_r: 255.0,
            pulley_color_g: 255.0,
            pulley_color_b: 255.0,
            slider_color_r: 255.0,
            slider_color_g: 255.0,
            slider_color_b: 255.0,
            winch_color_r: 255.0,
            winch_color_g: 255.0,
            winch_color_b: 255.0,
            balloon_r: 255.0,
            balloon_g: 255.0,
            balloon_b: 0.0,
            ballsocket_torquelimit: 0.0,
            thruster_effect: "fire".into(),
            thruster_damageable: false,
            button_description: "".into(),
            thruster_collision: false,
            hoverball_keyup: 46,
            hoverball_keydn: 43,
            hoverball_keyon: 40,
            hoverball_speed: 5.0,
            hoverball_resistance: 5.0,
            hoverball_strength: 10.0,
            hoverball_model: "models/dav0r/hoverball.mdl".into(),
            balloon_ropelength: 64.0,
            balloon_force: 500.0,
            balloon_model: "models/maxofs2d/balloon_classic.mdl".into(),
            wheel_torque: 3000.0,
            wheel_friction: 1.0,
            wheel_nocollide: true,
            wheel_forcelimit: 0.0,
            wheel_fwd: 45,
            wheel_bck: 42,
            wheel_toggle: false,
            wheel_model: "models/props_vehicles/carparts_wheel01a.mdl".into(),
            wheel_rx: 90.0,
            wheel_ry: 0.0,
            wheel_rz: 90.0,
            motor_torque: 500.0,
            motor_friction: 1.0,
            motor_forcelimit: 0.0,
            motor_forcetime: 0.0,
            motor_nocollide: true,
            motor_toggle: false,
            motor_fwd: 45,
            motor_bwd: 42,
            button_model: "models/maxofs2d/button_05.mdl".into(),
            button_keygroup: 37,
            button_toggle: true,
            dynamite_group: 52,
            dynamite_damage: 200.0,
            dynamite_delay: 0.0,
            dynamite_model: "models/dav0r/tnt/tnt.mdl".into(),
            lamp_r: 255.0,
            lamp_g: 255.0,
            lamp_b: 255.0,
            lamp_fov: 90.0,
            lamp_distance: 1024.0,
            lamp_brightness: 4.0,
            lamp_toggle: true,
            lamp_texture: "effects/flashlight001".into(),
            lamp_model: "models/lamps/torch.mdl".into(),
            lamp_key: 37,
            light_r: 255.0,
            light_g: 255.0,
            light_b: 255.0,
            light_brightness: 2.0,
            light_size: 256.0,
            light_ropelength: 64.0,
            light_toggle: true,
            light_ropematerial: "cable/rope".into(),
            light_key: 37,
            material_override: "debug/env_cubemap_model".into(),
            colour_r: 255.0,
            colour_g: 255.0,
            colour_b: 255.0,
            colour_a: 255.0,
            colour_mode: 0.0,
            colour_fx: 0.0,
            physprop_gravity_toggle: true,
            physprop_material: "metal_bouncy".into(),
            trails_r: 255.0,
            trails_g: 255.0,
            trails_b: 255.0,
            trails_a: 255.0,
            trails_length: 5.0,
            trails_startsize: 32.0,
            trails_endsize: 0.0,
            trails_material: "trails/lol".into(),
            emitter_key: 51,
            emitter_delay: 1.0,
            emitter_scale: 1.0,
            emitter_toggle: true,
            emitter_starton: false,
            emitter_effect: "sparks".into(),
            camera_key: 37,
            eyeposer_x: 0.0,
            eyeposer_y: 0.0,
            eyeposer_strabismus: 0.0,
            faceposer_flex: vec![0.0; 96],
            faceposer_scale: 0.0,
            finger: vec![[0.0; 2]; 15],
            paint_decal: String::new(),
            creator_type: 0,
            creator_name: String::new(),
            camera_toggle: true,
            camera_locked: false,
            dynamite_remove: false,
        }
    }
}

/// A prop as the tool trace sees it: pose plus collision hulls as outward planes (entity frame).
#[derive(Clone, Debug, PartialEq)]
pub struct PropBody {
    pub id: u32,
    /// Physics object index on the entity (`GetPhysicsObjectNum`): 0 for a prop, the bone body
    /// for a ragdoll part (one `PropBody` per part).
    pub bone: u32,
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
    /// `PhysicsBone` of the hit: the ragdoll part's physics object index, 0 on a prop.
    pub bone: u32,
    pub fraction: f32,
    /// Where the trace started (the player's eye) and the direction it looked.
    pub start: Vec3,
    pub dir: Vec3,
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

/// The props' hulls swept by an axis-aligned box (`mins` / `maxs` around the trace line): each
/// hull's planes are pushed out by the box's extent along them (the server's hull checks use the
/// same approximation); the nearest hit wins, a start inside is `startsolid` at fraction 0.
pub fn sweep_hull_props(
    props: &[PropBody],
    start: Vec3,
    end: Vec3,
    mins: Vec3,
    maxs: Vec3,
) -> Option<ToolHit> {
    let center = (mins + maxs) * 0.5;
    let ext = (maxs - mins) * 0.5;
    let (cs, ce) = (start + center, end + center);
    let dir = (end - start).normalize_or_zero();
    let mut best: Option<ToolHit> = None;
    for p in props {
        let rot = rotation(p.angles);
        let inv = rot.inverse();
        let (ls, le) = (inv * (cs - p.pos), inv * (ce - p.pos));
        let (lx, ly, lz) = (inv * Vec3::X, inv * Vec3::Y, inv * Vec3::Z);
        for hull in &p.hulls {
            let grown: Vec<(Vec3, f32)> = hull
                .iter()
                .map(|&(n, d)| {
                    let r =
                        n.dot(lx).abs() * ext.x + n.dot(ly).abs() * ext.y + n.dot(lz).abs() * ext.z;
                    (n, d + r)
                })
                .collect();
            if grown.iter().all(|&(n, d)| n.dot(ls) - d <= 0.0) {
                return Some(ToolHit {
                    pos: start,
                    normal: Vec3::ZERO,
                    ent: Some(p.id),
                    bone: p.bone,
                    fraction: 0.0,
                    start,
                    dir,
                });
            }
            if let Some((t, n)) = ray_convex(ls, le, &grown) {
                if best.is_none_or(|b| t < b.fraction) {
                    best = Some(ToolHit {
                        pos: start + (end - start) * t,
                        normal: rot * n,
                        ent: Some(p.id),
                        bone: p.bone,
                        fraction: t,
                        start,
                        dir,
                    });
                }
            }
        }
    }
    best
}

/// A hull trace (`util.TraceHull`): the world swept by the box (`MASK_SOLID`) and the props by
/// [`sweep_hull_props`]; the nearest hit wins. `None` entity is the world.
pub fn trace_hull(
    world: &World,
    props: &[PropBody],
    start: Vec3,
    end: Vec3,
    mins: Vec3,
    maxs: Vec3,
) -> Option<ToolHit> {
    let dir = (end - start).normalize_or_zero();
    let tr = world.trace_box(start, end, mins, maxs, MASK_SOLID);
    let world_hit = tr.hit().then_some(ToolHit {
        pos: tr.end,
        normal: tr.normal,
        ent: None,
        bone: 0,
        fraction: tr.fraction,
        start,
        dir,
    });
    match (world_hit, sweep_hull_props(props, start, end, mins, maxs)) {
        (Some(w), Some(p)) => Some(if p.fraction < w.fraction { p } else { w }),
        (w, p) => w.or(p),
    }
}

/// A line trace against the world (brushes and displacements, `MASK_SOLID`) and the props'
/// collision hulls; the nearest hit wins. `None` entity is the world.
pub fn trace_line(world: &World, props: &[PropBody], start: Vec3, end: Vec3) -> Option<ToolHit> {
    let dir = (end - start).normalize_or_zero();
    let tr = world.trace_ray(start, end, MASK_SOLID);
    let mut best = tr.hit().then_some(ToolHit {
        pos: tr.end,
        normal: tr.normal,
        ent: None,
        bone: 0,
        fraction: tr.fraction,
        start,
        dir,
    });
    for p in props {
        let rot = rotation(p.angles);
        let inv = rot.inverse();
        let (ls, le) = (inv * (start - p.pos), inv * (end - p.pos));
        // a start inside a prop (a camera at the eye): the engine's `startsolid` hit at fraction
        // 0 on that entity, with a zero normal
        if p.hulls
            .iter()
            .any(|h| h.iter().all(|&(n, d)| n.dot(ls) - d <= 0.0))
        {
            return Some(ToolHit {
                pos: start,
                normal: Vec3::ZERO,
                ent: Some(p.id),
                bone: p.bone,
                fraction: 0.0,
                start,
                dir,
            });
        }
        for hull in &p.hulls {
            if let Some((t, n)) = ray_convex(ls, le, hull) {
                if best.is_none_or(|b| t < b.fraction) {
                    best = Some(ToolHit {
                        pos: start + (end - start) * t,
                        normal: rot * n,
                        ent: Some(p.id),
                        bone: p.bone,
                        fraction: t,
                        start,
                        dir,
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
    /// Server-side entities that are not props (NPCs) but live in undo / cleanup lists.
    foreign: std::collections::BTreeSet<u32>,
    undo: BTreeMap<u32, Vec<Undo>>,
    cleanup: BTreeMap<u32, BTreeMap<String, Vec<u32>>>,
    counts: BTreeMap<u32, BTreeMap<String, Vec<u32>>>,
    /// Prop masses (kg) the owner of the physics tells us; `CalcElasticConsts` needs them.
    masses: BTreeMap<u32, f32>,
    /// Numpad bindings per (player, key): which controllers a press and a release drive.
    numpad: BTreeMap<(u32, i32), Vec<NumpadBind>>,
    /// Keys currently held per player (`numpad.Activate` ignores a repeat while held).
    numpad_held: BTreeMap<u32, Vec<i32>>,
    /// Powered stool entities by prop id.
    devices: BTreeMap<u32, Device>,
    /// Shapes of the device models the stools may spawn (the physics owner registers them).
    shapes: BTreeMap<String, ModelShape>,
    /// Blasts that go off this tick (the physics owner deals them and drains the list).
    blasts: Vec<(Vec3, f32, u32)>,
    /// The remover's `timer.Simple(1)` deletions: seconds left per prop.
    removing: BTreeMap<u32, f32>,
    /// `Player:SetViewEntity`: the camera a player looks through.
    view: BTreeMap<u32, u32>,
    /// `ents.Remove` from a stool: gone at the end of the tick, untouched until then.
    deleting: Vec<u32>,
    /// `PhysObj:ApplyForceOffset` from a stool (the leafblower): `(prop, point, impulse)`,
    /// applied by the physics owner.
    impulses: Vec<(u32, Vec3, Vec3)>,
}

/// The thruster tool's sound list (`ThrusterSounds`); `StartThrustSound` ignores any other
/// name, so an unlisted setting means a silent thruster.
pub const THRUSTER_SOUNDS: [&str; 10] = [
    "PhysicsCannister.ThrusterLoop",
    "WeaponDissolve.Charge",
    "WeaponDissolve.Beam",
    "eli_lab.elevator_move",
    "combine.sheild_loop",
    "k_lab.ringsrotating",
    "k_lab.teleport_rings_high",
    "k_lab2.DropshipRotorLoop",
    "Town.d1_town_01_spin_loop",
    "",
];

/// A powered stool entity riding on an ordinary prop: `gmod_thruster`, `gmod_hoverball`,
/// `gmod_balloon` (the game's SENTs with a motion controller).
#[derive(Clone, Debug, PartialEq)]
pub enum Device {
    /// `gmod_thruster`: while `on`, the acceleration `force * mul * 50 * top_z / mass` along the
    /// body's -z at its top (`OBBMaxs.z`), i.e. the force `force * mul * 50 * top_z`.
    Thruster {
        force: f32,
        /// `Multiply`: +1 forward, -1 back, their sum while both keys are held; a toggle flips.
        mul: f32,
        /// Loop soundscript while on (empty: silent).
        sound: String,
        /// `ThrusterEffects` key drawn by the client while on (`none`: nothing).
        effect: String,
        /// `thruster_damageable`: damage switches it on for 5 s (`ActivateOnDamage`).
        damageable: bool,
        /// `SwitchOffTime`: the time it switches off again after damage (0: none).
        off_at: f32,
        toggle: bool,
        on: bool,
        top_z: f32,
        bottom_z: f32,
        key: i32,
        key_back: i32,
        /// `GetOffset()` sits at the model's bottom (`ThrustOffsetR`) once a key event left the
        /// thruster off; at the top (`ThrustOffset`) from spawn and while thrusting
        rear: bool,
    },
    /// `gmod_hoverball`: holds `target_z` with a damped quadratic acceleration, mass
    /// `150 * strength` while enabled (15 when off); gravity stays on at creation (the game
    /// enables it before `SetEnabled(true)`), the toggle then turns it off while enabled.
    Hoverball {
        target_z: f32,
        /// `ZVelocity`: `+-75` while a key is held (`FrameTime() * 5000`).
        zvel: f32,
        speed: f32,
        resistance: f32,
        strength: f32,
        enabled: bool,
        key_up: i32,
        key_down: i32,
        key_toggle: i32,
    },
    /// `gmod_balloon`: constant upward force `force * 5000 * dt` on a 100 kg body without gravity.
    Balloon { force: f32 },
    /// `gmod_wheel` with its `constraint.Motor` (`phys_torque` on the wheel's `Axis` constraint
    /// `axis`): `scale` is the motor's `forcescale` (`direction * mul * TorqueScale`, 0 at rest),
    /// the spin accelerates by `TORQUE_ACCEL * torque * scale` deg/s^2 about the axis towards
    /// the hit entity (measured), capped at `MAX_WHEEL_SPIN`.
    Wheel {
        torque: f32,
        friction: f32,
        scale: f32,
        toggle: bool,
        toggle_state: bool,
        direction: f32,
        key_f: i32,
        key_r: i32,
        axis: u32,
        /// The axis in the wheel's frame (`WPos2 - WPos1`, the hit side).
        local_axis: Vec3,
        /// The scale the motor drove with last tick: `phys_torque` picks a new `Scale` up on
        /// the step after the key (the game's wheel shows 0 deg/s on the key tick).
        applied: f32,
    },
    /// `constraint.Motor`'s `phys_torque` (its own entity, keyed by its id): `torque * scale`
    /// deg/s^2 about `axis` (the Axis constraint it rides on) on prop `ent`, the wheel's law
    /// (`wheel_spin_step`) - measured on a hinged crate at torque 500 / 3000: the drive is in
    /// angular-velocity units whatever the body, the axis friction and release are the wheel's.
    Motor {
        ent: u32,
        axis: u32,
        torque: f32,
        friction: f32,
        forcetime: f32,
        scale: f32,
        toggle: bool,
        is_on: bool,
        direction: f32,
        key_f: i32,
        key_r: i32,
        /// Hinge direction in `ent`'s frame.
        local_axis: Vec3,
        applied: f32,
    },
    /// `gmod_button` (`ONOFF_USE`): `numpad.Activate(owner, key, true)` while on. A toggle flips
    /// on each press; otherwise it is on while `user` holds +use.
    Button {
        key: i32,
        toggle: bool,
        on: bool,
        user: Option<u32>,
        /// `button_description`: the label shown as its world tip (`GetOverlayText`).
        description: String,
    },
    /// `gmod_dynamite`: `util.BlastDamage(radius 300, clamp(damage, 0, 1500))` on the key, after
    /// `delay` seconds when set (queued, fired by a 0.05 s think), removed when `remove`.
    Dynamite {
        damage: f32,
        delay: f32,
        remove: bool,
        key: i32,
        /// Explosions waiting (server ticks left), the queue of `Explode` with a delay.
        queued: Vec<u32>,
    },
    /// gmod_lamp: a projected-texture spotlight on a prop (torch and the like); `on` is the
    /// `On` network var, the numpad `LampToggle` flips or holds it
    Lamp {
        color: [u8; 3],
        texture: String,
        fov: f32,
        distance: f32,
        brightness: f32,
        toggle: bool,
        on: bool,
        key: i32,
    },
    /// gmod_light: the tubular point light, usually roped to what it was placed on
    Light {
        color: [u8; 3],
        brightness: f32,
        size: f32,
        toggle: bool,
        on: bool,
        key: i32,
    },
    /// env_spritetrail of the trails stool, parented to `ent` (its own entity: undo, cleanup)
    Trail {
        ent: u32,
        color: [u8; 4],
        length: f32,
        start_size: f32,
        end_size: f32,
        material: String,
    },
    /// gmod_emitter: a particle effect every `delay` seconds while `on`
    Emitter {
        key: i32,
        delay: f32,
        toggle: bool,
        on: bool,
        effect: String,
        scale: f32,
    },
    /// gmod_cameraprop: a view entity for its owner; `tracking` is `(entity, local point)`
    /// with `None` for the player, `viewer` the player who last turned it on
    Camera {
        key: i32,
        toggle: bool,
        locked: bool,
        tracking: Option<(Option<u32>, Vec3)>,
        viewer: Option<u32>,
    },
}

/// gmod_cameraprop's model.
pub const CAMERA_MODEL: &str = "models/dav0r/camera.mdl";

/// trails.lua's `trail_materials` (outside single player only these get through).
pub const TRAIL_MATERIALS: &[&str] = &[
    "trails/plasma",
    "trails/tube",
    "trails/electric",
    "trails/smoke",
    "trails/laser",
    "trails/physbeam",
    "trails/love",
    "trails/lol",
    "effects/beam001_blu",
    "effects/beam001_red",
    "effects/beam001_white",
    "effects/arrowtrail_blu",
    "effects/arrowtrail_red",
    "effects/repair_claw_trail_blue",
    "effects/repair_claw_trail_red",
    "effects/australiumtrail_red",
    "effects/beam_generic01",
];

/// gmod_emitter's model.
pub const EMITTER_MODEL: &str = "models/props_lab/tpplug.mdl";

/// material.lua's `OverrideMaterials` list: what the material stool may apply outside single
/// player.
pub const OVERRIDE_MATERIALS: &[&str] = &[
    "models/wireframe",
    "debug/env_cubemap_model",
    "models/shadertest/shader3",
    "models/shadertest/shader4",
    "models/shadertest/shader5",
    "models/shiny",
    "models/debug/debugwhite",
    "Models/effects/comball_sphere",
    "Models/effects/comball_tape",
    "Models/effects/splodearc_sheet",
    "Models/effects/vol_light001",
    "models/props_combine/stasisshield_sheet",
    "models/props_combine/portalball001_sheet",
    "models/props_combine/com_shield001a",
    "models/props_c17/frostedglass_01a",
    "models/props_lab/Tank_Glass001",
    "models/props_combine/tprings_globe",
    "models/rendertarget",
    "models/screenspace",
    "brick/brick_model",
    "models/props_pipes/GutterMetal01a",
    "models/props_pipes/Pipesystem01a_skin3",
    "models/props_wasteland/wood_fence01a",
    "models/props_foliage/tree_deciduous_01a_trunk",
    "models/props_c17/FurnitureFabric003a",
    "models/props_c17/FurnitureMetal001a",
    "models/props_c17/paper01",
    "models/flesh",
    "phoenix_storms/metalset_1-2",
    "phoenix_storms/metalfloor_2-3",
    "phoenix_storms/plastic",
    "phoenix_storms/wood",
    "phoenix_storms/bluemetal",
    "phoenix_storms/cube",
    "phoenix_storms/dome",
    "phoenix_storms/gear",
    "phoenix_storms/stripes",
    "phoenix_storms/wire/pcb_green",
    "phoenix_storms/wire/pcb_red",
    "phoenix_storms/wire/pcb_blue",
    "hunter/myplastic",
    "models/XQM/LightLinesRed_tool",
    "models/player/shared/gold_player",
    "models/player/shared/ice_player",
];

/// gmod_light's one model.
pub const LIGHT_MODEL: &str = "models/maxofs2d/light_tubular.mdl";

/// The server tick the device timers count in (seconds).
pub const SERVER_TICK: f32 = 0.015;

/// `phys_torque`: deg/s^2 of wheel spin per unit of `force * scale` (measured on the 70 kg
/// `carparts_wheel01a`: 2917 deg/s^2 for 3000 without axis friction).
pub const TORQUE_ACCEL: f32 = 1.0;
/// Axis friction: a motor torque up to this (per unit of the wheel's `friction`) does not move
/// a resting wheel (torque 300 against friction 1 measured still); while driving the loss is
/// this less 0.027 of the torque (272 / 217 / 30 deg/s² measured at torque 1000 / 3000 / 10000).
pub const WHEEL_FRICTION_TORQUE: f32 = 300.0;
/// The same cap for a body hinged in the air (`constraint.Motor` on a crate): 280 - the crate
/// starts at 500 - 280 = 220 deg/s^2 and runs down at 270-276 under 120 deg/s; the wheel's extra
/// 20 is its floor contact (a wheel at torque 300 never moves, `constraint_device_wheel_toggle`).
pub const MOTOR_FRICTION_TORQUE: f32 = 280.0;
/// The spin vphysics caps a driven wheel at (deg/s).
pub const MAX_WHEEL_SPIN: f32 = 7200.0;
/// `util.BlastDamage` radius of the dynamite stool and the impulse rule of a blast on a body:
/// `min(damage * 300, 30000)` kg u/s towards the body's centre.
pub const BLAST_RADIUS: f32 = 300.0;
pub fn blast_impulse(damage: f32) -> f32 {
    (damage * 300.0).min(30000.0)
}
/// A body of this mass in the way of a blast absorbs all of it; lighter ones `mass / 350`.
pub const BLAST_ABSORB_MASS: f32 = 350.0;
/// The engine's per-victim fudge on the blast push, `RandomFloat(0.85, 1.15)` of `u` in 0..1.
pub fn blast_jitter(u: f32) -> f32 {
    0.85 + 0.3 * u.clamp(0.0, 1.0)
}

/// One tick of a wheel's spin about its motor axis (deg/s, signed along the axis towards the
/// hit entity): `phys_torque` accelerates by `torque * scale` deg/s² less the axis friction
/// torque (which also holds a resting wheel the motor cannot overcome), the spin is capped at
/// 7200 deg/s; the free wheel's decay is the body's quadratic angular drag plus the axis
/// friction law above (no extra linear damping: the frictionless wheel decays as 1.0e-4 spin^2).
/// Axis friction of a turning wheel per unit of `friction`, deg/s^2: `A + B / |spin|`,
/// capped at `WHEEL_FRICTION_TORQUE`. Measured on the 261031 wheel recordings with the air
/// drag (1.0e-4 spin^2, the body's quadratic angular drag) taken out: ~305 at 400 deg/s,
/// ~215 at 900, ~150-200 at 3000, the same while driven and released.
pub const WHEEL_AXIS_FRICTION_A: f32 = 190.0;
pub const WHEEL_AXIS_FRICTION_B: f32 = 35000.0;
/// `on_floor`: the wheel recordings (a wheel on the floor beside its crate) stop dead under
/// 60 deg/s once the motor is off; a hinged crate in the air (`constraint_device_motor_crate`)
/// runs down through 92 -> 52 -> 11 deg/s at the axis friction instead.
pub fn wheel_spin_step(
    spin: f32,
    torque: f32,
    scale: f32,
    friction: f32,
    dt: f32,
    on_floor: bool,
) -> f32 {
    let motor = TORQUE_ACCEL * torque * scale.abs();
    let cap = if on_floor {
        WHEEL_FRICTION_TORQUE
    } else {
        MOTOR_FRICTION_TORQUE
    };
    if on_floor && spin.abs() < 60.0 && motor <= cap * friction {
        return 0.0;
    }
    // at rest the axis friction holds what the motor cannot overcome
    if spin.abs() < 1.0 && motor <= cap * friction {
        return 0.0;
    }
    let loss =
        (WHEEL_AXIS_FRICTION_A + WHEEL_AXIS_FRICTION_B / spin.abs().max(60.0)).min(cap) * friction;
    let drive = if scale == 0.0 {
        // released: the axis friction brakes the spin on top of the body's air drag
        -(loss * dt).min(spin.abs()) * spin.signum() / dt
    } else if spin.abs() >= 60.0 && spin.signum() != scale.signum() {
        // braking against the spin the axis friction opposes the spin, not the motor
        (motor + loss) * scale.signum()
    } else {
        (motor - loss).max(0.0) * scale.signum()
    };
    let s = spin + drive * dt;
    s.clamp(-MAX_WHEEL_SPIN, MAX_WHEEL_SPIN)
}

impl Device {
    /// The body mass the device sets (`PhysObj:SetMass`), `None` to keep the model's.
    pub fn mass(&self) -> Option<f32> {
        match self {
            Device::Hoverball {
                strength, enabled, ..
            } => Some(if *enabled { 150.0 * strength } else { 15.0 }),
            Device::Balloon { .. } => Some(100.0),
            Device::Thruster { .. }
            | Device::Wheel { .. }
            | Device::Motor { .. }
            | Device::Button { .. }
            | Device::Dynamite { .. }
            | Device::Lamp { .. }
            | Device::Light { .. }
            | Device::Trail { .. }
            | Device::Emitter { .. }
            | Device::Camera { .. } => None,
        }
    }

    /// `PhysObj:EnableGravity` as the device leaves it.
    pub fn gravity(&self) -> bool {
        !matches!(self, Device::Balloon { .. })
    }
}

/// The hoverball's `PhysicsSimulate`: global acceleration (u/s^2) for a body at `pos` with
/// `vel` (as before the step), from its target height; `None` when it holds nothing. The game's
/// controller runs after the step's gravity, so it sees `vel.z - gravity * dt` (measured: a free
/// hoverball's acceleration is `4.5 D^2 - 54 vz - 114.0` with gravity on, 114 = 600 - 54 * 9).
pub fn hoverball_acceleration(
    target_z: f32,
    resistance: f32,
    pos: Vec3,
    vel: Vec3,
    gravity: f32,
    dt: f32,
) -> Option<Vec3> {
    let distance = target_z - pos.z;
    if distance == 0.0 {
        return None;
    }
    let mut exponent = distance * distance * if distance < 0.0 { -1.0 } else { 1.0 };
    exponent *= dt * 300.0;
    exponent -= (vel.z - gravity * dt) * dt * 600.0 * (resistance + 1.0);
    let exponent = exponent.clamp(-5000.0, 5000.0);
    let mut a = Vec3::new(0.0, 0.0, exponent);
    if resistance > 0.0 {
        a.x = -vel.x * resistance;
        a.y = -vel.y * resistance;
    }
    Some(a)
}

/// A device model's shape for placement: its OBB and collision hull points (model frame).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ModelShape {
    pub min: Vec3,
    pub max: Vec3,
    pub points: Vec<Vec3>,
}

/// What a numpad key does to one constraint's controller (`numpad.OnDown` / `OnUp`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NumpadBind {
    HydraulicToggle(u32),
    /// Down sets +1, up sets -1 (a non-toggle hydraulic).
    HydraulicDir(u32),
    /// `Thruster_On` / `Thruster_Off`: down adds `mul`, up takes it back (a toggle flips).
    Thruster(u32, f32),
    /// `Hoverball_Up` / `Hoverball_Down`: `ZVelocity` +-75 while held.
    HoverballMove(u32, f32),
    HoverballToggle(u32),
    /// `WheelForward` / `WheelReverse` (`mul` +1 / -1).
    Wheel(u32, f32),
    /// `MotorControl` (`dir` +1 / -1).
    Motor(u32, f32),
    /// `DynamiteBlow` on the press.
    Dynamite(u32),
    /// `LampToggle` / `LightToggle`: hold or flip the light
    LightSwitch(u32),
    /// `Emitter_On` / `Emitter_Off`
    Emitter(u32),
    /// `Camera_Toggle` or `Camera_On` / `Camera_Off`
    Camera(u32),
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
            | NumpadBind::Thruster(c, _)
            | NumpadBind::HoverballMove(c, _)
            | NumpadBind::HoverballToggle(c)
            | NumpadBind::Wheel(c, _)
            | NumpadBind::Motor(c, _)
            | NumpadBind::Dynamite(c)
            | NumpadBind::LightSwitch(c)
            | NumpadBind::Emitter(c)
            | NumpadBind::Camera(c)
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

/// `pos - NearestPoint(pos - normal * 512)` along `normal`: how far the model's OBB (the MDL
/// hull box, not the collision hull) reaches against `normal` when rotated by `rot`.
pub fn obb_depth(shape: &ModelShape, rot: Quat, normal: Vec3) -> f32 {
    let (lo, hi) = (shape.min, shape.max);
    let mut depth = 0.0f32;
    for i in 0..8 {
        let c = Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        depth = depth.max((rot * c).dot(-normal));
    }
    depth
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

/// `DoPlayerEntitySpawn` for a `prop_ragdoll` (measured, `phys_ragdoll_kleiner_spawn`): the
/// model is posed at the hit point with angles (-90, yaw + 180, 0); `InitRagdoll` then moves the
/// entity origin to the root bone with zero angles and collision bounds of the hull box widened
/// by the root solid's convex extents, so the flush offset (hit minus the box point nearest to 512 u behind the surface)
/// is measured on that unrotated box about the root; it moves every physics object (kleiner on
/// the floor: 25.6 u back toward the player, 5.9 u up). No per-axis nudges.
pub fn place_ragdoll(
    world: &World,
    props: &[PropBody],
    eye: Vec3,
    eye_angles: Vec3,
    rag: &RagdollSpawn,
) -> Placement {
    let dir = forward(eye_angles.x, eye_angles.y);
    let end = eye + dir * 2048.0;
    let (hit, normal, hit_ent) = match trace_line(world, props, eye, end) {
        Some(h) => (h.pos, h.normal, h.ent),
        None => (end, Vec3::ZERO, None),
    };
    let angles = Vec3::new(-90.0, normalize_angle(eye_angles.y + 180.0), 0.0);
    let root = hit + rotation(angles) * rag.root_local;
    let far = hit - normal * 512.0;
    let nearest = nearest_point(root, Quat::IDENTITY, &rag.bounds, far);
    Placement {
        hit,
        normal,
        hit_ent,
        pos: hit + (hit - nearest),
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
            material: String::new(),
            color: [255; 4],
            render_mode: 0,
            render_fx: 0,
            phys_material: String::new(),
        });
        id
    }

    /// A script-made constraint of any kind (`constraint.Rope` and friends): the record the
    /// physics sync builds the joint from. Nothing for a body against itself or world-to-world.
    pub fn constrain(
        &mut self,
        owner: u32,
        ent1: Option<u32>,
        ent2: Option<u32>,
        mut kind: ConstraintKind,
    ) -> Option<u32> {
        if ent1 == ent2 {
            return None;
        }
        let id = self.next_id;
        let (m1, m2) = (self.mass_of(ent1), self.mass_of(ent2));
        let mut binds: Vec<(i32, NumpadBind)> = Vec::new();
        match &mut kind {
            ConstraintKind::Hydraulic {
                constant,
                damping,
                fixed,
                key,
                toggle,
                ..
            } => {
                (*constant, *damping) = elastic_consts(m1, m2, *fixed);
                binds.push((
                    *key,
                    if *toggle {
                        NumpadBind::HydraulicToggle(id)
                    } else {
                        NumpadBind::HydraulicDir(id)
                    },
                ));
            }
            ConstraintKind::Winch {
                constant,
                damping,
                fwd_key,
                bwd_key,
                toggle,
                ..
            } => {
                (*constant, *damping) = elastic_consts(m1, m2, false);
                if *toggle {
                    binds.push((*fwd_key, NumpadBind::WinchToggle(id, 1)));
                    binds.push((*bwd_key, NumpadBind::WinchToggle(id, -1)));
                } else {
                    binds.push((*fwd_key, NumpadBind::WinchDir(id, 1)));
                    binds.push((*bwd_key, NumpadBind::WinchDir(id, -1)));
                }
            }
            ConstraintKind::Muscle {
                constant,
                damping,
                fixed,
                key,
                ..
            } => {
                (*constant, *damping) = elastic_consts(m1, m2, *fixed);
                binds.push((*key, NumpadBind::MuscleToggle(id)));
            }
            _ => {}
        }
        let id = self.restore_constraint(owner, ent1, ent2, kind, [255, 255, 255]);
        for (key, bind) in binds {
            self.numpad.entry((owner, key)).or_default().push(bind);
        }
        Some(id)
    }

    /// `constraint.Motor` from a script: the stool's pair of an `Axis` constraint (from `lpos1`
    /// of `ent1` towards `axis_point`, both in its frame) and a motor device on it, with the
    /// two numpad keys bound under the owner. `(motor id, axis id)`.
    #[allow(clippy::too_many_arguments)]
    pub fn motor(
        &mut self,
        owner: u32,
        ent1: u32,
        ent2: Option<u32>,
        lpos1: Vec3,
        lpos2: Vec3,
        axis_point: Vec3,
        friction: f32,
        torque: f32,
        forcetime: f32,
        nocollide: bool,
        toggle: bool,
        forcelimit: f32,
        key_f: i32,
        key_r: i32,
        direction: f32,
    ) -> Option<(u32, u32)> {
        if ent2 == Some(ent1) {
            return None;
        }
        let axis_id = self.next_id;
        self.next_id += 1;
        self.constraints.push(Constraint {
            id: axis_id,
            owner,
            ent1: Some(ent1),
            ent2,
            kind: ConstraintKind::Axis {
                lpos1,
                lpos2,
                local_axis: axis_point,
                forcelimit,
                torquelimit: 0.0,
                friction,
                nocollide,
            },
            color: [255, 255, 255],
        });
        let motor_id = self.next_id;
        self.next_id += 1;
        self.devices.insert(
            motor_id,
            Device::Motor {
                ent: ent1,
                axis: axis_id,
                torque,
                friction,
                forcetime,
                scale: 0.0,
                toggle,
                is_on: false,
                direction: if direction < 0.0 { -1.0 } else { 1.0 },
                key_f,
                key_r,
                local_axis: (axis_point - lpos1).normalize_or_zero(),
                applied: 0.0,
            },
        );
        for (key, bind) in [
            (key_f, NumpadBind::Motor(motor_id, 1.0)),
            (key_r, NumpadBind::Motor(motor_id, -1.0)),
        ] {
            self.numpad.entry((owner, key)).or_default().push(bind);
        }
        Some((motor_id, axis_id))
    }

    /// A constraint record straight from a save (no limits, undo entry or duplicate check).
    pub fn restore_constraint(
        &mut self,
        owner: u32,
        ent1: Option<u32>,
        ent2: Option<u32>,
        kind: ConstraintKind,
        color: [u8; 3],
    ) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.constraints.push(Constraint {
            id,
            owner,
            ent1,
            ent2,
            kind,
            color,
        });
        id
    }

    pub fn props(&self) -> &[Prop] {
        &self.props
    }

    /// `Player:GetCount("props")`: live props owned by the player, minus the bodies counted
    /// under another kind (a scripted entity is a sent, not a prop).
    pub fn count(&self, player: u32) -> usize {
        let other: Vec<u32> = self
            .counts
            .get(&player)
            .map(|m| {
                m.iter()
                    .filter(|(k, _)| k.as_str() != "props")
                    .flat_map(|(_, v)| v.iter().copied())
                    .collect()
            })
            .unwrap_or_default();
        self.props
            .iter()
            .filter(|p| p.owner == player && !other.contains(&p.id))
            .count()
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
        self.foreign.contains(&id)
            || self.props.iter().any(|p| p.id == id)
            || self.constraints.iter().any(|c| c.id == id)
            || self.devices.contains_key(&id)
    }

    pub fn constraints(&self) -> &[Constraint] {
        &self.constraints
    }

    pub fn tool(&self, player: u32) -> ToolGun {
        self.tools.get(&player).cloned().unwrap_or_default()
    }

    /// undo.lua's `CleanupInvalidUndos` (run by its EntityRemoved hook every 16 ticks that
    /// removed something): a player whose every undo entry has no live entity left loses the
    /// whole list; lists with a live entry keep even their dead entries.
    pub fn purge_dead_undos(&mut self) {
        let live: std::collections::HashSet<u32> = self
            .props
            .iter()
            .map(|p| p.id)
            .chain(self.foreign.iter().copied())
            .collect();
        self.undo.retain(|_, list| {
            list.iter()
                .any(|u| u.entities.iter().any(|e| live.contains(e)))
        });
    }

    /// Live entities of an undo entry (dead ones stay listed until the whole list is purged).
    pub fn undo_live(&self, u: &Undo) -> usize {
        u.entities
            .iter()
            .filter(|e| {
                self.props.iter().any(|p| p.id == **e)
                    || self.foreign.contains(e)
                    || self.constraints.iter().any(|c| c.id == **e)
            })
            .count()
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

    /// The player whose cleanup lists hold `id`, if any.
    pub fn cleanup_owner(&self, id: u32) -> Option<u32> {
        self.cleanup
            .iter()
            .find(|(_, kinds)| kinds.values().any(|ids| ids.contains(&id)))
            .map(|(p, _)| *p)
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
            material: String::new(),
            color: [255; 4],
            render_mode: 0,
            render_fx: 0,
            phys_material: String::new(),
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

    /// `GMODSpawnRagdoll` (`commands.lua`): the same placement as a prop except the spawn
    /// angles are pitched -90 (`DoPlayerEntitySpawn` for `prop_ragdoll`), the flush offset moves
    /// every physics object, limit `ragdolls` (`sbox_maxragdolls`), undo `prop_ragdoll` /
    /// `#prop_ragdoll (<model>)`, cleanup `ragdolls`.
    pub fn spawn_ragdoll(
        &mut self,
        world: &World,
        props: &[PropBody],
        player: u32,
        req: &SpawnRequest<'_>,
        rag: &RagdollSpawn,
        events: &mut Vec<Event>,
    ) -> Option<u32> {
        if !self.check_limit(player, "ragdolls", events) {
            return None;
        }
        let model = req.model;
        let placement = place_ragdoll(world, props, req.eye, req.eye_angles, rag);
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
            material: String::new(),
            color: [255; 4],
            render_mode: 0,
            render_fx: 0,
            phys_material: String::new(),
        });
        events.push(Event::Spawned { prop: id });
        self.push_undo(
            player,
            "prop_ragdoll",
            format!("#prop_ragdoll ({model})"),
            None,
            vec![id],
        );
        self.add_cleanup(player, "ragdolls", id);
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
                material: String::new(),
                color: [255; 4],
                render_mode: 0,
                render_fx: 0,
                phys_material: String::new(),
            });
            self.next_id = self.next_id.max(b.id + 1);
        }
    }

    /// Removes a prop or constraint by id (`Entity:Remove`): constraints attached to a removed
    /// prop go with it, undo entries keep their ids (they skip dead entities when run).
    pub fn remove_entity(&mut self, id: u32) {
        let was_prop = self.props.iter().any(|p| p.id == id);
        self.foreign.remove(&id);
        self.props.retain(|p| p.id != id);
        if was_prop {
            // the device state of a prop (camera, lamp, ...) dies with it
            self.devices.remove(&id);
        }
        self.constraints
            .retain(|c| c.id != id && !(was_prop && (c.ent1 == Some(id) || c.ent2 == Some(id))));
        // a motor and its Axis delete each other, both go with their prop
        let axes: Vec<u32> = self
            .devices
            .iter()
            .filter_map(|(k, d)| match d {
                Device::Motor { ent, axis, .. } if *k == id || *ent == id || *axis == id => {
                    Some(*axis)
                }
                _ => None,
            })
            .collect();
        if !axes.is_empty() {
            self.devices.retain(
                |k, d| !matches!(d, Device::Motor { ent, axis, .. } if *k == id || *ent == id || *axis == id),
            );
            self.constraints.retain(|c| !axes.contains(&c.id));
        }
        // a prop's trail goes with it
        self.remove_trail(id);
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
        if tool == "weld" {
            // the weld's right click (weld to world) sets `Op` 1, freeing the axis again
            self.tools.entry(player).or_default().operation = 1;
        }
        if !self.can_tool(hit) {
            return false;
        }
        if tool == "camera" {
            // camera.lua: `RightClick = LeftClick + SetTracking(trace.Entity, local hit)`
            return self.camera_click(player, hit, true, events);
        }
        if tool == "balloon" || tool == "light" {
            // light.lua: `RightClick = LeftClick(trace, false)`, no rope
            return self.place_device(player, tool, hit, false, events);
        }
        if tool == "emitter" {
            // emitter.lua: `RightClick = LeftClick(trace, worldweld = true)`
            return self.place_device(player, tool, hit, true, events);
        }
        if tool == "trails" {
            // SetTrails(ply, ent, nil): the trail goes, true on any entity
            let Some(ent) = hit.ent.filter(|e| self.props.iter().any(|p| p.id == *e)) else {
                return false;
            };
            self.remove_trail(ent);
            return true;
        }
        if matches!(tool, "material" | "colour") {
            // copies the entity's material / colour into the convars
            return hit
                .ent
                .is_some_and(|e| self.props.iter().any(|p| p.id == e));
        }
        if tool == "physprop" {
            return false;
        }
        if tool == "lamp" {
            // lamp.lua: copies a gmod_lamp's settings into the convars, anyone's lamp
            return hit
                .ent
                .is_some_and(|e| matches!(self.devices.get(&e), Some(Device::Lamp { .. })));
        }
        if tool == "remover" {
            let Some(ent) = hit.ent else {
                return false;
            };
            let n = self
                .constrained_entities(ent)
                .into_iter()
                .filter(|e| self.remover_remove(*e))
                .count();
            events.push(Event::Lua {
                code: format!("for i=1,{n} do achievements.Remover() end"),
            });
            return true;
        }
        if tool != "weld" {
            return false;
        }
        let n = self.tool(player).objects.len();
        if n == 0 && hit.ent.is_none() {
            return false;
        }
        self.tools.entry(player).or_default().operation = 2;
        let n = self.set_object(player, tool, hit);
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
            color: [255, 255, 255],
        });
        Some(id)
    }

    /// `ToolObj:SetObject`: the hit is stored in the entity frame (world space for the world).
    pub fn set_object(&mut self, player: u32, tool: &str, hit: &ToolHit) -> usize {
        {
            let t = self.tools.entry(player).or_default();
            if t.objects_tool != tool {
                t.objects.clear();
                t.objects_tool = tool.to_string();
            }
        }
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
        if tool == "remover" {
            return self.remove_all_constraints(ent) != 0;
        }
        if matches!(tool, "material" | "colour") {
            let Some(p) = self.props.iter_mut().find(|p| p.id == ent) else {
                return false;
            };
            if tool == "material" {
                p.material.clear();
            } else {
                p.color = [255; 4];
                p.render_mode = 0;
                p.render_fx = 0;
            }
            return true;
        }
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
        if tool == "motor" {
            // `RemoveConstraints(ent, "Motor")`: the phys_torque goes and takes its Axis along
            let motors: Vec<u32> = self
                .devices
                .iter()
                .filter(|(_, d)| matches!(d, Device::Motor { ent: e, .. } if *e == ent))
                .map(|(id, _)| *id)
                .collect();
            for id in &motors {
                self.remove_entity(*id);
            }
            return !motors.is_empty();
        }
        let before = self.constraints.len();
        self.constraints
            .retain(|c| !(is_type(&c.kind) && (c.ent1 == Some(ent) || c.ent2 == Some(ent))));
        self.drop_parts();
        self.constraints.len() != before
    }

    /// `constraint.RemoveAll(ent)`: every constraint in the entity's table goes (a motor's
    /// phys_torque sits there too and takes its Axis along); returns how many went.
    pub fn remove_all_constraints(&mut self, ent: u32) -> usize {
        let motors: Vec<u32> = self
            .devices
            .iter()
            .filter(|(_, d)| matches!(d, Device::Motor { ent: e, .. } if *e == ent))
            .map(|(id, _)| *id)
            .collect();
        for id in &motors {
            self.remove_entity(*id);
        }
        let before = self.constraints.len();
        self.constraints
            .retain(|c| !(c.ent1 == Some(ent) || c.ent2 == Some(ent)));
        let n = before - self.constraints.len() + motors.len() * 2;
        self.drop_parts();
        n
    }

    /// `constraint.GetAllConstrainedEntities(ent)`: the entity and everything reachable over
    /// constraints (the world is not an entity).
    pub fn constrained_entities(&self, ent: u32) -> Vec<u32> {
        let mut out = vec![ent];
        let mut i = 0;
        while i < out.len() {
            let e = out[i];
            i += 1;
            for c in &self.constraints {
                let other = if c.ent1 == Some(e) {
                    c.ent2
                } else if c.ent2 == Some(e) {
                    c.ent1
                } else {
                    None
                };
                if let Some(o) = other {
                    if !out.contains(&o) {
                        out.push(o);
                    }
                }
            }
        }
        out
    }

    /// The remover's `DoRemoveEntity`: the constraints go now, the entity turns non-solid and
    /// invisible, and `timer.Simple(1)` deletes it a second later (a second call on the same
    /// entity within that second is a no-op that still counts).
    fn remover_remove(&mut self, id: u32) -> bool {
        if !self.props.iter().any(|p| p.id == id) {
            return false;
        }
        self.remove_all_constraints(id);
        self.removing.entry(id).or_insert(1.0);
        true
    }

    /// `ENT:CanTool`: a locked or tracking camera (`MOVETYPE_NONE`) refuses every stool.
    fn can_tool(&self, hit: &ToolHit) -> bool {
        let Some(e) = hit.ent else {
            return true;
        };
        !matches!(
            self.devices.get(&e),
            Some(Device::Camera { locked: true, .. })
                | Some(Device::Camera {
                    tracking: Some(_),
                    ..
                })
        )
    }

    /// `Player:GetViewEntity()` when it is not the player.
    pub fn view_entity(&self, player: u32) -> Option<u32> {
        self.view.get(&player).copied()
    }

    fn set_viewer(&mut self, cam: u32, who: Option<u32>) {
        if let Some(Device::Camera { viewer, .. }) = self.devices.get_mut(&cam) {
            *viewer = who;
        }
    }

    /// gmod_cameraprop's numpad: `Camera_Toggle` flips the view on key down; `Camera_On` /
    /// `Camera_Off` hold it. The camera remembers its last user (`UsingPlayer`) until it is
    /// turned off through it.
    fn camera_key(&mut self, player: u32, cam: u32, down: bool) {
        let Some(Device::Camera { toggle, .. }) = self.devices.get(&cam) else {
            return;
        };
        let toggle = *toggle;
        let viewing = self.view.get(&player) == Some(&cam);
        let on = if toggle {
            if !down {
                return;
            }
            !viewing
        } else {
            down
        };
        if on {
            self.view.insert(player, cam);
            self.set_viewer(cam, Some(player));
        } else if viewing {
            self.view.remove(&player);
            self.set_viewer(cam, None);
        }
    }

    /// camera.lua `LeftClick` / `MakeCamera`: `false` for key -1; your cameras on the same key
    /// go (next tick, `ents.Remove`) and let you past the `cameras` limit; the new one sits at
    /// the eye with the eye angles, asleep; locked or tracking makes it `MOVETYPE_NONE`. A
    /// right click tracks the hit entity at the local hit point (the world and players: the
    /// player at its origin).
    fn camera_click(
        &mut self,
        player: u32,
        hit: &ToolHit,
        track: bool,
        events: &mut Vec<Event>,
    ) -> bool {
        let s = self.tool_settings.clone();
        let key = s.camera_key;
        if key == -1 {
            return false;
        }
        let same: Vec<u32> = self
            .devices
            .iter()
            .filter(|(k, d)| {
                matches!(d, Device::Camera { key: ck, .. } if *ck == key)
                    && self.props.iter().any(|p| p.id == **k && p.owner == player)
            })
            .map(|(k, _)| *k)
            .collect();
        if same.is_empty() && !self.check_limit(player, "cameras", events) {
            return false;
        }
        self.deleting.extend(same);
        // `ply:EyeAngles()`, not the trace (which may end at the eye inside an old camera)
        let dir = hit.dir;
        let angles = Vec3::new(
            (-dir.z).clamp(-1.0, 1.0).asin().to_degrees(),
            dir.y.atan2(dir.x).to_degrees(),
            0.0,
        );
        let tracking = track.then(|| {
            let ent = hit.ent.filter(|e| self.props.iter().any(|p| p.id == *e));
            let lpos = match ent.and_then(|e| self.props.iter().find(|p| p.id == e)) {
                Some(p) => hit.pos - p.pos,
                None => Vec3::ZERO,
            };
            (ent, lpos)
        });
        let locked = s.camera_locked;
        let id = self.next_id;
        self.next_id += 1;
        self.props.push(Prop {
            id,
            owner: player,
            model: CAMERA_MODEL.into(),
            pos: hit.start,
            angles,
            frozen: locked || tracking.is_some(),
            gravity: true,
            collide: true,
            upright: None,
            health: 0,
            skin: 0,
            body: String::new(),
            fire: None,
            material: String::new(),
            color: [255; 4],
            render_mode: 0,
            render_fx: 0,
            phys_material: String::new(),
        });
        self.devices.insert(
            id,
            Device::Camera {
                key,
                toggle: s.camera_toggle,
                locked,
                tracking,
                viewer: None,
            },
        );
        self.numpad
            .entry((player, key))
            .or_default()
            .push(NumpadBind::Camera(id));
        self.add_count(player, "cameras", id);
        self.add_cleanup(player, "cameras", id);
        self.push_undo(
            player,
            "gmod_cameraprop",
            "gmod_cameraprop".into(),
            None,
            vec![id],
        );
        true
    }

    /// The trail on an entity goes (`SetTrails` always removes the old one first).
    fn remove_trail(&mut self, ent: u32) {
        self.devices
            .retain(|_, d| !matches!(d, Device::Trail { ent: e, .. } if *e == ent));
    }

    /// trails.lua `LeftClick`: the old trail goes, then `SetTrails` with the convars - nothing
    /// for a zero-sized trail or (outside single player) a material off the list, else an
    /// env_spritetrail of its own with length 0.1..10 and sizes 0..128.
    fn trails_click(&mut self, player: u32, hit: &ToolHit) -> bool {
        let s = self.tool_settings.clone();
        let Some(ent) = hit.ent.filter(|e| self.props.iter().any(|p| p.id == *e)) else {
            return false;
        };
        self.remove_trail(ent);
        if s.trails_startsize <= 0.0 && s.trails_endsize <= 0.0 {
            return false;
        }
        if !TRAIL_MATERIALS.contains(&s.trails_material.as_str()) {
            return false;
        }
        let c = |v: f32| v.clamp(0.0, 255.0) as u8;
        let id = self.next_id;
        self.next_id += 1;
        self.devices.insert(
            id,
            Device::Trail {
                ent,
                color: [c(s.trails_r), c(s.trails_g), c(s.trails_b), c(s.trails_a)],
                length: s.trails_length.clamp(0.1, 10.0),
                start_size: s.trails_startsize.clamp(0.0, 128.0).max(0.0001),
                end_size: s.trails_endsize.clamp(0.0, 128.0),
                material: s.trails_material.clone(),
            },
        );
        self.add_cleanup(player, "trails", id);
        self.push_undo(
            player,
            "Trail",
            "#tool.trails.name".into(),
            Some("Undone #tool.trails.name".into()),
            vec![id],
        );
        true
    }

    /// The material / colour stools' state on a prop (saves, dupes).
    pub fn set_render(&mut self, id: u32, material: &str, color: [u8; 4], mode: u8, fx: u8) {
        if let Some(p) = self.props.iter_mut().find(|p| p.id == id) {
            p.material = material.to_string();
            p.color = color;
            p.render_mode = mode;
            p.render_fx = fx;
        }
    }

    /// The physprop stool's state on a prop (saves, dupes).
    pub fn set_phys_props(&mut self, id: u32, gravity: bool, material: &str) {
        if let Some(p) = self.props.iter_mut().find(|p| p.id == id) {
            p.gravity = gravity;
            p.phys_material = material.to_string();
        }
    }

    /// Whether the remover has this prop non-solid and invisible, waiting for its deletion.
    pub fn is_removing(&self, id: u32) -> bool {
        self.removing.contains_key(&id)
    }

    /// Advances the remover's timers; the props whose second is up are to be deleted
    /// (`remove_entity`) by the caller.
    pub fn tick_removals(&mut self, dt: f32) -> Vec<u32> {
        // `OnRemove` of a camera runs with the engine's deferred deletion at the end of the
        // tick: until then its viewer still looks through it
        self.view.retain(|_, c| self.devices.contains_key(c));
        let mut due = std::mem::take(&mut self.deleting);
        for (id, left) in self.removing.iter_mut() {
            *left -= dt;
            if *left <= 1e-4 {
                due.push(*id);
            }
        }
        for id in &due {
            self.removing.remove(id);
        }
        due
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
                | "remover"
                | "lamp"
                | "light"
                | "material"
                | "colour"
                | "physprop"
                | "trails"
                | "emitter"
                | "camera"
                | "leafblower"
                | "inflator"
                | "eyeposer"
                | "faceposer"
                | "finger"
                | "paint"
                | "creator"
                | "duplicator"
                | "ballsocket"
                | "slider"
                | "hydraulic"
                | "winch"
                | "muscle"
                | "pulley"
                | "thruster"
                | "hoverball"
                | "balloon"
                | "wheel"
                | "button"
                | "dynamite"
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
                        torquelimit: s.ballsocket_torquelimit,
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
                        stretchonly: s.elastic_stretch_only,
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
                color: [255, 255, 255],
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

    /// One click of `thruster`, `hoverball` or `balloon`: a prop of the stool's model placed on
    /// the hit, welded (thruster, hoverball) or roped (balloon, `attach`) to a hit entity, with
    /// its device state and numpad bindings. The thruster sits with its bottom on the surface
    /// (`HitPos - normal * OBBMins.z`), the others with their nearest hull point on it.
    fn place_device(
        &mut self,
        player: u32,
        tool: &str,
        hit: &ToolHit,
        attach: bool,
        events: &mut Vec<Event>,
    ) -> bool {
        let s = self.tool_settings.clone();
        let (count, model) = match tool {
            "thruster" => ("thrusters", s.thruster_model.clone()),
            "hoverball" => ("hoverballs", s.hoverball_model.clone()),
            "wheel" => ("wheels", s.wheel_model.clone()),
            "button" => ("buttons", s.button_model.clone()),
            "dynamite" => ("dynamite", s.dynamite_model.clone()),
            "lamp" => ("lamps", s.lamp_model.clone()),
            "light" => ("lights", LIGHT_MODEL.to_string()),
            "emitter" => ("emitters", EMITTER_MODEL.to_string()),
            _ => ("balloons", s.balloon_model.clone()),
        };
        // a click on the player's own lamp / light re-applies the settings instead
        if matches!(tool, "lamp" | "light" | "emitter") {
            if let Some(e) = hit.ent {
                let own = self.props.iter().any(|p| p.id == e && p.owner == player);
                let key = match (tool, self.devices.get_mut(&e)) {
                    (
                        "lamp",
                        Some(Device::Lamp {
                            color,
                            texture,
                            fov,
                            distance,
                            brightness,
                            toggle,
                            key,
                            ..
                        }),
                    ) if own => {
                        *color = [
                            s.lamp_r.clamp(0.0, 255.0) as u8,
                            s.lamp_g.clamp(0.0, 255.0) as u8,
                            s.lamp_b.clamp(0.0, 255.0) as u8,
                        ];
                        *texture = s.lamp_texture.clone();
                        *fov = s.lamp_fov;
                        *distance = s.lamp_distance;
                        *brightness = s.lamp_brightness;
                        *toggle = s.lamp_toggle;
                        *key = s.lamp_key;
                        Some(s.lamp_key)
                    }
                    (
                        "emitter",
                        Some(Device::Emitter {
                            key,
                            delay,
                            toggle,
                            effect,
                            scale,
                            ..
                        }),
                    ) if own => {
                        *effect = s.emitter_effect.clone();
                        *delay = s.emitter_delay.clamp(0.05, 20.0);
                        *toggle = s.emitter_toggle;
                        *scale = s.emitter_scale.clamp(0.1, 6.0);
                        *key = s.emitter_key;
                        Some(s.emitter_key)
                    }
                    (
                        "light",
                        Some(Device::Light {
                            color,
                            brightness,
                            size,
                            toggle,
                            key,
                            ..
                        }),
                    ) if own => {
                        *color = [
                            s.light_r.clamp(0.0, 255.0) as u8,
                            s.light_g.clamp(0.0, 255.0) as u8,
                            s.light_b.clamp(0.0, 255.0) as u8,
                        ];
                        *brightness = s.light_brightness.clamp(-10.0, 20.0);
                        *size = s.light_size;
                        *toggle = s.light_toggle;
                        *key = s.light_key;
                        Some(s.light_key)
                    }
                    _ => None,
                };
                if let Some(key) = key {
                    let bind = if tool == "emitter" {
                        NumpadBind::Emitter(e)
                    } else {
                        NumpadBind::LightSwitch(e)
                    };
                    self.rebind(player, e, key, bind);
                    return true;
                }
            }
        }
        let Some(shape) = self.shapes.get(&model).cloned() else {
            return false;
        };
        if !self.check_limit(player, count, events) {
            return false;
        }
        let n = hit.normal;
        // `HitNormal:Angle()` with the pitch turned 90 degrees up: the model's z along the normal
        let angles = match tool {
            "balloon" | "dynamite" | "lamp" => Vec3::ZERO,
            // emitter.lua: `HitNormal:Angle()` as is
            "emitter" => angle_of(n),
            // light.lua: `HitNormal:Angle() - Angle(90, 0, 0)`
            "light" => {
                let a = angle_of(n);
                Vec3::new(a.x - 90.0, a.y, a.z)
            }
            "wheel" => {
                let a = angle_of(n);
                Vec3::new(
                    normalize_angle(a.x + s.wheel_rx),
                    normalize_angle(a.y + s.wheel_ry),
                    normalize_angle(a.z + s.wheel_rz),
                )
            }
            _ => {
                let a = angle_of(n);
                Vec3::new(a.x + 90.0, a.y, a.z)
            }
        };
        let rot = rotation(angles);
        let pos = if tool == "thruster" || tool == "button" {
            hit.pos - n * shape.min.z
        } else if tool == "light" {
            hit.pos + n * 8.0
        } else if tool == "emitter" {
            // on the hit point when welded, one unit out when left on the world
            if hit.ent.is_some() || attach {
                hit.pos
            } else {
                hit.pos + n
            }
        } else {
            hit.pos + n * obb_depth(&shape, rot, n)
        };
        let id = self.next_id;
        self.next_id += 1;
        self.props.push(Prop {
            id,
            owner: player,
            model: model.clone(),
            pos,
            angles,
            frozen: false,
            gravity: tool != "balloon",
            // a welded emitter: `EnableCollisions(false)`
            collide: !(tool == "emitter" && (hit.ent.is_some() || attach)),
            upright: None,
            health: 0,
            // gmod_lamp: `SetSkin(lightInfo.Skin)`, 1 for the torch and the other stock lamps
            skin: u8::from(matches!(tool, "balloon" | "lamp")),
            body: String::new(),
            fire: None,
            material: String::new(),
            color: [255; 4],
            render_mode: 0,
            render_fx: 0,
            phys_material: String::new(),
        });
        let mut undo_ents = vec![id];
        let mut binds: Vec<(i32, NumpadBind)> = Vec::new();
        let device = match tool {
            "thruster" => {
                binds.push((s.thruster_keygroup, NumpadBind::Thruster(id, 1.0)));
                binds.push((s.thruster_keygroup_back, NumpadBind::Thruster(id, -1.0)));
                Device::Thruster {
                    force: s.thruster_force.clamp(0.0, 1e10),
                    mul: 0.0,
                    sound: if THRUSTER_SOUNDS.contains(&s.thruster_soundname.as_str()) {
                        s.thruster_soundname.clone()
                    } else {
                        String::new()
                    },
                    effect: s.thruster_effect.clone(),
                    damageable: s.thruster_damageable,
                    off_at: 0.0,
                    toggle: s.thruster_toggle,
                    on: false,
                    top_z: shape.max.z,
                    bottom_z: shape.min.z,
                    key: s.thruster_keygroup,
                    key_back: s.thruster_keygroup_back,
                    rear: false,
                }
            }
            "lamp" => {
                binds.push((s.lamp_key, NumpadBind::LightSwitch(id)));
                Device::Lamp {
                    color: [
                        s.lamp_r.clamp(0.0, 255.0) as u8,
                        s.lamp_g.clamp(0.0, 255.0) as u8,
                        s.lamp_b.clamp(0.0, 255.0) as u8,
                    ],
                    texture: s.lamp_texture.clone(),
                    fov: s.lamp_fov,
                    distance: s.lamp_distance,
                    brightness: s.lamp_brightness,
                    toggle: s.lamp_toggle,
                    // `MakeLamp(.., toggle, .., on = !toggle)`: lit from the start with the toggle convar
                    on: s.lamp_toggle,
                    key: s.lamp_key,
                }
            }
            "emitter" => {
                binds.push((s.emitter_key, NumpadBind::Emitter(id)));
                Device::Emitter {
                    key: s.emitter_key,
                    delay: s.emitter_delay.clamp(0.05, 20.0),
                    toggle: s.emitter_toggle,
                    on: s.emitter_starton,
                    effect: s.emitter_effect.clone(),
                    scale: s.emitter_scale.clamp(0.1, 6.0),
                }
            }
            "light" => {
                binds.push((s.light_key, NumpadBind::LightSwitch(id)));
                if attach {
                    // `constraint.Rope(light, ent, 0, bone, (0, 0, 6.5), local hit, 0, length, 0, 1, mat)`
                    let lpos2 = match hit.ent.and_then(|e| self.props.iter().find(|p| p.id == e)) {
                        Some(p) => rotation(p.angles).inverse() * (hit.pos - p.pos),
                        None => hit.pos,
                    };
                    // (length 0, addlength = the convar)
                    let mut rs = s.clone();
                    rs.rope_addlength = s.light_ropelength.clamp(4.0, 1024.0);
                    rs.rope_material = s.light_ropematerial.clone();
                    rs.rope_width = 1.0;
                    rs.rope_rigid = false;
                    if let Some(rope) = self.rope(
                        player,
                        Some(id),
                        hit.ent,
                        Vec3::new(0.0, 0.0, 6.5),
                        lpos2,
                        0.0,
                        &rs,
                    ) {
                        undo_ents.push(rope);
                        self.add_cleanup(player, count, rope);
                    }
                }
                Device::Light {
                    color: [
                        s.light_r.clamp(0.0, 255.0) as u8,
                        s.light_g.clamp(0.0, 255.0) as u8,
                        s.light_b.clamp(0.0, 255.0) as u8,
                    ],
                    brightness: s.light_brightness.clamp(-10.0, 20.0),
                    size: s.light_size,
                    toggle: s.light_toggle,
                    on: s.light_toggle,
                    key: s.light_key,
                }
            }
            "hoverball" => {
                binds.push((s.hoverball_keyup, NumpadBind::HoverballMove(id, 1.0)));
                binds.push((s.hoverball_keydn, NumpadBind::HoverballMove(id, -1.0)));
                binds.push((s.hoverball_keyon, NumpadBind::HoverballToggle(id)));
                Device::Hoverball {
                    // the target is the hit point: `SetTargetZ(GetPos().z)` before the offset
                    target_z: hit.pos.z,
                    zvel: 0.0,
                    speed: s.hoverball_speed,
                    resistance: s.hoverball_resistance.clamp(0.0, 20.0),
                    strength: s.hoverball_strength.clamp(0.1, 20.0),
                    enabled: true,
                    key_up: s.hoverball_keyup,
                    key_down: s.hoverball_keydn,
                    key_toggle: s.hoverball_keyon,
                }
            }
            "wheel" => {
                binds.push((s.wheel_fwd, NumpadBind::Wheel(id, 1.0)));
                binds.push((s.wheel_bck, NumpadBind::Wheel(id, -1.0)));
                Device::Wheel {
                    torque: s.wheel_torque,
                    friction: s.wheel_friction,
                    scale: 0.0,
                    toggle: s.wheel_toggle,
                    toggle_state: false,
                    direction: 1.0,
                    key_f: s.wheel_fwd,
                    key_r: s.wheel_bck,
                    axis: 0,
                    local_axis: rot.inverse() * (hit.pos - pos),
                    applied: 0.0,
                }
            }
            "button" => Device::Button {
                description: s.button_description.clone(),
                key: s.button_keygroup,
                toggle: s.button_toggle,
                on: false,
                user: None,
            },
            "dynamite" => {
                binds.push((s.dynamite_group, NumpadBind::Dynamite(id)));
                Device::Dynamite {
                    damage: s.dynamite_damage,
                    delay: s.dynamite_delay,
                    remove: s.dynamite_remove,
                    key: s.dynamite_group,
                    queued: Vec::new(),
                }
            }
            _ => Device::Balloon {
                force: s.balloon_force.clamp(-1e34, 1e34),
            },
        };
        self.devices.insert(id, device);
        for (key, bind) in binds {
            self.numpad.entry((player, key)).or_default().push(bind);
        }
        if let Some(target) = hit.ent {
            if tool == "balloon" {
                if attach {
                    let (tpos, trot) = self.prop_pose(target).unwrap_or_default();
                    let lpos2 = trot.inverse() * (hit.pos - tpos);
                    let rope = self.next_id;
                    self.next_id += 1;
                    self.constraints.push(Constraint {
                        id: rope,
                        owner: player,
                        ent1: Some(id),
                        ent2: Some(target),
                        kind: ConstraintKind::Rope {
                            lpos1: Vec3::ZERO,
                            lpos2,
                            length: 0.0,
                            addlength: s.balloon_ropelength,
                            width: 0.5,
                            material: "cable/rope".into(),
                            rigid: false,
                        },
                        color: [255, 255, 255],
                    });
                    undo_ents.push(rope);
                    self.add_cleanup(player, count, rope);
                    // the keyframe_rope entity
                    undo_ents.push(self.next_id);
                    self.next_id += 1;
                }
            } else if tool == "wheel" {
                // `constraint.Motor`: an Axis (no table of its own) plus the phys_torque
                let (tpos, trot) = self.prop_pose(target).unwrap_or_default();
                let lpos1 = rot.inverse() * n;
                let lpos2 = trot.inverse() * (hit.pos - tpos);
                let axis_id = self.next_id;
                self.next_id += 1;
                self.constraints.push(Constraint {
                    id: axis_id,
                    owner: player,
                    ent1: Some(id),
                    ent2: Some(target),
                    kind: ConstraintKind::Axis {
                        lpos1,
                        lpos2,
                        local_axis: lpos1,
                        forcelimit: s.wheel_forcelimit,
                        torquelimit: 0.0,
                        friction: s.wheel_friction,
                        nocollide: s.wheel_nocollide,
                    },
                    color: [255, 255, 255],
                });
                if let Some(Device::Wheel { axis, .. }) = self.devices.get_mut(&id) {
                    *axis = axis_id;
                }
                // undo: the torque entity, the axis, the wheel
                undo_ents.insert(0, axis_id);
                undo_ents.insert(0, self.next_id);
                self.next_id += 1;
                self.add_cleanup(player, count, axis_id);
            } else if !matches!(tool, "dynamite" | "lamp" | "light") {
                let nocollide = (tool == "thruster" && !s.thruster_collision) || tool == "emitter";
                if let Some(weld) = self.weld(player, Some(id), Some(target), 0.0, nocollide) {
                    undo_ents.push(weld);
                    self.add_cleanup(player, count, weld);
                }
            }
        } else if tool == "button" || (tool == "emitter" && attach) {
            // a button on the world is welded to the world (an emitter on a right click too)
            if let Some(weld) = self.weld(player, Some(id), None, 0.0, false) {
                undo_ents.push(weld);
                self.add_cleanup(player, count, weld);
            }
        }
        self.push_undo(
            player,
            &format!("gmod_{tool}"),
            format!("#tool.{tool}.name"),
            Some(format!("Undone #tool.{tool}.name")),
            undo_ents,
        );
        self.add_count(player, count, id);
        self.add_cleanup(player, count, id);
        events.push(Event::Spawned { prop: id });
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
                        binds.push((s.hydraulic_group, NumpadBind::HydraulicToggle(id)));
                    } else {
                        binds.push((s.hydraulic_group, NumpadBind::HydraulicDir(id)));
                    }
                    (
                        ConstraintKind::Hydraulic {
                            lpos1: o1.pos,
                            lpos2: o2.pos,
                            length1: length,
                            length2,
                            width: s.hydraulic_width,
                            material: s.hydraulic_material.clone(),
                            key: s.hydraulic_group,
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
                        binds.push((s.winch_fwd_group, NumpadBind::WinchToggle(id, 1)));
                        binds.push((s.winch_bwd_group, NumpadBind::WinchToggle(id, -1)));
                    } else {
                        binds.push((s.winch_fwd_group, NumpadBind::WinchDir(id, 1)));
                        binds.push((s.winch_bwd_group, NumpadBind::WinchDir(id, -1)));
                    }
                    (
                        ConstraintKind::Winch {
                            lpos1: o1.pos,
                            lpos2: o2.pos,
                            width: s.winch_rope_width,
                            material: s.winch_rope_material.clone(),
                            fwd_key: s.winch_fwd_group,
                            bwd_key: s.winch_bwd_group,
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
                        s.winch_rope_width,
                        false,
                    )
                }
                _ => {
                    let (constant, damping) = elastic_consts(m1, m2, s.muscle_fixed);
                    // the stool hands the whole added length to the controller as amplitude
                    let amplitude = s.muscle_addlength;
                    let length2 = length + s.muscle_addlength;
                    binds.push((s.muscle_group, NumpadBind::MuscleToggle(id)));
                    (
                        ConstraintKind::Muscle {
                            lpos1: o1.pos,
                            lpos2: o2.pos,
                            length1: length,
                            length2,
                            width: s.muscle_width,
                            material: s.muscle_material.clone(),
                            key: s.muscle_group,
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
                color: [255, 255, 255],
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
                    color: [255, 255, 255],
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
                color: color_u8(s.pulley_color_r, s.pulley_color_g, s.pulley_color_b),
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
                        friction: s.axis_hingefriction,
                        nocollide: s.axis_nocollide,
                    },
                    color: [255, 255, 255],
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

    /// `motor.lua`'s second click: the Axis of `finish_axis` (the hinge point 64 u out along
    /// the second normal), friction / forcelimit / nocollide from the motor's settings, plus the
    /// `phys_torque` device bound to the numpad keys; undo and counts take both.
    pub fn finish_motor(&mut self, player: u32, events: &mut Vec<Event>) -> bool {
        let objs = self.tool(player).objects;
        if objs.len() < 2 {
            return false;
        }
        let (o1, o2) = (objs[0].clone(), objs[1].clone());
        if !self.check_limit(player, "constraints", events) {
            self.clear_objects(player);
            return false;
        }
        let s = self.tool_settings.clone();
        if let Some(ent1) = o1.ent {
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
            let lpos1 = rot1.inverse() * (wpos2 + norm2 * 64.0 - pos1);
            // constraint.Motor passes no LocalAxis, so the Axis keeps the second click point
            // (prop-1 local) as its axis point: the hinge runs from LPos1 back to the surface
            let axis_point = rot1.inverse() * (wpos2 - pos1);
            if o2.ent != o1.ent {
                let axis_id = self.next_id;
                self.next_id += 1;
                self.constraints.push(Constraint {
                    id: axis_id,
                    owner: player,
                    ent1: o1.ent,
                    ent2: o2.ent,
                    kind: ConstraintKind::Axis {
                        lpos1,
                        lpos2: o2.pos,
                        local_axis: axis_point,
                        forcelimit: s.motor_forcelimit,
                        torquelimit: 0.0,
                        friction: s.motor_friction,
                        nocollide: s.motor_nocollide,
                    },
                    color: [255, 255, 255],
                });
                let motor_id = self.next_id;
                self.next_id += 1;
                self.devices.insert(
                    motor_id,
                    Device::Motor {
                        ent: ent1,
                        axis: axis_id,
                        torque: s.motor_torque,
                        friction: s.motor_friction,
                        forcetime: s.motor_forcetime,
                        scale: 0.0,
                        toggle: s.motor_toggle,
                        is_on: false,
                        direction: 1.0,
                        key_f: s.motor_fwd,
                        key_r: s.motor_bwd,
                        local_axis: (axis_point - lpos1).normalize_or_zero(),
                        applied: 0.0,
                    },
                );
                for (key, bind) in [
                    (s.motor_fwd, NumpadBind::Motor(motor_id, 1.0)),
                    (s.motor_bwd, NumpadBind::Motor(motor_id, -1.0)),
                ] {
                    self.numpad.entry((player, key)).or_default().push(bind);
                }
                self.push_undo(
                    player,
                    "Motor",
                    "#tool.motor.name".into(),
                    Some("Undone #tool.motor.name".into()),
                    vec![motor_id, axis_id],
                );
                self.add_count(player, "constraints", motor_id);
                self.add_cleanup(player, "constraints", motor_id);
                self.add_cleanup(player, "constraints", axis_id);
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
        if !self.can_tool(hit) {
            return false;
        }
        if tool == "trails" {
            return self.trails_click(player, hit);
        }
        if tool == "camera" {
            return self.camera_click(player, hit, false, events);
        }
        if tool == "leafblower" {
            self.leafblower_click(hit);
            return false;
        }
        if matches!(tool, "material" | "colour" | "physprop") {
            let s = self.tool_settings.clone();
            let Some(p) = hit
                .ent
                .and_then(|e| self.props.iter_mut().find(|p| p.id == e))
            else {
                return false;
            };
            match tool {
                // material.lua: outside single player only the OverrideMaterials list (or
                // clearing) gets through, but the click still answers true
                "material" => {
                    if s.material_override.is_empty()
                        || OVERRIDE_MATERIALS.contains(&s.material_override.as_str())
                    {
                        p.material = s.material_override.clone();
                    }
                }
                "colour" => {
                    // measured: 300 -> 255, -5 -> 251, 99.6 -> 99 (capped above, cast below)
                    let c = |v: f32| (v.min(255.0) as i32) as u8;
                    p.color = [c(s.colour_r), c(s.colour_g), c(s.colour_b), c(s.colour_a)];
                    let mut mode = s.colour_mode;
                    // SetColour: a translucent colour in RENDERMODE_NORMAL goes TRANSCOLOR
                    if p.color[3] < 255 && mode == 0.0 {
                        mode = 1.0;
                    }
                    p.render_mode = mode.clamp(0.0, 9.0) as u8;
                    let fx = s.colour_fx;
                    p.render_fx = if (0.0..=16.0).contains(&fx) || fx == 24.0 {
                        fx as u8
                    } else {
                        0
                    };
                }
                _ => {
                    // construct.SetPhysProp: gravity and the physics material of the object
                    p.gravity = s.physprop_gravity_toggle;
                    p.phys_material = s.physprop_material.clone();
                }
            }
            return true;
        }
        if tool == "remover" {
            let Some(ent) = hit.ent else {
                return false;
            };
            if !self.remover_remove(ent) {
                return false;
            }
            events.push(Event::Lua {
                code: "achievements.Remover()".into(),
            });
            return true;
        }
        if (tool == "nocollide" || tool == "axis" || tool == "motor")
            && hit.ent.is_none()
            && self.tool(player).objects.is_empty()
        {
            return false;
        }
        // measured `server_stools_vehicle`: the nocollide's click on the world is refused
        // outright (`util.IsValidPhysicsObject` on worldspawn is false), the first pick stays
        if tool == "nocollide" && hit.ent.is_none() {
            return false;
        }
        if tool == "axis" {
            if self.tool(player).operation == 2 {
                return false;
            }
            self.tools.entry(player).or_default().operation = 1;
        }
        if matches!(
            tool,
            "thruster"
                | "hoverball"
                | "balloon"
                | "wheel"
                | "button"
                | "dynamite"
                | "lamp"
                | "light"
                | "emitter"
        ) {
            // a wheel needs an entity to drive against
            if tool == "wheel" && hit.ent.is_none() {
                return false;
            }
            return self.place_device(
                player,
                tool,
                hit,
                matches!(tool, "balloon" | "light"),
                events,
            );
        }
        // a pulley's first and last click must be entities; the two pulley points are anywhere
        if tool == "pulley"
            && hit.ent.is_none()
            && !matches!(self.tool(player).objects.len(), 1 | 2)
        {
            return false;
        }
        let n = self.set_object(player, tool, hit);
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
            "motor" => self.finish_motor(player, events),
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
            color: [255, 255, 255],
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
            color: color_u8(s.rope_color_r, s.rope_color_g, s.rope_color_b),
        });
        Some(id)
    }

    /// An entity id for something the server owns outside the prop list (an NPC): undo and
    /// cleanup remove it like a prop, the server drops its own state when `is_foreign` is false.
    pub fn alloc_foreign(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.foreign.insert(id);
        id
    }

    pub fn is_foreign(&self, id: u32) -> bool {
        self.foreign.contains(&id)
    }

    /// Removes an entity; a prop takes its constraints with it.
    fn remove(&mut self, id: u32) -> bool {
        if self.foreign.remove(&id) {
            return true;
        }
        let props_before = self.props.len();
        self.props.retain(|p| p.id != id);
        if self.props.len() != props_before {
            self.constraints
                .retain(|c| c.ent1 != Some(id) && c.ent2 != Some(id));
            self.devices.remove(&id);
            self.drop_parts();
            return true;
        }
        // a device of its own (trail, motor torque entity): it and what hangs off it go
        if self.devices.contains_key(&id) {
            self.remove_entity(id);
            self.devices.remove(&id);
            self.drop_parts();
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
    /// `numpad.Remove(old); numpad.OnDown/OnUp(ply, key, ..)`: the entity's binds move to `key`.
    fn rebind(&mut self, player: u32, ent: u32, key: i32, bind: NumpadBind) {
        for ((p, _), binds) in self.numpad.iter_mut() {
            if *p == player {
                binds.retain(|b| b.constraint() != ent);
            }
        }
        self.numpad.retain(|_, b| !b.is_empty());
        self.numpad.entry((player, key)).or_default().push(bind);
    }

    /// `gmod_thruster:OnTakeDamage` with `thruster_damageable`: `Switch(true)` and
    /// `SwitchOffTime = CurTime() + 5`; false when the entity is not such a thruster.
    pub fn thruster_damaged(&mut self, id: u32, now: f32) -> bool {
        let Some(Device::Thruster {
            damageable,
            mul,
            on,
            rear,
            off_at,
            ..
        }) = self.devices.get_mut(&id)
        else {
            return false;
        };
        if !*damageable {
            return false;
        }
        if *mul == 0.0 {
            *mul = 1.0;
        }
        *on = true;
        *rear = *mul <= 0.0;
        *off_at = now + 5.0;
        true
    }

    /// The thruster think: `Switch(false)` once `SwitchOffTime` has passed.
    pub fn thruster_timeouts(&mut self, now: f32) {
        for d in self.devices.values_mut() {
            if let Device::Thruster {
                on, mul, off_at, ..
            } = d
            {
                if *off_at > 0.0 && now > *off_at {
                    *off_at = 0.0;
                    *on = false;
                    *mul = 0.0;
                }
            }
        }
    }

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
        let live: Vec<u32> = self
            .constraints
            .iter()
            .map(|c| c.id)
            .chain(self.devices.keys().copied())
            .collect();
        binds.retain(|b| live.contains(&b.constraint()));
        let binds = binds.clone();
        let mut removed = Vec::new();
        for b in binds {
            if let NumpadBind::Camera(cam) = &b {
                self.camera_key(player, *cam, down);
                continue;
            }
            if let Some(dev) = self.devices.get_mut(&b.constraint()) {
                match (b, dev) {
                    (
                        NumpadBind::Thruster(_, m),
                        Device::Thruster {
                            mul,
                            toggle,
                            on,
                            rear,
                            ..
                        },
                    ) => {
                        if *toggle {
                            if !down {
                                continue;
                            }
                            *mul = if *mul == m { 0.0 } else { m };
                        } else {
                            *mul += if down { m } else { -m };
                        }
                        *on = *mul != 0.0;
                        *rear = *mul <= 0.0;
                    }
                    (
                        NumpadBind::Wheel(_, mul),
                        Device::Wheel {
                            scale,
                            toggle,
                            toggle_state,
                            direction,
                            ..
                        },
                    ) => {
                        // `ENT:Forward(onoff, mul)`: a toggle ignores the release
                        if *toggle && !down {
                            continue;
                        }
                        let mut speed = *direction * mul;
                        if !down {
                            speed = 0.0;
                        }
                        if *toggle {
                            *toggle_state = !*toggle_state;
                            if !*toggle_state {
                                speed = 0.0;
                            }
                        }
                        *scale = speed;
                    }
                    (
                        NumpadBind::Motor(_, dir),
                        Device::Motor {
                            scale,
                            toggle,
                            is_on,
                            direction,
                            ..
                        },
                    ) => {
                        // `MotorControl`: a toggle flips on a press of the running direction
                        // (or when off), a press of the other direction turns it round and
                        // keeps it on, releases do nothing; otherwise down = on, up = off
                        let activate = if *toggle {
                            if !down {
                                continue;
                            }
                            if *direction == dir || !*is_on {
                                *is_on = !*is_on;
                                *is_on
                            } else {
                                true
                            }
                        } else {
                            down
                        };
                        if activate {
                            *direction = dir;
                            *scale = dir;
                        } else {
                            *scale = 0.0;
                        }
                    }
                    (NumpadBind::Emitter(_), Device::Emitter { toggle, on, .. }) => {
                        // Emitter_On flips a toggle, else holds; Emitter_Off only without toggle
                        if *toggle {
                            if down {
                                *on = !*on;
                            }
                        } else {
                            *on = down;
                        }
                    }
                    (
                        NumpadBind::LightSwitch(_),
                        Device::Lamp { toggle, on, .. } | Device::Light { toggle, on, .. },
                    ) => {
                        // no toggle: the key holds the light; toggle: a press flips it
                        if !*toggle {
                            *on = down;
                        } else if down {
                            *on = !*on;
                        }
                    }
                    (
                        NumpadBind::Dynamite(_),
                        Device::Dynamite {
                            delay,
                            damage,
                            remove,
                            queued,
                            ..
                        },
                    ) if down => {
                        let delay = delay.min(3600.0);
                        // measured: a 1 s delay blows 70 ticks after the key (the 0.05 s think)
                        let ticks = if delay == 0.0 {
                            0
                        } else {
                            (delay / SERVER_TICK).ceil() as u32 + 3
                        };
                        let ent = b.constraint();
                        if ticks == 0 {
                            let stick = self.props.iter().find(|p| p.id == ent);
                            if let Some(p) = stick {
                                self.blasts.push((p.pos, *damage, p.owner));
                            }
                            if *remove {
                                removed.push(ent);
                            }
                        } else {
                            queued.push(ticks);
                        }
                    }
                    (NumpadBind::HoverballMove(_, d), Device::Hoverball { zvel, .. }) => {
                        *zvel = if down { d * 75.0 } else { 0.0 };
                    }
                    (
                        NumpadBind::HoverballToggle(_),
                        Device::Hoverball {
                            enabled, target_z, ..
                        },
                    ) if down => {
                        *enabled = !*enabled;
                        if *enabled {
                            *target_z = f32::NAN;
                        }
                    }
                    _ => {}
                }
                continue;
            }
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
        for id in removed {
            self.remove_entity(id);
        }
    }

    /// A device model's shape, needed before its stool can place it.
    pub fn set_model_shape(&mut self, model: &str, shape: ModelShape) {
        self.shapes.insert(model.to_string(), shape);
    }

    pub fn has_model_shape(&self, model: &str) -> bool {
        self.shapes.contains_key(model)
    }

    pub fn devices(&self) -> &BTreeMap<u32, Device> {
        &self.devices
    }

    pub fn device_mut(&mut self, prop: u32) -> Option<&mut Device> {
        self.devices.get_mut(&prop)
    }

    /// A device straight from a save, on an existing prop.
    pub fn restore_device(&mut self, prop: u32, device: Device, binds: &[(i32, NumpadBind)]) {
        self.devices.insert(prop, device);
        for (key, bind) in binds {
            self.numpad.entry((prop, *key)).or_default().push(*bind);
        }
    }

    /// Numpad bindings of a player's key (for saves).
    pub fn numpad_binds(&self, player: u32) -> Vec<(i32, NumpadBind)> {
        self.numpad
            .iter()
            .filter(|((p, _), _)| *p == player)
            .flat_map(|((_, k), v)| v.iter().map(move |b| (*k, *b)))
            .collect()
    }

    /// `gmod_button:Use` on a +use edge by `player` (`ONOFF_USE`: `on` = USE_ON): a toggle flips
    /// on the press, a plain button is on from the press and off on the release (`Think` sees
    /// the user no longer holding +use). The key goes through `numpad` as the owner's.
    pub fn button_use(&mut self, prop: u32, player: u32, on: bool) -> bool {
        let Some(Device::Button {
            key,
            toggle,
            on: is_on,
            user,
            ..
        }) = self.devices.get_mut(&prop)
        else {
            return false;
        };
        let (key, owner) = (
            *key,
            self.props.iter().find(|p| p.id == prop).map(|p| p.owner),
        );
        let new_state = if *toggle {
            if !on {
                return true;
            }
            !*is_on
        } else {
            if on && user.is_some() {
                return true;
            }
            on
        };
        *is_on = new_state;
        *user = if new_state { Some(player) } else { None };
        let owner = owner.unwrap_or(player);
        self.numpad(owner, key, new_state);
        true
    }

    /// Blasts due this tick as `(origin, damage, attacker)` (drained): the physics owner deals
    /// them; the attacker is the stick's owner (`util.BlastDamage(self, self:GetPlayer(), ..)`).
    /// leafblower.lua `LeftClick` (no return, so nil): on an entity with a physics object,
    /// `ApplyForceOffset(-(StartPos - HitPos) * 32 * clamp(1 - distance / 512, 0, 1), HitPos)` -
    /// an impulse away from the player, fading to nothing at 512 units.
    pub fn leafblower_click(&mut self, hit: &ToolHit) {
        let Some(ent) = hit.ent.filter(|e| self.props.iter().any(|p| p.id == *e)) else {
            return;
        };
        let direction = hit.start - hit.pos;
        let ratio = (1.0 - direction.length() / 512.0).clamp(0.0, 1.0);
        self.impulses
            .push((ent, hit.pos, -direction * (32.0 * ratio)));
    }

    pub fn take_impulses(&mut self) -> Vec<(u32, Vec3, Vec3)> {
        std::mem::take(&mut self.impulses)
    }

    pub fn take_blasts(&mut self) -> Vec<(Vec3, f32, u32)> {
        std::mem::take(&mut self.blasts)
    }

    /// The hoverballs' `Think`: a held key moves the target height by `zvel * speed * dt`; a
    /// re-enabled one targets where it is (`NaN` marks that); queued dynamite counts down.
    /// After the motors drove this tick: `phys_torque` picks a changed `Scale` up next step.
    pub fn settle_wheel_scales(&mut self) {
        for d in self.devices.values_mut() {
            if let Device::Wheel { scale, applied, .. } | Device::Motor { scale, applied, .. } = d {
                *applied = *scale;
            }
        }
    }

    pub fn tick_devices(&mut self, dt: f32) {
        // queued dynamite: the blast from where the stick is now, then `remove` takes it away
        let mut due = Vec::new();
        for (id, d) in &mut self.devices {
            if let Device::Dynamite {
                queued,
                damage,
                remove,
                ..
            } = d
            {
                for q in queued.iter_mut() {
                    *q = q.saturating_sub(1);
                }
                let n = queued.iter().filter(|q| **q == 0).count();
                queued.retain(|q| *q > 0);
                due.extend(std::iter::repeat_n((*id, *damage, *remove), n));
            }
        }
        for (id, damage, remove) in due {
            if let Some(p) = self.props.iter().find(|p| p.id == id) {
                self.blasts.push((p.pos, damage, p.owner));
            }
            if remove {
                self.remove_entity(id);
            }
        }
        let poses: Vec<(u32, f32)> = self.props.iter().map(|p| (p.id, p.pos.z)).collect();
        for (id, d) in &mut self.devices {
            if let Device::Hoverball {
                target_z,
                zvel,
                speed,
                enabled,
                ..
            } = d
            {
                if target_z.is_nan() {
                    *target_z = poses.iter().find(|(p, _)| p == id).map_or(0.0, |(_, z)| *z);
                }
                if *enabled && *zvel != 0.0 {
                    *target_z += *zvel * dt * *speed;
                }
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
            events.push(Event::Undone {
                id: u.id,
                removed,
                name: u.name.clone(),
            });
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
    #[test]
    fn script_motor_is_an_axis_plus_a_device() {
        use super::*;
        let mut sb = Sandbox::new(Limits::default());
        let a = sb.restore_prop(
            1,
            "models/props_junk/wood_crate001a.mdl",
            Vec3::ZERO,
            Vec3::ZERO,
            false,
        );
        assert!(sb
            .motor(
                1,
                a,
                Some(a),
                Vec3::ZERO,
                Vec3::ZERO,
                Vec3::Z,
                0.0,
                100.0,
                0.0,
                false,
                false,
                0.0,
                1,
                2,
                1.0
            )
            .is_none());
        let (motor, axis) = sb
            .motor(
                1,
                a,
                None,
                Vec3::ZERO,
                Vec3::ZERO,
                Vec3::Z,
                0.0,
                100.0,
                0.0,
                false,
                false,
                0.0,
                1,
                2,
                1.0,
            )
            .unwrap();
        assert!(
            matches!(sb.constraints()[0].kind, ConstraintKind::Axis { local_axis, .. } if local_axis == Vec3::Z)
        );
        assert_eq!(sb.constraints()[0].id, axis);
        assert!(
            matches!(sb.devices.get(&motor), Some(Device::Motor { axis: ax, torque, .. }) if *ax == axis && *torque == 100.0)
        );
        assert!(sb
            .numpad_binds(1)
            .contains(&(1, NumpadBind::Motor(motor, 1.0))));
        assert!(sb
            .numpad_binds(1)
            .contains(&(2, NumpadBind::Motor(motor, -1.0))));
    }

    /// `constraint.Hydraulic` from a script: the elastic constants come from the lighter body
    /// and the numpad key drives the controller like a stool's.
    #[test]
    fn script_constrain_binds_the_keys() {
        use super::*;
        let mut sb = Sandbox::new(Limits::default());
        let crate_ = "models/props_junk/wood_crate001a.mdl";
        let a = sb.restore_prop(1, crate_, Vec3::ZERO, Vec3::ZERO, false);
        sb.set_mass(a, 30.0);
        let kind = ConstraintKind::Hydraulic {
            lpos1: Vec3::ZERO,
            lpos2: Vec3::new(0.0, 0.0, 60.0),
            length1: 60.0,
            length2: 100.0,
            width: 3.0,
            material: "cable/rope".into(),
            key: 37,
            fixed: false,
            speed: 64.0,
            toggle: false,
            constant: 0.0,
            damping: 0.0,
            controller: LengthController::new(60.0, 60.0, Some(100.0), 64.0, 64.0, false),
        };
        assert!(sb.constrain(1, Some(a), Some(a), kind.clone()).is_none());
        let id = sb.constrain(1, Some(a), None, kind).unwrap();
        let ConstraintKind::Hydraulic {
            constant, damping, ..
        } = &sb.constraints()[0].kind
        else {
            panic!()
        };
        assert_eq!(
            (*constant, *damping),
            elastic_consts(Some(30.0), None, false)
        );
        assert!(sb
            .numpad_binds(1)
            .contains(&(37, NumpadBind::HydraulicDir(id))));
        sb.numpad(1, 37, true);
        assert_eq!(sb.constraints()[0].kind.controller().unwrap().direction, 1);
    }

    use super::*;

    #[test]
    fn leafblower_impulse_fades_with_distance() {
        let mut sb = Sandbox::default();
        let mut ev = Vec::new();
        let b = crate_bounds();
        let req = SpawnRequest {
            eye: Vec3::new(0.0, 0.0, -100.0),
            eye_angles: Vec3::new(30.0, 0.0, 0.0),
            model: "m",
            bounds: b,
        };
        let id = sb.spawn_prop(&floor(), &[], 1, &req, &mut ev).unwrap();
        let hit = |dist: f32| ToolHit {
            pos: Vec3::new(dist, 0.0, 0.0),
            normal: Vec3::NEG_X,
            ent: Some(id),
            bone: 0,
            fraction: 0.1,
            start: Vec3::ZERO,
            dir: Vec3::X,
        };
        assert!(!sb.tool_left_click(1, "leafblower", &hit(100.0), &mut ev));
        let [(prop, point, impulse)] = sb.take_impulses()[..] else {
            panic!("one impulse");
        };
        assert_eq!(prop, id);
        assert_eq!(point, Vec3::new(100.0, 0.0, 0.0));
        // 100 * 32 * (1 - 100 / 512) along the look direction
        assert!(
            (impulse - Vec3::new(2575.0, 0.0, 0.0)).length() < 0.5,
            "{impulse}"
        );
        sb.tool_left_click(1, "leafblower", &hit(600.0), &mut ev);
        assert_eq!(sb.take_impulses()[0].2, Vec3::ZERO);
    }

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
            bone: 0,
            fraction: 0.5,
            start: Vec3::ZERO,
            dir: Vec3::NEG_Z,
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
            bone: 0,
            fraction: 0.5,
            start: Vec3::ZERO,
            dir: Vec3::NEG_Z,
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

#[cfg(test)]
mod hull_trace_tests {
    use super::*;

    fn box_prop(id: u32, pos: Vec3, half: f32) -> PropBody {
        let planes = vec![
            (Vec3::X, half),
            (-Vec3::X, half),
            (Vec3::Y, half),
            (-Vec3::Y, half),
            (Vec3::Z, half),
            (-Vec3::Z, half),
        ];
        PropBody {
            id,
            bone: 0,
            pos,
            angles: Vec3::ZERO,
            hulls: vec![planes],
        }
    }

    #[test]
    fn a_hull_sweep_hits_what_the_line_beside_the_prop_misses() {
        let props = [box_prop(7, Vec3::ZERO, 8.0)];
        let (start, end) = (Vec3::new(0.0, 20.0, -50.0), Vec3::new(0.0, 20.0, 50.0));
        assert!(sweep_hull_props(&props, start, end, Vec3::ZERO, Vec3::ZERO).is_none());
        let hit = sweep_hull_props(&props, start, end, Vec3::splat(-16.0), Vec3::splat(16.0))
            .expect("the 32 u box reaches the prop");
        assert_eq!(hit.ent, Some(7));
        // sweeping up, the box's top face (+16) meets the prop's bottom (z = -8) with the box centre
        // at z = -24 -> fraction 0.26
        assert!((hit.pos.z + 24.0).abs() < 0.5, "{hit:?}");
        assert!((hit.fraction - 0.26).abs() < 0.01, "{hit:?}");
        let inside =
            sweep_hull_props(&props, Vec3::ZERO, end, Vec3::splat(-1.0), Vec3::splat(1.0)).unwrap();
        assert_eq!((inside.fraction, inside.ent), (0.0, Some(7)));
    }
}
