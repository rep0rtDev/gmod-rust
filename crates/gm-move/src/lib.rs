//! Player movement on a fixed tick. The rules follow the behaviour described
//! in `docs/spec/movement.md` and the measured defaults in
//! `docs/spec/convars-defaults.md`; the names are ours.

use glam::Vec3;
use gm_collision::{Trace, World, MASK_PLAYERSOLID};

pub mod buttons {
    pub const ATTACK: u32 = 1 << 0;
    pub const JUMP: u32 = 1 << 1;
    pub const DUCK: u32 = 1 << 2;
    pub const FORWARD: u32 = 1 << 3;
    pub const BACK: u32 = 1 << 4;
    pub const USE: u32 = 1 << 5;
    pub const MOVELEFT: u32 = 1 << 9;
    pub const MOVERIGHT: u32 = 1 << 10;
    pub const ATTACK2: u32 = 1 << 11;
    pub const RELOAD: u32 = 1 << 13;
    pub const SPEED: u32 = 1 << 17;
    pub const WALK: u32 = 1 << 18;
    pub const ZOOM: u32 = 1 << 19;
}

/// Tunables, defaults as measured on a stock sandbox server.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveConfig {
    pub gravity: f32,
    pub friction: f32,
    pub stop_speed: f32,
    pub accelerate: f32,
    pub air_accelerate: f32,
    pub air_wish_cap: f32,
    pub max_velocity: f32,
    pub step_size: f32,
    pub jump_power: f32,
    pub walk_speed: f32,
    pub run_speed: f32,
    pub crouched_walk_speed: f32,
    pub duck_time: f32,
    pub unduck_time: f32,
    pub hull_half: f32,
    pub stand_height: f32,
    pub duck_height: f32,
    pub stand_eye: f32,
    pub duck_eye: f32,
    /// Ground is any surface with normal z at or above this.
    pub ground_normal_z: f32,
    pub tick_interval: f32,
}

