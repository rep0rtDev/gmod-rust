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

/// `path_track` spawnflag set by EnableAlternatePath (`SF_PATH_ALTERNATE`).
const SF_PATH_ALTERNATE: u32 = 0x8000;
/// Our marker: the node's `OnPass` fired (spawnflag 2 fires it once).
const SF_PATH_PASSED: u32 = 0x1_0000;

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
    Toggled {
        enabled: bool,
    },
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
    /// `prop_door_rotating` (props.md §8.1, gap-report #202): a frozen model prop `prop`
    /// turned about Z by `angle` toward `goal` at `speed` deg/s; opens away from the
    /// activator (`opendir` 1 forward only, 2 backward only), `returndelay` < 0 stays open.
    PropDoor {
        prop: u32,
        angle: f32,
        goal: f32,
        state: Toggle,
        speed: f32,
        distance: f32,
        return_delay: f32,
        open_dir: i32,
        locked: bool,
        return_at: Option<u64>,
    },
    /// `logic_branch`: a stored boolean tested into `OnTrue` / `OnFalse`.
    Branch {
        value: bool,
    },
    /// `logic_compare`: `value` against `compare` into `OnLessThan` / `OnEqualTo` /
    /// `OnNotEqualTo` / `OnGreaterThan`, the value as the output parameter.
    Compare {
        value: f32,
        compare: f32,
    },
    /// `math_counter`: Add / Subtract / Multiply / Divide / SetValue clamped to `min`..`max`
    /// (no clamp when both are 0), `OutValue` with the value, `OnHitMin` / `OnHitMax` once
    /// per arrival and `OnChangedFromMin` / `OnChangedFromMax` on leaving.
    Counter {
        value: f32,
        at_min: bool,
        at_max: bool,
    },
    /// `logic_auto` and anything else: outputs only.
    /// `ambient_generic`: `message` at `health` / 10 volume and `pitch`, looped unless
    /// spawnflag 32, silent at start with spawnflag 16.
    Ambient {
        playing: bool,
        volume: f32,
        pitch: f32,
        looped: bool,
    },
    /// A named `light` / `light_spot` on a style of its own (32..): TurnOn / TurnOff / Toggle /
    /// SetPattern drive the style's brightness on the clients.
    Light {
        on: bool,
        style: u8,
        pattern: String,
    },
    /// `env_sprite`: shown or hidden, `rendercolor` + `renderamt`, `scale`.
    Sprite {
        on: bool,
        color: [u8; 4],
        scale: f32,
    },
    /// `func_tracktrain` on its `path_track` chain: signed speed along the track, the node
    /// it heads to and the one it left, `height` above the track.
    Train {
        speed: f32,
        max: f32,
        last_speed: f32,
        height: f32,
        next: Option<u32>,
        prev: Option<u32>,
    },
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
            Kind::Train { speed, .. } => *speed != 0.0,
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
                Kind::Trigger { .. }
                | Kind::Relay { .. }
                | Kind::Timer { .. }
                | Kind::Ambient { .. }
                | Kind::Light { .. }
                | Kind::Logic => false,
                Kind::Train { .. } => self.flags & 8 == 0,
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
    /// A target by id (`+use`, Lua `Entity:Fire`) instead of by name.
    target_id: u32,
    input: String,
    param: String,
    activator: u32,
    caller: u32,
}

#[derive(Clone, Debug, Default)]
pub struct MapIo {
    pub entities: Vec<MapEntity>,
    pending: Vec<Pending>,
    /// Inputs due this tick, applied by `Server::apply_due_inputs` or, with `defer_inputs`,
    /// by `flush_due_inputs` once the `AcceptInput` hook has seen them.
    ready: Vec<Pending>,
    started: bool,
    /// Convexes of the solid moving brushes, rebuilt when a pose changes.
    convexes: Vec<Convex>,
    convexes_dirty: bool,
}

const TICK: f32 = gm_core::tick::DEFAULT_TICK_INTERVAL;

/// A train faces its travel: yaw, and pitch unless spawnflag 1 (no pitch); spawnflag 16
/// keeps the spawn orientation.
fn train_rot(dir: Vec3, flags: u32) -> Quat {
    if flags & 16 != 0 || dir.length_squared() < 1e-6 {
        return Quat::IDENTITY;
    }
    let yaw = dir.y.atan2(dir.x).to_degrees();
    let pitch = if flags & 1 != 0 {
        0.0
    } else {
        -dir.z.atan2(dir.truncate().length()).to_degrees()
    };
    gm_physics::quat_from_angles(pitch, yaw, 0.0)
}

