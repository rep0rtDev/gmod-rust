//! Map entity I/O: the brush and logic entities of the BSP (`func_door`, `func_movelinear`,
//! `func_button`, `func_brush`, `func_rotating`, `trigger_multiple`, `logic_relay`,
//! `logic_timer`, `logic_auto`, ...), their inputs and `OnXxx` outputs
//! (`"target,input,param,delay,times"`), the delayed output queue, and the moving brushes as
//! player-trace convexes plus a static physics body each.
//!
//! The entity data is parsed by `gm-dedicated` (`LoadedWorld::map_entities`) because the
//! server does not hold the BSP; the server owns the state machines.

use crate::Server;
use glam::{Quat, Vec3};
use gm_collision::Convex;
use gm_physics::{BodyHandle, Surface};

/// Map entity ids start here (above the sandbox's and below the projectiles').
pub const MAP_ENT_ID_BASE: u32 = 0x2000_0000;

/// One convex hull of a brush model in entity space: side planes `(normal, dist)`, its
/// vertices, and the vertices inflated by `REST_GAP` for the physics body.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HullDef {
    pub planes: Vec<(Vec3, f32)>,
    pub points: Vec<Vec3>,
    pub phys_points: Vec<Vec3>,
}

/// A map entity as the BSP describes it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapEntityDef {
    pub classname: String,
    /// Every key-value pair of the entity block (outputs included).
    pub kv: Vec<(String, String)>,
    pub origin: Vec3,
    /// Pitch, yaw, roll in degrees.
    pub angles: Vec3,
    /// The brush model (`model "*N"`) hulls, empty for point entities.
    pub hulls: Vec<HullDef>,
    /// Bounds of the brush model in entity space.
    pub mins: Vec3,
    pub maxs: Vec3,
}

impl MapEntityDef {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.kv
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }
    fn num(&self, key: &str, default: f32) -> f32 {
        self.get(key)
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(default)
    }
    fn flags(&self) -> u32 {
        self.get("spawnflags")
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(0)
    }
}

/// One stored output: `name` fires `input` with `param` at `target` after `delay` seconds,
/// `times` more times (-1 = forever).
#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    pub name: String,
    pub target: String,
    pub input: String,
    pub param: String,
    pub delay: f32,
    pub times: i32,
}

/// Parses `"target,input,param,delay,times"` (Hammer writes `\x1b` for the comma in newer
/// maps).
pub fn parse_output(name: &str, value: &str) -> Option<Output> {
    let sep = if value.contains('\x1b') { '\x1b' } else { ',' };
    let mut f = value.split(sep);
    let target = f.next()?.trim().to_string();
    let input = f.next()?.trim().to_string();
    let param = f.next().unwrap_or("").trim().to_string();
    let delay = f.next().and_then(|d| d.trim().parse().ok()).unwrap_or(0.0);
    let times = f.next().and_then(|t| t.trim().parse().ok()).unwrap_or(-1);
    Some(Output {
        name: name.to_string(),
        target,
        input,
        param,
        delay,
        times,
    })
}

/// Direction of a Source `movedir` angle (pitch yaw roll; `-90 0 0` is up, `90 0 0` down).
pub fn angles_dir(angles: Vec3) -> Vec3 {
    let (p, y) = (angles.x.to_radians(), angles.y.to_radians());
    Vec3::new(p.cos() * y.cos(), p.cos() * y.sin(), -p.sin())
}

fn parse_vec3(s: &str) -> Option<Vec3> {
    let mut it = s.split_whitespace().map(|v| v.parse::<f32>().ok());
    Some(Vec3::new(it.next()??, it.next()??, it.next()??))
}

/// Which of a moving brush's two positions it is at or heading for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle {
    AtBottom,
    GoingUp,
    AtTop,
    GoingDown,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    /// `func_door` / `func_movelinear` / `func_button`: slides from `pos1` to `pos2`.
    Mover {
        pos1: Vec3,
        pos2: Vec3,
        speed: f32,
        /// Seconds open before closing by itself, `< 0` stays.
        wait: f32,
        state: Toggle,
        locked: bool,
        /// Tick the return home is due at.
        return_at: Option<u64>,
    },
    /// `func_door_rotating`: turns about `axis` by `distance` degrees.
    Rotator {
        axis: Vec3,
        distance: f32,
        speed: f32,
        wait: f32,
        state: Toggle,
        locked: bool,
        return_at: Option<u64>,
        angle: f32,
    },
    /// `func_rotating`: spins about `axis` at `speed` deg/s while on.
    Spinner {
        axis: Vec3,
        speed: f32,
        on: bool,
        angle: f32,
    },
    /// `func_brush` / `func_wall_toggle`: solid while enabled.
    Toggled { enabled: bool },
    /// `trigger_*`: fires on touch.
    Trigger {
        enabled: bool,
        wait: f32,
        next_at: u64,
        touching: Vec<u32>,
        once: bool,
    },
    Relay {
        enabled: bool,
        once: bool,
        fired: bool,
    },
    Timer {
        enabled: bool,
        refire: f32,
        next_at: u64,
    },
    /// `logic_auto` and anything else: outputs only.
    Logic,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapEntity {
    pub id: u32,
    pub classname: String,
    pub targetname: String,
    pub def: MapEntityDef,
    pub outputs: Vec<Output>,
    pub kind: Kind,
    pub pos: Vec3,
    pub rot: Quat,
    pub body: Option<BodyHandle>,
    /// `spawnflags`.
    pub flags: u32,
    pub alive: bool,
    /// Pose changed since the physics body was last placed.
    dirty: bool,
    /// Position after the previous tick, for the step the riders and the pushed take.
    last_pos: Vec3,
}

