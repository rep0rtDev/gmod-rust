//! Weapon projectiles as server entities: the RPG rocket, the crossbow bolt and the AR2 ball
//! fly by the rules measured per tick in `fire_*_proj` (docs/spec/server.md, "Projectiles");
//! the frag grenade is a physics object and not simulated here yet.

use crate::weapons::Launch;
use glam::Vec3;
use gm_collision::{World, MASK_SOLID};
use gm_physics::{BodyHandle, PropDesc};

pub const DT: f32 = 0.015;
const GRAVITY: f32 = 600.0;
/// Rocket: muzzle offset from the eye (forward, right, up), launch speed along the aim plus the
/// upward kick, ticks of free fall before ignition, flight speed afterwards. The rocket moves
/// one tick from the muzzle in its spawn tick (recorded 16.5 forward, 1.15 below the eye).
const ROCKET_MUZZLE: (f32, f32, f32) = (12.0, 6.0, -3.0);
const ROCKET_LAUNCH: f32 = 300.0;
const ROCKET_KICK: f32 = 119.0;
const ROCKET_IGNITE_TICKS: u32 = 20;
const ROCKET_SPEED: f32 = 1500.0;
/// The ignited rocket steers to the laser dot (the owner's aim trace) as a damped second-order
/// turn in angles: `turn = ROCKET_TURN_KEEP * turn + ROCKET_TURN_GAIN * error` (degrees per
/// tick), fitted on four shots (rms 0.013 deg/tick). The first turn after ignition is a fixed
/// kick down and to the left (measured 0.50-0.91 pitch, 0.16-0.19 yaw; not explained by the
/// error to the dot, it shrinks as the error grows).
const ROCKET_TURN_KEEP: f32 = 0.856;
const ROCKET_TURN_GAIN: f32 = 0.00887;
const ROCKET_FIRST_TURN: glam::Vec2 = glam::Vec2::new(0.75, 0.17);
const BOLT_SPEED: f32 = 3500.0;
const BOLT_GRAVITY: f32 = 0.05;
const BOLT_REFLECT: f32 = 0.75;
/// The bolt's collision box half-extent: it stops that far before the surface (6.4 u short on
/// the grazing ramp of `fire_crossbow_proj`).
pub const BOLT_HALF: f32 = 1.0;
const BALL_SPEED: f32 = 1000.0;
/// The ball is a 10 u sphere: its centre reflects 10 u (along the normal) before the surface
/// (45 u early on the grazing ramp of `fire_ar2_proj`).
pub const BALL_RADIUS: f32 = 10.0;
/// SMG1 grenade (`fire_smg1_grenade*`, `hit_smg1_grenade`): 1000 along the aim from the eye,
/// FLYGRAVITY at 400 u/s^2 (6 u/s per tick), a 3-u box; it stops on the surface it hits and is
/// removed (exploding) the next tick, a player it reaches explodes it at once.
const AR2_GRENADE_SPEED: f32 = 1000.0;
const AR2_GRENADE_GRAVITY: f32 = 400.0 / 600.0;
pub const AR2_GRENADE_HALF: f32 = 3.0;
const BALL_STOP_TICKS: u32 = 267;
const BALL_LIFE_TICKS: u32 = 300;
/// Grenade (`npc_grenade_frag`): a VPHYSICS body of `GRENADE_MODEL` thrown from 8 u right of
/// the eye, 33.7 u ahead and 13.9 u up (yaw frame; measured at pitch -20 only) at 1189 along
/// the aim plus 110 up, its body x axis to the right of the aim (angles (0, yaw - 90, 0)),
/// rolling 600 deg/s about it plus a random pitch spin (three throws: 487, -633, 200 deg/s),
/// so its path after the first bounce is random; it explodes 175 ticks after the throw.
pub const GRENADE_MODEL: &str = "models/weapons/w_npcnade.mdl";
const GRENADE_SPEED: f32 = 1189.0;
const GRENADE_KICK: f32 = 110.0;
const GRENADE_OFFSET: Vec3 = Vec3::new(8.0, 33.69, 13.91);
const GRENADE_ROLL: f32 = 600.0;
const GRENADE_SPIN: f32 = 700.0;
const GRENADE_FUSE_TICKS: u32 = 175;