/// Mean brightness of a light pattern (`a` = 0, `m` = 1, `z` = 2).
fn pattern_mean(pattern: &str) -> f32 {
    let n = pattern.len().max(1) as f32;
    pattern
        .bytes()
        .map(|c| c.saturating_sub(b'a') as f32 / 12.0)
        .sum::<f32>()
        / n
}

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
        .filter(|(k, _)| (k.starts_with("On") && k.len() > 2) || k.eq_ignore_ascii_case("OutValue"))
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
        "light" | "light_spot"
            if def
                .get("style")
                .and_then(|v| v.trim().parse::<u32>().ok())
                .is_some_and(|v| (32..256).contains(&v)) =>
        {
            Kind::Light {
                on: flags & 1 == 0,
                style: def.num("style", 0.0) as u8,
                pattern: def.get("pattern").unwrap_or("").trim().to_string(),
            }
        }
        "env_sprite" | "env_glow" => {
            let mut color = [255u8; 4];
            if let Some(c) = def.get("rendercolor") {
                for (i, v) in c.split_whitespace().take(3).enumerate() {
                    color[i] = v
                        .parse::<f32>()
                        .map(|v| v.clamp(0.0, 255.0) as u8)
                        .unwrap_or(255);
                }
            }
            color[3] = def.num("renderamt", 255.0).clamp(0.0, 255.0) as u8;
            // CSprite::Spawn: a named sprite starts off unless Start On; unnamed ones are on
            Kind::Sprite {
                on: flags & 1 != 0 || def.get("targetname").is_none_or(|n| n.trim().is_empty()),
                color,
                scale: def.num("scale", 1.0),
            }
        }
        "func_tracktrain" => Kind::Train {
            speed: def.num("startspeed", 0.0),
            max: def.num("speed", 100.0).max(1.0),
            last_speed: def.num("speed", 100.0).max(1.0),
            height: def.num("height", 4.0),
            next: None,
            prev: None,
        },
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
        "prop_door_rotating" => Kind::PropDoor {
            prop: 0,
            angle: 0.0,
            goal: 0.0,
            state: Toggle::AtBottom,
            speed: def.num("speed", 100.0),
            distance: def.num("distance", 90.0),
            return_delay: def.num("returndelay", -1.0),
            open_dir: def.num("opendir", 0.0) as i32,
            locked: flags & 2048 != 0,
            return_at: None,
        },
        "logic_branch" => Kind::Branch {
            value: def.num("InitialValue", 0.0) != 0.0,
        },
        "logic_compare" => Kind::Compare {
            value: def.num("InitialValue", 0.0),
            compare: def.num("CompareValue", 0.0),
        },
        "math_counter" => Kind::Counter {
            value: def.num("startvalue", 0.0),
            at_min: false,
            at_max: false,
        },
        "ambient_generic" => Kind::Ambient {
            playing: false,
            volume: (def.num("health", 10.0) / 10.0).clamp(0.0, 1.0),
            pitch: def.num("pitch", 100.0),
            looped: flags & 32 == 0,
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

    /// The `prop_door_rotating` whose model prop is `prop`.
    pub fn door_of_prop(&self, prop: u32) -> Option<u32> {
        self.entities
            .iter()
            .find(|e| e.alive && matches!(&e.kind, Kind::PropDoor { prop: p, .. } if *p == prop))
            .map(|e| e.id)
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
            target_id: 0,
        });
    }

    /// Fires `name` of entity `idx`: queues its stored outputs (counting `times` down).
    fn fire_output(&mut self, now: u64, idx: usize, name: &str, activator: u32) {
        self.fire_output_value(now, idx, name, activator, "");
    }

    /// `fire_output` with the output's value: an output whose own parameter is empty passes
    /// `value` on (`OutValue`, `OnCaseNN`, `OnTrue`...).
    fn fire_output_value(&mut self, now: u64, idx: usize, name: &str, activator: u32, value: &str) {
        let caller = self.entities[idx].id;
        let mut fired = Vec::new();
        for o in self.entities[idx].outputs.iter_mut() {
            if o.times == 0 || !o.name.eq_ignore_ascii_case(name) {
                continue;
            }
            if o.times > 0 {
                o.times -= 1;
            }
            let param = if o.param.is_empty() {
                value.to_string()
            } else {
                o.param.clone()
            };
            fired.push((o.target.clone(), o.input.clone(), param, o.delay));
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
            if let Kind::PropDoor { prop, .. } = &mut e.kind {
                let model = e.def.get("model").unwrap_or("").to_string();
                if let Some(id) = self.script_spawn_prop(0, &model, e.pos, e.def.angles, true) {
                    *prop = id;
                }
            }
            if e.solid() || matches!(e.kind, Kind::Toggled { .. }) {
                let hulls: Vec<Vec<Vec3>> = e
                    .def
                    .hulls
                    .iter()
                    .map(|h| h.phys_points.iter().map(|p| e.rot * *p + e.pos).collect())
                    .collect();
                if let Ok(b) = self.physics.add_kinematic_hulls(&hulls, surface) {
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
    /// A `path_track` by name.
    fn path_idx(&self, name: &str) -> Option<usize> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        self.map_io.entities.iter().position(|e| {
            e.alive && e.classname == "path_track" && e.targetname.eq_ignore_ascii_case(name)
        })
    }

    /// The node before `idx` on the track: the `path_track` whose `target` it is, else the
    /// one whose `altpath` it is (`CPathTrack::Link` sets the alt node's previous too).
    fn path_prev(&self, idx: usize) -> Option<usize> {
        let name = &self.map_io.entities[idx].targetname;
        let linked = |key: &str| {
            self.map_io.entities.iter().position(|e| {
                e.alive
                    && e.classname == "path_track"
                    && e.def
                        .get(key)
                        .is_some_and(|t| t.trim().eq_ignore_ascii_case(name))
            })
        };
        linked("target").or_else(|| linked("altpath"))
    }

    /// `CPathTrack::GetNext`: the node's `altpath` while its alternate path is on
    /// (`SF_PATH_ALTERNATE`, set by EnableAlternatePath) and not reversed (spawnflag 4), else
    /// its `target`; a disabled node (spawnflag 1, `ValidPath`) ends the track.
    fn path_next(&self, n: usize) -> Option<usize> {
        let e = &self.map_io.entities[n];
        let alt = e.flags & SF_PATH_ALTERNATE != 0 && e.flags & 4 == 0;
        let by = |key: &str| {
            e.def
                .get(key)
                .map(str::to_string)
                .and_then(|t| self.path_idx(&t))
        };
        let idx = if alt {
            by("altpath").or_else(|| by("target"))
        } else {
            by("target")
        }?;
        (self.map_io.entities[idx].flags & 1 == 0).then_some(idx)
    }

    /// `CPathTrack::GetPrevious`: the `altpath` when the alternate path is on and reversed
    /// (spawnflag 4), else the node whose `target` this is; disabled nodes end the track.
    fn path_prev_of(&self, n: usize) -> Option<usize> {
        let e = &self.map_io.entities[n];
        let idx = if e.flags & SF_PATH_ALTERNATE != 0 && e.flags & 4 != 0 {
            e.def
                .get("altpath")
                .map(str::to_string)
                .and_then(|t| self.path_idx(&t))
                .or_else(|| self.path_prev(n))
        } else {
            self.path_prev(n)
        }?;
        (self.map_io.entities[idx].flags & 1 == 0).then_some(idx)
    }

    /// The train teleports to its first `path_track` (`target`) at spawn, `height` above it,
    /// facing the node after it.
    fn train_start(&mut self, idx: usize) {
        let target = self.map_io.entities[idx]
            .def
            .get("target")
            .unwrap_or("")
            .to_string();
        let Some(first) = self.path_idx(&target) else {
            return;
        };
        let node_pos = self.map_io.entities[first].pos;
        let first_id = self.map_io.entities[first].id;
        let after = self.path_next(first);
        let after_id = after.map(|i| self.map_io.entities[i].id);
        let after_pos = after.map(|i| self.map_io.entities[i].pos);
        let e = &mut self.map_io.entities[idx];
        let Kind::Train {
            height, next, prev, ..
        } = &mut e.kind
        else {
            return;
        };
        e.pos = node_pos + Vec3::new(0.0, 0.0, *height);
        e.last_pos = e.pos;
        *prev = Some(first_id);
        *next = after_id;
        if let Some(a) = after_pos {
            e.rot = train_rot(a - node_pos, e.flags);
        }
        e.dirty = true;
    }

    /// The train runs `speed` along the track; reaching a node fires its `OnPass` and the
    /// train's `OnNextPoint`, takes the node's `speed` when it has one and stops at a node
    /// with spawnflag 8 or at the end of the track.
    fn train_step(&mut self, idx: usize, now: u64) {
        let (speed, height, next_id) = match &self.map_io.entities[idx].kind {
            Kind::Train {
                speed,
                height,
                next,
                ..
            } if *speed != 0.0 => (*speed, *height, *next),
            _ => return,
        };
        let Some(next_id) = next_id else {
            return;
        };
        let Some(n) = self.map_io.entities.iter().position(|e| e.id == next_id) else {
            return;
        };
        let node_pos = self.map_io.entities[n].pos;
        let goal = node_pos + Vec3::new(0.0, 0.0, height);
        let step = speed.abs() * TICK;
        let to = goal - self.map_io.entities[idx].pos;
        let dist = to.length();
        if dist > step {
            let e = &mut self.map_io.entities[idx];
            e.pos += to / dist * step;
            e.rot = train_rot(to, e.flags);
            e.dirty = true;
            return;
        }
        let node_speed = self.map_io.entities[n].def.num("speed", 0.0);
        let node_flags = self.map_io.entities[n].flags;
        let mut following = if speed > 0.0 {
            self.path_next(n)
        } else {
            self.path_prev_of(n)
        };
        // spawnflag 16: the train teleports to that node and carries on past it
        let mut teleport = None;
        if let Some(f) = following.filter(|&f| self.map_io.entities[f].flags & 16 != 0) {
            teleport = Some((f, self.map_io.entities[f].pos));
            following = if speed > 0.0 {
                self.path_next(f)
            } else {
                self.path_prev_of(f)
            };
        }
        let following_id = following.map(|i| self.map_io.entities[i].id);
        let following_pos = following.map(|i| self.map_io.entities[i].pos);
        // spawnflag 2: OnPass fires once
        if node_flags & 2 == 0 || node_flags & SF_PATH_PASSED == 0 {
            self.map_io.fire_output(now, n, "OnPass", 0);
            self.map_io.entities[n].flags |= SF_PATH_PASSED;
        }
        self.map_io.fire_output(now, idx, "OnNextPoint", 0);
        if let Some((f, _)) = teleport {
            self.map_io.fire_output(now, f, "OnTeleport", 0);
        }
        let goal = match teleport {
            Some((_, p)) => p + Vec3::new(0.0, 0.0, height),
            None => goal,
        };
        let next_id = match teleport {
            Some((f, _)) => self.map_io.entities[f].id,
            None => next_id,
        };
        let e = &mut self.map_io.entities[idx];
        let Kind::Train {
            speed: sp,
            max,
            last_speed,
            next,
            prev,
            ..
        } = &mut e.kind
        else {
            return;
        };
        e.pos = goal;
        e.dirty = true;
        *prev = Some(next_id);
        *next = following_id;
        if node_speed > 0.0 {
            *sp = node_speed.min(*max).copysign(*sp);
            *last_speed = *sp;
        }
        if node_flags & 8 != 0 || following_id.is_none() {
            *sp = 0.0;
        }
        if let Some(fp) = following_pos {
            e.rot = train_rot(fp - node_pos, e.flags);
        }
    }

    /// The `env_*` point entities' inputs: env_explosion `Explode` (iMagnitude, radius
    /// iRadiusOverride or 2.5 x the magnitude, spawnflag 1 no damage), env_shake `StartShake`
    /// (amplitude / duration / frequency to the players within `radius`, spawnflag 1 everyone),
    /// env_fade `Fade` (duration / holdtime / renderamt / rendercolor; spawnflag 1 fade from,
    /// 2 modulate, 4 the activator only, 8 stay out).
    fn env_input(&mut self, idx: usize, input: &str, activator: u32) {
        let e = &self.map_io.entities[idx];
        let (class, pos, flags, def) = (e.classname.clone(), e.pos, e.flags, e.def.clone());
        match (class.as_str(), input) {
            ("env_explosion", "explode") => {
                let magnitude = def.num("iMagnitude", 100.0);
                let radius = match def.num("iRadiusOverride", 0.0) {
                    r if r > 0.0 => r,
                    _ => magnitude * 2.5,
                };
                if flags & 1 == 0 {
                    self.explode(pos, activator, magnitude, radius, "env_explosion");
                } else {
                    self.effects.push(crate::WorldEffect::Explosion {
                        pos: pos + Vec3::new(0.0, 0.0, 1.0),
                    });
                }
            }
            ("env_shake", "startshake") => {
                let amplitude = def.num("amplitude", 4.0);
                let radius = def.num("radius", 500.0);
                let duration = def.num("duration", 1.0);
                let frequency = def.num("frequency", 2.5);
                let text = format!("Shake {amplitude} {duration} {frequency}");
                let targets: Vec<u32> = self
                    .players
                    .iter()
                    .filter(|(_, p)| flags & 1 != 0 || (p.state.origin - pos).length() <= radius)
                    .map(|(id, _)| *id)
                    .collect();
                for id in targets {
                    self.notices.push((id, text.clone()));
                }
            }
            ("env_fade", "fade") => {
                let duration = def.num("duration", 2.0);
                let hold = def.num("holdtime", 0.0);
                let alpha = def.num("renderamt", 255.0);
                let mut c = def
                    .get("rendercolor")
                    .unwrap_or("0 0 0")
                    .split_whitespace()
                    .filter_map(|v| v.parse::<f32>().ok());
                let (r, g, b) = (
                    c.next().unwrap_or(0.0),
                    c.next().unwrap_or(0.0),
                    c.next().unwrap_or(0.0),
                );
                let mut f = if flags & 1 != 0 { 1 } else { 2 };
                if flags & 2 != 0 {
                    f |= 4;
                }
                if flags & 8 != 0 {
                    f |= 8;
                }
                let text = format!("ScreenFade {r} {g} {b} {alpha} {duration} {hold} {f}");
                let targets: Vec<u32> = if flags & 4 != 0 {
                    vec![activator]
                } else {
                    self.players.keys().copied().collect()
                };
                for id in targets {
                    if self.players.contains_key(&id) {
                        self.notices.push((id, text.clone()));
                    }
                }
            }
            _ => {}
        }
    }

    /// An `ambient_generic` starts (`PlaySound`: its `message` on its own channel, looped as
    /// the entity is) or stops (`StopSound` / `FadeOut` over `fade` seconds).
    fn ambient_sound(&mut self, idx: usize, play: bool, fade: f32) {
        let e = &mut self.map_io.entities[idx];
        let Kind::Ambient {
            playing,
            volume,
            pitch,
            looped,
        } = &mut e.kind
        else {
            return;
        };
        if play {
            let Some(name) = e.def.get("message").filter(|m| !m.is_empty()) else {
                return;
            };
            *playing = *looped;
            self.sounds.push(crate::WorldSound {
                name: name.to_string(),
                pos: e.pos,
                volume: *volume,
                ent: e.id,
                looping: *looped,
                pitch: *pitch,
                ..Default::default()
            });
        } else {
            *playing = false;
            self.sounds.push(crate::WorldSound {
                ent: e.id,
                stop: true,
                fade,
                ..Default::default()
            });
        }
    }

    /// The named light styles for the snapshot: style, brightness (the pattern's mean,
    /// `m` = 1 when none; 0 while every light on the style is off).
    pub fn lightstyles(&self) -> Vec<(u8, f32)> {
        let mut out: Vec<(u8, f32)> = Vec::new();
        for e in self.map_io.entities.iter().filter(|e| e.alive) {
            let Kind::Light { on, style, pattern } = &e.kind else {
                continue;
            };
            let b = if *on {
                pattern_mean(if pattern.is_empty() { "m" } else { pattern })
            } else {
                0.0
            };
            match out.iter_mut().find(|(s, _)| s == style) {
                Some(slot) => slot.1 = slot.1.max(b),
                None => out.push((*style, b)),
            }
        }
        out
    }

    /// The shown `env_sprite`s: id, sprite material (no extension), position, `scale`,
    /// rgba (`rendercolor`, `renderamt`), `rendermode`.
    pub fn map_sprite_states(&self) -> Vec<(u32, String, Vec3, f32, [u8; 4], u8)> {
        self.map_io
            .entities
            .iter()
            .filter(|e| e.alive)
            .filter_map(|e| {
                let Kind::Sprite { on, color, scale } = &e.kind else {
                    return None;
                };
                if !*on {
                    return None;
                }
                let model = e.def.get("model")?.replace('\\', "/").to_ascii_lowercase();
                let model = model.trim_end_matches(".spr").trim_end_matches(".vmt");
                Some((
                    e.id,
                    model.to_string(),
                    e.pos,
                    *scale,
                    *color,
                    e.def.num("rendermode", 0.0) as u8,
                ))
            })
            .collect()
    }

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
                let angles = if matches!(e.kind, Kind::Train { .. }) {
                    let (p, y, r) = gm_physics::angles_from_quat(e.rot);
                    Vec3::new(p, y, r)
                } else {
                    angles
                };
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

    /// `prop_door_rotating` motion: the model prop turns about Z toward the goal at `speed`
    /// deg/s; arrival fires `OnFullyOpen` / `OnFullyClosed` and starts the return delay.
    fn tick_prop_doors(&mut self) {
        let now = self.tick_count;
        let mut moves: Vec<(u32, Vec3, Vec3)> = Vec::new();
        let mut fired: Vec<(usize, &'static str)> = Vec::new();
        for idx in 0..self.map_io.entities.len() {
            let e = &mut self.map_io.entities[idx];
            if !e.alive {
                continue;
            }
            let (pos, base) = (e.pos, e.def.angles);
            let Kind::PropDoor {
                prop,
                angle,
                goal,
                state,
                speed,
                return_delay,
                return_at,
                ..
            } = &mut e.kind
            else {
                continue;
            };
            match *state {
                Toggle::AtTop => {
                    if return_at.is_some_and(|t| now >= t) {
                        *return_at = None;
                        *goal = 0.0;
                        *state = Toggle::GoingDown;
                        fired.push((idx, "OnClose"));
                    }
                }
                Toggle::GoingUp | Toggle::GoingDown => {
                    let step = *speed * TICK;
                    let d = *goal - *angle;
                    if d.abs() <= step {
                        *angle = *goal;
                        if *state == Toggle::GoingUp {
                            *state = Toggle::AtTop;
                            fired.push((idx, "OnFullyOpen"));
                            if *return_delay >= 0.0 {
                                *return_at = Some(now + ticks(*return_delay));
                            }
                        } else {
                            *state = Toggle::AtBottom;
                            fired.push((idx, "OnFullyClosed"));
                        }
                    } else {
                        *angle += step * d.signum();
                    }
                    moves.push((*prop, pos, Vec3::new(base.x, base.y + *angle, base.z)));
                }
                Toggle::AtBottom => {}
            }
        }
        for (id, p, a) in moves {
            self.teleport_prop(id, p, a);
        }
        for (idx, name) in fired {
            self.map_io.fire_output(now, idx, name, 0);
        }
    }

    /// Delivers `input` to map entity `id` now.
    pub fn map_input(&mut self, id: u32, input: &str, param: &str, activator: u32) {
        self.tick_prop_doors();
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
        if self.map_io.entities[idx].classname == "path_track" {
            let flags = &mut self.map_io.entities[idx].flags;
            match input_lc.as_str() {
                "enablealternatepath" => *flags |= SF_PATH_ALTERNATE,
                "disablealternatepath" => *flags &= !SF_PATH_ALTERNATE,
                "togglealternatepath" => *flags ^= SF_PATH_ALTERNATE,
                "enablepath" => *flags &= !1,
                "disablepath" => *flags |= 1,
                "togglepath" => *flags ^= 1,
                _ => {}
            }
        }
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
        // outputs that carry a value (logic / math entities, props.md §12, gap-report #205)
        let mut valued: Vec<(String, String)> = Vec::new();
        let p_num: f32 = param.trim().parse().unwrap_or(0.0);
        let num_str = |v: f32| -> String {
            if v.fract() == 0.0 && v.abs() < 1.0e9 {
                format!("{}", v as i64)
            } else {
                format!("{v}")
            }
        };
        // deterministic per (tick, entity, salt) draw in 0..1 for the random pickers
        let rnd = |salt: u64| -> f32 {
            let x = now
                .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                .wrapping_add(((id as u64) << 16) ^ salt)
                .wrapping_mul(0xBF58_476D_1CE4_E5B9);
            ((x >> 40) as f32) / 16_777_216.0
        };
        let act_pos = self.players.get(&activator).map(|p| p.state.origin);
        let e = &mut self.map_io.entities[idx];
        let was_moving = e.moving();
        let mut ambient: Option<(bool, f32)> = None;
        let mut env = false;
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
                        events.push("OnLockedUse");
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
                        events.push("OnLockedUse");
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
            Kind::PropDoor {
                goal,
                state,
                distance,
                open_dir,
                locked,
                return_at,
                ..
            } => match input_lc.as_str() {
                "lock" => *locked = true,
                "unlock" => *locked = false,
                "open" | "use" | "toggle"
                    if *state == Toggle::AtBottom || *state == Toggle::GoingDown =>
                {
                    if *locked && input_lc == "use" {
                        events.push("OnLockedUse");
                    } else {
                        // away from the activator: in front of the door's forward it swings
                        // backward (sign convention M), `opendir` forces a side
                        let fwd = e.rot * Vec3::X;
                        let front = act_pos.is_none_or(|a| (a - e.pos).dot(fwd) > 0.0);
                        let sign = match *open_dir {
                            1 => 1.0,
                            2 => -1.0,
                            _ => {
                                if front {
                                    -1.0
                                } else {
                                    1.0
                                }
                            }
                        };
                        *goal = sign * *distance;
                        *state = Toggle::GoingUp;
                        *return_at = None;
                        events.push("OnOpen");
                    }
                }
                "close" | "use" | "toggle"
                    if *state == Toggle::AtTop || *state == Toggle::GoingUp =>
                {
                    *goal = 0.0;
                    *state = Toggle::GoingDown;
                    *return_at = None;
                    events.push("OnClose");
                }
                _ => {}
            },
            Kind::Branch { value } => {
                let test = |v: bool, valued: &mut Vec<(String, String)>| {
                    valued.push((
                        if v { "OnTrue" } else { "OnFalse" }.to_string(),
                        if v { "1" } else { "0" }.to_string(),
                    ))
                };
                match input_lc.as_str() {
                    "setvalue" => *value = p_num != 0.0,
                    "setvaluetest" => {
                        *value = p_num != 0.0;
                        test(*value, &mut valued);
                    }
                    "toggle" => *value = !*value,
                    "toggletest" => {
                        *value = !*value;
                        test(*value, &mut valued);
                    }
                    "test" => test(*value, &mut valued),
                    _ => {}
                }
            }
            Kind::Compare { value, compare } => {
                let cmp = |v: f32, c: f32, valued: &mut Vec<(String, String)>| {
                    let out = num_str(v);
                    if v < c {
                        valued.push(("OnLessThan".to_string(), out.clone()));
                    } else if v > c {
                        valued.push(("OnGreaterThan".to_string(), out.clone()));
                    } else {
                        valued.push(("OnEqualTo".to_string(), out.clone()));
                    }
                    if v != c {
                        valued.push(("OnNotEqualTo".to_string(), out));
                    }
                };
                match input_lc.as_str() {
                    "setvalue" => *value = p_num,
                    "setvaluecompare" => {
                        *value = p_num;
                        cmp(*value, *compare, &mut valued);
                    }
                    "setcomparevalue" => *compare = p_num,
                    "compare" => cmp(*value, *compare, &mut valued),
                    _ => {}
                }
            }
            Kind::Counter {
                value,
                at_min,
                at_max,
            } => {
                let (min, max) = (e.def.num("min", 0.0), e.def.num("max", 0.0));
                let before = *value;
                let mut fire = true;
                match input_lc.as_str() {
                    "add" => *value += p_num,
                    "subtract" => *value -= p_num,
                    "multiply" => *value *= p_num,
                    "divide" => {
                        if p_num != 0.0 {
                            *value /= p_num;
                        }
                    }
                    "setvalue" => *value = p_num,
                    "setvaluenofire" => {
                        *value = p_num;
                        fire = false;
                    }
                    "getvalue" => {
                        valued.push(("OnGetValue".to_string(), num_str(*value)));
                        fire = false;
                    }
                    _ => fire = false,
                }
                if !(min == 0.0 && max == 0.0) {
                    *value = value.clamp(min.min(max), max.max(min));
                }
                if fire {
                    valued.push(("OutValue".to_string(), num_str(*value)));
                    if !(min == 0.0 && max == 0.0) {
                        if *value <= min && !*at_min {
                            *at_min = true;
                            valued.push(("OnHitMin".to_string(), num_str(*value)));
                        } else if *value > min && *at_min {
                            *at_min = false;
                            valued.push(("OnChangedFromMin".to_string(), num_str(*value)));
                        }
                        if *value >= max && !*at_max {
                            *at_max = true;
                            valued.push(("OnHitMax".to_string(), num_str(*value)));
                        } else if *value < max && *at_max {
                            *at_max = false;
                            valued.push(("OnChangedFromMax".to_string(), num_str(*value)));
                        }
                    }
                }
                let _ = before;
            }
            Kind::Logic => match (e.classname.as_str(), input_lc.as_str()) {
                (c, _) if c.starts_with("env_") => env = true,
                ("logic_case", "invalue") => {
                    let wanted = param.trim();
                    let hit = (1..=16).find(|i| {
                        e.def
                            .get(&format!("Case{i:02}"))
                            .is_some_and(|c| c.trim().eq_ignore_ascii_case(wanted))
                    });
                    match hit {
                        Some(i) => valued.push((format!("OnCase{i:02}"), wanted.to_string())),
                        None => valued.push(("OnDefault".to_string(), wanted.to_string())),
                    }
                }
                ("logic_case", "pickrandom" | "pickrandomshuffle") => {
                    let cases: Vec<usize> = (1..=16)
                        .filter(|i| e.def.get(&format!("Case{i:02}")).is_some())
                        .collect();
                    if !cases.is_empty() {
                        let i =
                            cases[((rnd(1) * cases.len() as f32) as usize).min(cases.len() - 1)];
                        let v = e
                            .def
                            .get(&format!("Case{i:02}"))
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        valued.push((format!("OnCase{i:02}"), v));
                    }
                }
                ("math_remap", "invalue") => {
                    let (in1, in2) = (e.def.num("in1", 0.0), e.def.num("in2", 1.0));
                    let (out1, out2) = (e.def.num("out1", 0.0), e.def.num("out2", 1.0));
                    let (lo, hi) = (in1.min(in2), in1.max(in2));
                    let mut v = p_num;
                    let out_of_range = v < lo || v > hi;
                    if !(out_of_range && e.flags & 1 != 0) {
                        if e.flags & 2 != 0 {
                            v = v.clamp(lo, hi);
                        }
                        let t = if in2 != in1 {
                            (v - in1) / (in2 - in1)
                        } else {
                            0.0
                        };
                        valued.push(("OutValue".to_string(), num_str(out1 + t * (out2 - out1))));
                    }
                }
                ("logic_random_outputs", "trigger") => {
                    for i in 1..=8u64 {
                        let chance = e.def.num(&format!("OnTriggerChance{i}"), 1.0);
                        if rnd(i) < chance {
                            valued.push((format!("OnRandomOutput{i}"), String::new()));
                            if e.flags & 1 != 0 {
                                break;
                            }
                        }
                    }
                }
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
            Kind::Train {
                speed,
                max,
                last_speed,
                next,
                prev,
                ..
            } => {
                let p: f32 = param.trim().parse().unwrap_or(0.0);
                let dir = |speed: f32, last: f32| if speed != 0.0 { speed } else { last }.signum();
                let before = *speed;
                let old_dir = dir(*speed, *last_speed);
                match input_lc.as_str() {
                    "setspeed" => *speed = *max * p.clamp(0.0, 1.0) * old_dir,
                    "setspeeddir" => *speed = *max * p.clamp(-1.0, 1.0),
                    "setspeedreal" => *speed = p.clamp(-*max, *max),
                    "setmaxspeed" => *max = p.max(1.0),
                    "stop" => *speed = 0.0,
                    "startforward" => *speed = *max,
                    "startbackward" => *speed = -*max,
                    "resume" => *speed = *last_speed,
                    "reverse" => {
                        if *speed != 0.0 {
                            *speed = -*speed;
                        } else {
                            *last_speed = -*last_speed;
                        }
                    }
                    "toggle" => *speed = if *speed != 0.0 { 0.0 } else { *last_speed },
                    _ => {}
                }
                if *speed != 0.0 {
                    *last_speed = *speed;
                }
                if dir(*speed, *last_speed) != old_dir {
                    std::mem::swap(next, prev);
                }
                if before == 0.0 && *speed != 0.0 {
                    events.push("OnStart");
                }
            }
            Kind::Sprite { on, color, scale } => {
                let channel =
                    |v: &str| v.trim().parse::<f32>().unwrap_or(255.0).clamp(0.0, 255.0) as u8;
                match input_lc.as_str() {
                    "showsprite" => *on = true,
                    "hidesprite" => *on = false,
                    "togglesprite" => *on = !*on,
                    "colorredvalue" => color[0] = channel(param),
                    "colorgreenvalue" => color[1] = channel(param),
                    "colorbluevalue" => color[2] = channel(param),
                    "alpha" => color[3] = channel(param),
                    "setscale" => *scale = param.trim().parse().unwrap_or(*scale),
                    _ => {}
                }
            }
            Kind::Light { on, pattern, .. } => match input_lc.as_str() {
                "turnon" => *on = true,
                "turnoff" => *on = false,
                "toggle" => *on = !*on,
                "setpattern" => *pattern = param.trim().to_string(),
                _ => {}
            },
            Kind::Ambient {
                playing,
                volume,
                pitch,
                ..
            } => {
                let p: f32 = param.trim().parse().unwrap_or(0.0);
                match input_lc.as_str() {
                    "playsound" | "fadein" => ambient = Some((true, 0.0)),
                    "stopsound" => ambient = Some((false, 0.0)),
                    "fadeout" => ambient = Some((false, p.max(0.0))),
                    "togglesound" | "use" => ambient = Some((!*playing, 0.0)),
                    "volume" => {
                        *volume = (p / 10.0).clamp(0.0, 1.0);
                        if *playing {
                            ambient = Some((true, 0.0));
                        }
                    }
                    "pitch" => {
                        *pitch = p.clamp(1.0, 255.0);
                        if *playing {
                            ambient = Some((true, 0.0));
                        }
                    }
                    _ => {}
                }
            }
        }
        // the SDK's door / button sounds: the move sound as the travel starts, the locked
        // sound on a locked use
        let e = &self.map_io.entities[idx];
        let center = e.center();
        let mut sounds: Vec<String> = Vec::new();
        let silent = e.classname.starts_with("func_door") && e.flags & 4096 != 0;
        if !was_moving && e.moving() && !silent {
            sounds.extend(e.sound(EntSound::Move));
        }
        if events.contains(&"OnLockedUse") {
            sounds.extend(e.sound(EntSound::Locked));
        }
        for name in sounds {
            self.play_sound(name, center, 1.0);
        }
        if let Some((play, fade)) = ambient {
            self.ambient_sound(idx, play, fade);
        }
        if env {
            self.env_input(idx, &input_lc, activator);
        }
        // a prop door's sounds come from its model's door_options (DoorSound.Null when absent)
        if let Kind::PropDoor { prop, .. } = &self.map_io.entities[idx].kind {
            let prop = *prop;
            let which = if events.contains(&"OnOpen") {
                Some("open")
            } else if events.contains(&"OnClose") {
                Some("close")
            } else if events.contains(&"OnLockedUse") {
                Some("locked")
            } else {
                None
            };
            if let Some(k) = which {
                let name = self
                    .sandbox
                    .props()
                    .iter()
                    .find(|p| p.id == prop)
                    .and_then(|p| self.models.get(&p.model))
                    .and_then(|m| m.door_options.iter().find(|(a, _)| a == k))
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(|| "DoorSound.Null".to_string());
                let pos = self.map_io.entities[idx].pos;
                self.sounds.push(crate::WorldSound {
                    name,
                    pos,
                    volume: 1.0,
                    ent: prop,
                    channel: 6,
                    ..Default::default()
                });
            }
        }
        for (name, value) in valued {
            self.map_io
                .fire_output_value(now, idx, &name, activator, &value);
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
            for idx in 0..self.map_io.entities.len() {
                let e = &self.map_io.entities[idx];
                if matches!(e.kind, Kind::Ambient { .. }) && e.flags & 16 == 0 {
                    self.ambient_sound(idx, true, 0.0);
                }
            }
            for idx in 0..self.map_io.entities.len() {
                if matches!(self.map_io.entities[idx].kind, Kind::Train { .. }) {
                    self.train_start(idx);
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
            let silent = e.classname.starts_with("func_door") && e.flags & 4096 != 0;
            let start = if !was_moving && e.moving() && !silent {
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
        for idx in 0..self.map_io.entities.len() {
            let e = &self.map_io.entities[idx];
            if e.alive && matches!(e.kind, Kind::Train { .. }) {
                self.train_step(idx, now);
            }
        }
        // physics bodies follow the poses; the players riding a mover take its step, the ones
        // in its way are pushed by it, and a mover that cannot clear a player reverses
        // the movers blocked this tick with who blocked them: (is an NPC, id)
        let mut blocked: Vec<(usize, Vec<(bool, u32)>)> = Vec::new();
        for idx in 0..self.map_io.entities.len() {
            let e = &mut self.map_io.entities[idx];
            if !e.dirty {
                continue;
            }
            e.dirty = false;
            self.map_io.convexes_dirty = true;
            if let Some(b) = e.body {
                if e.solid() {
                    // kinematic: the props in the way are pushed, the ones on it carried
                    self.physics.move_kinematic(b, e.pos, e.rot);
                } else {
                    self.physics.teleport(b, Vec3::new(0.0, 0.0, -1.0e6), e.rot);
                }
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
            let mut hit: Vec<(bool, u32)> = Vec::new();
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
                    hit.push((false, pid));
                }
            }
            // NPCs in the way go along the same way and block when still inside
            let npc_ids: Vec<u32> = self
                .npcs
                .iter()
                .filter(|(_, n)| {
                    n.dead_at.is_none() && e.overlaps_box(n.pos + n.mins, n.pos + n.maxs)
                })
                .map(|(id, _)| *id)
                .collect();
            for nid in npc_ids {
                let Some(n) = self.npcs.get_mut(&nid) else {
                    continue;
                };
                n.pos += step;
                if e.overlaps_box(n.pos + n.mins, n.pos + n.maxs) {
                    hit.push((true, nid));
                }
            }
            if !hit.is_empty() {
                blocked.push((idx, hit));
            }
        }
        for (idx, hit) in blocked {
            // `CBaseDoor::Blocked`: the blocker takes `dmg` (`blockdamage` on func_movelinear)
            // as crush damage, then the door turns back unless `forceclosed`
            let (dmg, force) = {
                let e = &self.map_io.entities[idx];
                (
                    e.def.num("dmg", e.def.num("blockdamage", 0.0)),
                    e.def.get("forceclosed").is_some_and(|v| v.trim() == "1"),
                )
            };
            if dmg > 0.0 {
                for (npc, id) in &hit {
                    if *npc {
                        self.damage_npc_typed(*id, 0, dmg, crate::weapons::HITGROUP_CHEST, false);
                    } else {
                        self.damage_player(*id, dmg, crate::weapons::DMG_CRUSH);
                    }
                }
            }
            // `OnBlockedOpening` / `OnBlockedClosing` with the blocker; a `forceclosed` door
            // keeps pushing (the SDK also keeps pushing with `wait` -1; the recorded door test
            // turns back, so that rule is not applied)
            let opening = match &self.map_io.entities[idx].kind {
                Kind::Mover { state, .. } | Kind::Rotator { state, .. } => {
                    *state == Toggle::GoingUp
                }
                _ => true,
            };
            let name = if opening {
                "OnBlockedOpening"
            } else {
                "OnBlockedClosing"
            };
            let blocker = hit.first().map(|h| h.1).unwrap_or(0);
            if force {
                self.map_io.fire_output(now, idx, name, blocker);
                continue;
            }
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
            self.map_io.fire_output(now, idx, name, blocker);
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
            // PassesTriggerFilters (props.md #106, gap-report #204): 1 clients, 2 NPCs,
            // 4 pushables, 8 physics objects, 0x40 everything; no flags -> nothing
            let all = flags & 0x40 != 0;
            let candidates = players
                .iter()
                .filter(|_| all || flags & 1 != 0)
                .chain(npcs.iter().filter(|_| all || flags & 2 != 0))
                .chain(props.iter().filter(|_| all || flags & (8 | 4) != 0));
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
        self.map_io.ready.extend(due);
        if !self.defer_inputs {
            self.apply_due_inputs();
        }
    }

    /// The entities a due input addresses.
    fn input_targets(&self, p: &Pending) -> Vec<u32> {
        if p.target_id != 0 {
            return vec![p.target_id];
        }
        match p.target.as_str() {
            "!self" | "!caller" => vec![p.caller],
            "!activator" => vec![p.activator],
            _ => self.map_io.find_by_name(&p.target),
        }
    }

    fn input_at(&mut self, t: u32, p: &Pending) {
        if (MAP_ENT_ID_BASE..crate::PROJECTILE_ID_BASE).contains(&t) {
            self.map_input(t, &p.input, &p.param, p.activator);
        } else if t != 0 {
            self.entity_input(t, &p.input, &p.param, p.activator);
        }
    }

    /// Applies the inputs that came due this tick.
    pub fn apply_due_inputs(&mut self) {
        for p in std::mem::take(&mut self.map_io.ready) {
            for t in self.input_targets(&p) {
                self.input_at(t, &p);
            }
        }
    }

    /// Applies the inputs that came due this tick through the `AcceptInput` hook: one the
    /// hook takes (returns true) is dropped.
    pub fn flush_due_inputs(&mut self, hooks: &mut dyn crate::ServerHooks) {
        for p in std::mem::take(&mut self.map_io.ready) {
            for t in self.input_targets(&p) {
                if hooks.accept_input(self, t, &p.input, p.activator, p.caller, &p.param) {
                    self.input_at(t, &p);
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
        self.map_io.ready.push(Pending {
            due: self.tick_count,
            target: String::new(),
            target_id: ent,
            input: "Use".into(),
            param: String::new(),
            activator: player,
            caller: player,
        });
        if !self.defer_inputs {
            self.apply_due_inputs();
        }
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
