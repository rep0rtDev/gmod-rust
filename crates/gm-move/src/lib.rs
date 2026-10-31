//! Player movement on a fixed tick. The rules follow the behaviour described
//! in `docs/spec/movement.md` and the measured defaults in
//! `docs/spec/convars-defaults.md`; the names are ours.

use glam::Vec3;
use gm_collision::{Trace, World, MASK_PLAYERSOLID, MASK_WATER};

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
    /// Reach of the downward ground probe in `categorize`. Measured on gm_construct: a player
    /// 0.97 u above the floor is grounded, 1.32 u (landing a jump at 191.5 u/s) and 1.66 u
    /// (settling at 63 u/s) are not; every value from 1.1 to 1.3 reproduces all recordings.
    pub ground_probe: f32,
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
            ground_probe: 1.2,
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
    /// Being pulled onto a ladder (the game's MOVETYPE_NONE while it slides the player in).
    LadderMount,
    Ladder,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerState {
    pub origin: Vec3,
    pub velocity: Vec3,
    /// Velocity added this tick by a hit, after friction and before the move (the game's
    /// knock-back shows in the hit tick undiminished, measured `hit_*`).
    pub impulse: Vec3,
    pub on_ground: bool,
    /// Set with a knockback on a grounded player: measured (`hit_rpg_floor*`), a blast's
    /// vertical push of up to 250 u/s shows in that tick's velocity but does not lift the
    /// player; `categorize` keeps the ground once and clears it.
    pub ground_push: bool,
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
    /// Surface friction multiplier; 1 on the ground, 0.25 after leaving it upwards.
    pub surface_friction: f32,
    pub move_type: MoveType,
    /// 0 dry, 1 feet, 2 waist, 3 eyes (the game's WaterLevel), from `check_water`.
    pub water_level: u8,
    /// Index into the world's ladders while mounting or climbing one.
    pub ladder: u16,
}

impl Default for PlayerState {
    fn default() -> Self {
        PlayerState {
            origin: Vec3::ZERO,
            velocity: Vec3::ZERO,
            impulse: Vec3::ZERO,
            ground_push: false,
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
            surface_friction: 1.0,
            move_type: MoveType::Walk,
            water_level: 0,
            ladder: 0,
        }
    }
}

/// Swimming: the ground max speed scaled by this, so forward is 160 and the idle sink 48.
pub const WATER_SPEED_SCALE: f32 = 0.8;
/// Idle in water the wish points down this fast (before the scale).
pub const WATER_SINK_SPEED: f32 = 60.0;
/// Holding jump while swimming sets the vertical speed to this every tick.
pub const WATER_JUMP_SPEED: f32 = 100.0;
/// Climbing speed along the ladder, and the speed the mount pulls the player in with
/// (measured `move_ladder_*`: 3 u per tick both ways).
pub const LADDER_SPEED: f32 = 200.0;
/// A forward press this close (origin to the ladder line) starts the mount (hull 16 + 8).
pub const LADDER_MOUNT_REACH: f32 = 24.0;
/// A climb wish pointing down more steeply than this (its z, unit length) descends.
pub const LADDER_DOWN_SLOPE: f32 = 0.3;
/// Jumping off: the view forward at `LADDER_SPEED` plus this much up.
pub const LADDER_JUMP_UP: f32 = 50.0;
/// Pseudo material for the footsteps on a ladder (`$surfaceprop ladder`).
pub const LADDER_MATERIAL: &str = "surfaceprop:ladder";

/// Footsteps in water up to the feet use this surface property instead of the floor's
/// (measured: `slosh*.wav` at the normal step volume), as the `surfaceprop:` pseudo material.
pub const WADE_MATERIAL: &str = "surfaceprop:wade";