/// A brush entity's sound events.
#[derive(Clone, Copy, PartialEq)]
enum EntSound {
    Move,
    Arrive,
    Locked,
}

impl MapEntity {
    /// The entity is travelling or spinning.
    pub fn moving(&self) -> bool {
        match &self.kind {
            Kind::Mover { state, .. } | Kind::Rotator { state, .. } => {
                matches!(state, Toggle::GoingUp | Toggle::GoingDown)
            }
            Kind::Spinner { on, .. } => *on,
            _ => false,
        }
    }

    /// World centre of the brush model.
    pub fn center(&self) -> Vec3 {
        self.pos + self.rot * ((self.def.mins + self.def.maxs) * 0.5)
    }

    /// The sound script of the event as the SDK picks it: doors `noise1` / `noise2` /
    /// `locked_sound` with the `DoorSound.*` (`RotDoorSound.*`) defaults, buttons
    /// `Buttons.snd<sounds>` / `<locked_sound>`, func_movelinear `startsound` / `stopsound`,
    /// func_rotating `message`, plats and trains `noise1` / `noise2`.
    fn sound(&self, which: EntSound) -> Option<String> {
        let kv = |k: &str| {
            self.def
                .get(k)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        };
        let button = |k: &str| {
            self.def
                .get(k)
                .and_then(|v| v.trim().parse::<u32>().ok())
                .filter(|&n| n > 0)
                .map(|n| format!("Buttons.snd{n}"))
        };
        let door = |prefix: &str| match which {
            EntSound::Move => kv("noise1").or_else(|| Some(format!("{prefix}.DefaultMove"))),
            EntSound::Arrive => kv("noise2").or_else(|| Some(format!("{prefix}.DefaultArrive"))),
            EntSound::Locked => {
                button("locked_sound").or_else(|| Some(format!("{prefix}.DefaultLocked")))
            }
        };
        match (self.classname.as_str(), which) {
            ("func_door_rotating", _) => door("RotDoorSound"),
            ("func_door", _) => door("DoorSound"),
            ("func_movelinear", EntSound::Move) => kv("startsound"),
            ("func_movelinear", EntSound::Arrive) => kv("stopsound"),
            ("func_button" | "func_rot_button", EntSound::Move) => button("sounds"),
            ("func_button" | "func_rot_button", EntSound::Locked) => button("locked_sound"),
            ("func_rotating", EntSound::Move) => kv("message"),
            ("func_plat" | "func_platrot" | "func_tracktrain", EntSound::Move) => kv("noise1"),
            ("func_plat" | "func_platrot" | "func_tracktrain", EntSound::Arrive) => kv("noise2"),
            _ => None,
        }
    }

    /// The entity blocks the player trace right now.
    pub fn solid(&self) -> bool {
        self.alive
            && !self.def.hulls.is_empty()
            && match &self.kind {
                Kind::Toggled { enabled } => *enabled,
                Kind::Trigger { .. } | Kind::Relay { .. } | Kind::Timer { .. } | Kind::Logic => {
                    false
                }
                _ => true,
            }
    }

    /// `+use` on this entity does something.
    pub fn usable(&self) -> bool {
        self.alive
            && match self.classname.as_str() {
                "func_button" | "func_rot_button" => true,
                "func_door" | "func_door_rotating" => self.flags & 256 != 0,
                _ => false,
            }
    }

    pub fn convexes(&self, out: &mut Vec<Convex>) {
        for h in &self.def.hulls {
            let (planes, (mins, maxs)) =
                gm_physics::pickup::world_convex(&h.planes, &h.points, self.rot, self.pos);
            out.push(Convex {
                entity: self.id,
                friction: 1.0,
                planes,
                mins,
                maxs,
            });
        }
    }

    /// The hull box `lo..hi` overlaps the entity's volume (separating planes of the convex
    /// only, which is exact for a box fully inside and generous at corners).
    pub fn overlaps_box(&self, lo: Vec3, hi: Vec3) -> bool {
        self.def.hulls.iter().any(|h| {
            let (planes, (mins, maxs)) =
                gm_physics::pickup::world_convex(&h.planes, &h.points, self.rot, self.pos);
            if mins.cmpgt(hi).any() || maxs.cmplt(lo).any() {
                return false;
            }
            planes.iter().all(|(n, d)| {
                let support = Vec3::new(
                    if n.x < 0.0 { hi.x } else { lo.x },
                    if n.y < 0.0 { hi.y } else { lo.y },
                    if n.z < 0.0 { hi.z } else { lo.z },
                );
                n.dot(support) <= *d
            })
        })
    }
}

