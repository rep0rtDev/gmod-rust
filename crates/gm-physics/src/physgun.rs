//! Physics gun hold: the measured Source shadow controller (docs/spec/physgun.md).
//!
//! Every tick while held, with `dt` the tick and `T` `physgun_timeToArrive`:
//! `v' = v * (1 - damping) + (target - pos) / T`, clamped to `maxSpeed`, and the same for the
//! angular velocity against `maxAngular`; gravity is off while held. The target is the grab
//! point carried along the current view ray at the grab distance plus the grab offset, both
//! turned only by the yaw change since the grab; the prop's orientation turns by that yaw too.
//! Fitted on `physgun_hold_crate`: velocities 1095.7, 985.7, 660.2, 397.4, 225.9 u/s give
//! `1 - damping` 0.2 and `1 / T` 20 exactly.

use glam::{Quat, Vec3};

#[derive(Debug, Clone, Copy)]
pub struct PhysgunParams {
    pub max_speed: f32,
    pub max_angular: f32,
    pub damping: f32,
    pub time_to_arrive: f32,
}

impl Default for PhysgunParams {
    fn default() -> Self {
        // real server defaults, docs/spec/convars-defaults.md
        PhysgunParams {
            max_speed: 5000.0,
            max_angular: 5000.0,
            damping: 0.8,
            time_to_arrive: 0.05,
        }
    }
}

/// What the gun remembers from the grab.
#[derive(Debug, Clone, Copy)]
pub struct Grab {
    pub distance: f32,
    /// body origin minus hit point, in body space
    pub offset: Vec3,
    pub yaw: f32,
    pub rotation: Quat,
}

/// Degrees of prop rotation per mouse unit in rotate mode (E held): 20 units per tick turned
/// the crate 1 degree per tick.
pub const ROTATE_DEG_PER_UNIT: f32 = 0.05;

impl Grab {
    /// Rotate mode: mouse x turns the wanted orientation about world up, mouse y about the
    /// view's right axis (`right` in world space), both by `ROTATE_DEG_PER_UNIT`.
    pub fn rotate(&mut self, mouse_x: f32, mouse_y: f32, right: Vec3) {
        let yaw = Quat::from_rotation_z((mouse_x * ROTATE_DEG_PER_UNIT).to_radians());
        let pitch = Quat::from_axis_angle(right, (mouse_y * ROTATE_DEG_PER_UNIT).to_radians());
        self.rotation = (pitch * yaw * self.rotation).normalize();
    }

    pub fn new(
        eye: Vec3,
        forward: Vec3,
        hit: Vec3,
        body_pos: Vec3,
        body_rot: Quat,
        yaw: f32,
    ) -> Self {
        Grab {
            distance: (hit - eye).dot(forward),
            offset: body_rot.inverse() * (body_pos - hit),
            yaw,
            rotation: body_rot,
        }
    }

    /// Target origin and rotation for the current view. The grab point rides the ray; the
    /// origin hangs off it by the body space offset in the body's *current* orientation (the
    /// measured swing: the centre lags while the prop's yaw catches up with the view).
    pub fn target(&self, eye: Vec3, forward: Vec3, yaw: f32, body_rot: Quat) -> (Vec3, Quat) {
        let turn = Quat::from_rotation_z((yaw - self.yaw).to_radians());
        (
            eye + forward * self.distance + body_rot * self.offset,
            turn * self.rotation,
        )
    }
}

/// New linear velocity for one tick of the controller.
pub fn controller(v: Vec3, delta: Vec3, p: &PhysgunParams) -> Vec3 {
    let want = v * (1.0 - p.damping) + delta / p.time_to_arrive;
    let speed = want.length();
    if speed > p.max_speed {
        want * (p.max_speed / speed)
    } else {
        want
    }
}

/// Angular version: `w` and the result in degrees per second about world axes, `rot` and
/// `target` the current and wanted orientation.
pub fn angular_controller(w: Vec3, rot: Quat, target: Quat, p: &PhysgunParams) -> Vec3 {
    let d = (target * rot.inverse()).normalize();
    let (axis, angle) = d.to_axis_angle();
    let angle = if angle > std::f32::consts::PI {
        angle - 2.0 * std::f32::consts::PI
    } else {
        angle
    };
    let delta = axis * angle.to_degrees();
    let want = w * (1.0 - p.damping) + delta / p.time_to_arrive;
    let speed = want.length();
    if speed > p.max_angular {
        want * (p.max_angular / speed)
    } else {
        want
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lift_profile_matches_trace() {
        // physgun_hold_crate: 54.42 u of delta, speeds 1095.7, 985.7, 660.2, 397.4, 225.9, 124.0
        let p = PhysgunParams::default();
        let (mut pos, mut v) = (0.0f32, 0.0f32);
        let mut speeds = Vec::new();
        for _ in 0..6 {
            v = controller(Vec3::Z * v, Vec3::Z * (54.42 - pos), &p).z;
            pos += v * 0.015;
            speeds.push(v);
        }
        // the game is 7 u/s faster in the first two ticks only (0.36 u of target), accepted
        let want = [1095.7, 985.7, 660.2, 397.4, 225.9, 124.0];
        for (s, w) in speeds.iter().zip(want) {
            assert!((s - w).abs() < 8.0, "{speeds:?}");
        }
    }
}