/// One tick's point-contents lookups of the three water probes: (point, contents) per probe.
#[derive(Default, Clone, Copy, Debug)]
pub struct WaterCache(pub [Option<(Vec3, i32)>; 3]);

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
        match st.move_type {
            MoveType::Noclip => {
                self.noclip(st, cmd, dt);
                return;
            }
            MoveType::LadderMount => {
                self.ladder_mount_step(st, dt);
                self.categorize(st);
                return;
            }
            MoveType::Ladder if cmd.buttons & buttons::JUMP == 0 => {
                self.ladder_move(st, cmd, dt);
                return;
            }
            MoveType::Ladder => {
                // measured (`move_ladder_jump`): off along the view at the climb speed, plus a hop
                let (forward, _) = angle_vectors(cmd.view_angles);
                st.move_type = MoveType::Walk;
                st.on_ground = false;
                st.velocity = forward * LADDER_SPEED + Vec3::Z * LADDER_JUMP_UP;
            }
            MoveType::Walk => {
                if self.ladder_mount(st, cmd) {
                    self.categorize(st);
                    return;
                }
            }
        }
        st.ducked_at_tick_start = st.ducked;
        self.duck(st, cmd, dt);
        self.categorize(st);
        let mut water = WaterCache::default();
        self.check_water(st, &mut water);
        if st.water_level >= 2 {
            // measured (`move_water_swim`): no gravity at all, jump held pins the rise speed
            if cmd.buttons & buttons::JUMP != 0 {
                st.on_ground = false;
                st.velocity.z = WATER_JUMP_SPEED;
            }
            st.jump_held = cmd.buttons & buttons::JUMP != 0;
            st.velocity += st.impulse;
            st.impulse = Vec3::ZERO;
            self.water_move(st, cmd, dt);
            st.surface_friction = if st.velocity.z > 0.0 { 0.25 } else { 1.0 };
            self.categorize(st);
            self.check_water(st, &mut water);
            if st.on_ground {
                st.velocity.z = 0.0;
            }
            self.clamp_velocity(st);
            return;
        }
        if st.boost_pending {
            st.boost_pending = false;
            self.jump_boost(st, cmd);
        }
        self.check_jump(st, cmd);
        if st.on_ground {
            st.surface_friction = 1.0;
            st.velocity.z = 0.0;
            self.friction(st, dt);
            // measured (`hit_rpg_floor*`, `hit_crossbow`): a blast's vertical push shows in the
            // velocity of the hit tick, but the player stays on the ground and loses it
            st.velocity += Vec3::new(st.impulse.x, st.impulse.y, 0.0);
            st.impulse = Vec3::ZERO;
            self.walk_move(st, cmd, dt);
        } else {
            self.start_gravity(st, dt);
            st.velocity += st.impulse;
            st.impulse = Vec3::ZERO;
            self.air_move(st, cmd, dt);
            // measured: the air friction multiplier follows the vertical speed before the second half of gravity
            st.surface_friction = if st.velocity.z > 0.0 { 0.25 } else { 1.0 };
            // falling into the water keeps only the first half of the tick's gravity
            self.check_water(st, &mut water);
            if st.water_level < 2 {
                self.finish_gravity(st, dt);
            }
        }
        self.categorize(st);
        self.check_water(st, &mut water);
        if st.on_ground {
            st.velocity.z = 0.0;
        }
        self.clamp_velocity(st);
    }

    /// The game's water level: water at the feet (origin + 1), the waist (half the hull) or
    /// the eyes. Within one tick each probe is looked up again only once it has moved more
    /// than 1 u from where it was last looked up (measured, `move_water_swim` ticks 249/381/422
    /// get the second half of the tick's gravity skipped, 432/442 with a sub-unit move do not).
    pub fn check_water(&self, st: &mut PlayerState, cache: &mut WaterCache) {
        let (_, maxs) = self.hull(st.ducked);
        let probes = [1.0, maxs.z * 0.5, self.eye_height(st)];
        let mut level = 0u8;
        for (i, h) in probes.iter().enumerate() {
            let p = st.origin + Vec3::Z * *h;
            let contents = match cache.0[i] {
                Some((at, c)) if at.distance_squared(p) <= 1.0 => c,
                _ => {
                    let c = self.world.point_contents(p);
                    cache.0[i] = Some((p, c));
                    c
                }
            };
            if contents & MASK_WATER != 0 {
                level = i as u8 + 1;
            } else {
                break;
            }
        }
        st.water_level = level;
    }

    /// Swimming (water level 2+), fitted tick for tick to `move_water_swim`: the wish follows
    /// the full view direction (jump adds the max speed upwards, no input sinks at 60), speed
    /// scaled by 0.8; friction on the whole vector, acceleration limited by the speed left.
    /// A forward press near a ladder the player faces grabs it: the game stops the player
    /// where he is and pulls him onto the ladder line over the following ticks
    /// (measured `move_ladder_back`: nothing moves on the press tick).
    fn ladder_mount(&self, st: &mut PlayerState, cmd: &UserCmd) -> bool {
        if cmd.forward_move <= 0.0 && cmd.buttons & buttons::FORWARD == 0 {
            return false;
        }
        let (forward, _) = angle_vectors(cmd.view_angles);
        let look = forward.truncate();
        for (i, l) in self.world.ladders().iter().enumerate() {
            let to = l.bottom.truncate() - st.origin.truncate();
            if to.length() > LADDER_MOUNT_REACH + 1e-3 || look.dot(to) <= 0.0 {
                continue;
            }
            if st.origin.z < l.bottom.z - self.cfg.step_size || st.origin.z > l.top.z {
                continue;
            }
            st.move_type = MoveType::LadderMount;
            st.ladder = i as u16;
            st.velocity = Vec3::ZERO;
            return true;
        }
        false
    }

    /// `LADDER_SPEED` straight at the ladder line; on the line the climb starts next tick.
    fn ladder_mount_step(&self, st: &mut PlayerState, dt: f32) {
        let Some(l) = self.world.ladders().get(st.ladder as usize) else {
            st.move_type = MoveType::Walk;
            return;
        };
        let to = l.bottom.truncate() - st.origin.truncate();
        let step = LADDER_SPEED * dt;
        st.velocity = Vec3::ZERO;
        if to.length() <= step {
            st.origin.x = l.bottom.x;
            st.origin.y = l.bottom.y;
            st.move_type = MoveType::Ladder;
        } else {
            let d = to.normalize() * step;
            st.origin.x += d.x;
            st.origin.y += d.y;
        }
    }

    /// Measured (`move_ladder_angles`, `move_ladder_thresh`): the move buttons build a wish
    /// from the view (forward, minus forward for back, right for the strafes) and the player
    /// climbs up at `LADDER_SPEED` unless that wish points down steeper than
    /// `LADDER_DOWN_SLOPE`; the floor and ceiling stop the climb dead and the player stays
    /// on the ladder at either end.
    fn ladder_move(&self, st: &mut PlayerState, cmd: &UserCmd, dt: f32) {
        let Some(l) = self.world.ladders().get(st.ladder as usize) else {
            st.move_type = MoveType::Walk;
            return;
        };
        st.on_ground = false;
        let dir = (l.top - l.bottom).normalize_or_zero();
        let (forward, right) = angle_vectors(cmd.view_angles);
        let mut wish = Vec3::ZERO;
        if cmd.buttons & buttons::FORWARD != 0 {
            wish += forward;
        }
        if cmd.buttons & buttons::BACK != 0 {
            wish -= forward;
        }
        if cmd.buttons & buttons::MOVERIGHT != 0 {
            wish += right;
        }
        if cmd.buttons & buttons::MOVELEFT != 0 {
            wish -= right;
        }
        let Some(wish) = wish.try_normalize() else {
            st.velocity = Vec3::ZERO;
            return;
        };
        let speed = if wish.z < -LADDER_DOWN_SLOPE {
            -LADDER_SPEED
        } else {
            LADDER_SPEED
        };
        st.velocity = dir * speed;
        let end = st.origin + st.velocity * dt;
        let t = self.trace(st, st.origin, end);
        st.origin = t.end;
        if t.fraction < 1.0 {
            st.velocity = Vec3::ZERO;
        }
    }

    fn water_move(&self, st: &mut PlayerState, cmd: &UserCmd, dt: f32) {
        let (forward, right) = angle_vectors(cmd.view_angles);
        let max_speed = self.max_speed(st, cmd);
        let fmove = cmd.forward_move.clamp(-max_speed, max_speed);
        let smove = cmd.side_move.clamp(-max_speed, max_speed);
        let mut wish = forward * fmove + right * smove;
        if cmd.buttons & buttons::JUMP != 0 {
            wish.z += max_speed;
        } else if fmove == 0.0 && smove == 0.0 && cmd.up_move == 0.0 {
            wish.z -= WATER_SINK_SPEED;
        } else {
            // measured (`move_water_swim` tick 382): looking up while swimming forward adds
            // twice the forward climb, capped at the max speed; looking down adds nothing
            let climb = (fmove * forward.z * 2.0).clamp(0.0, max_speed);
            wish.z += cmd.up_move + climb;
        }
        let mut wish_speed = wish.length();
        if wish_speed > max_speed {
            wish_speed = max_speed;
        }
        wish_speed *= WATER_SPEED_SCALE;
        let speed = st.velocity.length();
        let mut new_speed = 0.0;
        if speed > 0.0 {
            new_speed = speed - dt * speed * self.cfg.friction * st.surface_friction;
            if new_speed < 0.1 {
                new_speed = 0.0;
            }
            st.velocity *= new_speed / speed;
        }
        if wish_speed >= 0.1 {
            let add = wish_speed - new_speed;
            if add > 0.0 {
                let accel = (self.cfg.accelerate * wish_speed * dt * st.surface_friction).min(add);
                st.velocity += wish.normalize_or_zero() * accel;
            }
        }
        let dest = st.origin + st.velocity * dt;
        let t = self.trace(st, st.origin, dest);
        if t.fraction >= 1.0 {
            st.origin = dest;
            return;
        }
        if st.on_ground {
            self.step_move(st, dt);
        } else {
            self.slide_move(st, dt);
        }
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
        // only rising fast rules the ground out: a fall of 1089 u/s is grounded by the probe
        // 0.3 u above the floor (damage_fall_progressive)
        if st.velocity.z > NON_JUMP_VELOCITY && !st.ground_push {
            st.on_ground = false;
            return;
        }
        st.ground_push = false;
        let t = self.trace(st, st.origin, st.origin - Vec3::Z * self.cfg.ground_probe);
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
            let accel_speed =
                (self.cfg.air_accelerate * dt * wish_speed * st.surface_friction).min(add);
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
        // the move the boost sees is the command's, scaled down to max speed like the move
        // itself (forward 10000 on a standing walker jumps with a 100 u/s boost, not 300)
        let len = Vec3::new(cmd.forward_move, cmd.side_move, cmd.up_move).length();
        let max_speed = self.max_speed(st, cmd);
        let scale = if len > max_speed && len > 0.0 {
            max_speed / len
        } else {
            1.0
        };
        st.boost_forward = cmd.forward_move * scale;
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

    /// The player's max speed as the footstep timer sees it (`GetMaxSpeed`): run or walk
    /// speed by the sprint button; crouching does not lower it (measured `move_step_duck`).
    pub fn step_max_speed(&self, cmd: &UserCmd) -> f32 {
        if cmd.buttons & buttons::SPEED != 0 {
            self.cfg.run_speed
        } else {
            self.cfg.walk_speed
        }
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

/// One footstep to play: volume and which foot (`stepright` or `stepleft`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub volume: f32,
    pub right: bool,
}

/// Footstep timing and volume, measured on srcds (`move_step_*`, movement.md "Footsteps").
/// The step itself (wave by the surface under the feet) is the server's business.
/// Fall speed from which a landing hurts (`move_fall_*`, 526.5 u/s); the landing step's
/// volume thresholds are tied to it.
pub const FALL_SAFE_SPEED: f32 = 526.5;

#[derive(Clone, Copy, Debug, Default)]
pub struct Footsteps {
    /// Milliseconds until the next step may play.
    pub time_ms: f32,
    /// The foot the last step used; steps alternate, jumps included.
    pub right: bool,
    /// Milliseconds until a held jump in the water voices `Player.Swim` again.
    pub swim_ms: f32,
    /// Milliseconds of climbing until the next ladder step.
    pub ladder_ms: f32,
}

impl Footsteps {
    /// Standing: a step needs 90 u/s, 220 and up is running. Ducked: 60 and 80.
    const STAND: (f32, f32) = (90.0, 220.0);
    const DUCKED: (f32, f32) = (60.0, 80.0);

    /// Before the tick's move, with the state entering the tick (`UpdateStepSound`): counts the
    /// timer down by the tick and, when it has run out on the ground at walking speed, steps.
    /// `max_speed` is [`Mover::step_max_speed`].
    pub fn before_move(&mut self, st: &PlayerState, max_speed: f32, tick_ms: f32) -> Option<Step> {
        if self.time_ms > 0.0 {
            self.time_ms = (self.time_ms - tick_ms).max(0.0);
        }
        if self.swim_ms > 0.0 {
            self.swim_ms = (self.swim_ms - tick_ms).max(0.0);
        }
        // measured (`move_water_walk`): no steps once the water reaches the waist, even on the bottom
        if !st.on_ground || st.water_level >= 2 || self.time_ms > 0.0 {
            return None;
        }
        let speed = st.velocity.truncate().length();
        let (walk, run) = if st.ducked { Self::DUCKED } else { Self::STAND };
        if speed < walk {
            return None;
        }
        let mut volume = if speed < run { 0.2 } else { 0.5 };
        if st.ducked {
            volume *= 0.65;
        }
        // GMod's PlayerStepSoundTime: by max speed, 50 ms more crouching
        let interval = if max_speed <= 100.0 {
            400.0
        } else if max_speed <= 300.0 {
            350.0
        } else {
            250.0
        };
        self.time_ms = interval + if st.ducked { 50.0 } else { 0.0 };
        self.right = !self.right;
        Some(Step {
            volume,
            right: self.right,
        })
    }

    /// A jump plays a step at full volume on the jump tick and takes its turn in the alternation;
    /// landing plays nothing by itself.
    pub fn on_jump(&mut self) -> Step {
        self.right = !self.right;
        Step {
            volume: 1.0,
            right: self.right,
        }
    }

    /// Landing step (`move_step_fall_*`): a fall of at least `FALL_SAFE_SPEED` (the one that
    /// hurts) lands at 1.0, one of more than half that at 0.85 (342 and 486 u/s), a slower one
    /// (243 u/s, a standing jump) makes no sound. The timer is left alone.
    pub fn on_land(&mut self, fall_speed: f32) -> Option<Step> {
        let volume = if fall_speed >= FALL_SAFE_SPEED {
            1.0
        } else if fall_speed > FALL_SAFE_SPEED / 2.0 {
            0.85
        } else {
            return None;
        };
        self.right = !self.right;
        Some(Step {
            volume,
            right: self.right,
        })
    }

    /// Ladder steps (`ladder*.wav`, measured `move_ladder_*`): one at full volume on the tick
    /// the ladder is grabbed (no timer), then on the tick after a climbing tick whenever
    /// 450 ms have passed since the last climbing step, idle time included.
    pub fn ladder_step(
        &mut self,
        prev: MoveType,
        prev_vz: f32,
        st: &PlayerState,
        tick_ms: f32,
    ) -> Option<Step> {
        if self.ladder_ms > 0.0 {
            self.ladder_ms = (self.ladder_ms - tick_ms).max(0.0);
        }
        if prev == MoveType::Walk && st.move_type == MoveType::LadderMount {
            self.right = !self.right;
            return Some(Step {
                volume: 1.0,
                right: self.right,
            });
        }
        if prev != MoveType::Ladder || prev_vz == 0.0 || self.ladder_ms > 0.0 {
            return None;
        }
        self.ladder_ms = 450.0;
        self.right = !self.right;
        Some(Step {
            volume: 0.5,
            right: self.right,
        })
    }

    /// The water sound a tick voices (after `before_move`), from the water level entering the
    /// tick and the state after the move: `Player.Swim` (volume 0.5) on wading in from dry
    /// ground (`move_water_walk` tick 117) and on a jump held while the tick starts swimming,
    /// once a second (`move_water_swim` ticks 160, 250, 328: the held jump at 227 came while
    /// bobbing at the surface with the waist out, 250 is the next tick that starts at the waist).
    pub fn water_sound(
        &mut self,
        prev_level: u8,
        st: &PlayerState,
        cmd: &UserCmd,
    ) -> Option<&'static str> {
        if prev_level == 0 && st.water_level > 0 {
            return Some("Player.Swim");
        }
        if prev_level >= 2 && cmd.buttons & buttons::JUMP != 0 && self.swim_ms <= 0.0 {
            self.swim_ms = 1000.0;
            return Some("Player.Swim");
        }
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn footsteps_follow_the_measured_series() {
        use super::{Footsteps, PlayerState};
        use glam::Vec3;
        let walk = |speed: f32, ducked: bool| PlayerState {
            velocity: Vec3::new(0.0, speed, 0.0),
            on_ground: true,
            ducked,
            ..Default::default()
        };
        // move_step_walk: speeds entering ticks 0..4 are 0, 30, 56, 80, 100; steps at 4, 28, 52
        let mut f = Footsteps::default();
        let mut steps = Vec::new();
        for (t, speed) in [0.0, 30.0, 56.4, 79.6, 100.1]
            .into_iter()
            .chain(std::iter::repeat(200.0))
            .take(60)
            .enumerate()
        {
            if let Some(s) = f.before_move(&walk(speed, false), 200.0, 15.0) {
                steps.push((t, s.volume, s.right));
            }
        }
        assert_eq!(
            steps,
            vec![(4, 0.2, true), (28, 0.2, false), (52, 0.2, true)]
        );
        // move_step_run: 17 ticks apart at 0.5
        let mut f = Footsteps::default();
        let ticks: Vec<usize> = (0..40)
            .filter(|_| f.before_move(&walk(400.0, false), 400.0, 15.0).is_some())
            .collect();
        assert_eq!(ticks, vec![0, 17, 34]);
        assert_eq!(f.before_move(&walk(309.8, false), 400.0, 15.0), None);
        // move_step_duck: 27 ticks apart; 176 ducked already runs, 60 walks
        let mut f = Footsteps::default();
        assert_eq!(
            f.before_move(&walk(176.0, true), 200.0, 15.0)
                .map(|s| s.volume),
            Some(0.325)
        );
        let ticks: Vec<usize> = (1..60)
            .filter(|_| f.before_move(&walk(60.0, true), 200.0, 15.0).is_some())
            .collect();
        assert_eq!(ticks, vec![27, 54]);
        assert!((f.before_move(&walk(60.0, true), 200.0, 15.0).is_none()));
        // airborne and below the walk speed: nothing; a jump is a full step on the other foot
        let mut f = Footsteps::default();
        assert_eq!(
            f.before_move(
                &PlayerState {
                    velocity: Vec3::new(0.0, 200.0, 0.0),
                    ..Default::default()
                },
                200.0,
                15.0
            ),
            None
        );
        assert_eq!(f.before_move(&walk(85.4, false), 200.0, 15.0), None);
        assert!(f.on_jump().right);
        assert_eq!(f.on_land(243.0), None);
        assert_eq!(f.on_land(342.0).map(|s| s.volume), Some(0.85));
        assert_eq!(f.on_land(486.0).map(|s| s.volume), Some(0.85));
        assert_eq!(f.on_land(594.0).map(|s| s.volume), Some(1.0));
        assert_eq!(f.on_land(1035.0).map(|s| s.volume), Some(1.0));
        assert_eq!(
            f.before_move(&walk(130.0, false), 200.0, 15.0)
                .map(|s| s.right),
            Some(false)
        );
    }

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

    /// Flat ground at 0 with a pool (surface at 0) over a floor at -200 in x 1000..3000.
    fn pool_world() -> World {
        const SOLID: i32 = 0x1;
        const WATER: i32 = 0x20;
        World::from_boxes_contents(&[
            (
                Vec3::new(-4096.0, -4096.0, -64.0),
                Vec3::new(1000.0, 4096.0, 0.0),
                SOLID,
            ),
            (
                Vec3::new(1000.0, -4096.0, -264.0),
                Vec3::new(3000.0, 4096.0, -200.0),
                SOLID,
            ),
            (
                Vec3::new(1000.0, -4096.0, -200.0),
                Vec3::new(3000.0, 4096.0, 0.0),
                WATER,
            ),
        ])
    }

    fn swim(cmd: UserCmd, ticks: usize) -> (PlayerState, Vec<Vec3>) {
        let world = pool_world();
        let m = Mover::new(MoveConfig::default(), &world);
        let mut st = PlayerState {
            origin: Vec3::new(2000.0, 0.0, -100.0),
            ..PlayerState::default()
        };
        let mut vels = Vec::new();
        for _ in 0..ticks {
            m.tick(&mut st, &cmd);
            vels.push(st.velocity);
        }
        (st, vels)
    }

    /// Flat floor at 0, a wall at y 100 and a ladder 24 u in front of it up to z 120,
    /// under a ceiling at 200.
    fn ladder_world() -> World {
        World::from_boxes(&[
            (
                Vec3::new(-512.0, -512.0, -64.0),
                Vec3::new(512.0, 512.0, 0.0),
            ),
            (
                Vec3::new(-512.0, 100.0, 0.0),
                Vec3::new(512.0, 164.0, 300.0),
            ),
            (
                Vec3::new(-512.0, -512.0, 200.0),
                Vec3::new(512.0, 512.0, 264.0),
            ),
        ])
        .with_ladders(&[gm_collision::Ladder {
            bottom: Vec3::new(0.0, 76.0, -8.0),
            top: Vec3::new(0.0, 76.0, 120.0),
        }])
    }

    fn climb_cmd(buttons: u32, pitch: f32) -> UserCmd {
        UserCmd {
            buttons,
            forward_move: if buttons & buttons::FORWARD != 0 {
                450.0
            } else {
                0.0
            },
            view_angles: Vec3::new(pitch, 90.0, 0.0),
            ..UserCmd::default()
        }
    }

    /// Walks the player into the ladder from 24 u out and climbs 10 ticks.
    fn on_ladder(world: &World) -> (Mover<'_>, PlayerState) {
        let m = Mover::new(MoveConfig::default(), world);
        let mut st = PlayerState {
            origin: Vec3::new(0.0, 52.0, 0.0),
            on_ground: true,
            ..PlayerState::default()
        };
        for _ in 0..20 {
            m.tick(&mut st, &climb_cmd(buttons::FORWARD, -45.0));
        }
        assert_eq!(st.move_type, MoveType::Ladder);
        (m, st)
    }

    #[test]
    fn ladder_mount_pulls_in_at_200_then_climbs() {
        let world = ladder_world();
        let m = Mover::new(MoveConfig::default(), &world);
        let mut st = PlayerState {
            origin: Vec3::new(0.0, 52.0, 0.0),
            on_ground: true,
            ..PlayerState::default()
        };
        let cmd = climb_cmd(buttons::FORWARD, -45.0);
        // measured (`move_ladder_back`): the press tick stops the player where he is
        m.tick(&mut st, &cmd);
        assert_eq!(st.move_type, MoveType::LadderMount);
        assert_eq!(st.origin, Vec3::new(0.0, 52.0, 0.0));
        assert_eq!(st.velocity, Vec3::ZERO);
        for i in 1..=8 {
            m.tick(&mut st, &cmd);
            assert!(
                (st.origin.y - (52.0 + 3.0 * i as f32)).abs() < 1e-3,
                "tick {i}"
            );
        }
        assert_eq!(st.move_type, MoveType::Ladder);
        assert_eq!(st.origin.z, 0.0);
        m.tick(&mut st, &cmd);
        assert_eq!(st.velocity, Vec3::new(0.0, 0.0, 200.0));
        assert_eq!(st.origin.z, 3.0);
        assert!(!st.on_ground);
    }

    #[test]
    fn ladder_direction_by_view_and_button() {
        let world = ladder_world();
        let (m, start) = on_ladder(&world);
        // measured (`move_ladder_thresh`): forward at pitch 17 climbs, 18 descends; back and
        // the strafes point their own way, strafes being level always climb
        for (buttons, pitch, up) in [
            (buttons::FORWARD, 0.0, true),
            (buttons::FORWARD, 17.0, true),
            (buttons::FORWARD, 18.0, false),
            (buttons::FORWARD, 25.0, false),
            (buttons::BACK, 0.0, true),
            (buttons::BACK, 25.0, true),
            (buttons::BACK, -45.0, false),
            (buttons::MOVERIGHT, -45.0, true),
            (buttons::MOVELEFT, 25.0, true),
        ] {
            let mut st = start;
            m.tick(&mut st, &climb_cmd(buttons, pitch));
            assert_eq!(
                st.velocity.z,
                if up { 200.0 } else { -200.0 },
                "{buttons} {pitch}"
            );
        }
        let mut st = start;
        m.tick(&mut st, &climb_cmd(0, -45.0));
        assert_eq!(st.velocity, Vec3::ZERO);
        assert_eq!(st.origin, start.origin);
    }

    #[test]
    fn ladder_ends_stop_the_climb_and_jump_leaves() {
        let world = ladder_world();
        let (m, mut st) = on_ladder(&world);
        for _ in 0..60 {
            m.tick(&mut st, &climb_cmd(buttons::FORWARD, -45.0));
        }
        // the ceiling at 200 holds the 72 u hull at 128, still on the ladder, speed zeroed
        assert_eq!(st.move_type, MoveType::Ladder);
        assert!((st.origin.z - 128.0).abs() < 0.1, "{}", st.origin.z);
        assert_eq!(st.velocity, Vec3::ZERO);
        for _ in 0..60 {
            m.tick(&mut st, &climb_cmd(buttons::FORWARD, 45.0));
        }
        assert!(st.origin.z.abs() < 0.1, "{}", st.origin.z);
        assert_eq!(st.move_type, MoveType::Ladder);
        // measured (`move_ladder_jump`): off along the view at 200 plus 50 up, then gravity
        let mut cmd = climb_cmd(buttons::FORWARD | buttons::JUMP, -45.0);
        cmd.view_angles.y = 270.0;
        m.tick(&mut st, &cmd);
        assert_eq!(st.move_type, MoveType::Walk);
        assert!(!st.on_ground);
        assert!(st.velocity.y < -100.0, "{}", st.velocity.y);
        assert!(
            (st.velocity.z - (141.42 + 50.0 - 9.0)).abs() < 0.1,
            "{}",
            st.velocity.z
        );
    }

    #[test]
    fn ladder_steps_mount_then_every_450ms_of_wall_time() {
        let mut f = Footsteps::default();
        let mount = PlayerState {
            move_type: MoveType::LadderMount,
            ..PlayerState::default()
        };
        let climb = PlayerState {
            move_type: MoveType::Ladder,
            ..PlayerState::default()
        };
        let s = f.ladder_step(MoveType::Walk, 0.0, &mount, 15.0).unwrap();
        assert_eq!(s.volume, 1.0);
        for _ in 0..8 {
            assert!(f
                .ladder_step(MoveType::LadderMount, 0.0, &climb, 15.0)
                .is_none());
        }
        // first climbing tick: nothing yet, the step follows it
        assert!(f
            .ladder_step(MoveType::LadderMount, 0.0, &climb, 15.0)
            .is_none());
        assert_eq!(
            f.ladder_step(MoveType::Ladder, 200.0, &climb, 15.0)
                .unwrap()
                .volume,
            0.5
        );
        for _ in 0..29 {
            assert!(f
                .ladder_step(MoveType::Ladder, 200.0, &climb, 15.0)
                .is_none());
        }
        assert!(f
            .ladder_step(MoveType::Ladder, 200.0, &climb, 15.0)
            .is_some());
        // standing still runs the timer down too
        for _ in 0..40 {
            assert!(f.ladder_step(MoveType::Ladder, 0.0, &climb, 15.0).is_none());
        }
        assert!(f
            .ladder_step(MoveType::Ladder, -200.0, &climb, 15.0)
            .is_some());
    }

    #[test]
    fn water_levels_feet_waist_eyes() {
        let world = pool_world();
        let m = Mover::new(MoveConfig::default(), &world);
        let mut st = PlayerState::default();
        let mut cache = WaterCache::default();
        // probes at origin + 1, + 36 (half the hull), + 64 (the eyes) against a surface at 0
        for (z, level) in [
            (-0.5, 0),
            (-1.5, 1),
            (-30.0, 1),
            (-37.0, 2),
            (-60.0, 2),
            (-65.0, 3),
        ] {
            st.origin = Vec3::new(2000.0, 0.0, z);
            cache = WaterCache::default();
            m.check_water(&mut st, &mut cache);
            assert_eq!(st.water_level, level, "z {z}");
        }
        // within a tick a probe that moved less than 1 u keeps its last lookup
        st.origin = Vec3::new(2000.0, 0.0, -30.0);
        m.check_water(&mut st, &mut cache);
        assert_eq!(st.water_level, 1);
        st.origin.z = -30.9;
        m.check_water(&mut st, &mut cache);
        assert_eq!(st.water_level, 1);
        st.origin.z = -37.0;
        m.check_water(&mut st, &mut cache);
        assert_eq!(st.water_level, 2);
    }

    #[test]
    fn swim_forward_measured_ramp_and_cap() {
        let cmd = UserCmd {
            forward_move: 450.0,
            view_angles: Vec3::ZERO,
            ..UserCmd::default()
        };
        let (_, v) = swim(cmd, 200);
        // measured on srcds (`move_water_swim`): 24, 45.1, 63.7, 80.1 ... up to 160
        for (i, want) in [24.0, 45.1, 63.7, 80.1, 94.5].into_iter().enumerate() {
            assert!(
                (v[i].x - want).abs() < 0.2,
                "tick {i}: {} vs {want}",
                v[i].x
            );
        }
        assert!((v[199].x - 160.0).abs() < 0.5, "cap {}", v[199].x);
    }

    #[test]
    fn swim_idle_sinks_jump_rises() {
        // 60 ticks: the -48 u/s sink is reached in 7 and the floor (100 u down) is not
        let (_, v) = swim(UserCmd::default(), 60);
        assert!((v[59].z + 48.0).abs() < 0.5, "sink {}", v[59].z);
        let cmd = UserCmd {
            buttons: buttons::JUMP,
            ..UserCmd::default()
        };
        // the held jump sets 100 u/s up every tick, the water acceleration adds a few on top
        // (with forward input as well the game settles at 100.7, `move_water_swim` 206..209)
        let (st, v) = swim(cmd, 10);
        assert!(v[0].z > 100.0, "rise {}", v[0].z);
        assert!(v[9].z > 100.0 && v[9].z < 110.0, "settled {}", v[9].z);
        assert!(!st.on_ground);
    }

    #[test]
    fn water_sounds_swim_rule() {
        let mut f = Footsteps::default();
        let dry = PlayerState::default();
        let wet = PlayerState {
            water_level: 1,
            ..PlayerState::default()
        };
        let jump = UserCmd {
            buttons: buttons::JUMP,
            ..UserCmd::default()
        };
        assert_eq!(
            f.water_sound(0, &wet, &UserCmd::default()),
            Some("Player.Swim")
        );
        assert_eq!(f.water_sound(1, &wet, &UserCmd::default()), None);
        assert_eq!(f.water_sound(2, &wet, &jump), Some("Player.Swim"));
        // held: once a second (1000 ms, 15 ms ticks)
        for _ in 0..66 {
            f.before_move(&dry, 200.0, 15.0);
            assert_eq!(f.water_sound(2, &wet, &jump), None);
        }
        f.before_move(&dry, 200.0, 15.0);
        assert_eq!(f.water_sound(2, &wet, &jump), Some("Player.Swim"));
        // no footsteps with the water at the waist, even on the bottom
        let bottom = PlayerState {
            water_level: 2,
            on_ground: true,
            velocity: Vec3::new(160.0, 0.0, 0.0),
            ..PlayerState::default()
        };
        assert_eq!(f.before_move(&bottom, 200.0, 15.0), None);
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
