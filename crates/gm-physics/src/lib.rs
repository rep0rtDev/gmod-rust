//! Rigid body physics for props on Jolt.
//!
//! Source units in, Source units out: positions in inches, Z up, angles as (pitch, yaw, roll)
//! degrees. Internally Jolt runs in metres with Y up (the same convention IVP used, so PHY
//! data maps one to one): `jolt = (x, z, -y) * 0.0254`.
//!
//! Jolt's default gravity is 9.81 m/s^2 along -Y; `sv_gravity` is applied per body through
//! the gravity factor, so 600 u/s^2 is `600 * 0.0254 / 9.81`.

pub mod brush;
pub mod physgun;
pub mod surface;

extern "C" {
    fn gmp_set_contact_settings(
        system: *mut JPC_PhysicsSystem,
        speculative: f32,
        slop: f32,
        linear_cast_threshold: f32,
    );
    fn gmp_get_speculative_distance(system: *const JPC_PhysicsSystem) -> f32;
    fn gmp_get_penetration_slop(system: *const JPC_PhysicsSystem) -> f32;
    fn gmp_add_static_mesh(
        system: *mut JPC_PhysicsSystem,
        xyz: *const f32,
        nverts: u32,
        idx: *const u32,
        ntris: u32,
        layer: u16,
        friction: f32,
    ) -> u32;
    fn gmp_add_fixed_constraint(
        system: *mut JPC_PhysicsSystem,
        id1: u32,
        id2: u32,
    ) -> *mut std::ffi::c_void;
    #[allow(clippy::too_many_arguments)]
    fn gmp_add_distance_constraint(
        system: *mut JPC_PhysicsSystem,
        id1: u32,
        id2: u32,
        x1: f32,
        y1: f32,
        z1: f32,
        x2: f32,
        y2: f32,
        z2: f32,
        min_d: f32,
        max_d: f32,
    ) -> *mut std::ffi::c_void;
    fn gmp_remove_constraint(system: *mut JPC_PhysicsSystem, constraint: *mut std::ffi::c_void);
    fn gmp_rope_predict(system: *mut JPC_PhysicsSystem, dt: f32);
    fn gmp_rope_clear(system: *mut JPC_PhysicsSystem);
    fn gmp_add_hinge_constraint(
        system: *mut JPC_PhysicsSystem,
        id1: u32,
        id2: u32,
        px: f32,
        py: f32,
        pz: f32,
        ax: f32,
        ay: f32,
        az: f32,
    ) -> *mut std::ffi::c_void;
    fn gmp_remove_body(system: *mut JPC_PhysicsSystem, id: u32);
    fn gmp_set_pose(
        system: *mut JPC_PhysicsSystem,
        id: u32,
        px: f32,
        py: f32,
        pz: f32,
        qx: f32,
        qy: f32,
        qz: f32,
        qw: f32,
    );
    fn gmp_install_impact_listener(system: *mut JPC_PhysicsSystem);
    fn gmp_set_surface(
        system: *mut JPC_PhysicsSystem,
        body_id: u32,
        friction: f32,
        elasticity: f32,
    );
    fn gmp_set_hull_points(
        system: *mut JPC_PhysicsSystem,
        body_id: u32,
        xyz: *const f32,
        count: u32,
    );
    fn gmp_set_inertia(
        system: *mut JPC_PhysicsSystem,
        body_id: u32,
        mass: f32,
        ix: f32,
        iy: f32,
        iz: f32,
    );
    fn gmp_set_impact_step(system: *mut JPC_PhysicsSystem, dt: f32);
    fn gmp_apply_impact_offsets(system: *mut JPC_PhysicsSystem);
    fn gmp_set_impact_skip(system: *mut JPC_PhysicsSystem, body_id: u32, on: i32);
    fn gmp_set_impact_enabled(system: *mut JPC_PhysicsSystem, enabled: i32);
    fn gmp_take_impacts(system: *mut JPC_PhysicsSystem, out: *mut RawImpact, max: u32) -> u32;
    fn gmp_set_pair_collision(system: *mut JPC_PhysicsSystem, id1: u32, id2: u32, enabled: i32);
    fn gmp_set_solver(
        system: *mut JPC_PhysicsSystem,
        baumgarte: f32,
        velocity_steps: u32,
        position_steps: u32,
        min_velocity_for_restitution: f32,
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstraintHandle(*mut std::ffi::c_void);

/// Jolt only predicts contacts this far ahead (metres); props cross up to 0.11 m per tick at
/// typical speeds and the game resolves the hit in the tick it happens, so the default 0.02 m
/// (which lands bodies a tick late, penetrating) is widened.
pub const SPECULATIVE_CONTACT_M: f32 = 0.15;

/// Fraction of a body's inner radius it may move in one step before Jolt sweeps it to its time
/// of impact (`PhysicsSettings::mLinearCastThreshold`, Jolt default 0.75). The game's
/// event-driven collision catches a contact anywhere inside the step; with Jolt's default a
/// crate falling 9 u per tick from 5.9 u above the floor passes the speculative distance and
/// lands a tick late. 0.03 to 0.25 give identical harness results, see docs/spec/physics.md.
pub const LINEAR_CAST_THRESHOLD: f32 = 0.1;

/// Speculative contact distance actually used (metres): `GMP_SPECULATIVE` overrides the
/// default for experiments (the game catches a contact anywhere inside the step, Jolt only
/// within this distance).
fn speculative_contact_m() -> f32 {
    std::env::var("GMP_SPECULATIVE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(SPECULATIVE_CONTACT_M)
}
pub const PENETRATION_SLOP_M: f32 = 0.02;

use glam::{Quat, Vec3};
use joltc_sys::*;
use rolt::{BroadPhaseLayer, ObjectLayer};
use std::ffi::CStr;
use std::ptr;
use std::sync::Once;

pub const UNITS_TO_METERS: f32 = 0.0254;
pub const METERS_TO_UNITS: f32 = 1.0 / UNITS_TO_METERS;
pub const JOLT_GRAVITY: f32 = 9.81;
/// Resting distance the game keeps between bodies and the world (IVP's 1 cm tolerance), units.
pub const REST_GAP: f32 = 0.01 * METERS_TO_UNITS;
const DEG: f32 = std::f32::consts::PI / 180.0;

const LAYER_STATIC: u16 = 0;
const LAYER_MOVING: u16 = 1;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("jolt shape: {0}")]
    Shape(String),
    #[error("prop has no convex hulls")]
    EmptyShape,
}

pub type Result<T> = std::result::Result<T, Error>;

/// Source (x, y, z up) in units to Jolt (x, y up, z) in metres.
pub fn to_jolt(v: Vec3) -> Vec3 {
    Vec3::new(v.x, v.z, -v.y) * UNITS_TO_METERS
}

pub fn from_jolt(v: Vec3) -> Vec3 {
    Vec3::new(v.x, -v.z, v.y) * METERS_TO_UNITS
}

/// Same axis swap for a direction or angular velocity (no scale).
pub fn dir_to_jolt(v: Vec3) -> Vec3 {
    Vec3::new(v.x, v.z, -v.y)
}

pub fn dir_from_jolt(v: Vec3) -> Vec3 {
    Vec3::new(v.x, -v.z, v.y)
}

/// Source angles (pitch, yaw, roll) in degrees to a rotation in Source axes.
pub fn quat_from_angles(pitch: f32, yaw: f32, roll: f32) -> Quat {
    Quat::from_rotation_z(yaw.to_radians())
        * Quat::from_rotation_y(pitch.to_radians())
        * Quat::from_rotation_x(roll.to_radians())
}

/// Inverse of [`quat_from_angles`], degrees, yaw in (-180, 180].
pub fn angles_from_quat(q: Quat) -> (f32, f32, f32) {
    let f = q * Vec3::X;
    let right = q * Vec3::NEG_Y;
    let up = q * Vec3::Z;
    let xy = (f.x * f.x + f.y * f.y).sqrt();
    let pitch = (-f.z).atan2(xy);
    let yaw = if xy > 1e-6 { f.y.atan2(f.x) } else { 0.0 };
    let right0 = Vec3::new(yaw.sin(), -yaw.cos(), 0.0);
    let roll = up.dot(right0).atan2(right.dot(right0));
    (pitch.to_degrees(), yaw.to_degrees(), roll.to_degrees())
}

fn quat_to_jolt(q: Quat) -> Quat {
    // conjugate the rotation by the axis swap S: v' = S v, R' = S R S^-1; S is a rotation
    // (x, y, z) -> (x, z, -y), i.e. -90 degrees about X.
    let s = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    s * q * s.inverse()
}

fn quat_from_jolt(q: Quat) -> Quat {
    let s = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    s.inverse() * q * s
}

fn jv(v: Vec3) -> JPC_Vec3 {
    JPC_Vec3 {
        x: v.x,
        y: v.y,
        z: v.z,
        _w: 0.0,
    }
}

fn gv(v: JPC_Vec3) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

fn jq(q: Quat) -> JPC_Quat {
    JPC_Quat {
        x: q.x,
        y: q.y,
        z: q.z,
        w: q.w,
    }
}

fn gq(q: JPC_Quat) -> Quat {
    Quat::from_xyzw(q.x, q.y, q.z, q.w)
}

static INIT: Once = Once::new();

fn init_jolt() {
    INIT.call_once(|| {
        rolt::register_default_allocator();
        rolt::factory_init();
        rolt::register_types();
    });
}

struct Layers;

impl rolt::BroadPhaseLayerInterface for Layers {
    fn get_num_broad_phase_layers(&self) -> u32 {
        2
    }
    fn get_broad_phase_layer(&self, layer: ObjectLayer) -> BroadPhaseLayer {
        BroadPhaseLayer::new(layer.raw() as u8)
    }
}

impl rolt::ObjectVsBroadPhaseLayerFilter for Layers {
    fn should_collide(&self, layer1: ObjectLayer, layer2: BroadPhaseLayer) -> bool {
        layer1.raw() == LAYER_MOVING || layer2.raw() == LAYER_MOVING as u8
    }
}

impl rolt::ObjectLayerPairFilter for Layers {
    fn should_collide(&self, layer1: ObjectLayer, layer2: ObjectLayer) -> bool {
        layer1.raw() == LAYER_MOVING || layer2.raw() == LAYER_MOVING
    }
}

/// Material response of a body, Source `surfaceprop` numbers.
///
/// Measured: a crate sliding on concrete decelerates at 0.64 g, the product of the two
/// frictions (0.8 * 0.8). Jolt combines friction as `sqrt(f1 * f2)`, so each body carries its
/// friction squared and the pair comes out as the product.
///
/// Measured: a wood crate (elasticity 0.1) on the concrete floor (0.2) rebounds with 0.15 of
/// its speed, so the game averages the two elasticities. Jolt takes the max of the pair, so
/// the world body carries 0 and every prop carries the average of its own elasticity and the
/// world's; prop against prop then gets the larger of two averages, close to the game.
#[derive(Debug, Clone, Copy)]
pub struct Surface {
    pub friction: f32,
    pub elasticity: f32,
}

impl Default for Surface {
    fn default() -> Self {
        Surface {
            friction: 0.8,
            elasticity: 0.25,
        }
    }
}

/// A prop to add: convex hulls (Source units, body local), mass in kg and the solid's volume
/// in cubic units (the PHY text section carries it); Jolt derives mass from density, so the
/// density is `mass / volume`.
pub struct PropDesc<'a> {
    pub hulls: &'a [Vec<Vec3>],
    pub mass: f32,
    pub volume: f32,
    pub position: Vec3,
    pub angles: (f32, f32, f32),
    pub velocity: Vec3,
    pub angular_velocity: Vec3,
    pub surface: Surface,
    pub linear_damping: f32,
    pub angular_damping: f32,
    /// PHY header `dragAxisAreas` in Source axis order: projected hull area over the
    /// collision box face area per axis (1 for a box, about 0.77 across a cylinder).
    pub drag_axis_areas: Vec3,
    /// Rotation inertia per unit mass about the mass centre in Source axes (PHY
    /// `rotation_inertia`; the game's `GetInertia` is this times the mass). `None` lets Jolt
    /// derive it from the hull (about 1.4 times larger for the crate).
    pub inertia: Option<Vec3>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodyState {
    /// Source units.
    pub position: Vec3,
    pub rotation: Quat,
    pub angles: (f32, f32, f32),
    pub velocity: Vec3,
    /// Degrees per second about Source axes.
    pub angular_velocity: Vec3,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BodyHandle(u32);

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RawImpact {
    id1: u32,
    id2: u32,
    px: f32,
    py: f32,
    pz: f32,
    speed: f32,
}

/// A resolved first contact of a body pair in one step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Impact {
    pub bodies: [BodyHandle; 2],
    /// Contact point, Source units.
    pub point: Vec3,
    /// Approach speed along the contact normal before the impact, units/s.
    pub speed: f32,
}

struct Drag {
    body: u32,
    /// Projected face areas of the local collision box over mass, scaled so that
    /// `a = -(sum_i areas.i * |dir.i|) * |v| * v` is in units per second squared.
    areas: Vec3,
    /// Angular drag per body axis (1/rad): `dw_i = -ang_i * w_i |w_i| dt`, see `angular_drag_factors`.
    ang: Vec3,
}

pub struct World {
    system: rolt::PhysicsSystem,
    drags: Vec<Drag>,
    held: std::collections::HashSet<u32>,
    frozen: std::collections::HashSet<u32>,
    world_elasticity: f32,
    temp: *mut JPC_TempAllocatorImpl,
    jobs: *mut JPC_JobSystemThreadPool,
    shapes: Vec<*mut JPC_Shape>,
    pub gravity: f32,
    pub tick_interval: f32,
}

// Jolt's system is internally thread safe for the operations exposed here; the raw pointers
// are owned by this struct.
unsafe impl Send for World {}

impl World {
    pub fn new(gravity: f32, tick_interval: f32, threads: i32) -> Self {
        init_jolt();
        let mut system = rolt::PhysicsSystem::new();
        system.init(10240, 0, 65536, 20480, Layers, Layers, Layers);
        unsafe {
            let raw = system.as_raw();
            // layout check: the shim reads Jolt's defaults back before changing them
            debug_assert!((gmp_get_speculative_distance(raw) - 0.02).abs() < 1e-6);
            debug_assert!((gmp_get_penetration_slop(raw) - 0.02).abs() < 1e-6);
            gmp_set_contact_settings(
                raw,
                speculative_contact_m(),
                PENETRATION_SLOP_M,
                LINEAR_CAST_THRESHOLD,
            );
            gmp_install_impact_listener(raw);
            gmp_set_impact_step(raw, tick_interval);
        }
        let (temp, jobs) = unsafe {
            (
                JPC_TempAllocatorImpl_new(32 * 1024 * 1024),
                JPC_JobSystemThreadPool_new3(2048, 8, threads),
            )
        };
        World {
            system,
            temp,
            jobs,
            shapes: Vec::new(),
            drags: Vec::new(),
            held: std::collections::HashSet::new(),
            frozen: std::collections::HashSet::new(),
            world_elasticity: 0.2,
            gravity,
            tick_interval,
        }
    }

    fn gravity_factor(&self) -> f32 {
        self.gravity * UNITS_TO_METERS / JOLT_GRAVITY
    }

    fn convex_shape(&mut self, points: &[Vec3], density: f32) -> Result<*mut JPC_Shape> {
        let pts: Vec<JPC_Vec3> = points.iter().map(|&p| jv(to_jolt(p))).collect();
        unsafe {
            let mut settings: JPC_ConvexHullShapeSettings = std::mem::zeroed();
            JPC_ConvexHullShapeSettings_default(&mut settings);
            settings.Points = pts.as_ptr();
            settings.PointsLen = pts.len();
            settings.Density = density;
            // IVP hulls have sharp corners; Jolt's default 5 cm rounding would lift a tilted
            // crate's first contact by about 1 u
            settings.MaxConvexRadius = 0.005;
            self.create_shape(|out, err| JPC_ConvexHullShapeSettings_Create(&settings, out, err))
        }
    }

    unsafe fn create_shape(
        &mut self,
        f: impl FnOnce(*mut *mut JPC_Shape, *mut *mut JPC_String) -> bool,
    ) -> Result<*mut JPC_Shape> {
        let mut shape: *mut JPC_Shape = ptr::null_mut();
        let mut err: *mut JPC_String = ptr::null_mut();
        if f(&mut shape, &mut err) && !shape.is_null() {
            self.shapes.push(shape);
            Ok(shape)
        } else {
            let msg = if err.is_null() {
                "unknown".to_string()
            } else {
                let s = CStr::from_ptr(JPC_String_c_str(err))
                    .to_string_lossy()
                    .into_owned();
                JPC_String_delete(err);
                s
            };
            Err(Error::Shape(msg))
        }
    }

    fn compound(&mut self, shapes: &[*mut JPC_Shape]) -> Result<*mut JPC_Shape> {
        if shapes.len() == 1 {
            return Ok(shapes[0]);
        }
        let subs: Vec<JPC_SubShapeSettings> = shapes
            .iter()
            .map(|&s| JPC_SubShapeSettings {
                Shape: s,
                __bindgen_padding_0: 0,
                Position: jv(Vec3::ZERO),
                Rotation: jq(Quat::IDENTITY),
                UserData: 0,
            })
            .collect();
        unsafe {
            let mut settings: JPC_StaticCompoundShapeSettings = std::mem::zeroed();
            JPC_StaticCompoundShapeSettings_default(&mut settings);
            settings.SubShapes = subs.as_ptr();
            settings.SubShapesLen = subs.len();
            self.create_shape(|out, err| {
                JPC_StaticCompoundShapeSettings_Create(&settings, out, err)
            })
        }
    }

    /// Static world geometry from convex hulls (Source units, world space).
    pub fn add_static_hulls(
        &mut self,
        hulls: &[Vec<Vec3>],
        surface: Surface,
    ) -> Result<BodyHandle> {
        let mut parts = Vec::with_capacity(hulls.len());
        for h in hulls {
            parts.push(self.convex_shape(h, 1000.0)?);
        }
        if parts.is_empty() {
            return Err(Error::EmptyShape);
        }
        let shape = self.compound(&parts)?;
        unsafe {
            let mut settings: JPC_BodyCreationSettings = std::mem::zeroed();
            JPC_BodyCreationSettings_default(&mut settings);
            settings.Position = jv(Vec3::ZERO);
            settings.Rotation = jq(Quat::IDENTITY);
            settings.ObjectLayer = LAYER_STATIC;
            settings.MotionType = JPC_MOTION_TYPE_STATIC;
            settings.Friction = surface.friction * surface.friction;
            settings.Restitution = 0.0;
            self.world_elasticity = surface.elasticity;
            settings.Shape = shape;
            let bi = JPC_PhysicsSystem_GetBodyInterface(self.system.as_raw());
            let id =
                JPC_BodyInterface_CreateAndAddBody(bi, &settings, JPC_ACTIVATION_DONT_ACTIVATE);
            gmp_set_surface(
                self.system.as_raw(),
                id,
                surface.friction,
                surface.elasticity,
            );
            Ok(BodyHandle(id))
        }
    }

    /// Static triangle soup (displacement ground), Source units and axes like the hulls.
    pub fn add_static_mesh(
        &mut self,
        positions: &[Vec3],
        indices: &[u32],
        surface: Surface,
    ) -> Result<BodyHandle> {
        if positions.is_empty() || indices.len() < 3 {
            return Err(Error::EmptyShape);
        }
        let mut xyz = Vec::with_capacity(positions.len() * 3);
        for &p in positions {
            let q = to_jolt(p);
            xyz.extend_from_slice(&[q.x, q.y, q.z]);
        }
        let id = unsafe {
            gmp_add_static_mesh(
                self.system.as_raw(),
                xyz.as_ptr(),
                positions.len() as u32,
                indices.as_ptr(),
                (indices.len() / 3) as u32,
                LAYER_STATIC,
                surface.friction * surface.friction,
            )
        };
        if id == u32::MAX {
            return Err(Error::EmptyShape);
        }
        unsafe {
            gmp_set_surface(
                self.system.as_raw(),
                id,
                surface.friction,
                surface.elasticity,
            )
        };
        Ok(BodyHandle(id))
    }

    /// Measured on the real game: a falling crate loses `k * v^2` of acceleration with
    /// `k = A / m` where A is the body's projected area in m^2 along the axis of motion and m
    /// its mass (`drag_coefficient` 1): see docs/spec/physics.md.
    fn drag_factors(hulls: &[Vec<Vec3>], mass: f32, drag_axis_areas: Vec3) -> Vec3 {
        let mut mins = Vec3::splat(f32::MAX);
        let mut maxs = Vec3::splat(f32::MIN);
        for p in hulls.iter().flatten() {
            mins = mins.min(*p);
            maxs = maxs.max(*p);
        }
        let e = (maxs - mins).max(Vec3::ZERO) * UNITS_TO_METERS;
        let area = Vec3::new(e.y * e.z, e.x * e.z, e.x * e.y);
        area * drag_axis_areas / mass.max(1e-3) * UNITS_TO_METERS
    }

    /// Measured on the real game (props spun in free fall about each axis at 200 and 400
    /// deg/s): the angular speed about body axis i decays as `dw_i/dt = -k_i w_i |w_i|` with
    /// `k_i = (1/16) / I_i * (a_k T(d_i, d_j, d_k) + a_j T(d_i, d_k, d_j))`,
    /// `T(l, w, h) = w^2 l^3 / 3 + w^4 l / 2 + l w^2 h^2`, d the hull box in metres, a the PHY
    /// `dragAxisAreas`, I the body inertia in kg m^2; 7 measurements on 3 bodies agree within
    /// 0.1 % (docs/spec/physics.md).
    fn angular_drag_factors(
        hulls: &[Vec<Vec3>],
        inertia: Option<Vec3>,
        mass: f32,
        a: Vec3,
    ) -> Vec3 {
        let Some(i) = inertia else {
            return Vec3::ZERO;
        };
        let mut mins = Vec3::splat(f32::MAX);
        let mut maxs = Vec3::splat(f32::MIN);
        for p in hulls.iter().flatten() {
            mins = mins.min(*p);
            maxs = maxs.max(*p);
        }
        let d = (maxs - mins).max(Vec3::ZERO) * UNITS_TO_METERS;
        let t = |l: f32, w: f32, h: f32| {
            w * w * l * l * l / 3.0 + w * w * w * w * l / 2.0 + l * w * w * h * h
        };
        let inertia = i * mass;
        let axis = |di: f32, dj: f32, dk: f32, aj: f32, ak: f32, ii: f32| {
            if ii <= 0.0 {
                0.0
            } else {
                (ak * t(di, dj, dk) + aj * t(di, dk, dj)) / (16.0 * ii)
            }
        };
        Vec3::new(
            axis(d.x, d.y, d.z, a.y, a.z, inertia.x),
            axis(d.y, d.x, d.z, a.x, a.z, inertia.y),
            axis(d.z, d.x, d.y, a.x, a.y, inertia.z),
        )
    }

    pub fn add_prop(&mut self, desc: &PropDesc<'_>) -> Result<BodyHandle> {
        let volume_m3 = desc.volume.max(1.0) * UNITS_TO_METERS * UNITS_TO_METERS * UNITS_TO_METERS;
        let density = desc.mass / volume_m3;
        let mut parts = Vec::with_capacity(desc.hulls.len());
        for h in desc.hulls {
            parts.push(self.convex_shape(h, density)?);
        }
        if parts.is_empty() {
            return Err(Error::EmptyShape);
        }
        let shape = self.compound(&parts)?;
        let rot = quat_from_angles(desc.angles.0, desc.angles.1, desc.angles.2);
        unsafe {
            let mut settings: JPC_BodyCreationSettings = std::mem::zeroed();
            JPC_BodyCreationSettings_default(&mut settings);
            settings.Position = jv(to_jolt(desc.position));
            settings.Rotation = jq(quat_to_jolt(rot));
            settings.LinearVelocity = jv(to_jolt(desc.velocity));
            settings.AngularVelocity = jv(dir_to_jolt(desc.angular_velocity) * DEG);
            settings.ObjectLayer = LAYER_MOVING;
            settings.MotionType = JPC_MOTION_TYPE_DYNAMIC;
            // props cross several units per tick; a discrete step lands them a tick late
            settings.MotionQuality = JPC_MOTION_QUALITY_LINEAR_CAST;
            settings.Friction = desc.surface.friction * desc.surface.friction;
            settings.Restitution = 0.5 * (desc.surface.elasticity + self.world_elasticity);
            settings.ApplyGyroscopicForce = true;
            settings.LinearDamping = desc.linear_damping;
            settings.AngularDamping = desc.angular_damping;
            settings.GravityFactor = self.gravity_factor();
            settings.Shape = shape;
            let bi = JPC_PhysicsSystem_GetBodyInterface(self.system.as_raw());
            let id = JPC_BodyInterface_CreateAndAddBody(bi, &settings, JPC_ACTIVATION_ACTIVATE);
            JPC_BodyInterface_SetMotionQuality(bi, id, JPC_MOTION_QUALITY_LINEAR_CAST);
            if std::env::var_os("GMP_DRAG_DEBUG").is_some() {
                let mut mins = Vec3::splat(f32::MAX);
                let mut maxs = Vec3::splat(f32::MIN);
                for p in desc.hulls.iter().flatten() {
                    mins = mins.min(*p);
                    maxs = maxs.max(*p);
                }
                eprintln!(
                    "drag body mass {} axis_areas {:?} inertia {:?} hull_box {:?}",
                    desc.mass,
                    desc.drag_axis_areas,
                    desc.inertia,
                    maxs - mins
                );
            }
            self.drags.push(Drag {
                body: id,
                areas: Self::drag_factors(desc.hulls, desc.mass, desc.drag_axis_areas),
                ang: Self::angular_drag_factors(
                    desc.hulls,
                    desc.inertia,
                    desc.mass,
                    desc.drag_axis_areas,
                ),
            });
            gmp_set_surface(
                self.system.as_raw(),
                id,
                desc.surface.friction,
                desc.surface.elasticity,
            );
            let verts: Vec<f32> = desc
                .hulls
                .iter()
                .flat_map(|h| h.iter().flat_map(|&p| to_jolt(p).to_array()))
                .collect();
            gmp_set_hull_points(
                self.system.as_raw(),
                id,
                verts.as_ptr(),
                (verts.len() / 3) as u32,
            );
            if let Some(i) = desc.inertia {
                gmp_set_inertia(
                    self.system.as_raw(),
                    id,
                    desc.mass,
                    i.x * desc.mass,
                    i.z * desc.mass,
                    i.y * desc.mass,
                );
            }
            Ok(BodyHandle(id))
        }
    }

    /// Drive a held body toward `target_pos` / `target_rot` for the coming tick (gravity off).
    pub fn hold(
        &mut self,
        body: BodyHandle,
        target_pos: Vec3,
        target_rot: Quat,
        p: &physgun::PhysgunParams,
    ) {
        let st = self.state(body);
        let v = physgun::controller(st.velocity, target_pos - st.position, p);
        let w = physgun::angular_controller(st.angular_velocity, st.rotation, target_rot, p);
        unsafe {
            let bi = JPC_PhysicsSystem_GetBodyInterface(self.system.as_raw());
            JPC_BodyInterface_SetGravityFactor(bi, body.0, 0.0);
            JPC_BodyInterface_SetLinearAndAngularVelocity(
                bi,
                body.0,
                jv(to_jolt(v)),
                jv(dir_to_jolt(w) * DEG),
            );
            JPC_BodyInterface_ActivateBody(bi, body.0);
        }
        self.held.insert(body.0);
        unsafe { gmp_set_impact_skip(self.system.as_raw(), body.0, 1) };
    }

    /// Let a held body go: gravity is back from this tick on, the velocity is kept.
    pub fn release(&mut self, body: BodyHandle) {
        unsafe { gmp_set_impact_skip(self.system.as_raw(), body.0, 0) };
        if self.held.remove(&body.0) {
            unsafe {
                let bi = JPC_PhysicsSystem_GetBodyInterface(self.system.as_raw());
                JPC_BodyInterface_SetGravityFactor(bi, body.0, self.gravity_factor());
            }
        }
    }

    /// Physgun freeze: the body stops where it is and ignores forces until unfrozen.
    pub fn freeze(&mut self, body: BodyHandle) {
        self.held.remove(&body.0);
        unsafe {
            let bi = JPC_PhysicsSystem_GetBodyInterface(self.system.as_raw());
            JPC_BodyInterface_SetLinearAndAngularVelocity(
                bi,
                body.0,
                jv(Vec3::ZERO),
                jv(Vec3::ZERO),
            );
            JPC_BodyInterface_SetMotionType(
                bi,
                body.0,
                JPC_MOTION_TYPE_KINEMATIC,
                JPC_ACTIVATION_DONT_ACTIVATE,
            );
        }
        self.frozen.insert(body.0);
    }

    pub fn unfreeze(&mut self, body: BodyHandle) {
        if self.frozen.remove(&body.0) {
            unsafe {
                let bi = JPC_PhysicsSystem_GetBodyInterface(self.system.as_raw());
                JPC_BodyInterface_SetMotionType(
                    bi,
                    body.0,
                    JPC_MOTION_TYPE_DYNAMIC,
                    JPC_ACTIVATION_ACTIVATE,
                );
                JPC_BodyInterface_SetGravityFactor(bi, body.0, self.gravity_factor());
            }
        }
    }

    pub fn is_frozen(&self, body: BodyHandle) -> bool {
        self.frozen.contains(&body.0)
    }

    /// Weld: the two bodies keep their current relative pose.
    pub fn weld(&mut self, a: BodyHandle, b: BodyHandle) -> Option<ConstraintHandle> {
        let c = unsafe { gmp_add_fixed_constraint(self.system.as_raw(), a.0, b.0) };
        (!c.is_null()).then_some(ConstraintHandle(c))
    }

    /// Rope between two world space points (Source units), slack up to `length`.
    pub fn rope(
        &mut self,
        a: BodyHandle,
        b: BodyHandle,
        pa: Vec3,
        pb: Vec3,
        length: f32,
        rigid: bool,
    ) -> Option<ConstraintHandle> {
        let (ja, jb) = (to_jolt(pa), to_jolt(pb));
        let max = length * UNITS_TO_METERS;
        let min = if rigid { max } else { 0.0 };
        let c = unsafe {
            gmp_add_distance_constraint(
                self.system.as_raw(),
                a.0,
                b.0,
                ja.x,
                ja.y,
                ja.z,
                jb.x,
                jb.y,
                jb.z,
                min,
                max,
            )
        };
        (!c.is_null()).then_some(ConstraintHandle(c))
    }

    /// Hinge (`phys_hinge`) through a world space point along a world axis (Source units).
    pub fn hinge(
        &mut self,
        a: BodyHandle,
        b: BodyHandle,
        point: Vec3,
        axis: Vec3,
    ) -> Option<ConstraintHandle> {
        let p = to_jolt(point);
        let ax = to_jolt(axis + Vec3::ZERO) - to_jolt(Vec3::ZERO);
        let c = unsafe {
            gmp_add_hinge_constraint(
                self.system.as_raw(),
                a.0,
                b.0,
                p.x,
                p.y,
                p.z,
                ax.x,
                ax.y,
                ax.z,
            )
        };
        (!c.is_null()).then_some(ConstraintHandle(c))
    }

    /// `PhysObj:SetVelocity` + `AddAngleVelocity`: Source units per second and body-local
    /// degrees per second.
    pub fn set_velocity(&mut self, body: BodyHandle, velocity: Vec3, angular: Vec3) {
        unsafe {
            let bi = JPC_PhysicsSystem_GetBodyInterface(self.system.as_raw());
            JPC_BodyInterface_SetLinearAndAngularVelocity(
                bi,
                body.0,
                jv(to_jolt(velocity)),
                jv(dir_to_jolt(angular) * DEG),
            );
        }
    }

    /// `PhysObj:ApplyForceOffset` with a damage force: an impulse in kg * units/s at a world
    /// point (Source units), waking the body; a frozen body ignores it like the game's.
    pub fn add_impulse_at(&mut self, body: BodyHandle, point: Vec3, impulse: Vec3) {
        if self.frozen.contains(&body.0) {
            return;
        }
        let i = dir_to_jolt(impulse) * UNITS_TO_METERS;
        let p = to_jolt(point);
        unsafe {
            let bi = JPC_PhysicsSystem_GetBodyInterface(self.system.as_raw());
            JPC_BodyInterface_ActivateBody(bi, body.0);
            JPC_BodyInterface_AddImpulse3(bi, body.0, jv(i), jv(p));
        }
    }

    /// `PhysObj:SetPos` + `SetAngles`: moves a body at rest to a pose (Source units, Z up).
    pub fn teleport(&mut self, body: BodyHandle, position: Vec3, rotation: Quat) {
        let p = to_jolt(position);
        let q = quat_to_jolt(rotation);
        unsafe {
            gmp_set_pose(
                self.system.as_raw(),
                body.0,
                p.x,
                p.y,
                p.z,
                q.x,
                q.y,
                q.z,
                q.w,
            )
        }
    }

    /// Removes a prop from the world (constraints attached to it must be removed first).
    pub fn remove_body(&mut self, body: BodyHandle) {
        self.release(body);
        self.frozen.remove(&body.0);
        unsafe { gmp_remove_body(self.system.as_raw(), body.0) }
    }

    pub fn remove_constraint(&mut self, c: ConstraintHandle) {
        unsafe { gmp_remove_constraint(self.system.as_raw(), c.0) }
    }

    /// Solver tuning: Baumgarte position correction factor, velocity and position iterations,
    /// minimum relative velocity for restitution (m/s).
    pub fn set_solver(
        &mut self,
        baumgarte: f32,
        velocity_steps: u32,
        position_steps: u32,
        min_restitution_velocity: f32,
    ) {
        unsafe {
            gmp_set_solver(
                self.system.as_raw(),
                baumgarte,
                velocity_steps,
                position_steps,
                min_restitution_velocity,
            )
        }
    }

    /// Toggle the IVP style single point impact response (on by default).
    /// `constraint.NoCollide`: whether two bodies collide with each other (default yes).
    pub fn set_pair_collision(&mut self, a: BodyHandle, b: BodyHandle, enabled: bool) {
        unsafe { gmp_set_pair_collision(self.system.as_raw(), a.0, b.0, enabled as i32) }
    }

    pub fn set_impact_enabled(&mut self, enabled: bool) {
        unsafe { gmp_set_impact_enabled(self.system.as_raw(), enabled as i32) }
    }

    pub fn is_held(&self, body: BodyHandle) -> bool {
        self.held.contains(&body.0)
    }

    pub fn optimize(&self) {
        self.system.optimize_broad_phase();
    }

    /// Measured on the real game (docs/spec/physics.md): the body loses `k |v| v dt` from the
    /// velocity gravity gives it this tick, before the position integrates; so it is taken off
    /// ahead of the Jolt step from the predicted velocity, and the loss is with `k = A(dir) / m`, A(dir) the collision box's projected area along
    /// the velocity, `sum_i A_i |dir_i|` in body axes (a tilted crate drags 1.43 times more).
    fn apply_drag(&self) {
        let dt = self.tick_interval;
        unsafe {
            let bi = JPC_PhysicsSystem_GetBodyInterface(self.system.as_raw());
            for d in &self.drags {
                if !JPC_BodyInterface_IsActive(bi, d.body)
                    || self.held.contains(&d.body)
                    || self.frozen.contains(&d.body)
                {
                    continue;
                }
                let mut p = jv(Vec3::ZERO);
                let mut r = jq(Quat::IDENTITY);
                JPC_BodyInterface_GetPositionAndRotation(bi, d.body, &mut p, &mut r);
                let rot = quat_from_jolt(gq(r)).normalize();
                let mut lv = jv(Vec3::ZERO);
                let mut av = jv(Vec3::ZERO);
                JPC_BodyInterface_GetLinearAndAngularVelocity(bi, d.body, &mut lv, &mut av);
                let v = from_jolt(gv(lv));
                if d.ang != Vec3::ZERO {
                    let w = rot.inverse() * dir_from_jolt(gv(av));
                    let loss = d.ang * w * w.abs() * dt;
                    let w = w - loss.clamp(-w.abs(), w.abs());
                    JPC_BodyInterface_SetAngularVelocity(bi, d.body, jv(dir_to_jolt(rot * w)));
                }
                let predicted = v + Vec3::new(0.0, 0.0, -self.gravity * dt);
                let speed = predicted.length();
                if speed < 1e-3 {
                    continue;
                }
                let dir = (rot.inverse() * predicted) / speed;
                let k = d.areas.dot(dir.abs());
                let loss = predicted * (k * speed * dt).min(1.0);
                JPC_BodyInterface_SetLinearVelocity(bi, d.body, jv(to_jolt(v - loss)));
            }
        }
    }

    /// The first contacts the impact response resolved since the last call: which pairs hit,
    /// where and how fast they approached (the input of the impact sound rule).
    pub fn take_impacts(&mut self) -> Vec<Impact> {
        let mut raw = [RawImpact::default(); 256];
        let n =
            unsafe { gmp_take_impacts(self.system.as_raw(), raw.as_mut_ptr(), raw.len() as u32) };
        raw[..n as usize]
            .iter()
            .map(|r| Impact {
                bodies: [BodyHandle(r.id1), BodyHandle(r.id2)],
                point: from_jolt(Vec3::new(r.px, r.py, r.pz)),
                speed: r.speed * METERS_TO_UNITS,
            })
            .collect()
    }

    /// Advance one server tick.
    pub fn step(&mut self) {
        self.apply_drag();
        unsafe {
            gmp_rope_predict(self.system.as_raw(), self.tick_interval);
            self.system
                .update(self.tick_interval, 1, self.temp, self.jobs);
            gmp_apply_impact_offsets(self.system.as_raw());
        }
    }

    pub fn state(&self, body: BodyHandle) -> BodyState {
        unsafe {
            let bi = JPC_PhysicsSystem_GetBodyInterface(self.system.as_raw());
            let mut p = jv(Vec3::ZERO);
            let mut r = jq(Quat::IDENTITY);
            JPC_BodyInterface_GetPositionAndRotation(bi, body.0, &mut p, &mut r);
            let mut lv = jv(Vec3::ZERO);
            let mut av = jv(Vec3::ZERO);
            JPC_BodyInterface_GetLinearAndAngularVelocity(bi, body.0, &mut lv, &mut av);
            let rotation = quat_from_jolt(gq(r)).normalize();
            BodyState {
                position: from_jolt(gv(p)),
                rotation,
                angles: angles_from_quat(rotation),
                velocity: from_jolt(gv(lv)),
                angular_velocity: dir_from_jolt(gv(av)) / DEG,
                active: JPC_BodyInterface_IsActive(bi, body.0),
            }
        }
    }
}

impl Drop for World {
    fn drop(&mut self) {
        unsafe { gmp_rope_clear(self.system.as_raw()) };
        unsafe {
            JPC_JobSystemThreadPool_delete(self.jobs);
            JPC_TempAllocatorImpl_delete(self.temp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_hull(c: Vec3, h: Vec3) -> Vec<Vec3> {
        let mut v = Vec::new();
        for sx in [-1.0, 1.0] {
            for sy in [-1.0, 1.0] {
                for sz in [-1.0, 1.0] {
                    v.push(c + Vec3::new(sx * h.x, sy * h.y, sz * h.z));
                }
            }
        }
        v
    }

    #[test]
    fn contact_settings_reach_jolt() {
        let w = World::new(600.0, 0.015, 1);
        unsafe {
            let raw = w.system.as_raw();
            assert!((gmp_get_speculative_distance(raw) - speculative_contact_m()).abs() < 1e-6);
            assert!((gmp_get_penetration_slop(raw) - PENETRATION_SLOP_M).abs() < 1e-6);
        }
    }

    #[test]
    fn angles_roundtrip() {
        for (p, y, r) in [(0.0, 0.0, 0.0), (20.0, 35.0, 10.0), (-45.0, 170.0, -30.0)] {
            let (p2, y2, r2) = angles_from_quat(quat_from_angles(p, y, r));
            assert!(
                (p - p2).abs() < 1e-3 && (y - y2).abs() < 1e-3 && (r - r2).abs() < 1e-3,
                "{p} {y} {r} -> {p2} {y2} {r2}"
            );
        }
    }

    #[test]
    fn axis_swap_is_a_rotation() {
        let q = quat_from_angles(20.0, 35.0, 10.0);
        let v = Vec3::new(1.0, 2.0, 3.0);
        let a = to_jolt(q * v);
        let b = quat_to_jolt(q) * to_jolt(v);
        assert!(a.distance(b) < 1e-5, "{a} {b}");
        assert!((quat_from_jolt(quat_to_jolt(q)).dot(q).abs() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn nocollided_crate_falls_through_a_frozen_one() {
        let mut w = World::new(600.0, 0.015, 1);
        let floor = box_hull(Vec3::new(0.0, 0.0, -160.0), Vec3::new(512.0, 512.0, 16.0));
        w.add_static_hulls(&[floor], Surface::default()).unwrap();
        let hull = box_hull(Vec3::ZERO, Vec3::splat(20.0));
        let desc = |z: f32| PropDesc {
            hulls: std::slice::from_ref(&hull),
            mass: 30.0,
            volume: 64000.0,
            position: Vec3::new(0.0, 0.0, z),
            angles: (0.0, 0.0, 0.0),
            velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            surface: Surface::default(),
            linear_damping: 0.0,
            angular_damping: 0.0,
            drag_axis_areas: Vec3::ONE,
            inertia: None,
        };
        let lower = w.add_prop(&desc(-124.0)).unwrap();
        w.freeze(lower);
        let upper = w.add_prop(&desc(-40.0)).unwrap();
        w.set_pair_collision(lower, upper, false);
        w.optimize();
        for _ in 0..200 {
            w.step();
        }
        let s = w.state(upper);
        assert!(
            s.position.z < -110.0,
            "no-collided crate should pass the frozen one, z {}",
            s.position.z
        );
        assert!(
            s.position.z > -130.0,
            "but rest on the floor, z {}",
            s.position.z
        );
    }

    #[test]
    fn crate_falls_and_rests_on_floor() {
        let mut w = World::new(600.0, 0.015, 1);
        let floor = box_hull(Vec3::new(0.0, 0.0, -160.0), Vec3::new(512.0, 512.0, 16.0));
        w.add_static_hulls(&[floor], Surface::default()).unwrap();
        let hull = box_hull(Vec3::ZERO, Vec3::splat(20.0));
        let b = w
            .add_prop(&PropDesc {
                hulls: &[hull],
                mass: 30.0,
                volume: 64000.0,
                position: Vec3::new(0.0, 0.0, -40.0),
                angles: (0.0, 0.0, 0.0),
                velocity: Vec3::ZERO,
                angular_velocity: Vec3::ZERO,
                surface: Surface::default(),
                linear_damping: 0.0,
                angular_damping: 0.0,
                drag_axis_areas: Vec3::ONE,
                inertia: None,
            })
            .unwrap();
        w.optimize();
        w.step();
        let s = w.state(b);
        assert!(
            (s.velocity.z + 9.0).abs() < 0.05,
            "first tick gravity {}",
            s.velocity.z
        );
        for _ in 0..29 {
            w.step();
        }
        // measured: the real crate is at -260.4 after 30 ticks, free fall would give -270
        let s = w.state(b);
        assert!(
            (s.velocity.z + 260.4).abs() < 2.0,
            "drag after 30 ticks {}",
            s.velocity.z
        );
        for _ in 0..300 {
            w.step();
        }
        let s = w.state(b);
        assert!(
            (s.position.z + 124.0).abs() < 1.0,
            "rest height {}",
            s.position.z
        );
        assert!(s.velocity.length() < 1.0);
    }
}