/// The rigid-body world grenades live in, with their model and the server's random stream.
pub struct Physics<'a> {
    pub world: &'a mut gm_physics::World,
    pub grenade: Option<&'a crate::ModelInfo>,
    pub rng: &'a mut u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveType {
    None = 0,
    Fly = 4,
    FlyGravity = 5,
    VPhysics = 6,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Projectile {
    pub kind: Launch,
    pub owner: u32,
    pub origin: Vec3,
    pub velocity: Vec3,
    pub movetype: MoveType,
    /// Ticks since launch (0 in the launch tick).
    pub age: u32,
    /// Where the rocket homes: the aim point on the world at launch.
    pub target: Vec3,
    /// The aim direction at launch: the rocket ignites along it.
    pub aim: Vec3,
    pub gravity: f32,
    /// Rocket heading (pitch, yaw in degrees) and its turn per tick once ignited.
    pub heading: glam::Vec2,
    pub turn: glam::Vec2,
    /// The grenade's rigid body.
    pub body: Option<BodyHandle>,
    /// Touched the world: removed (exploding) on the next step, the SMG1 grenade's rule.
    pub dead: bool,
}

/// What a step did to the world.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Event {
    None,
    /// Removed this tick at the point (rocket explosion, bolt on the sky, ball expiry).
    Removed(Vec3),
}

pub fn class_name(kind: Launch) -> &'static str {
    match kind {
        Launch::Rocket => "rpg_missile",
        Launch::Bolt => "crossbow_bolt",
        Launch::Grenade => "npc_grenade_frag",
        Launch::Ball => "prop_combine_ball",
        Launch::Smg1Grenade => "grenade_ar2",
    }
}

fn basis(pitch: f32, yaw: f32) -> (Vec3, Vec3, Vec3) {
    let forward = gm_sandbox::forward(pitch, yaw);
    let right = gm_sandbox::forward(0.0, yaw - 90.0);
    let up = right.cross(forward);
    (forward, right, up)
}

/// Spawn in the launch tick from the owner's eye and view angles; the bolt and the ball have
/// already moved one tick when the recording first sees them (their first think runs in the
/// spawn frame), the rocket sits at the muzzle.
pub fn spawn(
    kind: Launch,
    owner: u32,
    eye: Vec3,
    pitch: f32,
    yaw: f32,
    world: &World,
    phys: Option<&mut Physics<'_>>,
) -> Option<Projectile> {
    let (forward, right, up) = basis(pitch, yaw);
    let target = world
        .trace_ray(eye, eye + forward * 56755.84, MASK_SOLID)
        .end;
    let mut p = match kind {
        Launch::Rocket => Projectile {
            kind,
            owner,
            origin: eye
                + forward * ROCKET_MUZZLE.0
                + right * ROCKET_MUZZLE.1
                + up * ROCKET_MUZZLE.2,
            velocity: forward * ROCKET_LAUNCH + Vec3::new(0.0, 0.0, ROCKET_KICK),
            movetype: MoveType::FlyGravity,
            age: 0,
            target,
            aim: forward,
            heading: glam::Vec2::new(pitch, yaw),
            turn: glam::Vec2::ZERO,
            body: None,
            dead: false,
            gravity: 1.0,
        },
        Launch::Bolt => Projectile {
            kind,
            owner,
            origin: eye,
            velocity: forward * BOLT_SPEED,
            movetype: MoveType::FlyGravity,
            age: 0,
            target,
            aim: forward,
            heading: glam::Vec2::new(pitch, yaw),
            turn: glam::Vec2::ZERO,
            body: None,
            dead: false,
            gravity: BOLT_GRAVITY,
        },
        Launch::Smg1Grenade => Projectile {
            kind,
            owner,
            origin: eye,
            velocity: forward * AR2_GRENADE_SPEED,
            movetype: MoveType::FlyGravity,
            age: 0,
            target,
            aim: forward,
            heading: glam::Vec2::new(pitch, yaw),
            turn: glam::Vec2::ZERO,
            body: None,
            dead: false,
            gravity: AR2_GRENADE_GRAVITY,
        },
        Launch::Ball => Projectile {
            kind,
            owner,
            origin: eye,
            velocity: forward * BALL_SPEED,
            movetype: MoveType::VPhysics,
            age: 0,
            target,
            aim: forward,
            heading: glam::Vec2::new(pitch, yaw),
            turn: glam::Vec2::ZERO,
            body: None,
            dead: false,
            gravity: 0.0,
        },
        Launch::Grenade => {
            let phys = phys?;
            let info = phys.grenade?;
            let (forward, right, _) = basis(pitch, yaw);
            let ahead = gm_sandbox::forward(0.0, yaw);
            let origin = eye
                + right * GRENADE_OFFSET.x
                + ahead * GRENADE_OFFSET.y
                + Vec3::Z * GRENADE_OFFSET.z;
            let velocity = forward * GRENADE_SPEED + Vec3::Z * GRENADE_KICK;
            let spin = (xorshift(phys.rng) * 2.0 - 1.0) * GRENADE_SPIN;
            let body = phys
                .world
                .add_prop(&PropDesc {
                    hulls: &info.hulls,
                    mass: info.mass,
                    volume: info.volume,
                    position: origin,
                    angles: (0.0, yaw - 90.0, 0.0),
                    velocity,
                    angular_velocity: right * GRENADE_ROLL + ahead * spin,
                    surface: info.surface,
                    linear_damping: info.linear_damping,
                    angular_damping: info.angular_damping,
                    drag_axis_areas: info.drag_axis_areas,
                    inertia: info.inertia,
                })
                .ok()?;
            Projectile {
                kind,
                owner,
                origin,
                velocity,
                movetype: MoveType::VPhysics,
                age: 0,
                target: origin,
                aim: forward,
                heading: glam::Vec2::new(pitch, yaw),
                turn: glam::Vec2::ZERO,
                gravity: GRAVITY,
                body: Some(body),
                dead: false,
            }
        }
    };
    match kind {
        Launch::Rocket => {
            p.fly(world);
        }
        Launch::Bolt | Launch::Smg1Grenade => {
            p.gravity_half();
            p.fly(world);
            p.gravity_half();
        }
        Launch::Ball => {
            p.fly_bounce(world);
        }
        Launch::Grenade => {}
    }
    Some(p)
}