/// A queued input.
#[derive(Clone, Debug, PartialEq)]
struct Pending {
    due: u64,
    target: String,
    input: String,
    param: String,
    activator: u32,
    caller: u32,
}

#[derive(Clone, Debug, Default)]
pub struct MapIo {
    pub entities: Vec<MapEntity>,
    pending: Vec<Pending>,
    started: bool,
    /// Convexes of the solid moving brushes, rebuilt when a pose changes.
    convexes: Vec<Convex>,
    convexes_dirty: bool,
}

const TICK: f32 = gm_core::tick::DEFAULT_TICK_INTERVAL;

fn ticks(seconds: f32) -> u64 {
    (seconds / TICK).round().max(0.0) as u64
}

fn build(def: MapEntityDef, id: u32) -> MapEntity {
    let flags = def.flags();
    let class = def.classname.clone();
    let targetname = def.get("targetname").unwrap_or("").to_string();
    let outputs: Vec<Output> = def
        .kv
        .iter()
        .filter(|(k, _)| k.starts_with("On") && k.len() > 2)
        .filter_map(|(k, v)| parse_output(k, v))
        .collect();
    let size = def.maxs - def.mins;
    let movedir = def
        .get("movedir")
        .and_then(parse_vec3)
        .map(angles_dir)
        .unwrap_or(Vec3::Z);
    let kind = match class.as_str() {
        "func_door" | "func_movelinear" | "func_button" | "func_rot_button" => {
            let lip = def.num("lip", 0.0);
            let travel = if class == "func_movelinear" {
                def.num("movedistance", 0.0)
            } else {
                (size.x * movedir.x).abs() + (size.y * movedir.y).abs() + (size.z * movedir.z).abs()
                    - lip
            };
            let (mut pos1, mut pos2) = (def.origin, def.origin + movedir * travel);
            if class == "func_movelinear" {
                let t = def.num("startposition", 0.0).clamp(0.0, 1.0);
                pos1 -= movedir * travel * t;
                pos2 = pos1 + movedir * travel;
            }
            let start_open = class != "func_movelinear" && flags & 1 != 0 && class == "func_door";
            if start_open {
                std::mem::swap(&mut pos1, &mut pos2);
            }
            let wait = if class == "func_movelinear" {
                -1.0
            } else {
                def.num("wait", if class == "func_door" { 4.0 } else { 1.0 })
            };
            // a button with "don't move" (flag 1) stays put
            let pos2 = if class.ends_with("button") && flags & 1 != 0 {
                pos1
            } else {
                pos2
            };
            Kind::Mover {
                pos1,
                pos2,
                speed: def.num("speed", if class == "func_button" { 40.0 } else { 100.0 }),
                wait,
                state: Toggle::AtBottom,
                locked: def.get("locked") == Some("1")
                    || (class == "func_door" && flags & 2048 != 0)
                    || (class == "func_button" && flags & 2048 != 0),
                return_at: None,
            }
        }
        "func_door_rotating" => {
            let axis = if flags & 64 != 0 {
                Vec3::X
            } else if flags & 128 != 0 {
                Vec3::Y
            } else {
                Vec3::Z
            };
            let sign = if flags & 2 != 0 { -1.0 } else { 1.0 };
            Kind::Rotator {
                axis,
                distance: def.num("distance", 90.0) * sign,
                speed: def.num("speed", 100.0),
                wait: def.num("wait", 4.0),
                state: Toggle::AtBottom,
                locked: flags & 2048 != 0,
                return_at: None,
                angle: 0.0,
            }
        }
        "func_rotating" => {
            let axis = if flags & 4 != 0 {
                Vec3::X
            } else if flags & 8 != 0 {
                Vec3::Y
            } else {
                Vec3::Z
            };
            let sign = if flags & 2 != 0 { -1.0 } else { 1.0 };
            Kind::Spinner {
                axis,
                speed: def.num("maxspeed", 100.0) * sign,
                on: flags & 1 != 0,
                angle: 0.0,
            }
        }
        "func_brush" => Kind::Toggled {
            enabled: def.get("Solidity") != Some("1") && def.get("StartDisabled") != Some("1"),
        },
        "func_wall_toggle" => Kind::Toggled {
            enabled: flags & 1 == 0,
        },
        c if c.starts_with("trigger_") => Kind::Trigger {
            enabled: def.get("StartDisabled") != Some("1"),
            wait: def.num("wait", if c == "trigger_once" { -1.0 } else { 0.2 }),
            next_at: 0,
            touching: Vec::new(),
            once: c == "trigger_once",
        },
        "logic_relay" => Kind::Relay {
            enabled: def.get("StartDisabled") != Some("1"),
            once: flags & 1 != 0,
            fired: false,
        },
        "logic_timer" => Kind::Timer {
            enabled: def.get("StartDisabled") != Some("1"),
            refire: def.num("RefireTime", 1.0).max(TICK),
            next_at: 0,
        },
        _ => Kind::Logic,
    };
    let pos = match &kind {
        Kind::Mover { pos1, .. } => *pos1,
        _ => def.origin,
    };
    let rot = gm_physics::quat_from_angles(def.angles.x, def.angles.y, def.angles.z);
    MapEntity {
        id,
        classname: class,
        targetname,
        def,
        outputs,
        kind,
        pos,
        rot,
        body: None,
        flags,
        alive: true,
        dirty: true,
        last_pos: pos,
    }
}