impl Default for MoveConfig {
    fn default() -> Self {
        MoveConfig {
            gravity: 600.0,
            friction: 8.0,
            stop_speed: 10.0,
            accelerate: 10.0,
            air_accelerate: 10.0,
            air_wish_cap: 30.0,
            max_velocity: 3500.0,
            step_size: 18.0,
            jump_power: 200.0,
            walk_speed: 200.0,
            run_speed: 400.0,
            crouched_walk_speed: 0.3,
            duck_time: 0.4,
            unduck_time: 0.1,
            hull_half: 16.0,
            stand_height: 72.0,
            duck_height: 36.0,
            stand_eye: 64.0,
            duck_eye: 28.0,
            ground_normal_z: 0.7,
            tick_interval: gm_core::tick::DEFAULT_TICK_INTERVAL,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UserCmd {
    pub buttons: u32,
    /// Pitch, yaw, roll in degrees.
    pub view_angles: Vec3,
    pub forward_move: f32,
    pub side_move: f32,
    pub up_move: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveType {
    Walk,
    Noclip,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerState {
    pub origin: Vec3,
    pub velocity: Vec3,
    pub on_ground: bool,
    pub ground_normal: Vec3,
    pub ducked: bool,
    pub ducking: bool,
    pub unducking: bool,
    /// Seconds elapsed in the current duck or unduck transition.
    pub duck_timer: f32,
    pub jump_held: bool,
    /// A jump happened last tick; its forward boost lands this tick.
    pub boost_pending: bool,
    /// Forward input and view yaw of the jump tick, used by the boost.
    pub boost_forward: f32,
    pub boost_yaw: f32,
    /// Duck state at the start of the tick; the speed limit lags the duck by one tick.
    pub ducked_at_tick_start: bool,
    pub move_type: MoveType,
}

impl Default for PlayerState {
    fn default() -> Self {
        PlayerState {
            origin: Vec3::ZERO,
            velocity: Vec3::ZERO,
            on_ground: false,
            ground_normal: Vec3::Z,
            ducked: false,
            ducking: false,
            unducking: false,
            duck_timer: 0.0,
            jump_held: false,
            boost_pending: false,
            boost_forward: 0.0,
            boost_yaw: 0.0,
            ducked_at_tick_start: false,
            move_type: MoveType::Walk,
        }
    }
}

pub struct Mover<'a> {
    pub cfg: MoveConfig,
    pub world: &'a World,
}

fn angle_vectors(angles: Vec3) -> (Vec3, Vec3) {
    let (p, y) = (angles.x.to_radians(), angles.y.to_radians());
    let forward = Vec3::new(p.cos() * y.cos(), p.cos() * y.sin(), -p.sin());
    let right = Vec3::new(y.sin(), -y.cos(), 0.0);
    (forward, right)
}

/// Vertical speed above which the player is never considered on the ground.
pub const NON_JUMP_VELOCITY: f32 = 140.0;

/// Remove the component of `v` going into `normal`, with the standard overbounce of 1.
pub fn clip_velocity(v: Vec3, normal: Vec3, overbounce: f32) -> Vec3 {
    let backoff = v.dot(normal) * overbounce;
    let mut out = v - normal * backoff;
    for k in 0..3 {
        if out[k].abs() < 0.001 {
            out[k] = 0.0;
        }
    }
    out
}

impl<'a> Mover<'a> {
    pub fn new(cfg: MoveConfig, world: &'a World) -> Self {
        Mover { cfg, world }
    }

    pub fn hull(&self, ducked: bool) -> (Vec3, Vec3) {
        let h = if ducked {
            self.cfg.duck_height
        } else {
            self.cfg.stand_height
        };
        (
            Vec3::new(-self.cfg.hull_half, -self.cfg.hull_half, 0.0),
            Vec3::new(self.cfg.hull_half, self.cfg.hull_half, h),
        )
    }

    pub fn eye_height(&self, st: &PlayerState) -> f32 {
        if st.ducked {
            self.cfg.duck_eye
        } else {
            self.cfg.stand_eye
        }
    }

    fn trace(&self, st: &PlayerState, start: Vec3, end: Vec3) -> Trace {
        let (mins, maxs) = self.hull(st.ducked);
        self.world
            .trace_box(start, end, mins, maxs, MASK_PLAYERSOLID)
    }

    /// Advance one tick.
    pub fn tick(&self, st: &mut PlayerState, cmd: &UserCmd) {
        let dt = self.cfg.tick_interval;
        if st.move_type == MoveType::Noclip {
            self.noclip(st, cmd, dt);
            return;
        }
        st.ducked_at_tick_start = st.ducked;
        self.duck(st, cmd, dt);
        self.categorize(st);
        if st.boost_pending {
            st.boost_pending = false;
            self.jump_boost(st, cmd);
        }
        self.check_jump(st, cmd);
        if st.on_ground {
            st.velocity.z = 0.0;
            self.friction(st, dt);
            self.walk_move(st, cmd, dt);
        } else {
            self.start_gravity(st, dt);
            self.air_move(st, cmd, dt);
            self.finish_gravity(st, dt);
        }
        self.categorize(st);
        if st.on_ground {
            st.velocity.z = 0.0;
        }
        self.clamp_velocity(st);
    }

    fn clamp_velocity(&self, st: &mut PlayerState) {
        let m = self.cfg.max_velocity;
        st.velocity = st.velocity.clamp(Vec3::splat(-m), Vec3::splat(m));
    }

    fn start_gravity(&self, st: &mut PlayerState, dt: f32) {
        st.velocity.z -= self.cfg.gravity * dt * 0.5;
    }

    fn finish_gravity(&self, st: &mut PlayerState, dt: f32) {
        st.velocity.z -= self.cfg.gravity * dt * 0.5;
    }

    /// Ground test: a short downward sweep; moving up fast leaves the ground.
    pub fn categorize(&self, st: &mut PlayerState) {
        if st.velocity.z.abs() > NON_JUMP_VELOCITY {
            st.on_ground = false;
            return;
        }
        let t = self.trace(st, st.origin, st.origin - Vec3::Z * 2.0);
        if !t.hit() || t.normal.z < self.cfg.ground_normal_z {
            st.on_ground = false;
        } else {
            st.on_ground = true;
            st.ground_normal = t.normal;
        }
    }

    fn friction(&self, st: &mut PlayerState, dt: f32) {
        let speed = st.velocity.length();
        if speed < 0.1 {
            return;
        }
        let control = speed.max(self.cfg.stop_speed);
        let drop = control * self.cfg.friction * dt;
        let new_speed = (speed - drop).max(0.0) / speed;
        st.velocity *= new_speed;
    }

    fn wish(&self, st: &PlayerState, cmd: &UserCmd) -> (Vec3, f32) {
        let (mut forward, mut right) = angle_vectors(cmd.view_angles);
        forward.z = 0.0;
        right.z = 0.0;
        forward = forward.normalize_or_zero();
        right = right.normalize_or_zero();
        let max_speed = self.max_speed(st, cmd);
        let fmove = cmd.forward_move.clamp(-max_speed, max_speed);
        let smove = cmd.side_move.clamp(-max_speed, max_speed);
        let wish = forward * fmove + right * smove;
        let speed = wish.length();
        if speed > max_speed {
            (wish / speed, max_speed)
        } else {
            (wish.normalize_or_zero(), speed)
        }
    }

    fn accelerate(
        &self,
        st: &mut PlayerState,
        wish_dir: Vec3,
        wish_speed: f32,
        accel: f32,
        dt: f32,
    ) {
        let current = st.velocity.dot(wish_dir);
        let add = wish_speed - current;
        if add <= 0.0 {
            return;
        }
        let accel_speed = (accel * dt * wish_speed).min(add);
        st.velocity += wish_dir * accel_speed;
    }

    fn walk_move(&self, st: &mut PlayerState, cmd: &UserCmd, dt: f32) {
        let (wish_dir, wish_speed) = self.wish(st, cmd);
        self.accelerate(st, wish_dir, wish_speed, self.cfg.accelerate, dt);
        st.velocity.z = 0.0;
        if st.velocity.length_squared() < 1.0 {
            st.velocity = Vec3::ZERO;
            return;
        }
        self.step_move(st, dt);
        self.stay_on_ground(st);
    }

    /// Keep a walking player glued to the floor: up 2, then down a step, and take the
    /// floor hit if it moved us by more than half the coordinate resolution.
    fn stay_on_ground(&self, st: &mut PlayerState) {
        let up = self.trace(st, st.origin, st.origin + Vec3::Z * 2.0);
        let start = up.end;
        let down = self.trace(
            st,
            start,
            Vec3::new(start.x, start.y, st.origin.z - self.cfg.step_size),
        );
        if down.fraction > 0.0
            && down.fraction < 1.0
            && !down.start_solid
            && down.normal.z >= self.cfg.ground_normal_z
        {
            let delta = (st.origin.z - down.end.z).abs();
            if delta > 0.5 * gm_collision::DIST_EPSILON {
                st.origin = down.end;
            }
        }
    }

    fn air_move(&self, st: &mut PlayerState, cmd: &UserCmd, dt: f32) {
        let (wish_dir, wish_speed) = self.wish(st, cmd);
        // the cap limits how much speed can be added, the rate still uses the full wish speed
        let capped = wish_speed.min(self.cfg.air_wish_cap);
        let current = st.velocity.dot(wish_dir);
        let add = capped - current;
        if add > 0.0 {
            let accel_speed = (self.cfg.air_accelerate * dt * wish_speed).min(add);
            st.velocity += wish_dir * accel_speed;
        }
        self.slide_move(st, dt);
    }

    /// Try the move flat, then again from `step_size` higher, keep the longer one.
    fn step_move(&self, st: &mut PlayerState, dt: f32) {
        let start = *st;
        let mut down = *st;
        self.slide_move(&mut down, dt);
        let flat_dist = (down.origin - start.origin).truncate().length_squared();

        // step up
        let mut up = start;
        let t = self.trace(&up, up.origin, up.origin + Vec3::Z * self.cfg.step_size);
        if !t.start_solid && !t.all_solid {
            up.origin = t.end;
        }
        self.slide_move(&mut up, dt);
        let t = self.trace(&up, up.origin, up.origin - Vec3::Z * self.cfg.step_size);
        if !t.start_solid && !t.all_solid {
            up.origin = t.end;
        }
        if t.normal.z < self.cfg.ground_normal_z {
            *st = down;
            return;
        }
        let up_dist = (up.origin - start.origin).truncate().length_squared();
        if flat_dist > up_dist {
            *st = down;
        } else {
            *st = up;
            st.velocity.z = down.velocity.z;
        }
    }

    /// Move along velocity, clipping against up to four planes.
    fn slide_move(&self, st: &mut PlayerState, dt: f32) {
        let original = st.velocity;
        let mut time_left = dt;
        let mut planes: Vec<Vec3> = Vec::new();
        for _ in 0..4 {
            if st.velocity == Vec3::ZERO {
                break;
            }
            let end = st.origin + st.velocity * time_left;
            let t = self.trace(st, st.origin, end);
            if t.all_solid {
                st.velocity = Vec3::ZERO;
                return;
            }
            if t.fraction > 0.0 {
                st.origin = t.end;
                planes.clear();
            }
            if t.fraction == 1.0 {
                break;
            }
            time_left -= time_left * t.fraction;
            planes.push(t.normal);
            // clip against all touched planes
            let mut v = st.velocity;
            let mut ok = false;
            for i in 0..planes.len() {
                v = clip_velocity(st.velocity, planes[i], 1.0);
                if planes
                    .iter()
                    .enumerate()
                    .all(|(j, p)| j == i || v.dot(*p) >= 0.0)
                {
                    ok = true;
                    break;
                }
            }
            if ok {
                st.velocity = v;
            } else if planes.len() == 2 {
                let dir = planes[0].cross(planes[1]).normalize_or_zero();
                st.velocity = dir * dir.dot(st.velocity);
            } else {
                st.velocity = Vec3::ZERO;
                break;
            }
            if st.velocity.dot(original) <= 0.0 {
                st.velocity = Vec3::ZERO;
                break;
            }
        }
    }

    fn check_jump(&self, st: &mut PlayerState, cmd: &UserCmd) {
        let pressed = cmd.buttons & buttons::JUMP != 0;
        if !pressed {
            st.jump_held = false;
            return;
        }
        if st.jump_held || !st.on_ground {
            return;
        }
        st.on_ground = false;
        st.velocity.z = self.cfg.jump_power;
        // the jump finishes the current tick's gravity itself, on top of the air move's halves
        st.velocity.z -= self.cfg.gravity * self.cfg.tick_interval * 0.5;
        st.boost_pending = true;
        st.boost_forward = cmd.forward_move;
        st.boost_yaw = cmd.view_angles.y;
        st.jump_held = true;
    }

    /// Forward jump boost: a share of the forward input is added along the view yaw,
    /// clipped so the horizontal speed does not exceed max speed times (1 + share).
    fn jump_boost(&self, st: &mut PlayerState, cmd: &UserCmd) {
        let (mut forward, _) = angle_vectors(Vec3::new(0.0, st.boost_yaw, 0.0));
        forward.z = 0.0;
        let forward = forward.normalize_or_zero();
        let share = if st.ducked { 0.1 } else { 0.5 };
        let max_speed = self.max_speed(st, cmd);
        let forward_move = st.boost_forward;
        let mut addition = forward_move.abs() * share;
        let cap = max_speed + max_speed * share;
        let new_speed = addition + st.velocity.truncate().length();
        if new_speed > cap {
            addition -= new_speed - cap;
        }
        if forward_move < 0.0 {
            addition = -addition;
        }
        st.velocity += forward * addition;
    }

    fn max_speed(&self, st: &PlayerState, cmd: &UserCmd) -> f32 {
        if st.ducked_at_tick_start {
            self.cfg.walk_speed * self.cfg.crouched_walk_speed
        } else if cmd.buttons & buttons::SPEED != 0 {
            self.cfg.run_speed
        } else {
            self.cfg.walk_speed
        }
    }

    /// Duck timing follows the original: the press tick starts the clock, every later
    /// tick adds dt, and the transition completes once the clock exceeds the duration.
    fn duck(&self, st: &mut PlayerState, cmd: &UserCmd, dt: f32) {
        let want = cmd.buttons & buttons::DUCK != 0;
        if want {
            if st.unducking {
                st.unducking = false;
                st.duck_timer = 0.0;
            }
            if !st.ducked {
                if !st.ducking {
                    st.ducking = true;
                    st.duck_timer = 0.0;
                } else {
                    st.duck_timer += dt;
                }
                if st.duck_timer > self.cfg.duck_time || !st.on_ground {
                    self.finish_duck(st);
                }
            }
        } else if st.ducked || st.ducking {
            if st.ducking && !st.ducked {
                // released before finishing: stand straight away
                st.ducking = false;
                st.duck_timer = 0.0;
                return;
            }
            if !st.unducking {
                if !self.can_unduck(st) {
                    return;
                }
                st.unducking = true;
                st.duck_timer = 0.0;
            } else {
                st.duck_timer += dt;
            }
            if st.duck_timer > self.cfg.unduck_time || !st.on_ground {
                self.try_unduck(st);
            }
        }
    }

    fn finish_duck(&self, st: &mut PlayerState) {
        st.ducked = true;
        st.ducking = false;
        st.duck_timer = 0.0;
        if !st.on_ground {
            // in the air the hull shrinks from the top and the origin moves up by the full height difference
            st.origin.z += self.cfg.stand_height - self.cfg.duck_height;
        }
    }

    fn standing_origin(&self, st: &PlayerState) -> Vec3 {
        if !st.on_ground && st.ducked {
            st.origin - Vec3::Z * (self.cfg.stand_height - self.cfg.duck_height)
        } else {
            st.origin
        }
    }

    fn can_unduck(&self, st: &PlayerState) -> bool {
        let mut standing = *st;
        standing.ducked = false;
        standing.origin = self.standing_origin(st);
        let t = self.trace(&standing, standing.origin, standing.origin);
        !t.start_solid && !t.all_solid
    }

    fn try_unduck(&self, st: &mut PlayerState) {
        if !self.can_unduck(st) {
            return;
        }
        st.origin = self.standing_origin(st);
        st.ducked = false;
        st.ducking = false;
        st.unducking = false;
        st.duck_timer = 0.0;
    }

    fn noclip(&self, st: &mut PlayerState, cmd: &UserCmd, dt: f32) {
        let (forward, right) = angle_vectors(cmd.view_angles);
        let speed: f32 = if cmd.buttons & buttons::SPEED != 0 {
            1000.0
        } else {
            400.0
        };
        let wish = forward * cmd.forward_move + right * cmd.side_move + Vec3::Z * cmd.up_move;
        st.velocity = wish.normalize_or_zero() * speed.min(wish.length());
        st.origin += st.velocity * dt;
        st.on_ground = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_world() -> World {
        World::from_boxes(&[
            (
                Vec3::new(-4096.0, -4096.0, -64.0),
                Vec3::new(4096.0, 4096.0, 0.0),
            ),
            // a 16 unit step and a 24 unit block (too tall to step)
            (Vec3::new(200.0, -64.0, 0.0), Vec3::new(264.0, 64.0, 16.0)),
            (Vec3::new(400.0, -64.0, 0.0), Vec3::new(464.0, 64.0, 24.0)),
            // low ceiling at 50 units over x in 600..700
            (Vec3::new(600.0, -64.0, 50.0), Vec3::new(700.0, 64.0, 100.0)),
        ])
    }

    fn standing(world: &World) -> (Mover<'_>, PlayerState) {
        let m = Mover::new(MoveConfig::default(), world);
        let mut st = PlayerState {
            origin: Vec3::new(0.0, 0.0, 1.0),
            ..Default::default()
        };
        m.tick(&mut st, &UserCmd::default());
        assert!(st.on_ground);
        (m, st)
    }

    #[test]
    fn walk_accelerates_to_walk_speed_and_stops_by_friction() {
        let w = flat_world();
        let (m, mut st) = standing(&w);
        let cmd = UserCmd {
            buttons: buttons::FORWARD,
            forward_move: 10000.0,
            view_angles: Vec3::new(0.0, 90.0, 0.0),
            ..Default::default()
        };
        for _ in 0..200 {
            m.tick(&mut st, &cmd);
        }
        let speed = st.velocity.truncate().length();
        // friction and acceleration settle below the 200 cap: f = 8, a = 10 gives v where a*(200-v) = f*v
        assert!(speed > 100.0 && speed <= 200.0, "{speed}");
        assert!(st.on_ground);
        let mut ticks = 0;
        while st.velocity.length() > 0.0 {
            m.tick(&mut st, &UserCmd::default());
            ticks += 1;
            assert!(ticks < 200);
        }
        assert!(ticks > 5, "stopped in {ticks} ticks");
    }

    #[test]
    fn jump_apex_matches_gravity() {
        let w = flat_world();
        let (m, mut st) = standing(&w);
        let z0 = st.origin.z;
        m.tick(
            &mut st,
            &UserCmd {
                buttons: buttons::JUMP,
                ..Default::default()
            },
        );
        assert!(!st.on_ground);
        let mut apex = z0;
        let mut ticks = 0;
        while !st.on_ground {
            m.tick(&mut st, &UserCmd::default());
            apex = apex.max(st.origin.z);
            ticks += 1;
            assert!(ticks < 200);
        }
        // v^2 / 2g = 200^2 / 1200 = 33.3; the discrete integrator lands a little under
        assert!((apex - z0 - 33.3).abs() < 2.0, "apex {}", apex - z0);
        // airtime 2v/g = 0.667 s = 44 ticks
        assert!((ticks - 44i32).abs() <= 2, "{ticks} ticks");
        assert!(
            (st.origin.z - gm_collision::DIST_EPSILON).abs() < 0.1,
            "landed at {}",
            st.origin.z
        );
    }

    #[test]
    fn steps_up_16_but_not_24() {
        let w = flat_world();
        let (m, mut st) = standing(&w);
        let cmd = UserCmd {
            buttons: buttons::FORWARD | buttons::SPEED,
            forward_move: 400.0,
            ..Default::default()
        };
        let mut stood_on_step = false;
        for _ in 0..300 {
            m.tick(&mut st, &cmd);
            if st.origin.x > 210.0 && st.origin.x < 250.0 {
                assert!(
                    (st.origin.z - 16.0).abs() < 0.1,
                    "on the step at z {}",
                    st.origin.z
                );
                assert!(st.on_ground);
                stood_on_step = true;
            }
        }
        assert!(stood_on_step, "never crossed the step, x {}", st.origin.x);
        // blocked by the 24 unit block: hull front face (16 half width) a hair before x = 400
        assert!((st.origin.x - 384.0).abs() < 0.1, "{}", st.origin.x);
        assert!(
            st.on_ground && (st.origin.z - 0.03).abs() < 0.1,
            "{}",
            st.origin.z
        );
    }

    #[test]
    fn duck_then_cannot_unduck_under_ceiling() {
        let w = flat_world();
        let m = Mover::new(MoveConfig::default(), &w);
        let mut st = PlayerState {
            origin: Vec3::new(650.0, 0.0, 1.0),
            ..Default::default()
        };
        let duck = UserCmd {
            buttons: buttons::DUCK,
            ..Default::default()
        };
        for _ in 0..40 {
            m.tick(&mut st, &duck);
        }
        assert!(st.ducked);
        m.tick(&mut st, &UserCmd::default());
        assert!(st.ducked, "stood up into the ceiling");
        st.origin.x = 100.0;
        m.tick(&mut st, &UserCmd::default());
        assert!(!st.ducked);
    }
}
