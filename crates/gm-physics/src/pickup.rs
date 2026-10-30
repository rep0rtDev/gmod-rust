//! +use carry: a light prop held in front of the player (docs/spec/sandbox.md, "+use pickup").
//!
//! The public SDK's `CGrabController` for the player pickup, measured on `use_*`: the prop sits
//! at `eye + forward * (24 + radius)` where `radius` is the player's horizontal half-diagonal
//! (22.63 u for the 32x32 hull) plus the prop's hull extent along the view, the view pitch is
//! clamped to -75..=75 for the position and ignored for the rotation, the prop's orientation is
//! snapped to the player's axes at pickup when within 30 degrees and then only turns with the
//! player's yaw, and the prop moves at up to 2500 u/s and 3600 deg/s (37.5 u and 54 degrees a
//! tick: the crate's 20.5 u pickup takes one tick, the 71 u swing of a 60 degree turn two).
//! The use trace is the SDK's `FindUseEntity`: a 1024 u line from the eyes, then 16 u box sweeps
//! of 72 u tilted down from the view (45, 30, 20, 15, 10, -10, -15 degrees) for props at the
//! feet; a hit counts when its horizontal distance (plus any part above or below the player's
//! hull) is under 80 u.

use glam::{Quat, Vec3};

use crate::physgun::PhysgunParams;

/// `24 + radius * 2` is traced; the prop is held at `24 + radius` when the trace is clear.
pub const HOLD_BASE: f32 = 24.0;
/// `OBBMaxs().Length2D()` of the standing player hull (16, 16).
pub const PLAYER_RADIUS: f32 = 22.627_417;
/// `DOT_30DEGREE`: a prop axis within this cosine of a player axis snaps to it at pickup.
pub const ALIGN_COS: f32 = 0.866_025_4;
/// `CanPickupObject(obj, 35, 128)`.
pub const MAX_MASS: f32 = 35.0;
pub const MAX_SIZE: f32 = 128.0;
/// `PLAYER_USE_RADIUS`: how far the use hit may be (horizontal, plus the part above or below
/// the player's hull).
pub const USE_RADIUS: f32 = 80.0;
/// Standing and ducked player hull heights (`VEC_HULL_MAX.z`, `VEC_DUCK_HULL_MAX.z`).
pub const HULL_HEIGHT: f32 = 72.0;
pub const DUCK_HULL_HEIGHT: f32 = 36.0;

/// Measured caps of the carry's shadow controller (`use_popcan`: 2424 u/s with the cap's 2500
/// split over three axes; `use_crate`: 54 degrees in the first tick of a 60 degree turn).
pub const MAX_SPEED: f32 = 2500.0;
pub const MAX_ANGULAR: f32 = 3600.0;
/// `UTIL_TraceHull` sweeps of `FindUseEntity`: length, box half extent and the tangents of the
/// tilt below the view (`tan(45)`, `tan(30)`, `tan(20)`, `tan(15)`, `tan(10)`, `-tan(10)`,
/// `-tan(15)`).
pub const SWEEP_LEN: f32 = 72.0;
pub const SWEEP_HALF: f32 = 16.0;
pub const SWEEP_TANGENTS: [f32; 7] = [
    1.0,
    0.577_350_26,
    0.363_970_23,
    0.267_949_2,
    0.176_326_98,
    -0.176_326_98,
    -0.267_949_2,
];

/// Controller parameters that put the prop on target as fast as the caps allow.
pub fn params(tick: f32) -> PhysgunParams {
    PhysgunParams {
        max_speed: MAX_SPEED,
        max_angular: MAX_ANGULAR,
        damping: 1.0,
        time_to_arrive: tick,
    }
}

/// View vectors from Source angles (pitch down positive, yaw): `(forward, up)`.
pub fn view_vectors(pitch: f32, yaw: f32) -> (Vec3, Vec3) {
    let (sp, cp) = (pitch.to_radians().sin(), pitch.to_radians().cos());
    let (sy, cy) = (yaw.to_radians().sin(), yaw.to_radians().cos());
    (
        Vec3::new(cp * cy, cp * sy, -sp),
        Vec3::new(sp * cy, sp * sy, cp),
    )
}

/// The sweep directions of `FindUseEntity` after the line trace, in the order tried.
pub fn sweep_dirs(forward: Vec3, up: Vec3) -> [Vec3; 7] {
    SWEEP_TANGENTS.map(|t| (forward - up * t).normalize())
}