impl Projectile {
    /// One tick; `Event::Removed` means the projectile is gone after this call.
    pub fn step(&mut self, world: &World, phys: Option<&mut Physics<'_>>) -> Event {
        self.age += 1;
        match self.kind {
            Launch::Rocket => self.step_rocket(world),
            Launch::Bolt => self.step_bolt(world),
            Launch::Ball => self.step_ball(world),
            Launch::Grenade => self.step_grenade(phys),
            Launch::Smg1Grenade => self.step_ar2_grenade(world),
        }
    }

    /// The contact tick shows the grenade at the surface with its velocity; it is removed the
    /// tick after (a player it reaches is resolved by the server at once).
    fn step_ar2_grenade(&mut self, world: &World) -> Event {
        if self.dead {
            return Event::Removed(self.origin);
        }
        self.gravity_half();
        let hit = self.fly(world).is_some();
        self.gravity_half();
        // a touch, not a trace hit: the game's trace counts a box ending within its 1/32-u
        // plane epsilon as a hit (`fire_smg1_grenade`: the floor was 0.008 u under the box);
        // probe 1/8 u on along the flight, which covers that gap down to 15 deg of incidence
        // (probed per axis: gm-collision misses an oblique 0.125-u box sweep that crosses a
        // plane 0.008 u away, the straight probe hits it - open)
        let touching = || {
            let dir = self.velocity.normalize_or_zero();
            (0..3).any(|a| {
                dir[a] != 0.0 && {
                    let mut probe = Vec3::ZERO;
                    probe[a] = dir[a].signum() * 0.125;
                    self.trace(world, self.origin + probe).fraction < 1.0
                }
            })
        };
        if hit || touching() {
            self.dead = true;
        }
        Event::None
    }

    /// The grenade follows its rigid body; the fuse removes it.
    fn step_grenade(&mut self, phys: Option<&mut Physics<'_>>) -> Event {
        let (Some(phys), Some(body)) = (phys, self.body) else {
            return Event::Removed(self.origin);
        };
        let st = phys.world.state(body);
        self.origin = st.position;
        self.velocity = st.velocity;
        if self.age >= GRENADE_FUSE_TICKS {
            phys.world.remove_body(body);
            self.body = None;
            return Event::Removed(self.origin);
        }
        Event::None
    }

    /// Half a tick of gravity: MOVETYPE_FLYGRAVITY applies it before and after the move, so the
    /// move uses the mid-tick velocity (rocket z steps 114.5, 105.5, ... u/s from 119, 110, ...).
    fn gravity_half(&mut self) {
        self.velocity.z -= GRAVITY * self.gravity * DT * 0.5;
    }

    fn trace(&self, world: &World, end: Vec3) -> gm_collision::Trace {
        let half = match self.kind {
            Launch::Bolt => Some(BOLT_HALF),
            Launch::Smg1Grenade => Some(AR2_GRENADE_HALF),
            _ => None,
        };
        if let Some(half) = half {
            let h = Vec3::splat(half);
            world.trace_box(self.origin, end, -h, h, MASK_SOLID)
        } else {
            world.trace_ray(self.origin, end, MASK_SOLID)
        }
    }