impl MapIo {
    pub fn get(&self, id: u32) -> Option<&MapEntity> {
        self.entities.iter().find(|e| e.id == id && e.alive)
    }

    /// Entities a target string names: `name`, `name*`, or nothing for the `!` specials.
    pub fn find_by_name(&self, name: &str) -> Vec<u32> {
        let name = name.trim();
        if name.is_empty() || name.starts_with('!') {
            return Vec::new();
        }
        self.entities
            .iter()
            .filter(|e| e.alive && !e.targetname.is_empty())
            .filter(|e| match name.strip_suffix('*') {
                Some(prefix) => e.targetname.starts_with(prefix),
                None => e.targetname.eq_ignore_ascii_case(name),
            })
            .map(|e| e.id)
            .collect()
    }

    /// The solid moving brushes as player-trace convexes.
    pub fn convexes(&mut self) -> &[Convex] {
        if self.convexes_dirty {
            self.convexes.clear();
            for e in self.entities.iter().filter(|e| e.solid()) {
                e.convexes(&mut self.convexes);
            }
            self.convexes_dirty = false;
        }
        &self.convexes
    }

    /// Queues an input for every entity `target` names (specials resolve against the caller).
    #[allow(clippy::too_many_arguments)]
    pub fn queue(
        &mut self,
        now: u64,
        target: &str,
        input: &str,
        param: &str,
        delay: f32,
        activator: u32,
        caller: u32,
    ) {
        self.pending.push(Pending {
            due: now + ticks(delay),
            target: target.to_string(),
            input: input.to_string(),
            param: param.to_string(),
            activator,
            caller,
        });
    }

    /// Fires `name` of entity `idx`: queues its stored outputs (counting `times` down).
    fn fire_output(&mut self, now: u64, idx: usize, name: &str, activator: u32) {
        let caller = self.entities[idx].id;
        let mut fired = Vec::new();
        for o in self.entities[idx].outputs.iter_mut() {
            if o.times == 0 || !o.name.eq_ignore_ascii_case(name) {
                continue;
            }
            if o.times > 0 {
                o.times -= 1;
            }
            fired.push((o.target.clone(), o.input.clone(), o.param.clone(), o.delay));
        }
        self.entities[idx].outputs.retain(|o| o.times != 0);
        for (target, input, param, delay) in fired {
            self.queue(now, &target, &input, &param, delay, activator, caller);
        }
    }
}

/// What happened to a moving brush this tick, for sounds and outputs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MoveEvent {
    None,
    Opened,
    Closed,
}