/// A box of half extent `half` swept from `start` along the unit `dir` for `len` against a
/// convex given by world-space outward planes (`n . x <= d`) and its world AABB: the distance at
/// which the box first touches it. The Minkowski sum is approximated by the planes pushed out
/// by the box and the AABB grown by it.
pub fn box_sweep_convex(
    start: Vec3,
    dir: Vec3,
    len: f32,
    half: f32,
    planes: &[(Vec3, f32)],
    aabb: (Vec3, Vec3),
) -> Option<f32> {
    let (mut t0, mut t1) = (0.0_f32, len);
    let h = Vec3::splat(half);
    let mut clip = |n: Vec3, d: f32| -> bool {
        let denom = n.dot(dir);
        let dist = d - n.dot(start);
        if denom.abs() < 1e-6 {
            return dist >= 0.0;
        }
        let t = dist / denom;
        if denom > 0.0 {
            t1 = t1.min(t);
        } else {
            t0 = t0.max(t);
        }
        true
    };
    for &(n, d) in planes {
        if !clip(n, d + n.abs().dot(h)) {
            return None;
        }
    }
    for (axis, e) in [Vec3::X, Vec3::Y, Vec3::Z].into_iter().enumerate() {
        if !clip(e, aabb.1[axis] + half) || !clip(-e, -(aabb.0[axis] - half)) {
            return None;
        }
    }
    (t0 <= t1).then_some(t0)
}

/// Body-frame planes and points moved into the world for `box_sweep_convex`.
pub fn world_convex(
    planes: &[(Vec3, f32)],
    points: &[Vec3],
    rot: Quat,
    pos: Vec3,
) -> (Vec<(Vec3, f32)>, (Vec3, Vec3)) {
    let planes = planes
        .iter()
        .map(|&(n, d)| {
            let n = rot * n;
            (n, d + n.dot(pos))
        })
        .collect();
    let aabb = points.iter().fold(
        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
        |(lo, hi), &p| {
            let w = rot * p + pos;
            (lo.min(w), hi.max(w))
        },
    );
    (planes, aabb)
}

/// What the carry remembers from the pickup.
#[derive(Debug, Clone, Copy)]
pub struct Carry {
    /// Player yaw at pickup (degrees).
    pub yaw: f32,
    /// Prop orientation at pickup, aligned to the player's axes.
    pub rotation: Quat,
}

/// Snap `rot` to the nearest orientation whose axes match the yaw frame's axes when every axis
/// is within `ALIGN_COS` of one (`AlignAngles`); otherwise keep it.
pub fn align(rot: Quat, yaw: f32) -> Quat {
    let frame = Quat::from_rotation_z(yaw.to_radians());
    let rel = frame.inverse() * rot;
    let axes = [rel * Vec3::X, rel * Vec3::Y, rel * Vec3::Z];
    let mut snapped = [Vec3::ZERO; 3];
    for (i, a) in axes.iter().enumerate() {
        let (k, v) = [Vec3::X, Vec3::Y, Vec3::Z]
            .iter()
            .enumerate()
            .map(|(k, b)| (k, a.dot(*b)))
            .max_by(|x, y| x.1.abs().total_cmp(&y.1.abs()))
            .unwrap();
        if v.abs() < ALIGN_COS {
            return rot;
        }
        snapped[i] = [Vec3::X, Vec3::Y, Vec3::Z][k] * v.signum();
    }
    if snapped[0].cross(snapped[1]).dot(snapped[2]) < 0.0 {
        return rot;
    }
    frame * Quat::from_mat3(&glam::Mat3::from_cols(snapped[0], snapped[1], snapped[2]))
}

/// The use reach test: `delta = hit - eye` with `delta.z` replaced by how far `hit.z` is outside
/// the player's hull (`origin.z..origin.z + hull_height`).
pub fn within_use_reach(eye: Vec3, hit: Vec3, origin: Vec3, hull_height: f32) -> bool {
    let (lo, hi) = (origin.z, origin.z + hull_height);
    let dz = if hit.z < lo {
        lo - hit.z
    } else if hit.z > hi {
        hit.z - hi
    } else {
        0.0
    };
    Vec3::new(hit.x - eye.x, hit.y - eye.y, dz).length() < USE_RADIUS
}

impl Carry {
    pub fn new(body_rot: Quat, yaw: f32) -> Self {
        Carry {
            yaw,
            rotation: align(body_rot, yaw),
        }
    }

