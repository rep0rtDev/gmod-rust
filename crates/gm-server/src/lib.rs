//! One server tick: usercmds through `gm-move`, sandbox commands through `gm-sandbox`, and the
//! props and constraints the sandbox owns mirrored into the Jolt world of `gm-physics`
//! (weld -> fixed, rope -> distance, axis -> hinge). Nothing here is networked yet.

pub mod weapons;

use glam::Vec3;
use gm_physics::physgun::{Grab, PhysgunParams};
use gm_physics::{BodyHandle, ConstraintHandle, PropDesc, Surface, World};
use gm_sandbox::{ConstraintKind, Event, ModelBounds, PropBody, Sandbox, SpawnRequest, ToolHit};
use std::collections::BTreeMap;

/// Everything the server needs to know about a model to spawn and trace it.
#[derive(Clone, Debug)]
pub struct ModelInfo {
    pub bounds: ModelBounds,
    /// PHY convexes in the entity frame (Source units).
    pub hulls: Vec<Vec<Vec3>>,
    /// The same convexes as outward planes (normal, distance) for the sandbox traces.
    pub hull_planes: Vec<Vec<(Vec3, f32)>>,
    pub mass: f32,
    pub volume: f32,
    pub surface: Surface,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub drag_axis_areas: Vec3,
    pub inertia: Option<Vec3>,
}

/// Result of a tool gun click on the server.
#[derive(Clone, Debug, PartialEq)]
pub enum ToolClick {
    /// A Rust stool handled it (its return value).
    Done(bool),
    /// No hit at all.
    Miss,
    /// The sandbox has no such stool: scripts may (hit passed on).
    Unknown(ToolHit),
}

