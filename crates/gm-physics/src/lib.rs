//! Rigid body physics for props on Jolt.
//!
//! Source units in, Source units out: positions in inches, Z up, angles as (pitch, yaw, roll)
//! degrees. Internally Jolt runs in metres with Y up (the same convention IVP used, so PHY
//! data maps one to one): `jolt = (x, z, -y) * 0.0254`.
//!
//! Jolt's default gravity is 9.81 m/s^2 along -Y; `sv_gravity` is applied per body through
//! the gravity factor, so 600 u/s^2 is `600 * 0.0254 / 9.81`.

pub mod brush;
pub mod surface;

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
    /// Source `drag_coefficient`, 1 for props.
    pub drag_coefficient: f32,
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

struct Drag {
    body: u32,
    /// Local axis drag factors `k` in 1/units: `a = -k * |v| * v` along each body axis.
    linear: Vec3,
}

pub struct World {
    system: rolt::PhysicsSystem,
    drags: Vec<Drag>,
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
            settings.Friction = surface.friction;
            settings.Restitution = 0.0;
            self.world_elasticity = surface.elasticity;
            settings.Shape = shape;
            let bi = JPC_PhysicsSystem_GetBodyInterface(self.system.as_raw());
            let id =
                JPC_BodyInterface_CreateAndAddBody(bi, &settings, JPC_ACTIVATION_DONT_ACTIVATE);
            Ok(BodyHandle(id))
        }
    }

    /// Measured on the real game: a falling crate loses `k * v^2` of acceleration with
    /// `k = A / m` where A is the body's projected area in m^2 along the axis of motion and m
    /// its mass (`drag_coefficient` 1): see docs/spec/physics.md.
    fn drag_factors(hulls: &[Vec<Vec3>], mass: f32, drag_coefficient: f32) -> Vec3 {
        let mut mins = Vec3::splat(f32::MAX);
        let mut maxs = Vec3::splat(f32::MIN);
        for p in hulls.iter().flatten() {
            mins = mins.min(*p);
            maxs = maxs.max(*p);
        }
        let e = (maxs - mins).max(Vec3::ZERO) * UNITS_TO_METERS;
        let area = Vec3::new(e.y * e.z, e.x * e.z, e.x * e.y);
        area * drag_coefficient / mass.max(1e-3) * UNITS_TO_METERS
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
            settings.Friction = desc.surface.friction;
            settings.Restitution = 0.5 * (desc.surface.elasticity + self.world_elasticity);
            settings.LinearDamping = desc.linear_damping;
            settings.AngularDamping = desc.angular_damping;
            settings.GravityFactor = self.gravity_factor();
            settings.Shape = shape;
            let bi = JPC_PhysicsSystem_GetBodyInterface(self.system.as_raw());
            let id = JPC_BodyInterface_CreateAndAddBody(bi, &settings, JPC_ACTIVATION_ACTIVATE);
            self.drags.push(Drag {
                body: id,
                linear: Self::drag_factors(desc.hulls, desc.mass, desc.drag_coefficient),
            });
            Ok(BodyHandle(id))
        }
    }

    pub fn optimize(&self) {
        self.system.optimize_broad_phase();
    }

    fn apply_drag(&self) {
        let dt = self.tick_interval;
        unsafe {
            let bi = JPC_PhysicsSystem_GetBodyInterface(self.system.as_raw());
            for d in &self.drags {
                if !JPC_BodyInterface_IsActive(bi, d.body) {
                    continue;
                }
                let mut p = jv(Vec3::ZERO);
                let mut r = jq(Quat::IDENTITY);
                JPC_BodyInterface_GetPositionAndRotation(bi, d.body, &mut p, &mut r);
                let rot = quat_from_jolt(gq(r)).normalize();
                let mut lv = jv(Vec3::ZERO);
                let mut av = jv(Vec3::ZERO);
                JPC_BodyInterface_GetLinearAndAngularVelocity(bi, d.body, &mut lv, &mut av);
                let local = rot.inverse() * from_jolt(gv(lv));
                let speed = local.length();
                if speed < 1e-3 {
                    continue;
                }
                // implicit `v' = v / (1 + k |v| dt)` per body axis stays stable at any speed
                let damped = Vec3::new(
                    local.x / (1.0 + d.linear.x * speed * dt),
                    local.y / (1.0 + d.linear.y * speed * dt),
                    local.z / (1.0 + d.linear.z * speed * dt),
                );
                JPC_BodyInterface_SetLinearVelocity(bi, d.body, jv(to_jolt(rot * damped)));
            }
        }
    }

    /// Advance one server tick.
    pub fn step(&mut self) {
        self.apply_drag();
        unsafe {
            self.system
                .update(self.tick_interval, 1, self.temp, self.jobs);
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
                drag_coefficient: 1.0,
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