    /// Where the prop's origin should be this tick. `hulls` are the PHY convexes in the body
    /// frame, `com` the PHY mass centre in it (the game puts the mass centre on the hold
    /// point: the chair, whose origin is 2.9 u above its mass centre, hangs that much above
    /// the view ray), `brush` traces the world from `a` to `b` and returns the hit fraction (1
    /// for none), `origin`/`hull_height` describe the player's hull for the keep-out clamp.
    #[allow(clippy::too_many_arguments)]
    pub fn target(
        &self,
        eye: Vec3,
        view: Vec3,
        hulls: &[Vec<Vec3>],
        com: Vec3,
        body_rot: Quat,
        origin: Vec3,
        hull_height: f32,
        brush: &dyn Fn(Vec3, Vec3) -> f32,
    ) -> (Vec3, Quat) {
        let pitch = view.x.clamp(-75.0, 75.0).to_radians();
        let yaw = view.y.to_radians();
        let forward = Vec3::new(
            pitch.cos() * yaw.cos(),
            pitch.cos() * yaw.sin(),
            -pitch.sin(),
        );
        let extent = hulls
            .iter()
            .flatten()
            .map(|&p| (body_rot * p).dot(-forward))
            .fold(0.0_f32, f32::max);
        let radius = PLAYER_RADIUS + extent.abs();
        let distance = HOLD_BASE + radius * 2.0;
        let fraction = brush(eye, eye + forward * distance);
        let mut end = if fraction < 0.5 {
            eye + forward * (radius * 0.5)
        } else {
            eye + forward * (distance - radius)
        };
        let line = Vec3::new(
            origin.x,
            origin.y,
            end.z.clamp(origin.z, origin.z + hull_height),
        );
        let delta = end - line;
        let len = delta.length();
        if len < radius && len > 1e-4 {
            end = line + delta * (radius / len);
        }
        let rot = Quat::from_rotation_z((view.y - self.yaw).to_radians()) * self.rotation;
        (end - rot * com, rot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(h: f32) -> Vec<Vec<Vec3>> {
        let mut pts = Vec::new();
        for &x in &[-h, h] {
            for &y in &[-h, h] {
                for &z in &[-h, h] {
                    pts.push(Vec3::new(x, y, z));
                }
            }
        }
        vec![pts]
    }

    #[test]
    fn crate_sits_where_the_game_put_it() {
        // use_crate: eye (823.2, -600, -79.6), pitch 20 yaw 90 -> the crate 67.7 ahead and 24.5
        // below (72.0 out); pitch 0 yaw 150 with the crate turned by the same 60 degrees -> 66.4
        // out at eye height, crate yaw 60
        let eye = Vec3::new(823.235, -600.0, -79.6);
        let origin = Vec3::new(823.235, -600.0, -143.6);
        let c = Carry::new(Quat::IDENTITY, 90.0);
        let clear = |_: Vec3, _: Vec3| 1.0;
        let hull = cube(19.8);
        let (p, _) = c.target(
            eye,
            Vec3::new(20.0, 90.0, 0.0),
            &hull,
            Vec3::ZERO,
            Quat::IDENTITY,
            origin,
            72.0,
            &clear,
        );
        assert!(
            (p - eye - Vec3::new(0.0, 67.7, -24.5)).length() < 0.3,
            "{:?}",
            p - eye
        );
        let turned = Quat::from_rotation_z(60.0_f32.to_radians());
        let (p, r) = c.target(
            eye,
            Vec3::new(0.0, 150.0, 0.0),
            &hull,
            Vec3::ZERO,
            turned,
            origin,
            72.0,
            &clear,
        );
        assert!(
            (p - eye - Vec3::new(-57.5, 33.2, 0.0)).length() < 0.3,
            "{:?}",
            p - eye
        );
        assert!(r.dot(turned).abs() > 0.9999, "{r:?}");
    }

    #[test]
    fn alignment_snaps_small_tilts_and_keeps_big_ones() {
        let tilted = Quat::from_rotation_y(3.5_f32.to_radians());
        assert!(align(tilted, 90.0).dot(Quat::IDENTITY).abs() > 0.9999);
        let big = Quat::from_rotation_y(40.0_f32.to_radians());
        assert!(align(big, 90.0).dot(big).abs() > 0.9999);
    }

    #[test]
    fn a_box_sweep_finds_a_can_under_the_view_ray() {
        // use_popcan: the can 57.6 ahead and 60.5 below the eyes, view pitch 20; the line misses,
        // the 30 degree sweep touches it
        let eye = Vec3::new(0.0, 0.0, 64.0);
        let can = Vec3::new(57.6, 0.0, 3.5);
        let (forward, up) = view_vectors(20.0, 0.0);
        let r = 3.3;
        let planes = vec![
            (Vec3::X, r),
            (-Vec3::X, r),
            (Vec3::Y, r),
            (-Vec3::Y, r),
            (Vec3::Z, 4.5),
            (-Vec3::Z, 4.5),
        ];
        let pts: Vec<Vec3> = planes.iter().map(|&(n, d)| n * d).collect();
        let (wp, aabb) = world_convex(&planes, &pts, Quat::IDENTITY, can);
        let dirs = sweep_dirs(forward, up);
        assert!(box_sweep_convex(eye, forward, 1024.0, 0.0, &wp, aabb).is_none());
        assert!(box_sweep_convex(eye, dirs[0], SWEEP_LEN, SWEEP_HALF, &wp, aabb).is_none());
        let hit = box_sweep_convex(eye, dirs[1], SWEEP_LEN, SWEEP_HALF, &wp, aabb);
        assert!(hit.is_some_and(|t| t > 30.0 && t <= SWEEP_LEN), "{hit:?}");
    }

    #[test]
    fn use_reach_is_horizontal_within_the_hull() {
        let eye = Vec3::new(0.0, 0.0, 64.0);
        let origin = Vec3::ZERO;
        assert!(within_use_reach(
            eye,
            Vec3::new(0.0, 79.6, 35.0),
            origin,
            72.0
        ));
        assert!(!within_use_reach(
            eye,
            Vec3::new(0.0, 89.6, 35.0),
            origin,
            72.0
        ));
        assert!(!within_use_reach(
            eye,
            Vec3::new(0.0, 60.0, -60.0),
            origin,
            72.0
        ));
    }
}