/// Game-logic hooks around server commands (the addon runtime implements them).
pub trait ServerHooks {
    /// `prop.spawn_request`: false refuses the spawn.
    fn prop_spawn_request(&mut self, server: &mut Server<'_>, player: u32, model: &str) -> bool;
    /// `entity.spawned` after a prop was created.
    fn entity_spawned(&mut self, server: &mut Server<'_>, player: u32, ent: u32);
    /// A click of a stool the sandbox does not know: a scripted tool may handle it.
    fn tool_click(
        &mut self,
        _server: &mut Server<'_>,
        _player: u32,
        _tool: &str,
        _hit: &ToolHit,
        _right: bool,
    ) -> bool {
        false
    }
    /// Admin command: hot reload of a loaded addon by id.
    fn reload_addon(&mut self, _addon: &str) -> Result<String, String> {
        Err("no addon runtime".into())
    }
    /// Around every tick.
    fn pre_tick(&mut self, server: &mut Server<'_>);
    fn post_tick(&mut self, server: &mut Server<'_>);
}

/// Loads a model by name on first use.
pub type ModelLoader = Box<dyn FnMut(&str) -> Option<ModelInfo>>;

pub struct Player {
    pub state: gm_move::PlayerState,
    pub view_angles: Vec3,
    /// The prop held by the physgun and how it was grabbed.
    pub held: Option<(u32, Grab)>,
    pub alive: bool,
    pub health: i32,
    pub armor: i32,
    pub max_health: i32,
    pub model: String,
    /// Weapon classes carried, sorted (`Player:GetWeapons`).
    pub weapons: Vec<String>,
    pub active_weapon: String,
    pub inventory: weapons::Inventory,
    pub deaths: u32,
    pub frags: u32,
    /// The ragdoll of the last death, while the player is dead.
    pub ragdoll: Option<Ragdoll>,
    /// Tick of the last death: the player respawns `RESPAWN_TICKS` later.
    death_tick: Option<u64>,
    /// Fractional damage carried to the next hit (health is whole); measured to survive
    /// `SetHealth` and respawns, so it is only ever reset with the player.
    damage_acc: f32,
    /// Vertical velocity before the last movement tick, for fall damage on landing.
    prev_vz: f32,
    prev_on_ground: bool,
}

/// Damage types (`DMG_*` bits) that matter to the rules here.
pub const DMG_FALL: u32 = 32;
pub const DMG_DROWN: u32 = 16384;
/// Measured (`damage_hits`): armour takes 80 % of a hit up to what it has, health the rest;
/// fall and drowning damage ignore armour.
pub const ARMOR_SHARE: f32 = 0.8;
/// Measured (`damage_fall_*`): landing with the previous tick's downward speed above this
/// hurts, 10 flat with `mp_falldamage 0`, `(v - threshold) * 25/99` with `mp_falldamage 1`
/// (fits 540 to 1197 u/s to 0.01; 522 u/s is safe, 540 is not).
pub const FALL_SAFE_SPEED: f32 = 526.5;
pub const FALL_DAMAGE_PER_SPEED: f32 = 25.0 / 99.0;
pub const FALL_DAMAGE_FLAT: f32 = 10.0;

impl Player {
    pub fn new(origin: Vec3, view_angles: Vec3) -> Self {
        Player {
            state: gm_move::PlayerState {
                origin,
                ..Default::default()
            },
            view_angles,
            held: None,
            alive: true,
            health: PLAYER_HEALTH,
            armor: 0,
            max_health: PLAYER_HEALTH,
            model: PLAYER_MODEL.to_string(),
            weapons: LOADOUT.iter().map(|w| w.to_string()).collect(),
            active_weapon: SPAWN_WEAPON.to_string(),
            inventory: {
                let mut inv = weapons::Inventory::default();
                inv.give_loadout();
                inv
            },
            deaths: 0,
            frags: 0,
            ragdoll: None,
            death_tick: None,
            damage_acc: 0.0,
            prev_vz: 0.0,
            prev_on_ground: true,
        }
    }
}

/// Measured on the real sandbox (`player_lifecycle`): spawn health 100, armour 0, Kleiner, the
/// 13 weapon loadout with the crowbar out, a ragdoll the tick of death, weapons stripped the
/// tick after, respawn 135 ticks after death at a random spawn point 1 u above it.
pub const PLAYER_HEALTH: i32 = 100;
pub const PLAYER_MODEL: &str = "models/player/kleiner.mdl";
pub const SPAWN_WEAPON: &str = "weapon_crowbar";
pub const RESPAWN_TICKS: u64 = 135;
pub const SPAWN_Z_OFFSET: f32 = 1.0;
pub const LOADOUT: [&str; 13] = [
    "gmod_camera",
    "gmod_tool",
    "weapon_357",
    "weapon_ar2",
    "weapon_crossbow",
    "weapon_crowbar",
    "weapon_frag",
    "weapon_physcannon",
    "weapon_physgun",
    "weapon_pistol",
    "weapon_rpg",
    "weapon_shotgun",
    "weapon_smg1",
];

/// Measured (`ragdoll_still`): the server side of a dead player is an `hl2mp_ragdoll` entity
/// at the player's origin with zero angles and no physics objects at all (0 on the server; the
/// ragdoll is simulated on each client), so the server only keeps the record.
#[derive(Clone, Debug, PartialEq)]
pub struct Ragdoll {
    pub class: &'static str,
    pub model: String,
    pub origin: Vec3,
    pub angles: Vec3,
    pub velocity: Vec3,
    pub tick: u64,
}

pub const RAGDOLL_CLASS: &str = "hl2mp_ragdoll";

/// `info_player_start`: where players (re)spawn, picked at random.
#[derive(Clone, Debug)]
pub struct SpawnPoint {
    pub pos: Vec3,
    pub angles: Vec3,
}

/// `IN_ATTACK`, `IN_USE`, `IN_ATTACK2` of a usercmd.
pub const IN_ATTACK: u32 = 1;
pub const IN_USE: u32 = 32;
pub const IN_ATTACK2: u32 = 2048;

/// A player's input for one tick: movement plus what the held weapon needs.
#[derive(Clone, Debug, Default)]
pub struct PlayerCmd {
    pub cmd: gm_move::UserCmd,
    /// The player holds `weapon_physgun`: attack grabs, attack2 freezes, use + mouse rotates.
    pub physgun: bool,
    pub mouse_x: f32,
    pub mouse_y: f32,
}

/// One bullet (or pellet group) that hit a player this tick.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub tick: u64,
    pub attacker: u32,
    pub target: u32,
    pub damage: f32,
    pub hitgroup: u8,
    pub pos: Vec3,
    pub ammo: &'static str,
}

pub struct Server<'w> {
    pub hits: Vec<Hit>,
    pub collision: &'w gm_collision::World,
    pub physics: World,
    /// The static world body constraints to the world attach to.
    world_body: BodyHandle,
    pub sandbox: Sandbox,
    pub players: BTreeMap<u32, Player>,
    mover: gm_move::Mover<'w>,
    bodies: BTreeMap<u32, BodyHandle>,
    constraints: BTreeMap<u32, ConstraintHandle>,
    /// NoCollide pairs by sandbox constraint id.
    nocollides: BTreeMap<u32, (BodyHandle, BodyHandle)>,
    models: BTreeMap<String, ModelInfo>,
    hull_planes: BTreeMap<String, Vec<Vec<(Vec3, f32)>>>,
    pub events: Vec<Event>,
    pub physgun_params: PhysgunParams,
    /// Loads a model the first time something spawns it (the dedicated server reads the MDL/PHY
    /// from the mounted content on demand).
    pub model_loader: Option<ModelLoader>,
    pub spawn_points: Vec<SpawnPoint>,
    spawn_rng: u64,
    tick_count: u64,
    /// `mp_falldamage`: progressive fall damage instead of the flat 10.
    pub fall_damage_progressive: bool,
}

impl<'w> Server<'w> {
    pub fn new(collision: &'w gm_collision::World, physics: World, world_body: BodyHandle) -> Self {
        Server {
            hits: Vec::new(),
            collision,
            physics,
            world_body,
            sandbox: Sandbox::new(Default::default()),
            players: BTreeMap::new(),
            mover: gm_move::Mover::new(gm_move::MoveConfig::default(), collision),
            bodies: BTreeMap::new(),
            constraints: BTreeMap::new(),
            nocollides: BTreeMap::new(),
            models: BTreeMap::new(),
            hull_planes: BTreeMap::new(),
            events: Vec::new(),
            physgun_params: PhysgunParams::default(),
            model_loader: None,
            spawn_points: Vec::new(),
            spawn_rng: 0x9E37_79B9_7F4A_7C15,
            tick_count: 0,
            fall_damage_progressive: false,
        }
    }

    /// `Player:SetPos` with the velocity cleared (how the probe drops a bot).
    pub fn teleport_player(&mut self, id: u32, pos: Vec3) {
        if let Some(p) = self.players.get_mut(&id) {
            p.state.origin = pos;
            p.state.velocity = Vec3::ZERO;
            p.state.on_ground = false;
            p.prev_vz = 0.0;
            p.prev_on_ground = false;
        }
    }

    pub fn set_player_health(&mut self, id: u32, health: i32) {
        // measured: the fractional damage carried over survives `SetHealth` (and respawns)
        if let Some(p) = self.players.get_mut(&id) {
            p.health = health;
        }
    }

    pub fn set_player_armor(&mut self, id: u32, armor: i32) {
        if let Some(p) = self.players.get_mut(&id) {
            p.armor = armor;
        }
    }

    /// `Entity:TakeDamage` on a player: armour share, whole-point health with the fraction
    /// carried over, death at zero. Returns the health damage applied.
    pub fn damage_player(&mut self, id: u32, amount: f32, dmg_type: u32) -> f32 {
        let tick = self.tick_count;
        let Some(p) = self.players.get_mut(&id) else {
            return 0.0;
        };
        if !p.alive || amount <= 0.0 {
            return 0.0;
        }
        let mut to_health = amount;
        if p.armor > 0 && dmg_type & (DMG_FALL | DMG_DROWN) == 0 {
            let mut to_armor = amount * ARMOR_SHARE;
            if to_armor > p.armor as f32 {
                to_armor = p.armor as f32;
            }
            // the armour value is kept as a float by the game and read truncated: 50 - 19.2 -> 30
            p.armor = (p.armor as f32 - to_armor).floor() as i32;
            to_health = amount - to_armor;
        }
        p.damage_acc += to_health;
        let whole = p.damage_acc.floor();
        p.damage_acc -= whole;
        p.health -= whole as i32;
        if p.health <= 0 {
            p.alive = false;
            p.deaths += 1;
            p.ragdoll = Some(Ragdoll {
                class: RAGDOLL_CLASS,
                model: p.model.clone(),
                origin: p.state.origin,
                angles: Vec3::ZERO,
                velocity: p.state.velocity,
                tick,
            });
            p.active_weapon.clear();
            p.held = None;
            p.state.velocity = Vec3::ZERO;
            if dmg_type & DMG_FALL != 0 {
                // measured: a bot killed by a fall keeps the negative health and does not
                // respawn by itself within the recording
                p.death_tick = None;
            } else {
                p.health = 0;
                p.death_tick = Some(tick);
            }
        }
        to_health
    }

    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Spawn points from the map's entities.
    pub fn set_spawn_points_from_bsp(&mut self, bsp: &gm_formats::bsp::Bsp) {
        self.spawn_points = bsp
            .entities
            .iter()
            .filter(|e| e.classname() == "info_player_start")
            .filter_map(|e| {
                e.origin().map(|o| SpawnPoint {
                    pos: Vec3::from(o),
                    angles: Vec3::from(e.angles().unwrap_or([0.0; 3])),
                })
            })
            .collect();
    }

    fn pick_spawn(&mut self) -> Option<SpawnPoint> {
        if self.spawn_points.is_empty() {
            return None;
        }
        // xorshift64*: deterministic for the harnesses, uniform like the game's random pick
        let mut x = self.spawn_rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.spawn_rng = x;
        let r = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        let i = (r >> 33) as usize % self.spawn_points.len();
        Some(self.spawn_points[i].clone())
    }

    /// `Player:Spawn`: full health, the loadout, a spawn point 1 u above its origin.
    pub fn spawn_player(&mut self, id: u32) {
        let spawn = self.pick_spawn();
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        if let Some(sp) = spawn {
            p.state = gm_move::PlayerState {
                origin: sp.pos + Vec3::new(0.0, 0.0, SPAWN_Z_OFFSET),
                ..Default::default()
            };
            p.view_angles = sp.angles;
        } else {
            p.state.velocity = Vec3::ZERO;
        }
        p.alive = true;
        p.health = p.max_health;
        p.armor = 0;
        p.weapons = LOADOUT.iter().map(|w| w.to_string()).collect();
        p.active_weapon = SPAWN_WEAPON.to_string();
        p.inventory.give_loadout();
        p.inventory.select(SPAWN_WEAPON);
        p.ragdoll = None;
        p.death_tick = None;
        p.held = None;
    }

    /// `Player:Kill`: dead where they stood, ragdoll there, weapons gone next tick, respawn in
    /// `RESPAWN_TICKS`.
    /// `SelectWeapon`: a carried weapon becomes active in the same tick, its deploy time arms the
    /// fire timers (measured `weapon_switch`). Dead players and unknown classes are ignored.
    pub fn select_weapon(&mut self, id: u32, class: &str) -> bool {
        let Some(p) = self.players.get_mut(&id) else {
            return false;
        };
        if !p.alive || !p.weapons.iter().any(|w| w == class) || p.active_weapon == class {
            return false;
        }
        p.active_weapon = class.to_string();
        p.inventory.select(class);
        true
    }

    /// A trigger pull: the bullet (all pellets along the aim, no spread yet) against the other
    /// live players' hulls and the world; the hit group from the height on the target
    /// (`weapons::hitgroup_at`), the ammo's multiplayer damage scaled by it, applied as
    /// DMG_BULLET the same tick (recordings `hit_*`).
    fn resolve_shot(&mut self, attacker: u32, eye: Vec3, forward: Vec3, shot: weapons::Shot) {
        let Some(base) = weapons::bullet_damage(shot.ammo) else {
            return;
        };
        const MAX_TRACE: f32 = 56755.84;
        let end = eye + forward * MAX_TRACE;
        let world = self
            .collision
            .trace_ray(eye, end, gm_collision::MASK_PLAYERSOLID);
        let d = end - eye;
        let mut best: Option<(u32, f32)> = None;
        for (id, p) in &self.players {
            if *id == attacker || !p.alive {
                continue;
            }
            let (mins, maxs) = self.mover.hull(p.state.ducked);
            let (lo, hi) = (p.state.origin + mins, p.state.origin + maxs);
            let (mut t0, mut t1) = (0.0f32, 1.0f32);
            let mut inside = true;
            for a in 0..3 {
                if d[a].abs() < 1e-6 {
                    if eye[a] < lo[a] || eye[a] > hi[a] {
                        inside = false;
                        break;
                    }
                    continue;
                }
                let (mut ta, mut tb) = ((lo[a] - eye[a]) / d[a], (hi[a] - eye[a]) / d[a]);
                if ta > tb {
                    std::mem::swap(&mut ta, &mut tb);
                }
                t0 = t0.max(ta);
                t1 = t1.min(tb);
                if t0 > t1 {
                    inside = false;
                    break;
                }
            }
            if inside && best.is_none_or(|b| t0 < b.1) {
                best = Some((*id, t0));
            }
        }
        let Some((target, frac)) = best else {
            return;
        };
        if world.fraction < frac {
            return;
        }
        let pos = eye + d * frac;
        let rel_z = pos.z - self.players[&target].state.origin.z;
        let Some(group) = weapons::hitgroup_at(rel_z) else {
            return;
        };
        let damage = base * weapons::hitgroup_scale(group) * shot.pellets as f32;
        let dmg_type = if shot.pellets > 1 {
            weapons::DMG_BULLET | weapons::DMG_BUCKSHOT
        } else {
            weapons::DMG_BULLET
        };
        self.hits.push(Hit {
            tick: self.tick_count,
            attacker,
            target,
            damage,
            hitgroup: group,
            pos,
            ammo: shot.ammo,
        });
        let push = Vec3::new(forward.x, forward.y, 0.0).normalize_or_zero()
            * (weapons::KNOCKBACK_PER_DAMAGE * damage);
        if let Some(t) = self.players.get_mut(&target) {
            t.state.impulse += push;
        }
        self.damage_player(target, damage, dmg_type);
    }

    pub fn kill_player(&mut self, id: u32) {
        let tick = self.tick_count;
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        if !p.alive {
            return;
        }
        p.alive = false;
        p.health = 0;
        p.deaths += 1;
        p.ragdoll = Some(Ragdoll {
            class: RAGDOLL_CLASS,
            model: p.model.clone(),
            origin: p.state.origin,
            angles: Vec3::ZERO,
            velocity: p.state.velocity,
            tick,
        });
        p.death_tick = Some(tick);
        p.active_weapon.clear();
        p.held = None;
        p.state.velocity = Vec3::ZERO;
    }

    pub fn register_model(&mut self, name: &str, info: ModelInfo) {
        self.hull_planes
            .insert(name.to_string(), info.hull_planes.clone());
        self.models.insert(name.to_string(), info);
    }

    pub fn add_player(&mut self, id: u32, origin: Vec3, view_angles: Vec3) {
        self.players.insert(id, Player::new(origin, view_angles));
    }

    /// Props already on the map (owner 0): registered models at given poses, optionally frozen.
    pub fn add_existing_props(&mut self, props: &[(String, Vec3, Vec3, bool)]) {
        let bodies: Vec<PropBody> = props
            .iter()
            .enumerate()
            .map(|(i, (model, pos, angles, _))| PropBody {
                id: i as u32 + 1,
                pos: *pos,
                angles: *angles,
                hulls: self.hull_planes.get(model).cloned().unwrap_or_default(),
            })
            .collect();
        let models: Vec<String> = props.iter().map(|p| p.0.clone()).collect();
        self.sandbox.place_existing(0, &bodies, &models);
        for (i, (_, _, _, frozen)) in props.iter().enumerate() {
            self.sandbox.set_frozen(i as u32 + 1, *frozen);
        }
        self.sync_physics();
    }

    /// `GetShootPos()` and view angles of a player.
    pub fn eye(&self, player: u32) -> Option<(Vec3, Vec3)> {
        let p = self.players.get(&player)?;
        let eye = p.state.origin + Vec3::new(0.0, 0.0, self.mover.eye_height(&p.state));
        Some((eye, p.view_angles))
    }

    /// Props as the traces see them, at their current physics poses.
    pub fn trace_bodies(&self) -> Vec<PropBody> {
        self.sandbox
            .props()
            .iter()
            .filter_map(|p| {
                let hulls = self.hull_planes.get(&p.model)?.clone();
                Some(PropBody {
                    id: p.id,
                    pos: p.pos,
                    angles: p.angles,
                    hulls,
                })
            })
            .collect()
    }

    /// `gm_spawn <model>` by a player.
    pub fn spawn_prop(&mut self, player: u32, model: &str) -> Option<u32> {
        let (eye, eye_angles) = self.eye(player)?;
        if !self.models.contains_key(model) {
            let loaded = self.model_loader.as_mut().and_then(|f| f(model))?;
            self.register_model(model, loaded);
        }
        let info = self.models.get(model)?.clone();
        let bodies = self.trace_bodies();
        let mut events = Vec::new();
        let id = self.sandbox.spawn_prop(
            self.collision,
            &bodies,
            player,
            &SpawnRequest {
                eye,
                eye_angles,
                model,
                bounds: info.bounds,
            },
            &mut events,
        );
        self.events.extend(events);
        self.sync_physics();
        id
    }

    /// A tool gun click of `player` with the current view.
    pub fn tool_click(&mut self, player: u32, tool: &str, right: bool) -> bool {
        matches!(
            self.tool_click_full(player, tool, right),
            ToolClick::Done(true)
        )
    }

    /// Like `tool_click`, telling the caller when the stool is not a Rust one.
    pub fn tool_click_full(&mut self, player: u32, tool: &str, right: bool) -> ToolClick {
        let Some((eye, eye_angles)) = self.eye(player) else {
            return ToolClick::Miss;
        };
        let bodies = self.trace_bodies();
        let dir = gm_sandbox::forward(eye_angles.x, eye_angles.y);
        let Some(hit): Option<ToolHit> = gm_sandbox::tool_trace(self.collision, &bodies, eye, dir)
        else {
            return ToolClick::Miss;
        };
        if !Sandbox::knows_tool(tool) {
            return ToolClick::Unknown(hit);
        }
        let mut events = Vec::new();
        let ret = if right {
            self.sandbox
                .tool_right_click(player, tool, &hit, &mut events)
        } else {
            self.sandbox
                .tool_left_click(player, tool, &hit, &mut events)
        };
        self.events.extend(events);
        self.sync_physics();
        ToolClick::Done(ret)
    }

    /// After a scripted tool changed the sandbox (constraints, poses): mirror into physics.
    pub fn sync_after_script(&mut self) {
        self.sync_physics();
    }

    pub fn undo(&mut self, player: u32) {
        let mut events = Vec::new();
        self.sandbox.undo(player, &mut events);
        self.events.extend(events);
        self.sync_physics();
    }

    pub fn cleanup(&mut self, player: u32) {
        let mut events = Vec::new();
        self.sandbox.cleanup(player, &mut events);
        self.events.extend(events);
        self.sync_physics();
    }

    /// Mirrors the sandbox's props and constraints into Jolt: new props get bodies at their
    /// placement, props the tool moved are teleported, frozen flags follow, gone entities are
    /// removed (constraints first), new constraints are created from their recorded points.
    fn sync_physics(&mut self) {
        let live: Vec<u32> = self.sandbox.props().iter().map(|p| p.id).collect();
        let live_c: Vec<u32> = self.sandbox.constraints().iter().map(|c| c.id).collect();
        for (id, c) in std::mem::take(&mut self.constraints) {
            if live_c.contains(&id) {
                self.constraints.insert(id, c);
            } else {
                self.physics.remove_constraint(c);
            }
        }
        for (id, pair) in std::mem::take(&mut self.nocollides) {
            if live_c.contains(&id) {
                self.nocollides.insert(id, pair);
            } else {
                self.physics.set_pair_collision(pair.0, pair.1, true);
            }
        }
        for (id, b) in std::mem::take(&mut self.bodies) {
            if live.contains(&id) {
                self.bodies.insert(id, b);
            } else {
                self.physics.remove_body(b);
            }
        }
        let props: Vec<_> = self.sandbox.props().to_vec();
        for p in &props {
            match self.bodies.get(&p.id) {
                None => {
                    let Some(info) = self.models.get(&p.model) else {
                        continue;
                    };
                    let Ok(body) = self.physics.add_prop(&PropDesc {
                        hulls: &info.hulls,
                        mass: info.mass,
                        volume: info.volume,
                        position: p.pos,
                        angles: (p.angles.x, p.angles.y, p.angles.z),
                        velocity: Vec3::ZERO,
                        angular_velocity: Vec3::ZERO,
                        surface: info.surface,
                        linear_damping: info.linear_damping,
                        angular_damping: info.angular_damping,
                        drag_axis_areas: info.drag_axis_areas,
                        inertia: info.inertia,
                    }) else {
                        continue;
                    };
                    if p.frozen {
                        self.physics.freeze(body);
                    }
                    self.bodies.insert(p.id, body);
                }
                Some(&body) => {
                    let st = self.physics.state(body);
                    if (st.position - p.pos).length() > 1e-3 {
                        self.physics
                            .teleport(body, p.pos, gm_sandbox::rotation(p.angles));
                    }
                    if p.frozen != self.physics.is_frozen(body) {
                        if p.frozen {
                            self.physics.freeze(body);
                        } else {
                            self.physics.unfreeze(body);
                        }
                    }
                }
            }
        }
        let constraints: Vec<_> = self.sandbox.constraints().to_vec();
        for c in &constraints {
            if self.constraints.contains_key(&c.id) || self.nocollides.contains_key(&c.id) {
                continue;
            }
            let body = |e: Option<u32>| -> Option<BodyHandle> {
                match e {
                    Some(id) => self.bodies.get(&id).copied(),
                    None => Some(self.world_body),
                }
            };
            let (Some(a), Some(b)) = (body(c.ent1), body(c.ent2)) else {
                continue;
            };
            let pose = |e: Option<u32>| {
                props
                    .iter()
                    .find(|p| Some(p.id) == e)
                    .map(|p| (p.pos, gm_sandbox::rotation(p.angles)))
            };
            let world_of = |e: Option<u32>, local: Vec3| match pose(e) {
                Some((pos, rot)) => pos + rot * local,
                None => local,
            };
            let handle = match &c.kind {
                ConstraintKind::Weld { .. } => self.physics.weld(a, b),
                ConstraintKind::Rope {
                    lpos1,
                    lpos2,
                    length,
                    addlength,
                    rigid,
                    ..
                } => self.physics.rope(
                    a,
                    b,
                    world_of(c.ent1, *lpos1),
                    world_of(c.ent2, *lpos2),
                    length + addlength,
                    *rigid,
                ),
                ConstraintKind::Axis {
                    lpos1, local_axis, ..
                } => {
                    let point = world_of(c.ent1, *lpos1);
                    let axis_end = world_of(c.ent1, *local_axis);
                    let axis = (axis_end - point).normalize_or_zero();
                    let axis = if axis == Vec3::ZERO { Vec3::Z } else { axis };
                    self.physics.hinge(a, b, point, axis)
                }
                ConstraintKind::NoCollide => {
                    self.physics.set_pair_collision(a, b, false);
                    self.nocollides.insert(c.id, (a, b));
                    None
                }
            };
            if let Some(h) = handle {
                self.constraints.insert(c.id, h);
            }
        }
    }

    /// The physgun of one player for this tick, with the eye before the player moves (the weapon
    /// fires in the player's think before movement and physics).
    fn physgun(&mut self, player: u32, pc: &PlayerCmd) {
        let Some((eye, _)) = self.eye(player) else {
            return;
        };
        let view = pc.cmd.view_angles;
        let forward = gm_sandbox::forward(view.x, view.y);
        let (sy, cy) = (view.y.to_radians().sin(), view.y.to_radians().cos());
        let right = Vec3::new(sy, -cy, 0.0);
        let attack = pc.cmd.buttons & IN_ATTACK != 0;
        let use_key = pc.cmd.buttons & IN_USE != 0;
        let attack2 = pc.cmd.buttons & IN_ATTACK2 != 0;
        let held = self.players.get(&player).and_then(|p| p.held);
        let held = match held {
            None if attack && pc.physgun => {
                let bodies = self.trace_bodies();
                let hit = gm_sandbox::tool_trace(self.collision, &bodies, eye, forward);
                match hit.and_then(|h| h.ent.map(|e| (e, h))) {
                    Some((ent, h))
                        if !self.sandbox.props().iter().any(|p| p.id == ent && p.frozen) =>
                    {
                        let body = self.bodies.get(&ent).copied();
                        body.map(|b| {
                            let st = self.physics.state(b);
                            (
                                ent,
                                Grab::new(eye, forward, h.pos, st.position, st.rotation, view.y),
                            )
                        })
                    }
                    _ => None,
                }
            }
            Some((ent, _)) if !attack => {
                if let Some(b) = self.bodies.get(&ent).copied() {
                    self.physics.release(b);
                }
                None
            }
            Some((ent, _)) if attack2 => {
                if let Some(b) = self.bodies.get(&ent).copied() {
                    self.physics.freeze(b);
                }
                self.sandbox.set_frozen(ent, true);
                None
            }
            other => other,
        };
        let held = match held {
            Some((ent, mut g)) => {
                if let Some(b) = self.bodies.get(&ent).copied() {
                    if use_key {
                        g.rotate(pc.mouse_x, pc.mouse_y, right);
                    }
                    let st = self.physics.state(b);
                    let (tp, tr) = g.target(eye, forward, view.y, st.rotation);
                    self.physics.hold(b, tp, tr, &self.physgun_params);
                    Some((ent, g))
                } else {
                    None
                }
            }
            None => None,
        };
        if let Some(p) = self.players.get_mut(&player) {
            p.held = held;
        }
    }

    /// Moves one player by one usercmd outside the tick (catching up a usercmd backlog).
    pub fn move_player(&mut self, player: u32, pc: &PlayerCmd) {
        if let Some(p) = self.players.get_mut(&player) {
            p.view_angles = pc.cmd.view_angles;
            let cmd = gm_move::UserCmd {
                buttons: pc.cmd.buttons & !(IN_ATTACK | IN_USE | IN_ATTACK2),
                ..pc.cmd
            };
            self.mover.tick(&mut p.state, &cmd);
        }
    }

    /// One fixed tick: weapons fire with the pre-move eye, physics steps, players move, prop
    /// poses flow back into the sandbox.
    pub fn tick(&mut self, cmds: &BTreeMap<u32, PlayerCmd>) {
        self.tick_count += 1;
        let tick = self.tick_count;
        let mut respawn = Vec::new();
        for (id, p) in self.players.iter_mut() {
            if p.alive {
                continue;
            }
            if let Some(d) = p.death_tick {
                if tick == d + 1 {
                    p.weapons.clear();
                    p.inventory.clear();
                }
                if tick >= d + RESPAWN_TICKS {
                    respawn.push(*id);
                }
            }
        }
        for id in respawn {
            self.spawn_player(id);
        }
        for (id, pc) in cmds {
            if self.players.get(id).is_some_and(|p| p.alive) {
                self.physgun(*id, pc);
            }
        }
        self.physics.step();
        // weapons first: a hit pushes and hurts the target in the tick of the shot, before the
        // target's own move (recordings `hit_*`)
        let mut shots = Vec::new();
        for (id, p) in self.players.iter_mut() {
            if !p.alive {
                continue;
            }
            if let Some(pc) = cmds.get(id) {
                if let Some(shot) = p.inventory.command(&p.active_weapon, pc.cmd.buttons) {
                    let eye = p.state.origin + Vec3::new(0.0, 0.0, self.mover.eye_height(&p.state));
                    let forward = gm_sandbox::forward(pc.cmd.view_angles.x, pc.cmd.view_angles.y);
                    shots.push((*id, eye, forward, shot));
                }
                p.inventory.tick(&p.active_weapon);
            }
        }
        for (attacker, eye, forward, shot) in shots {
            self.resolve_shot(attacker, eye, forward, shot);
        }
        for (id, p) in self.players.iter_mut() {
            if !p.alive {
                continue;
            }
            if let Some(pc) = cmds.get(id) {
                p.view_angles = pc.cmd.view_angles;
                let cmd = gm_move::UserCmd {
                    buttons: pc.cmd.buttons & !(IN_ATTACK | IN_USE | IN_ATTACK2),
                    ..pc.cmd
                };
                let (prev_vz, was_airborne) = (p.state.velocity.z, !p.state.on_ground);
                self.mover.tick(&mut p.state, &cmd);
                p.prev_vz = prev_vz;
                p.prev_on_ground = !was_airborne;
            }
        }
        let landed: Vec<(u32, f32)> = self
            .players
            .iter()
            .filter(|(_, p)| {
                p.alive && p.state.on_ground && !p.prev_on_ground && -p.prev_vz > FALL_SAFE_SPEED
            })
            .map(|(id, p)| (*id, -p.prev_vz))
            .collect();
        for (id, speed) in landed {
            let dmg = if self.fall_damage_progressive {
                (speed - FALL_SAFE_SPEED) * FALL_DAMAGE_PER_SPEED
            } else {
                FALL_DAMAGE_FLAT
            };
            self.damage_player(id, dmg, DMG_FALL);
        }
        for (id, body) in &self.bodies {
            let st = self.physics.state(*body);
            self.sandbox
                .set_prop_pose(*id, st.position, gm_sandbox::angles_of(st.rotation));
        }
    }

    /// `Entity:SetPos/SetAngles` from a script: the sandbox pose and the body move together.
    pub fn teleport_prop(&mut self, id: u32, pos: Vec3, angles: Vec3) -> bool {
        if !self.sandbox.props().iter().any(|p| p.id == id) {
            return false;
        }
        self.sandbox.set_prop_pose(id, pos, angles);
        if let Some(&b) = self.bodies.get(&id) {
            self.physics.teleport(b, pos, gm_sandbox::rotation(angles));
        }
        true
    }

    /// `PhysObj:EnableMotion` from a script.
    pub fn set_prop_frozen(&mut self, id: u32, frozen: bool) -> bool {
        if !self.sandbox.props().iter().any(|p| p.id == id) {
            return false;
        }
        self.sandbox.set_frozen(id, frozen);
        self.sync_physics();
        true
    }

    /// `Entity:Remove` from a script: constraints on it go too, like the sandbox's own removal.
    pub fn remove_prop(&mut self, id: u32) -> bool {
        if !self.sandbox.props().iter().any(|p| p.id == id) {
            return false;
        }
        self.sandbox.remove_entity(id);
        self.sync_physics();
        true
    }

    /// Physics state of a sandbox prop (velocity in Source units, angular body-local deg/s).
    pub fn prop_state(&self, prop: u32) -> Option<gm_physics::BodyState> {
        self.bodies.get(&prop).map(|b| self.physics.state(*b))
    }

    pub fn body_of(&self, prop: u32) -> Option<BodyHandle> {
        self.bodies.get(&prop).copied()
    }
}