    /// Move along the velocity; on a hit stop at the surface and return its normal.
    fn fly(&mut self, world: &World) -> Option<Vec3> {
        let end = self.origin + self.velocity * DT;
        let tr = self.trace(world, end);
        if tr.fraction < 1.0 {
            self.origin = tr.end;
            return Some(tr.normal);
        }
        self.origin = end;
        None
    }

    /// Move along the velocity, reflecting off whatever is hit and finishing the tick with the
    /// reflected velocity (the ball is a physics object: its bounce happens inside the step).
    /// The ball is a sphere: its centre turns `BALL_RADIUS` along the normal before the plane.
    fn fly_bounce(&mut self, world: &World) {
        let mut left = DT;
        for _ in 0..4 {
            let dir = self.velocity.normalize_or_zero();
            let want = self.velocity.length() * left;
            let tr = self.trace(world, self.origin + dir * (want + BALL_RADIUS * 8.0));
            let approach = -tr.normal.dot(dir);
            let travel = if tr.fraction < 1.0 && approach > 1e-3 {
                (tr.end - self.origin).length() - BALL_RADIUS / approach
            } else {
                f32::INFINITY
            };
            if travel >= want {
                self.origin += dir * want;
                return;
            }
            self.origin += dir * travel.max(0.0);
            left *= 1.0 - travel.max(0.0) / want;
            let n = tr.normal;
            self.velocity -= 2.0 * self.velocity.dot(n) * n;
            if left <= 1e-6 {
                return;
            }
        }
    }

    fn step_rocket(&mut self, world: &World) -> Event {
        if self.movetype == MoveType::FlyGravity {
            if self.age >= ROCKET_IGNITE_TICKS {
                self.movetype = MoveType::Fly;
                self.velocity = self.aim * ROCKET_SPEED;
            } else {
                self.gravity_half();
            }
        } else {
            self.turn = if self.age == ROCKET_IGNITE_TICKS + 1 {
                ROCKET_FIRST_TURN
            } else {
                let to = heading_of(self.target - self.origin);
                let mut err = to - self.heading;
                err.y = (err.y + 180.0).rem_euclid(360.0) - 180.0;
                self.turn * ROCKET_TURN_KEEP + err * ROCKET_TURN_GAIN
            };
            self.heading += self.turn;
            self.velocity = gm_sandbox::forward(self.heading.x, self.heading.y) * ROCKET_SPEED;
        }
        if self.fly(world).is_some() {
            return Event::Removed(self.origin);
        }
        if self.movetype == MoveType::FlyGravity {
            self.gravity_half();
        }
        Event::None
    }

    fn step_bolt(&mut self, world: &World) -> Event {
        if self.movetype == MoveType::None {
            return Event::None;
        }
        self.gravity_half();
        let hit = self.fly(world);
        self.gravity_half();
        if let Some(n) = hit {
            // the box trace stops its support along the normal plus 1/32 before the surface
            let surface =
                self.origin - n * (BOLT_HALF * (n.x.abs() + n.y.abs() + n.z.abs()) + 0.03125);
            if world.is_sky(surface, n) {
                return Event::Removed(self.origin);
            }
            let speed = self.velocity.length();
            let dir = self.velocity / speed;
            let hit_dot = n.dot(-dir);
            if hit_dot < 0.5 && speed > 100.0 {
                self.velocity = (2.0 * n * hit_dot + dir) * speed * BOLT_REFLECT;
                self.gravity = 1.0;
            } else {
                self.movetype = MoveType::None;
                self.velocity = Vec3::ZERO;
            }
        }
        Event::None
    }

    fn step_ball(&mut self, world: &World) -> Event {
        if self.age >= BALL_LIFE_TICKS {
            return Event::Removed(self.origin);
        }
        if self.age >= BALL_STOP_TICKS {
            self.movetype = MoveType::None;
            return Event::None;
        }
        self.fly_bounce(world);
        Event::None
    }
}

/// Pitch (positive down) and yaw in degrees of a direction, as Source angles.
fn heading_of(v: Vec3) -> glam::Vec2 {
    glam::Vec2::new(
        -v.z.atan2(v.x.hypot(v.y)).to_degrees(),
        v.y.atan2(v.x).to_degrees(),
    )
}

/// One xorshift64* step as a uniform number in [0, 1).
fn xorshift(state: &mut u64) -> f32 {
    let mut x = *state;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    *state = x;
    (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32 / (1u64 << 24) as f32
}