impl Server<'_> {
    /// Installs the map's entities: ids from `MAP_ENT_ID_BASE`, a static physics body per
    /// brush entity (placed at its pose), `logic_auto` fires on the first tick.
    pub fn set_map_entities(&mut self, defs: Vec<MapEntityDef>, surface: Surface) {
        let mut io = MapIo::default();
        for (i, def) in defs.into_iter().enumerate() {
            let mut e = build(def, MAP_ENT_ID_BASE + i as u32);
            if e.solid() || matches!(e.kind, Kind::Toggled { .. }) {
                let hulls: Vec<Vec<Vec3>> = e
                    .def
                    .hulls
                    .iter()
                    .map(|h| h.phys_points.iter().map(|p| e.rot * *p + e.pos).collect())
                    .collect();
                if let Ok(b) = self.physics.add_static_hulls(&hulls, surface) {
                    e.body = Some(b);
                    if !e.solid() {
                        self.physics.teleport(b, Vec3::new(0.0, 0.0, -1.0e6), e.rot);
                    }
                }
            }
            e.dirty = false;
            io.entities.push(e);
        }
        io.convexes_dirty = true;
        self.map_io = io;
    }

    /// The solid movable brush entities for the snapshot: id, model `*N`, position and
    /// angles (the rotators' turn about their axis folded into the yaw / pitch / roll).
    pub fn map_entity_states(&self) -> Vec<(u32, String, Vec3, Vec3)> {
        self.map_io
            .entities
            .iter()
            .filter(|e| e.solid())
            .filter_map(|e| {
                let model = e.def.get("model")?;
                if !model.starts_with('*') {
                    return None;
                }
                let (axis, angle) = match &e.kind {
                    Kind::Rotator { axis, angle, .. } | Kind::Spinner { axis, angle, .. } => {
                        (*axis, *angle)
                    }
                    _ => (Vec3::Z, 0.0),
                };
                let mut angles = e.def.angles;
                if axis == Vec3::X {
                    angles.x += angle;
                } else if axis == Vec3::Y {
                    angles.z += angle;
                } else {
                    angles.y += angle;
                }
                Some((e.id, model.to_string(), e.pos, angles))
            })
            .collect()
    }

    /// `ent_fire <target> <input> [param]` and Lua `Entity:Fire` on a map entity.
    pub fn fire_input(&mut self, target: &str, input: &str, param: &str, activator: u32) {
        let now = self.tick_count;
        self.map_io
            .queue(now, target, input, param, 0.0, activator, 0);
    }

    /// An input at entity `id` (a map entity, or `Kill` on a prop), true when something took it.
    pub fn fire_input_id(&mut self, id: u32, input: &str, param: &str, activator: u32) -> bool {
        if (MAP_ENT_ID_BASE..crate::PROJECTILE_ID_BASE).contains(&id) {
            if self.map_io.get(id).is_none() {
                return false;
            }
            self.map_input(id, input, param, activator);
            true
        } else {
            self.entity_input(id, input, param, activator);
            true
        }
    }

    /// Delivers `input` to map entity `id` now.
    pub fn map_input(&mut self, id: u32, input: &str, param: &str, activator: u32) {
        let now = self.tick_count;
        let Some(idx) = self
            .map_io
            .entities
            .iter()
            .position(|e| e.id == id && e.alive)
        else {
            return;
        };
        let input_lc = input.to_ascii_lowercase();
        match input_lc.as_str() {
            "kill" => {
                self.map_io.entities[idx].alive = false;
                if let Some(b) = self.map_io.entities[idx].body.take() {
                    self.physics.remove_body(b);
                }
                self.map_io.convexes_dirty = true;
                return;
            }
            "fireuser1" | "fireuser2" | "fireuser3" | "fireuser4" => {
                let n = &input_lc[8..];
                let name = format!("OnUser{n}");
                self.map_io.fire_output(now, idx, &name, activator);
                return;
            }
            "addoutput" => {
                // "OnName target:input:param:delay:times"
                let (name, rest) = param.split_once(' ').unwrap_or((param, ""));
                if let Some(o) = parse_output(name.trim(), &rest.replace(':', ",")) {
                    self.map_io.entities[idx].outputs.push(o);
                }
                return;
            }
            _ => {}
        }
        let mut events: Vec<&'static str> = Vec::new();
        let e = &mut self.map_io.entities[idx];
        let was_moving = e.moving();
        match &mut e.kind {
            Kind::Mover {
                state,
                locked,
                return_at,
                ..
            } => match input_lc.as_str() {
                "lock" => *locked = true,
                "unlock" => *locked = false,
                "open" | "press" | "setposition" | "use" | "toggle"
                    if *state == Toggle::AtBottom || *state == Toggle::GoingDown =>
                {
                    if *locked && input_lc != "setposition" {
                        events.push("OnUseLocked");
                    } else {
                        *state = Toggle::GoingUp;
                        *return_at = None;
                        events.push("OnOpen");
                        if e.classname.ends_with("button") {
                            events.push("OnPressed");
                        }
                    }
                }
                "close" | "toggle" | "setposition"
                    if *state == Toggle::AtTop || *state == Toggle::GoingUp =>
                {
                    *state = Toggle::GoingDown;
                    *return_at = None;
                    events.push("OnClose");
                }
                _ => {}
            },
            Kind::Rotator {
                state,
                locked,
                return_at,
                ..
            } => match input_lc.as_str() {
                "lock" => *locked = true,
                "unlock" => *locked = false,
                "open" | "use" | "toggle"
                    if *state == Toggle::AtBottom || *state == Toggle::GoingDown =>
                {
                    if *locked {
                        events.push("OnUseLocked");
                    } else {
                        *state = Toggle::GoingUp;
                        *return_at = None;
                        events.push("OnOpen");
                    }
                }
                "close" | "toggle" if *state == Toggle::AtTop || *state == Toggle::GoingUp => {
                    *state = Toggle::GoingDown;
                    *return_at = None;
                    events.push("OnClose");
                }
                _ => {}
            },
            Kind::Spinner { on, .. } => match input_lc.as_str() {
                "start" | "startforward" | "startbackward" => *on = true,
                "stop" => *on = false,
                "toggle" => *on = !*on,
                _ => {}
            },
            Kind::Toggled { enabled } => {
                let before = *enabled;
                match input_lc.as_str() {
                    "enable" => *enabled = true,
                    "disable" => *enabled = false,
                    "toggle" => *enabled = !*enabled,
                    _ => {}
                }
                if *enabled != before {
                    e.dirty = true;
                }
            }
            Kind::Trigger { enabled, .. } => match input_lc.as_str() {
                "enable" => *enabled = true,
                "disable" => *enabled = false,
                "toggle" => *enabled = !*enabled,
                _ => {}
            },
            Kind::Relay {
                enabled,
                once,
                fired,
            } => match input_lc.as_str() {
                "enable" => *enabled = true,
                "disable" => *enabled = false,
                "toggle" => *enabled = !*enabled,
                "trigger" if *enabled && !(*once && *fired) => {
                    *fired = true;
                    events.push("OnTrigger");
                }
                _ => {}
            },
            Kind::Timer {
                enabled, next_at, ..
            } => match input_lc.as_str() {
                "enable" => {
                    *enabled = true;
                    *next_at = 0;
                }
                "disable" => *enabled = false,
                "toggle" => *enabled = !*enabled,
                "firetimer" => events.push("OnTimer"),
                _ => {}
            },
            Kind::Logic => {}
        }
        // the SDK's door / button sounds: the move sound as the travel starts, the locked
        // sound on a locked use
        let e = &self.map_io.entities[idx];
        let center = e.center();
        let mut sounds: Vec<String> = Vec::new();
        if !was_moving && e.moving() {
            sounds.extend(e.sound(EntSound::Move));
        }
        if events.contains(&"OnUseLocked") {
            sounds.extend(e.sound(EntSound::Locked));
        }
        for name in sounds {
            self.play_sound(name, center, 1.0);
        }
        for name in events {
            self.map_io.fire_output(now, idx, name, activator);
        }
    }

    /// Advances the movers, touches the triggers, fires the timers, delivers due inputs.
    pub(crate) fn tick_map_io(&mut self) {
        if self.map_io.entities.is_empty() {
            return;
        }
        let now = self.tick_count;
        if !self.map_io.started {
            self.map_io.started = true;
            for idx in 0..self.map_io.entities.len() {
                if self.map_io.entities[idx].classname == "logic_auto" {
                    self.map_io.fire_output(now, idx, "OnMapSpawn", 0);
                }
            }
        }
        // movers
        for idx in 0..self.map_io.entities.len() {
            let e = &mut self.map_io.entities[idx];
            if !e.alive {
                continue;
            }
            let mut ev = MoveEvent::None;
            let was_moving = e.moving();
            match &mut e.kind {
                Kind::Mover {
                    pos1,
                    pos2,
                    speed,
                    wait,
                    state,
                    return_at,
                    ..
                } => {
                    let step = *speed * TICK;
                    match *state {
                        Toggle::GoingUp => {
                            let to = *pos2 - e.pos;
                            if to.length() <= step {
                                e.pos = *pos2;
                                *state = Toggle::AtTop;
                                ev = MoveEvent::Opened;
                                if *wait >= 0.0 {
                                    *return_at = Some(now + ticks(*wait));
                                }
                            } else {
                                e.pos += to.normalize() * step;
                            }
                            e.dirty = true;
                        }
                        Toggle::GoingDown => {
                            let to = *pos1 - e.pos;
                            if to.length() <= step {
                                e.pos = *pos1;
                                *state = Toggle::AtBottom;
                                ev = MoveEvent::Closed;
                            } else {
                                e.pos += to.normalize() * step;
                            }
                            e.dirty = true;
                        }
                        Toggle::AtTop => {
                            if return_at.is_some_and(|t| now >= t) {
                                *return_at = None;
                                *state = Toggle::GoingDown;
                            }
                        }
                        Toggle::AtBottom => {}
                    }
                }
                Kind::Rotator {
                    axis,
                    distance,
                    speed,
                    wait,
                    state,
                    return_at,
                    angle,
                    ..
                } => {
                    let step = *speed * TICK;
                    let target = match *state {
                        Toggle::GoingUp => Some(*distance),
                        Toggle::GoingDown => Some(0.0),
                        Toggle::AtTop => {
                            if return_at.is_some_and(|t| now >= t) {
                                *return_at = None;
                                *state = Toggle::GoingDown;
                            }
                            None
                        }
                        Toggle::AtBottom => None,
                    };
                    if let Some(t) = target {
                        let d = t - *angle;
                        if d.abs() <= step {
                            *angle = t;
                            if *state == Toggle::GoingUp {
                                *state = Toggle::AtTop;
                                ev = MoveEvent::Opened;
                                if *wait >= 0.0 {
                                    *return_at = Some(now + ticks(*wait));
                                }
                            } else {
                                *state = Toggle::AtBottom;
                                ev = MoveEvent::Closed;
                            }
                        } else {
                            *angle += step * d.signum();
                        }
                        let base = gm_physics::quat_from_angles(
                            e.def.angles.x,
                            e.def.angles.y,
                            e.def.angles.z,
                        );
                        e.rot = Quat::from_axis_angle(*axis, angle.to_radians()) * base;
                        e.dirty = true;
                    }
                }
                Kind::Spinner {
                    axis,
                    speed,
                    on,
                    angle,
                } => {
                    if *on {
                        *angle = (*angle + *speed * TICK).rem_euclid(360.0);
                        let base = gm_physics::quat_from_angles(
                            e.def.angles.x,
                            e.def.angles.y,
                            e.def.angles.z,
                        );
                        e.rot = Quat::from_axis_angle(*axis, angle.to_radians()) * base;
                        e.dirty = true;
                    }
                }
                Kind::Timer {
                    enabled,
                    refire,
                    next_at,
                } if *enabled && now >= *next_at => {
                    *next_at = now + ticks(*refire).max(1);
                    self.map_io.fire_output(now, idx, "OnTimer", 0);
                }
                _ => {}
            }
            let e = &self.map_io.entities[idx];
            let center = e.center();
            let start = if !was_moving && e.moving() {
                e.sound(EntSound::Move)
            } else {
                None
            };
            let arrive = if matches!(ev, MoveEvent::None) {
                None
            } else {
                e.sound(EntSound::Arrive)
            };
            for name in start.into_iter().chain(arrive) {
                self.play_sound(name, center, 1.0);
            }
            match ev {
                MoveEvent::Opened => self.map_io.fire_output(now, idx, "OnFullyOpen", 0),
                MoveEvent::Closed => self.map_io.fire_output(now, idx, "OnFullyClosed", 0),
                MoveEvent::None => {}
            }
        }
        // physics bodies follow the poses; the players riding a mover take its step, the ones
        // in its way are pushed by it, and a mover that cannot clear a player reverses
        let mut blocked: Vec<usize> = Vec::new();
        for idx in 0..self.map_io.entities.len() {
            let e = &mut self.map_io.entities[idx];
            if !e.dirty {
                continue;
            }
            e.dirty = false;
            self.map_io.convexes_dirty = true;
            if let Some(b) = e.body {
                let pos = if e.solid() {
                    e.pos
                } else {
                    Vec3::new(0.0, 0.0, -1.0e6)
                };
                self.physics.teleport(b, pos, e.rot);
            }
            let step = e.pos - e.last_pos;
            e.last_pos = e.pos;
            if step.length_squared() < 1e-8 || !e.solid() {
                continue;
            }
            let e = &self.map_io.entities[idx];
            let mut riders = Vec::new();
            let mut pushed = Vec::new();
            for (pid, p) in self.players.iter() {
                if !p.alive {
                    continue;
                }
                let h = if p.state.ducked {
                    gm_physics::pickup::DUCK_HULL_HEIGHT
                } else {
                    gm_physics::pickup::HULL_HEIGHT
                };
                let o = p.state.origin;
                if p.state.ground_ent == e.id {
                    riders.push(*pid);
                } else if e.overlaps_box(
                    o + Vec3::new(-16.0, -16.0, 0.0),
                    o + Vec3::new(16.0, 16.0, h),
                ) {
                    pushed.push((*pid, h));
                }
            }
            for pid in riders {
                if let Some(p) = self.players.get_mut(&pid) {
                    p.state.origin += step;
                }
            }
            for (pid, h) in pushed {
                let Some(p) = self.players.get_mut(&pid) else {
                    continue;
                };
                p.state.origin += step;
                let o = p.state.origin;
                if e.overlaps_box(
                    o + Vec3::new(-16.0, -16.0, 0.0),
                    o + Vec3::new(16.0, 16.0, h),
                ) {
                    blocked.push(idx);
                }
            }
        }
        for idx in blocked {
            let e = &mut self.map_io.entities[idx];
            match &mut e.kind {
                Kind::Mover {
                    state, return_at, ..
                }
                | Kind::Rotator {
                    state, return_at, ..
                } => {
                    *state = match *state {
                        Toggle::GoingUp => Toggle::GoingDown,
                        Toggle::GoingDown => Toggle::GoingUp,
                        s => s,
                    };
                    *return_at = None;
                }
                _ => {}
            }
            self.map_io.fire_output(now, idx, "OnBlocked", 0);
        }
        // triggers: players (flag 1), NPCs (flag 2), props (flag 8)
        let players: Vec<(u32, Vec3, Vec3)> = self
            .players
            .iter()
            .filter(|(_, p)| p.alive)
            .map(|(id, p)| {
                let h = if p.state.ducked {
                    gm_physics::pickup::DUCK_HULL_HEIGHT
                } else {
                    gm_physics::pickup::HULL_HEIGHT
                };
                let o = p.state.origin;
                (
                    *id,
                    o + Vec3::new(-16.0, -16.0, 0.0),
                    o + Vec3::new(16.0, 16.0, h),
                )
            })
            .collect();
        let npcs: Vec<(u32, Vec3, Vec3)> = self
            .npcs
            .iter()
            .filter(|(_, n)| n.dead_at.is_none())
            .map(|(id, n)| (*id, n.pos + n.mins, n.pos + n.maxs))
            .collect();
        let props: Vec<(u32, Vec3, Vec3)> = self
            .sandbox
            .props()
            .iter()
            .map(|p| (p.id, p.pos - Vec3::splat(1.0), p.pos + Vec3::splat(1.0)))
            .collect();
        for idx in 0..self.map_io.entities.len() {
            let e = &self.map_io.entities[idx];
            if !e.alive {
                continue;
            }
            let Kind::Trigger {
                enabled,
                wait,
                next_at,
                touching,
                once,
            } = &e.kind
            else {
                continue;
            };
            if !*enabled {
                continue;
            }
            let flags = e.flags;
            let mut inside = Vec::new();
            let candidates = players
                .iter()
                .filter(|_| flags & 1 != 0 || flags == 0)
                .chain(npcs.iter().filter(|_| flags & 2 != 0))
                .chain(props.iter().filter(|_| flags & 8 != 0));
            for (id, lo, hi) in candidates {
                if e.overlaps_box(*lo, *hi) {
                    inside.push(*id);
                }
            }
            let started: Vec<u32> = inside
                .iter()
                .copied()
                .filter(|id| !touching.contains(id))
                .collect();
            let ended: Vec<u32> = touching
                .iter()
                .copied()
                .filter(|id| !inside.contains(id))
                .collect();
            let fire = !started.is_empty() && now >= *next_at;
            let (wait, once) = (*wait, *once);
            let e = &mut self.map_io.entities[idx];
            if let Kind::Trigger {
                touching, next_at, ..
            } = &mut e.kind
            {
                *touching = inside;
                if fire {
                    *next_at = if wait < 0.0 {
                        u64::MAX
                    } else {
                        now + ticks(wait).max(1)
                    };
                }
            }
            for id in &started {
                self.map_io.fire_output(now, idx, "OnStartTouch", *id);
            }
            for id in &ended {
                self.map_io.fire_output(now, idx, "OnEndTouch", *id);
            }
            if fire {
                self.map_io.fire_output(now, idx, "OnTrigger", started[0]);
                if once {
                    self.map_io.entities[idx].alive = false;
                }
            }
        }
        // due inputs
        let mut due = Vec::new();
        let mut i = 0;
        while i < self.map_io.pending.len() {
            if self.map_io.pending[i].due <= now {
                due.push(self.map_io.pending.swap_remove(i));
            } else {
                i += 1;
            }
        }
        due.sort_by_key(|p| p.due);
        for p in due {
            let targets: Vec<u32> = match p.target.as_str() {
                "!self" | "!caller" => vec![p.caller],
                "!activator" => vec![p.activator],
                _ => self.map_io.find_by_name(&p.target),
            };
            for t in targets {
                if (MAP_ENT_ID_BASE..crate::PROJECTILE_ID_BASE).contains(&t) {
                    self.map_input(t, &p.input, &p.param, p.activator);
                } else if t != 0 {
                    self.entity_input(t, &p.input, &p.param, p.activator);
                }
            }
        }
    }

    /// `+use` pressed by `player` looking along `view` from `eye`: presses the button or opens
    /// the door in reach, if any.
    pub(crate) fn map_use(&mut self, player: u32, eye: Vec3, view: Vec3) -> bool {
        let (forward, _) = gm_physics::pickup::view_vectors(view.x, view.y);
        let mut convexes = Vec::new();
        for e in self.map_io.entities.iter().filter(|e| e.usable()) {
            e.convexes(&mut convexes);
        }
        if convexes.is_empty() {
            return false;
        }
        let (t, ent) = gm_collision::sweep_convexes(
            eye,
            eye + forward * gm_physics::pickup::USE_RADIUS,
            Vec3::ZERO,
            Vec3::ZERO,
            &convexes,
        );
        let Some(ent) = ent.filter(|_| t.fraction < 1.0) else {
            return false;
        };
        self.map_input(ent, "Use", "", player);
        true
    }

    /// An input aimed at a non-map entity (a Lua entity or a player): `Kill` removes a prop.
    fn entity_input(&mut self, id: u32, input: &str, _param: &str, _activator: u32) {
        if input.eq_ignore_ascii_case("kill") && self.sandbox.props().iter().any(|p| p.id == id) {
            self.remove_prop(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_outputs_and_movedir() {
        let o = parse_output("OnPressed", "door1,Open,,0.5,1").unwrap();
        assert_eq!(o.target, "door1");
        assert_eq!(o.input, "Open");
        assert_eq!(o.delay, 0.5);
        assert_eq!(o.times, 1);
        let o = parse_output("OnTrigger", "a\x1bToggle\x1b\x1b0\x1b-1").unwrap();
        assert_eq!(
            (o.target.as_str(), o.input.as_str(), o.times),
            ("a", "Toggle", -1)
        );
        assert!((angles_dir(Vec3::new(-90.0, 0.0, 0.0)) - Vec3::Z).length() < 1e-5);
        assert!((angles_dir(Vec3::new(0.0, 90.0, 0.0)) - Vec3::Y).length() < 1e-5);
    }
}
