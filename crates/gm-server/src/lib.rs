//! One server tick: usercmds through `gm-move`, sandbox commands through `gm-sandbox`, and the
//! props and constraints the sandbox owns mirrored into the Jolt world of `gm-physics`
//! (weld -> fixed, rope -> distance, axis -> hinge). Nothing here is networked yet.

pub mod projectile;
pub mod save;
pub mod weapons;

use glam::Vec3;
use gm_physics::physgun::{Grab, PhysgunParams};
use gm_physics::{BodyHandle, ConstraintHandle, PropDesc, Surface, World};
use gm_sandbox::{ConstraintKind, Event, ModelBounds, PropBody, Sandbox, SpawnRequest, ToolHit};
use std::collections::BTreeMap;

/// Everything the server needs to know about a model to spawn and trace it.
pub mod player_models;

#[derive(Clone, Debug)]
pub struct ModelInfo {
    pub bounds: ModelBounds,
    /// PHY convexes in the entity frame (Source units).
    pub hulls: Vec<Vec<Vec3>>,
    /// The same convexes as outward planes (normal, distance) for the sandbox traces.
    pub hull_planes: Vec<Vec<(Vec3, f32)>>,
    /// PHY mass centre in the entity frame.
    pub mass_center: Vec3,
    pub mass: f32,
    pub volume: f32,
    pub surface: Surface,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub drag_axis_areas: Vec3,
    pub inertia: Option<Vec3>,
    /// The `impacthard` sound script of the model's surface property (`Wood_Crate.ImpactHard`).
    pub impact_hard: Option<String>,
    /// The model's `prop_data` through `scripts/propdata.txt`.
    pub prop_data: PropData,
    /// The `break` sound script of the surface property (`Wood_Crate.Break`).
    pub break_sound: Option<String>,
    /// `Entity:SkinCount`: skin families in the MDL (1 when unknown).
    pub skins: u8,
    /// `Entity:GetBodygroupCount` per body part (empty when unknown).
    pub body_groups: Vec<u8>,
}

/// A model's `prop_data` (`$keyvalues`, through the `base` chain of `scripts/propdata.txt`):
/// the health (0: unbreakable; the crate is `Wooden.Medium`, 30) and the damage modifiers by
/// type (`Wooden.Base`: bullets 0.75, club 2.0, explosive 1.5), applied before the whole-number
/// truncation (`shoot_prop_*`: a pistol's 5 takes 3 from the crate, the crowbar's 10 takes 20,
/// the .357's 40 breaks it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropData {
    pub health: i32,
    pub dmg_bullets: f32,
    pub dmg_club: f32,
    pub dmg_explosive: f32,
}

impl Default for PropData {
    fn default() -> Self {
        PropData {
            health: 0,
            dmg_bullets: 1.0,
            dmg_club: 1.0,
            dmg_explosive: 1.0,
        }
    }
}

impl PropData {
    /// The modifier a damage type gets (`CBreakableProp::OnTakeDamage`).
    pub fn modifier(&self, dmg_type: u32) -> f32 {
        if dmg_type & weapons::DMG_CLUB != 0 {
            self.dmg_club
        } else if dmg_type & weapons::DMG_BLAST != 0 {
            self.dmg_explosive
        } else if dmg_type & weapons::DMG_BULLET != 0 {
            self.dmg_bullets
        } else {
            1.0
        }
    }
}

/// A prop impact is heard from this approach speed (units/s) up: the crate's drops at 62.9
/// and 69.1 u/s are silent on the game server, 71.8 u/s plays (`phys_sound_drop_crate_*`).
pub const IMPACT_SOUND_MIN_SPEED: f32 = 70.0;
/// The impact volume is `(speed / 320)^2` capped at 1, times the script's own volume (the
/// crate: 80.7 u/s 0.0636, 177 u/s 0.306, 292 u/s 0.834, 349 u/s and up 1.0, all exact).
pub const IMPACT_SOUND_FULL_SPEED: f32 = 320.0;

/// `entityflame` on a prop (`Ignite`, measured `lua_fire`): the first point of damage lands
/// 7 ticks after the ignition, then one every 13 ticks (0.2 s thinks), `DMG_BURN | DMG_DIRECT`,
/// while the lifetime lasts; the think that finds the lifetime over stops the fire sound and
/// the flame stays (the prop still "on fire") for 0.5 s before it is gone.
pub const FIRE_FIRST_DAMAGE_TICKS: u64 = 7;
pub const FIRE_DAMAGE_INTERVAL_TICKS: u64 = 13;
pub const FIRE_LINGER_TICKS: u64 = 33;
pub const FIRE_DAMAGE: i32 = 1;
pub const DMG_BURN: u32 = 8;
pub const DMG_DIRECT: u32 = 0x1000_0000;
/// The `ignite` property is `Ignite(360)`.
pub const PROPERTY_IGNITE_SECONDS: f32 = 360.0;

/// A sound the server tells every client to play in the world this tick.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldSound {
    /// Sound script name.
    pub name: String,
    pub pos: Vec3,
    /// Multiplies the script's volume.
    pub volume: f32,
}

/// Game events of this tick for the addon runtime (`ServerHooks::game_events`), drained by the
/// network layer after the tick.
#[derive(Clone, Debug, PartialEq)]
pub enum GameEvent {
    /// `spawn_player`: the first spawn and every respawn.
    PlayerSpawned {
        player: u32,
    },
    /// A prop's health reached zero (fire, so far): it is gone after this.
    PropBreak {
        ent: u32,
    },
    /// Health damage applied (after the armour share, fall damage included), damage type bits.
    PlayerDamaged {
        player: u32,
        amount: f32,
        dmg_type: u32,
    },
    /// `dmg_type` 0 for `kill_player`.
    PlayerDied {
        player: u32,
        dmg_type: u32,
    },
    PhysgunPickup {
        player: u32,
        ent: u32,
    },
    PhysgunDrop {
        player: u32,
        ent: u32,
    },
    PhysgunFreeze {
        player: u32,
        ent: u32,
    },
    /// `set_noclip` toggled the player's move type.
    Noclip {
        player: u32,
        on: bool,
    },
    /// A +use press whose use trace found a prop and `Request::PlayerUse` allowed.
    Use {
        player: u32,
        ent: u32,
    },
    /// The player picked a prop up with +use and carries it.
    Pickup {
        player: u32,
        ent: u32,
    },
    /// The carried prop was let go (second +use, death, the prop removed or physgunned).
    Drop {
        player: u32,
        ent: u32,
    },
    /// A context-menu property was applied.
    Property {
        player: u32,
        ent: u32,
        name: String,
    },
    /// The physgun's reload unfroze this many of the player's props (`PhysgunUnfreeze`).
    Unfroze {
        player: u32,
        count: u32,
    },
    /// A recorded prop impact (the same capture that makes impact sounds, no speed cut): one
    /// per prop involved, `other` 0 for the world.
    Collide {
        ent: u32,
        other: u32,
        point: Vec3,
        speed: f32,
    },
}

/// Something about to happen that the addon runtime may veto or change (`request_hook`), asked
/// synchronously inside the tick.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// `hurt` before the armour share: the amount as dealt.
    PlayerDamage {
        player: u32,
        amount: f32,
        dmg_type: u32,
    },
    /// The physgun's trace found an unfrozen prop while attack is pressed.
    PhysgunPickup { player: u32, ent: u32 },
    /// A +use press (the edge, not the hold) whose trace from the eyes found a prop within
    /// `PLAYER_USE_RADIUS`.
    PlayerUse { player: u32, ent: u32 },
    /// `AllowPlayerPickup`: the used prop is light enough (`gm_physics::pickup::MAX_MASS`,
    /// `MAX_SIZE`) and would be carried.
    PlayerPickup { player: u32, ent: u32 },
    /// `CanProperty`: the player asked for a context-menu property on a prop.
    Property { player: u32, ent: u32, name: String },
    /// `PlayerSetModel` on (re)spawn with the model the player's choice translates to;
    /// `Text` replaces it.
    SetModel { player: u32, model: String },
    /// `PlayerLoadout`: the player (re)spawned with an empty inventory; `Cancel` skips the
    /// default loadout (the hook gives its own with `player.give_weapon`).
    Loadout { player: u32 },
    /// `CanUndo`: the player asked to undo this entry.
    Undo { player: u32, name: String },
    /// `CanPlayerUnfreeze`: the physgun's reload is about to unfreeze this prop of the player.
    Unfreeze { player: u32, ent: u32 },
    /// `GetFallDamage`: landing at `speed` is about to deal `amount`; `Amount` replaces it.
    FallDamage {
        player: u32,
        speed: f32,
        amount: f32,
    },
}

/// How far from the eyes +use reaches (the public SDK's `PLAYER_USE_RADIUS`).
pub const PLAYER_USE_RADIUS: f32 = 80.0;

/// One setting of a scripted stool as its `tool.register` declared it: the client shows it,
/// fills `default` when unset and clamps numbers to `min..=max` before the click.
#[derive(Clone, Debug, PartialEq)]
pub struct ToolSetting {
    pub name: String,
    pub kind: ToolSettingKind,
    pub min: f32,
    pub max: f32,
    pub default: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolSettingKind {
    Number,
    Bool,
    Text,
}

impl ToolSettingKind {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Bool,
            2 => Self::Text,
            _ => Self::Number,
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            Self::Number => 0,
            Self::Bool => 1,
            Self::Text => 2,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Verdict {
    Allow,
    Cancel,
    /// `PlayerDamage` only: deal this amount instead.
    Amount(f32),
    /// `SetModel` only: use this model path instead.
    Text(String),
}

/// Answers `Request`s; the server lets go of the hook while it runs, so the hook may use the
/// server (its own writes are not asked again).
pub type RequestHook = Box<dyn FnMut(&mut Server<'_>, &Request) -> Verdict>;

/// `(speed / IMPACT_SOUND_FULL_SPEED)^2` capped at 1; `None` under the audible threshold.
pub fn impact_volume(speed: f32) -> Option<f32> {
    (speed >= IMPACT_SOUND_MIN_SPEED).then(|| (speed / IMPACT_SOUND_FULL_SPEED).powi(2).min(1.0))
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

/// A `.gmr` package with client-realm actors, ready to be handed to clients.
pub struct PublishedPackage {
    pub id: String,
    pub version: String,
    pub sha: [u8; 32],
    pub bytes: Vec<u8>,
}

/// Game-logic hooks around server commands (the addon runtime implements them).
pub trait ServerHooks {
    /// `prop.spawn_request`: false refuses the spawn.
    fn prop_spawn_request(&mut self, server: &mut Server<'_>, player: u32, model: &str) -> bool;
    /// `entity.spawned` after a prop was created.
    fn entity_spawned(&mut self, server: &mut Server<'_>, player: u32, ent: u32);
    /// `entity.removed` after an entity left the table through a player's command.
    fn entity_removed(&mut self, _server: &mut Server<'_>, _player: u32, _ent: u32) {}
    /// A click of a stool the sandbox does not know: a scripted tool may handle it.
    fn tool_click(
        &mut self,
        _server: &mut Server<'_>,
        _player: u32,
        _tool: &str,
        _hit: &ToolHit,
        _right: bool,
        _settings: &[(String, String)],
    ) -> bool {
        false
    }
    /// Admin command: hot reload of a loaded addon by id.
    fn reload_addon(&mut self, _addon: &str) -> Result<String, String> {
        Err("no addon runtime".into())
    }
    /// `player.disconnected` after the player left the table (its props stay).
    fn player_disconnected(&mut self, _server: &mut Server<'_>, _player: u32) {}
    /// This tick's `GameEvent`s, after the tick and before `post_tick`.
    fn game_events(&mut self, _server: &mut Server<'_>, _events: Vec<GameEvent>) {}
    /// A tool gun click that hit something, before the stool runs: `false` cancels it.
    fn tool_before(
        &mut self,
        _server: &mut Server<'_>,
        _player: u32,
        _tool: &str,
        _right: bool,
        _hit: &ToolHit,
    ) -> bool {
        true
    }
    /// After a tool gun click that hit something: `done` is the stool's return value.
    fn tool_after(
        &mut self,
        _server: &mut Server<'_>,
        _player: u32,
        _tool: &str,
        _right: bool,
        _done: bool,
    ) {
    }
    /// A player's chat line before it is sent: `None` drops it, `Some` is what every client
    /// gets (possibly rewritten).
    fn player_chat(
        &mut self,
        _server: &mut Server<'_>,
        _player: u32,
        text: &str,
    ) -> Option<String> {
        Some(text.to_owned())
    }
    /// An addon message from a client's actor (`net.send` on the client).
    fn net_message(&mut self, _server: &mut Server<'_>, _player: u32, _name: &str, _data: &[u8]) {}
    /// Addon messages queued by server actors: (player or 0 for everyone, name, payload).
    fn drain_net(&mut self) -> Vec<(u32, String, Vec<u8>)> {
        Vec::new()
    }
    /// Around every tick.
    fn pre_tick(&mut self, server: &mut Server<'_>);
    fn post_tick(&mut self, server: &mut Server<'_>);
    /// After the tick's player commands were applied (the game runs console commands at the end
    /// of the frame, after `Tick`): `cvar.changed` for what they changed.
    fn after_commands(&mut self, _server: &mut Server<'_>) {}
    /// Packages (re)built since the last call: addons with client-realm actors as they were
    /// loaded or hot-reloaded, for the network to publish to clients.
    fn drain_published(&mut self) -> Vec<PublishedPackage> {
        Vec::new()
    }
}

/// A prop convex in the world for the use sweeps: outward planes and the AABB.
type WorldConvex = (Vec<(Vec3, f32)>, (Vec3, Vec3));

/// Loads a model by name on first use.
pub type ModelLoader = Box<dyn FnMut(&str) -> Option<ModelInfo>>;

pub struct Player {
    pub state: gm_move::PlayerState,
    /// Authenticated identity (ed25519 public key) once the connection proved it.
    pub identity: Option<[u8; 32]>,
    /// (tick, origin, ducked) after each of the last 64 ticks, for lag compensation.
    pub history: std::collections::VecDeque<(u32, Vec3, bool)>,
    pub view_angles: Vec3,
    /// The prop held by the physgun and how it was grabbed.
    pub held: Option<(u32, Grab)>,
    /// The prop carried with +use (`gm_physics::pickup`).
    pub carry: Option<(u32, gm_physics::pickup::Carry)>,
    /// `IN_USE` was down in the last command (a use fires on the press, not the hold).
    pub use_down: bool,
    /// `IN_RELOAD` was down in the last command (the physgun unfreezes on the press).
    pub reload_down: bool,
    pub alive: bool,
    pub health: i32,
    pub armor: i32,
    pub max_health: i32,
    pub model: String,
    /// The player's model choice (`cl_playermodel`), applied on the next spawn.
    pub wanted_model: String,
    /// `Entity:GetSkin` / `GetBodyGroups` of the worn model: set on the spawn from the wanted
    /// values, stored as given (measured: the game keeps `SetSkin(7)` on a one-skin model).
    pub skin: u8,
    pub body: String,
    /// `cl_playerskin` / `cl_playerbodygroups`, applied on the next spawn.
    pub wanted_skin: u8,
    pub wanted_body: String,
    /// Weapon classes carried, sorted (`Player:GetWeapons`).
    pub weapons: Vec<String>,
    pub active_weapon: String,
    pub inventory: weapons::Inventory,
    pub deaths: u32,
    pub frags: u32,
    /// The ragdoll of the last death, while the player is dead.
    pub ragdoll: Option<Ragdoll>,
    /// Tick of the last death: the player respawns `respawn_ticks` later.
    death_tick: Option<u64>,
    /// `RESPAWN_TICKS` for a death in the command phase (bullets, falls), one less for a death
    /// in the entity phase (projectiles, explosions; measured `hit_ball`: 134).
    respawn_ticks: u64,
    /// Fractional damage carried to the next hit (health is whole); measured to survive
    /// `SetHealth` and respawns, so it is only ever reset with the player.
    damage_acc: f32,
    /// Vertical velocity before the last movement tick, for fall damage on landing.
    prev_vz: f32,
    prev_on_ground: bool,
    /// Footstep timer and foot alternation (measured rule in `gm_move::Footsteps`).
    steps: gm_move::Footsteps,
}

/// Step soundscripts of a surface (`stepleft`/`stepright` of its surfaceprop).
#[derive(Clone, Debug, PartialEq)]
pub struct StepSounds {
    pub left: String,
    pub right: String,
}

/// Host hook: step sounds of a brush material (`concrete/concretefloor001a`), `None` material
/// for ground with no brush side under it (displacements, props); the result is cached.
pub type StepLoader = Box<dyn FnMut(Option<&str>) -> Option<StepSounds>>;

/// Damage types (`DMG_*` bits) that matter to the rules here.
pub const DMG_FALL: u32 = 32;
pub const DMG_DROWN: u32 = 16384;
/// Measured (`damage_hits`): armour takes 80 % of a hit up to what it has, health the rest;
/// fall and drowning damage ignore armour.
pub const ARMOR_SHARE: f32 = 0.8;
/// Measured (`damage_fall_*`): landing with the previous tick's downward speed above this
/// hurts, 10 flat with `mp_falldamage 0`, `(v - threshold) * 25/99` with `mp_falldamage 1`
/// (fits 540 to 1197 u/s to 0.01; 522 u/s is safe, 540 is not).
pub const FALL_SAFE_SPEED: f32 = gm_move::FALL_SAFE_SPEED;
pub const FALL_DAMAGE_PER_SPEED: f32 = 25.0 / 99.0;
pub const FALL_DAMAGE_FLAT: f32 = 10.0;

impl Player {
    pub fn new(origin: Vec3, view_angles: Vec3) -> Self {
        Player {
            history: Default::default(),
            identity: None,
            state: gm_move::PlayerState {
                origin,
                ..Default::default()
            },
            view_angles,
            held: None,
            carry: None,
            use_down: false,
            reload_down: false,
            alive: true,
            health: PLAYER_HEALTH,
            armor: 0,
            max_health: PLAYER_HEALTH,
            model: PLAYER_MODEL.to_string(),
            wanted_model: String::new(),
            skin: 0,
            body: String::new(),
            wanted_skin: 0,
            wanted_body: String::new(),
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
            respawn_ticks: RESPAWN_TICKS,
            damage_acc: 0.0,
            prev_vz: 0.0,
            prev_on_ground: true,
            steps: gm_move::Footsteps::default(),
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
    /// Server tick the client rendered when it made the command (0 = unknown): shots of this
    /// command are resolved against the other players as they stood then.
    pub seen_tick: u32,
    /// The player holds `weapon_physgun`: attack grabs, attack2 freezes, use + mouse rotates.
    pub physgun: bool,
    pub mouse_x: f32,
    pub mouse_y: f32,
}

/// One bullet (or pellet group) that hit a player this tick.
/// `EntityTakeDamage` on a prop (`shoot_prop_*`).
#[derive(Clone, Debug, PartialEq)]
pub struct PropHit {
    pub tick: u64,
    pub attacker: u32,
    pub prop: u32,
    pub damage: f32,
    pub dmg_type: u32,
    pub pos: Vec3,
    pub force: Vec3,
    pub inflictor: &'static str,
    /// The prop broke (removed this tick).
    pub broke: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub tick: u64,
    pub attacker: u32,
    pub target: u32,
    pub damage: f32,
    pub dmg_type: u32,
    /// Hit group for bullets; explosions and projectiles have none.
    pub hitgroup: Option<u8>,
    pub pos: Vec3,
    pub ammo: &'static str,
    /// Entity class the damage came from (`rpg_missile`, `crossbow_bolt`, ...).
    pub inflictor: &'static str,
}

/// Distance along `dir` (unit) from `start` to the box `lo..hi`, 0 when `start` is inside,
/// None when the ray misses or the box is behind `start`.
fn ray_box_entry(start: Vec3, dir: Vec3, lo: Vec3, hi: Vec3) -> Option<f32> {
    let (mut t0, mut t1) = (0.0f32, f32::MAX);
    for a in 0..3 {
        if dir[a].abs() < 1e-9 {
            if start[a] < lo[a] || start[a] > hi[a] {
                return None;
            }
            continue;
        }
        let (mut ta, mut tb) = ((lo[a] - start[a]) / dir[a], (hi[a] - start[a]) / dir[a]);
        if ta > tb {
            std::mem::swap(&mut ta, &mut tb);
        }
        t0 = t0.max(ta);
        t1 = t1.min(tb);
        if t0 > t1 {
            return None;
        }
    }
    Some(t0)
}

pub struct Server<'w> {
    /// Duplicator clipboards: the copied group with poses relative to the copied prop.
    pub clipboards: BTreeMap<u32, save::Save>,
    pub hits: Vec<Hit>,
    /// Hit boxes of the player model; without one, bullets use the hull and height bands.
    pub hit_model: Option<weapons::HitModel>,
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
    /// World sounds of this tick (prop impacts), drained by the network layer.
    sounds: Vec<WorldSound>,
    hull_planes: BTreeMap<String, Vec<Vec<(Vec3, f32)>>>,
    pub events: Vec<Event>,
    pub game_events: Vec<GameEvent>,
    pub request_hook: Option<RequestHook>,
    pub physgun_params: PhysgunParams,
    pub projectiles: Vec<projectile::Projectile>,
    /// Loads a model the first time something spawns it (the dedicated server reads the MDL/PHY
    /// from the mounted content on demand).
    pub model_loader: Option<ModelLoader>,
    pub step_loader: Option<StepLoader>,
    /// Step sounds by material, `""` for no material; filled through `step_loader`.
    step_sounds: BTreeMap<String, Option<StepSounds>>,
    /// `sbox_playershurtplayers`: with it off hits are still resolved and recorded but neither
    /// hurt nor push (`hit_pistol_nohurt`, `hit_rpg_nohurt`).
    pub players_hurt_players: bool,
    pub prop_hits: Vec<PropHit>,

    pub spawn_points: Vec<SpawnPoint>,
    spawn_rng: u64,
    tick_count: u64,
    /// `mp_falldamage`: progressive fall damage instead of the flat 10.
    pub fall_damage_progressive: bool,
    /// Console variables: the sandbox `sbox_*` set plus what addons create.
    pub cvars: BTreeMap<String, Cvar>,
    /// (name, old, new) since the last `take_cvar_changes` (the net layer takes them after the
    /// script hooks read them).
    pub cvar_changes: Vec<(String, String, String)>,
    /// Spawn menu categories registered by addons (`spawnmenu.add`), in registration order;
    /// `spawn_lists_version` grows with every change so the net layer resends them.
    pub spawn_lists: Vec<(String, Vec<String>)>,
    pub spawn_lists_version: u32,
    /// Setting schemas of scripted stools (`tool.register` with `settings`), by tool name;
    /// `tool_schemas_version` grows with every change so the net layer resends them.
    pub tool_schemas: Vec<(String, Vec<ToolSetting>)>,
    pub tool_schemas_version: u32,
    /// Names created since the last `take_cvar_new` (replicated to connected clients).
    pub cvar_new: Vec<String>,
}

/// A console variable: a string value with the sandbox's flags that matter here.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cvar {
    pub value: String,
    pub default: String,
    /// Sent to every client and kept in step (`FCVAR_REPLICATED`).
    pub replicated: bool,
    /// Saved with the server config (`FCVAR_ARCHIVE`).
    pub archive: bool,
    pub min: Option<f32>,
    pub max: Option<f32>,
    pub help: String,
}

impl Cvar {
    pub fn replicated(default: &str) -> Self {
        Cvar {
            value: default.to_string(),
            default: default.to_string(),
            replicated: true,
            ..Default::default()
        }
    }
}

/// The sandbox `sbox_*` variables with their measured defaults (convars-defaults.md), all
/// replicated like the game's.
pub const SBOX_CVARS: [(&str, &str); 24] = [
    ("sbox_maxprops", "200"),
    ("sbox_maxragdolls", "10"),
    ("sbox_maxnpcs", "10"),
    ("sbox_maxvehicles", "4"),
    ("sbox_maxballoons", "100"),
    ("sbox_maxeffects", "200"),
    ("sbox_maxdynamite", "10"),
    ("sbox_maxlamps", "3"),
    ("sbox_maxthrusters", "50"),
    ("sbox_maxwheels", "50"),
    ("sbox_maxhoverballs", "50"),
    ("sbox_maxbuttons", "50"),
    ("sbox_maxemitters", "20"),
    ("sbox_maxlights", "5"),
    ("sbox_maxcameras", "10"),
    ("sbox_maxsents", "100"),
    ("sbox_maxconstraints", "2000"),
    ("sbox_maxropeconstraints", "1000"),
    ("sbox_godmode", "0"),
    ("sbox_noclip", "1"),
    ("sbox_playershurtplayers", "1"),
    ("sbox_weapons", "1"),
    ("sbox_bonemanip_npc", "1"),
    ("sbox_bonemanip_player", "0"),
];

impl<'w> Server<'w> {
    pub fn new(collision: &'w gm_collision::World, physics: World, world_body: BodyHandle) -> Self {
        Server {
            hits: Vec::new(),
            hit_model: None,
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
            sounds: Vec::new(),
            clipboards: BTreeMap::new(),
            hull_planes: BTreeMap::new(),
            events: Vec::new(),
            game_events: Vec::new(),
            request_hook: None,
            physgun_params: PhysgunParams::default(),
            projectiles: Vec::new(),
            model_loader: None,
            step_loader: None,
            step_sounds: BTreeMap::new(),
            players_hurt_players: true,
            prop_hits: Vec::new(),

            spawn_points: Vec::new(),
            spawn_rng: 0x9E37_79B9_7F4A_7C15,
            tick_count: 0,
            fall_damage_progressive: false,
            cvars: SBOX_CVARS
                .iter()
                .map(|(n, d)| (n.to_string(), Cvar::replicated(d)))
                .collect(),
            cvar_changes: Vec::new(),
            spawn_lists: Vec::new(),
            spawn_lists_version: 0,
            tool_schemas: Vec::new(),
            tool_schemas_version: 0,
            cvar_new: Vec::new(),
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
        self.hurt(id, amount, dmg_type, RESPAWN_TICKS)
    }

    /// `damage_player` with the respawn delay of the phase the death happens in.
    fn hurt(&mut self, id: u32, amount: f32, dmg_type: u32, respawn_ticks: u64) -> f32 {
        let tick = self.tick_count;
        if !self.players.get(&id).is_some_and(|p| p.alive) || amount <= 0.0 {
            return 0.0;
        }
        let amount = match self.ask(Request::PlayerDamage {
            player: id,
            amount,
            dmg_type,
        }) {
            Verdict::Cancel => return 0.0,
            Verdict::Amount(a) => a,
            Verdict::Allow => amount,
            Verdict::Text(_) => amount,
        };
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
                p.respawn_ticks = respawn_ticks;
            }
        }
        let died = !p.alive;
        if to_health > 0.0 {
            self.game_events.push(GameEvent::PlayerDamaged {
                player: id,
                amount: to_health,
                dmg_type,
            });
        }
        if died {
            self.game_events.push(GameEvent::PlayerDied {
                player: id,
                dmg_type,
            });
        }
        to_health
    }

    /// `CreateConVar`: a new variable gets its default; an existing one keeps its value and
    /// flags (false).
    pub fn create_cvar(&mut self, name: &str, cvar: Cvar) -> bool {
        if name.is_empty() || self.cvars.contains_key(name) {
            return false;
        }
        let mut c = cvar;
        if c.value.is_empty() {
            c.value = c.default.clone();
        }
        self.cvars.insert(name.to_string(), c);
        self.cvar_new.push(name.to_string());
        true
    }

    /// Sets a known variable (numbers clamped to min/max), applies what the server reads from it
    /// and logs the change; false for an unknown name or an unchanged value.
    pub fn set_cvar(&mut self, name: &str, value: &str) -> bool {
        let Some(c) = self.cvars.get_mut(name) else {
            return false;
        };
        let mut v = value.to_string();
        if let Ok(n) = value.trim().parse::<f32>() {
            let mut n2 = n;
            if let Some(lo) = c.min {
                n2 = n2.max(lo);
            }
            if let Some(hi) = c.max {
                n2 = n2.min(hi);
            }
            if n2 != n {
                v = if n2.fract() == 0.0 {
                    format!("{}", n2 as i64)
                } else {
                    format!("{n2}")
                };
            }
        }
        if c.value == v {
            return false;
        }
        let old = std::mem::replace(&mut c.value, v.clone());
        let num = v.trim().parse::<f32>().unwrap_or(0.0);
        match name {
            "sbox_maxprops" => self.sandbox.limits.max_props = num as i32,
            "sbox_maxconstraints" => self.sandbox.limits.max_constraints = num as i32,
            "sbox_maxropeconstraints" => self.sandbox.limits.max_ropeconstraints = num as i32,
            "sbox_playershurtplayers" => self.players_hurt_players = num != 0.0,
            _ => {}
        }
        self.cvar_changes.push((name.to_string(), old, v));
        true
    }

    pub fn cvar(&self, name: &str) -> Option<&str> {
        self.cvars.get(name).map(|c| c.value.as_str())
    }

    pub fn take_cvar_new(&mut self) -> Vec<String> {
        std::mem::take(&mut self.cvar_new)
    }

    pub fn take_cvar_changes(&mut self) -> Vec<(String, String, String)> {
        std::mem::take(&mut self.cvar_changes)
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
        if !self.players.contains_key(&id) {
            return;
        }
        self.game_events
            .push(GameEvent::PlayerSpawned { player: id });
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
        p.ragdoll = None;
        p.death_tick = None;
        p.held = None;
        // `PlayerSetModel`: the choice through the game's table, a hook may replace it
        let model = player_models::translate(&p.wanted_model).to_string();
        let model = match self.ask(Request::SetModel {
            player: id,
            model: model.clone(),
        }) {
            Verdict::Text(t) => t,
            _ => model,
        };
        if let Some(p) = self.players.get_mut(&id) {
            p.model = model;
            p.skin = p.wanted_skin;
            p.body = p.wanted_body.clone();
        }
        self.give_loadout(id);
    }

    /// `Entity:Ignite(seconds)`: a flame on the prop; false when there is no such prop or it
    /// already burns. The fire sound (`General.BurningObject`) plays at the prop.
    pub fn ignite(&mut self, ent: u32, seconds: f32) -> bool {
        let Some(prop) = self.sandbox.props().iter().find(|p| p.id == ent).cloned() else {
            return false;
        };
        if prop.fire.is_some() {
            return false;
        }
        let now = self.tick_count;
        let ticks = (seconds / gm_core::tick::DEFAULT_TICK_INTERVAL).round() as u64;
        self.sandbox.set_fire(
            ent,
            Some(gm_sandbox::Fire {
                until: now + ticks,
                next_think: now + FIRE_FIRST_DAMAGE_TICKS,
                out_at: None,
            }),
        );
        let pos = self.prop_state(ent).map_or(prop.pos, |s| s.position);
        self.sounds.push(WorldSound {
            name: "General.BurningObject".to_string(),
            pos,
            volume: 1.0,
        });
        true
    }

    /// `Entity:Extinguish`: the flame is gone at once, the health stays.
    pub fn extinguish(&mut self, ent: u32) -> bool {
        let burning = self
            .sandbox
            .props()
            .iter()
            .any(|p| p.id == ent && p.fire.is_some());
        if burning {
            self.sandbox.set_fire(ent, None);
        }
        burning
    }

    /// Damage on a prop (`CBreakableProp::OnTakeDamage`): the amount times the `prop_data`
    /// modifier of its type, truncated to whole points, off the health; zero breaks the prop
    /// (the surface's `break` sound, `PropBreak`, gone). A prop without health (metal) takes
    /// none. Returns whether it broke.
    pub fn damage_prop(&mut self, ent: u32, amount: f32, dmg_type: u32) -> bool {
        let Some(prop) = self.sandbox.props().iter().find(|p| p.id == ent).cloned() else {
            return false;
        };
        if prop.health <= 0 {
            return false;
        }
        let data = self
            .models
            .get(&prop.model)
            .map_or(PropData::default(), |m| m.prop_data);
        let left = prop.health - (amount * data.modifier(dmg_type)) as i32;
        if left > 0 {
            self.sandbox.set_health(ent, left);
            return false;
        }
        self.sandbox.set_health(ent, 0);
        let pos = self.prop_state(ent).map_or(prop.pos, |s| s.position);
        if let Some(name) = self
            .models
            .get(&prop.model)
            .and_then(|m| m.break_sound.clone())
        {
            self.sounds.push(WorldSound {
                name,
                pos,
                volume: 1.0,
            });
        }
        self.game_events.push(GameEvent::PropBreak { ent });
        self.remove_prop(ent);
        true
    }

    fn fire_tick(&mut self) {
        let now = self.tick_count;
        let burning: Vec<(u32, gm_sandbox::Fire)> = self
            .sandbox
            .props()
            .iter()
            .filter_map(|p| Some((p.id, p.fire.clone()?)))
            .collect();
        for (id, mut fire) in burning {
            if let Some(out) = fire.out_at {
                if now >= out {
                    self.sandbox.set_fire(id, None);
                }
                continue;
            }
            if now < fire.next_think {
                continue;
            }
            if now >= fire.until {
                fire.out_at = Some(now + FIRE_LINGER_TICKS);
                self.sandbox.set_fire(id, Some(fire));
                continue;
            }
            fire.next_think = now + FIRE_DAMAGE_INTERVAL_TICKS;
            self.sandbox.set_fire(id, Some(fire));
            self.damage_prop(id, FIRE_DAMAGE as f32, DMG_BURN | DMG_DIRECT);
        }
    }

    /// `cl_playermodel`: remembered now, worn from the next spawn on (as in the game).
    /// `cl_playerskin` / `cl_playerbodygroups`: worn from the next spawn, the body string cut
    /// to its digits (one per body part, at most 32).
    pub fn set_wanted_skin_body(&mut self, id: u32, skin: u8, body: &str) {
        if let Some(p) = self.players.get_mut(&id) {
            p.wanted_skin = skin;
            p.wanted_body = body.chars().filter(char::is_ascii_digit).take(32).collect();
        }
    }

    pub fn set_wanted_model(&mut self, id: u32, name: &str) {
        if let Some(p) = self.players.get_mut(&id) {
            p.wanted_model = name.chars().take(64).collect();
        }
    }

    /// `PlayerLoadout`: the inventory is emptied, `Request::Loadout` asked (a hook may give
    /// its own weapons there and cancel), then the default 13 weapons with the crowbar out.
    fn give_loadout(&mut self, id: u32) {
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        p.weapons.clear();
        p.active_weapon.clear();
        p.inventory.clear();
        if self.ask(Request::Loadout { player: id }) == Verdict::Cancel {
            return;
        }
        let Some(p) = self.players.get_mut(&id) else {
            return;
        };
        p.weapons = LOADOUT.iter().map(|w| w.to_string()).collect();
        p.active_weapon = SPAWN_WEAPON.to_string();
        p.inventory.give_loadout();
        p.inventory.select(SPAWN_WEAPON);
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

    /// A fresh `Give` of a weapon the player already carries right after a spawn (the probe's
    /// `fire_*`/`hit_*` setup): selected with the give timers and reserve (`Inventory::give`).
    pub fn give_weapon(&mut self, id: u32, class: &str) -> bool {
        let Some(p) = self.players.get_mut(&id) else {
            return false;
        };
        if !p.alive || weapons::def(class).is_none() {
            return false;
        }
        if !p.weapons.iter().any(|w| w == class) {
            p.weapons.push(class.to_string());
        }
        p.active_weapon = class.to_string();
        p.inventory.give(class);
        true
    }

    /// Where a player stood (origin, ducked) at server tick `seen`, interpolated between the two
    /// recorded ticks around it; the current pose when `seen` is 0 or outside the history.
    fn pose_at(&self, p: &Player, seen: u32) -> (Vec3, bool) {
        let now = (p.state.origin, p.state.ducked);
        if seen == 0 {
            return now;
        }
        let (Some(first), Some(last)) = (p.history.front(), p.history.back()) else {
            return now;
        };
        if seen >= last.0 || seen < first.0 {
            return now;
        }
        let i = p.history.partition_point(|h| h.0 <= seen);
        let (a, b) = (&p.history[i - 1], &p.history[i]);
        let f = (seen - a.0) as f32 / (b.0 - a.0).max(1) as f32;
        (a.1.lerp(b.1, f), a.2)
    }

    /// A trigger pull: the bullet (all pellets along the aim, no spread yet) against the other
    /// live players' hulls and the world; the hit group from the height on the target
    /// (`weapons::hitgroup_at`), the ammo's multiplayer damage scaled by it, applied as
    /// DMG_BULLET the same tick (recordings `hit_*`).
    fn resolve_shot(
        &mut self,
        attacker: u32,
        eye: Vec3,
        forward: Vec3,
        shot: weapons::Shot,
        class: &'static str,
        seen_tick: u32,
    ) {
        let Some(base) = weapons::bullet_damage(shot.ammo) else {
            return;
        };
        const MAX_TRACE: f32 = 56755.84;
        let end = eye + forward * MAX_TRACE;
        let world = self
            .collision
            .trace_ray(eye, end, gm_collision::MASK_PLAYERSOLID);
        let d = end - eye;
        let prop = gm_sandbox::trace_line(self.collision, &self.prop_bodies(), eye, end)
            .filter(|h| h.ent.is_some());
        let mut best: Option<(u32, f32)> = None;
        for (id, p) in &self.players {
            if *id == attacker || !p.alive {
                continue;
            }
            let (origin, ducked) = self.pose_at(p, seen_tick);
            let (mins, maxs) = self.mover.hull(ducked);
            let (lo, hi) = (origin + mins, origin + maxs);
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
        if let Some(h) = prop {
            if best.is_none_or(|b| h.fraction < b.1) {
                if let Some(damage) = weapons::prop_bullet_damage(shot.ammo) {
                    let n = shot.pellets as f32;
                    self.hit_prop(
                        h.ent.unwrap_or(0),
                        attacker,
                        damage * n,
                        weapons::prop_bullet_dmg_type(shot.ammo),
                        h.pos,
                        forward * (weapons::prop_bullet_force(shot.ammo) * n),
                        class,
                    );
                }
                return;
            }
        }
        let Some((target, frac)) = best else {
            return;
        };
        if world.fraction < frac {
            return;
        }
        let (frac, group, pos) = match &self.hit_model {
            Some(model) => {
                let p = &self.players[&target];
                let Some(h) = model.trace(p.state.origin, p.view_angles.y, eye, end) else {
                    return;
                };
                h
            }
            None => {
                let pos = eye + d * frac;
                let rel_z = pos.z - self.players[&target].state.origin.z;
                let Some(group) = weapons::hitgroup_at(rel_z) else {
                    return;
                };
                (frac, group, pos)
            }
        };
        if world.fraction < frac {
            return;
        }
        let damage = base * weapons::hitgroup_scale(group) * shot.pellets as f32;
        let dmg_type = weapons::bullet_dmg_type(shot.ammo);
        self.hits.push(Hit {
            tick: self.tick_count,
            attacker,
            target,
            damage,
            dmg_type,
            hitgroup: Some(group),
            pos,
            ammo: shot.ammo,
            inflictor: "player",
        });
        if !self.players_hurt_players {
            return;
        }
        let push = Vec3::new(forward.x, forward.y, 0.0).normalize_or_zero()
            * (weapons::KNOCKBACK_PER_DAMAGE * damage);
        if let Some(t) = self.players.get_mut(&target) {
            t.state.impulse += push;
        }
        self.damage_player(target, damage, dmg_type);
    }

    /// Sandbox props as trace bodies (hull planes in the entity frame).
    fn prop_bodies(&self) -> Vec<PropBody> {
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

    /// `Entity:TakeDamage` on a prop: the hit is logged, the damage force applied at the hit
    /// point as an impulse (`weapons::prop_bullet_force`), a breakable prop loses the damage and
    /// is removed at zero.
    #[allow(clippy::too_many_arguments)]
    fn hit_prop(
        &mut self,
        prop: u32,
        attacker: u32,
        damage: f32,
        dmg_type: u32,
        pos: Vec3,
        force: Vec3,
        inflictor: &'static str,
    ) {
        let broke = self.damage_prop(prop, damage, dmg_type);
        self.prop_hits.push(PropHit {
            tick: self.tick_count,
            attacker,
            prop,
            damage,
            dmg_type,
            pos,
            force,
            inflictor,
            broke,
        });
        if !broke {
            if let Some(&body) = self.bodies.get(&prop) {
                self.physics.add_impulse_at(body, pos, force);
            }
        }
    }

    /// A crowbar swing (`weapons::MELEE_*`): the first player hull on the aim within reach,
    /// before the world.
    fn resolve_melee(&mut self, attacker: u32, eye: Vec3, forward: Vec3) {
        let end = eye + forward * weapons::MELEE_RANGE;
        let world = self
            .collision
            .trace_ray(eye, end, gm_collision::MASK_PLAYERSOLID);
        let mut best: Option<(u32, f32)> = None;
        for (id, p) in &self.players {
            if *id == attacker || !p.alive {
                continue;
            }
            let (mins, maxs) = self.mover.hull(p.state.ducked);
            if let Some(t) =
                ray_box_entry(eye, forward, p.state.origin + mins, p.state.origin + maxs)
            {
                if t <= weapons::MELEE_RANGE && best.is_none_or(|b| t < b.1) {
                    best = Some((*id, t));
                }
            }
        }
        let prop = gm_sandbox::trace_line(self.collision, &self.prop_bodies(), eye, end)
            .filter(|h| h.ent.is_some());
        if let Some(h) = prop {
            if best.is_none_or(|b| h.fraction * weapons::MELEE_RANGE < b.1) {
                self.hit_prop(
                    h.ent.unwrap_or(0),
                    attacker,
                    weapons::MELEE_DAMAGE,
                    weapons::DMG_CLUB,
                    h.pos,
                    forward * weapons::MELEE_PROP_FORCE,
                    "weapon_crowbar",
                );
                return;
            }
        }
        let Some((target, t)) = best else {
            return;
        };
        if world.fraction * weapons::MELEE_RANGE < t {
            return;
        }
        let damage = weapons::MELEE_DAMAGE;
        self.hits.push(Hit {
            tick: self.tick_count,
            attacker,
            target,
            damage,
            dmg_type: weapons::DMG_CLUB,
            hitgroup: Some(0),
            pos: eye + forward * (t - 1.0 / 32.0),
            ammo: "",
            inflictor: "weapon_crowbar",
        });
        if !self.players_hurt_players {
            return;
        }
        let push = Vec3::new(forward.x, forward.y, 0.0).normalize_or_zero()
            * (weapons::MELEE_PUSH_PER_DAMAGE * damage);
        if let Some(t) = self.players.get_mut(&target) {
            t.state.impulse += push;
        }
        self.damage_player(target, damage, weapons::DMG_CLUB);
    }

    /// Knockback of a hit from `from` (the inflictor's position): see `weapons::PUSH_PER_DAMAGE`.
    fn push_player(&mut self, id: u32, from: Vec3, damage: f32) {
        let Some(t) = self.players.get_mut(&id) else {
            return;
        };
        let center = t.state.origin + Vec3::new(0.0, 0.0, weapons::PLAYER_CENTER_Z);
        let dir = (center - (from - Vec3::new(0.0, 0.0, weapons::PUSH_INFLICTOR_DROP)))
            .normalize_or_zero();
        let mut push = dir * (weapons::PUSH_PER_DAMAGE * damage);
        push.z = push.z.min(weapons::PUSH_MAX_Z);
        t.state.velocity += push;
        if t.state.on_ground {
            t.state.ground_push = true;
        }
    }

    /// Radius damage of an explosion at `at` (the inflictor's position; the blast itself is
    /// 1 u above it): `weapons::explosion` for the law. A player whose eye the blast cannot
    /// see takes nothing.
    fn explode(&mut self, at: Vec3, attacker: u32, base: f32, radius: f32, class: &'static str) {
        let src = at + Vec3::new(0.0, 0.0, 1.0);
        let bodies = self.prop_bodies();
        let targets: Vec<(u32, Vec3)> = self
            .sandbox
            .props()
            .iter()
            .filter_map(|p| {
                let b = self.models.get(&p.model)?.bounds;
                Some((p.id, p.pos + Vec3::new(0.0, 0.0, (b.min.z + b.max.z) * 0.5)))
            })
            .collect();
        for (id, center) in targets {
            let to = center - src;
            let len = to.length();
            if len < 1e-3 {
                continue;
            }
            let dir = to / len;
            let dist = match gm_sandbox::trace_line(self.collision, &bodies, src, center) {
                Some(h) if h.ent == Some(id) => ((h.pos - src).length() - 1.0 / 32.0).max(0.0),
                Some(h) if h.ent.is_none() => continue,
                Some(_) => continue,
                None => 0.0,
            };
            if dist >= radius {
                continue;
            }
            let damage = base * (1.0 - dist / radius);
            let force = (damage * weapons::BLAST_FORCE_PER_DAMAGE).min(weapons::BLAST_FORCE_MAX);
            self.hit_prop(
                id,
                attacker,
                damage,
                weapons::DMG_BLAST,
                src,
                dir * force,
                class,
            );
        }
        let ids: Vec<u32> = self.players.keys().copied().collect();
        for id in ids {
            let p = &self.players[&id];
            if !p.alive {
                continue;
            }
            let (mins, maxs) = self.mover.hull(p.state.ducked);
            let (lo, hi) = (p.state.origin + mins, p.state.origin + maxs);
            let eye = p.state.origin + Vec3::new(0.0, 0.0, weapons::PLAYER_EYE_Z);
            let to = eye - src;
            let len = to.length();
            if len < 1e-3 {
                continue;
            }
            let dir = to / len;
            // the game's trace to the eye stops 1/32 u before the hull
            let Some(dist) = ray_box_entry(src, dir, lo, hi).map(|d| (d - 1.0 / 32.0).max(0.0))
            else {
                continue;
            };
            if dist >= radius {
                continue;
            }
            let world = self
                .collision
                .trace_ray(src, eye, gm_collision::MASK_PLAYERSOLID);
            if world.fraction * len < dist {
                continue;
            }
            let damage = base * (1.0 - dist / radius);
            self.hits.push(Hit {
                tick: self.tick_count,
                attacker,
                target: id,
                damage,
                dmg_type: weapons::DMG_BLAST,
                // the game runs blasts through ScalePlayerDamage with the generic group
                hitgroup: Some(0),
                pos: src,
                ammo: "",
                inflictor: class,
            });
            if !self.players_hurt_players {
                continue;
            }
            self.push_player(id, at, damage);
            self.hurt(id, damage, weapons::DMG_BLAST, RESPAWN_TICKS - 1);
        }
    }

    /// The first player (other than `owner`, alive) whose hit boxes the segment crosses: what a
    /// rocket collides with and the laser dot lands on (`hit_rpg_direct`: the game's rocket
    /// passed 2 u under the hull's top, over the head box, without touching). Without a hit
    /// model the hull stands in.
    fn hitbox_touch(&self, owner: u32, start: Vec3, end: Vec3) -> Option<(u32, Vec3)> {
        let Some(model) = &self.hit_model else {
            return self.projectile_touch(owner, start, end, 0.0, true);
        };
        let mut best: Option<(u32, f32, Vec3)> = None;
        for (id, p) in &self.players {
            if *id == owner || !p.alive {
                continue;
            }
            if let Some((frac, _, pos)) = model.trace(p.state.origin, p.view_angles.y, start, end) {
                if best.is_none_or(|b| frac < b.1) {
                    best = Some((*id, frac, pos));
                }
            }
        }
        best.map(|(id, _, pos)| (id, pos))
    }

    /// The player hull (other than `owner`, alive) a projectile touches: `end` inside a hull
    /// grown by `grow` (rockets and balls are resolved where they end the tick), or, with
    /// `sweep`, the first hull on the way from `start` (bolts stop at the hull).
    fn projectile_touch(
        &self,
        owner: u32,
        start: Vec3,
        end: Vec3,
        grow: f32,
        sweep: bool,
    ) -> Option<(u32, Vec3)> {
        let d = end - start;
        let len = d.length();
        let mut best: Option<(u32, f32)> = None;
        for (id, p) in &self.players {
            if *id == owner || !p.alive {
                continue;
            }
            let (mins, maxs) = self.mover.hull(p.state.ducked);
            let lo = p.state.origin + mins - Vec3::splat(grow);
            let hi = p.state.origin + maxs + Vec3::splat(grow);
            let t = if sweep && len > 1e-6 {
                match ray_box_entry(start, d / len, lo, hi) {
                    Some(t) if t <= len => t,
                    _ => continue,
                }
            } else if end.cmpge(lo).all() && end.cmple(hi).all() {
                len
            } else {
                continue;
            };
            if best.is_none_or(|b| t < b.1) {
                best = Some((*id, t));
            }
        }
        // a swept box stops 1/32 u short of what it hits, like the game's traces
        best.map(|(id, t)| {
            let t = if sweep { (t - 1.0 / 32.0).max(0.0) } else { t };
            (
                id,
                if len > 1e-6 {
                    start + d * (t / len)
                } else {
                    end
                },
            )
        })
    }

    /// Existing projectiles fly one tick, then this tick's launches spawn (their first tick is
    /// the launch tick, recorded as such); a rocket's death reloads its owner's RPG. Rockets and
    /// grenades explode where they die (`explode`), a bolt or rocket that reaches a player hits
    /// it, a ball kills whoever it passes through (`weapons`).
    fn tick_projectiles(&mut self, launches: Vec<(u32, weapons::Launch, Vec3, Vec3)>) {
        if launches.iter().any(|l| l.1 == weapons::Launch::Grenade)
            && !self.models.contains_key(projectile::GRENADE_MODEL)
        {
            if let Some(loaded) = self
                .model_loader
                .as_mut()
                .and_then(|f| f(projectile::GRENADE_MODEL))
            {
                self.register_model(projectile::GRENADE_MODEL, loaded);
            }
        }
        let world = self.collision;
        let mut phys = projectile::Physics {
            world: &mut self.physics,
            grenade: self.models.get(projectile::GRENADE_MODEL),
            rng: &mut self.spawn_rng,
        };
        let mut steps = Vec::with_capacity(self.projectiles.len());
        for pr in &mut self.projectiles {
            let prev = pr.origin;
            steps.push((prev, pr.step(world, Some(&mut phys))));
        }
        // (kind, owner, where it died) for every projectile removed this tick
        let mut died = Vec::new();
        let mut keep = Vec::with_capacity(self.projectiles.len());
        for (i, (prev, ev)) in steps.into_iter().enumerate() {
            let pr = &self.projectiles[i];
            match ev {
                projectile::Event::Removed(at) => {
                    died.push((pr.kind, pr.owner, at));
                    keep.push(false);
                }
                projectile::Event::None => {
                    let (kind, owner, end) = (pr.kind, pr.owner, pr.origin);
                    let touch = match kind {
                        weapons::Launch::Rocket => self.hitbox_touch(owner, prev, end),
                        weapons::Launch::Bolt => {
                            self.projectile_touch(owner, prev, end, projectile::BOLT_HALF, true)
                        }
                        weapons::Launch::Ball => {
                            self.projectile_touch(owner, prev, end, projectile::BALL_RADIUS, false)
                        }
                        weapons::Launch::Smg1Grenade => self.projectile_touch(
                            owner,
                            prev,
                            end,
                            projectile::AR2_GRENADE_HALF,
                            true,
                        ),
                        weapons::Launch::Grenade => None,
                    };
                    let prop_touch = if kind == weapons::Launch::Grenade
                        || self.sandbox.props().is_empty()
                    {
                        None
                    } else {
                        gm_sandbox::trace_line(world, &self.prop_bodies(), prev, end).filter(|h| {
                            h.ent.is_some()
                                && touch.is_none_or(|(_, at)| {
                                    (h.pos - prev).length() < (at - prev).length()
                                })
                        })
                    };
                    if let Some(h) = prop_touch {
                        let prop = h.ent.unwrap_or(0);
                        let dir = (end - prev).normalize_or_zero();
                        // the swept box stops its half extent short of the hull
                        let back = match kind {
                            weapons::Launch::Bolt => projectile::BOLT_HALF,
                            weapons::Launch::Smg1Grenade => projectile::AR2_GRENADE_HALF,
                            weapons::Launch::Ball => projectile::BALL_RADIUS,
                            _ => 1.0 / 32.0,
                        };
                        let at = h.pos - dir * back;
                        match kind {
                            weapons::Launch::Rocket | weapons::Launch::Smg1Grenade => {
                                died.push((kind, owner, at));
                                keep.push(false);
                            }
                            weapons::Launch::Bolt => {
                                self.projectiles[i].origin = at;
                                self.projectiles[i].velocity = Vec3::ZERO;
                                self.hit_prop(
                                    prop,
                                    owner,
                                    weapons::BOLT_DAMAGE,
                                    weapons::DMG_BULLET | weapons::DMG_NEVERGIB,
                                    h.pos,
                                    dir * weapons::BOLT_PROP_FORCE,
                                    "crossbow_bolt",
                                );
                                keep.push(true);
                            }
                            _ => {
                                self.hit_prop(
                                    prop,
                                    owner,
                                    weapons::BALL_PROP_DAMAGE,
                                    weapons::BALL_DMG_TYPE,
                                    h.pos,
                                    dir * weapons::BALL_PROP_FORCE,
                                    "prop_combine_ball",
                                );
                                keep.push(true);
                            }
                        }
                        continue;
                    }
                    match (kind, touch) {
                        (weapons::Launch::Rocket | weapons::Launch::Smg1Grenade, Some((_, at))) => {
                            died.push((kind, owner, at));
                            keep.push(false);
                        }
                        (weapons::Launch::Bolt, Some((target, at))) => {
                            self.projectiles[i].origin = at;
                            self.hits.push(Hit {
                                tick: self.tick_count,
                                attacker: owner,
                                target,
                                damage: weapons::BOLT_DAMAGE,
                                dmg_type: weapons::DMG_BULLET | weapons::DMG_NEVERGIB,
                                hitgroup: Some(0),
                                pos: at,
                                ammo: "",
                                inflictor: "crossbow_bolt",
                            });
                            if self.players_hurt_players {
                                self.push_player(target, at, weapons::BOLT_DAMAGE);
                                self.hurt(
                                    target,
                                    weapons::BOLT_DAMAGE,
                                    weapons::DMG_BULLET | weapons::DMG_NEVERGIB,
                                    RESPAWN_TICKS - 1,
                                );
                            }
                            keep.push(false);
                        }
                        (weapons::Launch::Ball, Some((target, _))) => {
                            let t = &self.players[&target];
                            let (mins, maxs) = self.mover.hull(t.state.ducked);
                            let at = end.clamp(t.state.origin + mins, t.state.origin + maxs);
                            let hp = t.health as f32;
                            self.hits.push(Hit {
                                tick: self.tick_count,
                                attacker: owner,
                                target,
                                damage: hp,
                                dmg_type: weapons::BALL_DMG_TYPE,
                                hitgroup: None,
                                pos: at,
                                ammo: "",
                                inflictor: "prop_combine_ball",
                            });
                            if self.players_hurt_players {
                                self.hurt(target, hp, weapons::BALL_DMG_TYPE, RESPAWN_TICKS - 1);
                            }
                            keep.push(true);
                        }
                        _ => keep.push(true),
                    }
                }
            }
        }
        let mut k = keep.into_iter();
        self.projectiles.retain(|_| k.next().unwrap_or(true));
        for (kind, owner, at) in died {
            if kind == weapons::Launch::Rocket {
                if let Some(p) = self.players.get_mut(&owner) {
                    p.inventory.rocket_died();
                }
            }
            if let Some((base, radius)) = weapons::explosion(kind) {
                self.explode(at, owner, base, radius, kind.class());
            }
        }
        let mut phys = projectile::Physics {
            world: &mut self.physics,
            grenade: self.models.get(projectile::GRENADE_MODEL),
            rng: &mut self.spawn_rng,
        };
        let mut spawned = Vec::new();
        for (owner, kind, eye, angles) in launches {
            if let Some(pr) =
                projectile::spawn(kind, owner, eye, angles.x, angles.y, world, Some(&mut phys))
            {
                spawned.push((eye, pr));
            }
        }
        for (eye, mut pr) in spawned {
            if pr.kind == weapons::Launch::Rocket {
                // the laser dot sits on a player standing in the aim before the wall
                if let Some((_, at)) = self.hitbox_touch(pr.owner, eye, pr.target) {
                    pr.target = at;
                }
            }
            self.projectiles.push(pr);
        }
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
        self.game_events.push(GameEvent::PlayerDied {
            player: id,
            dmg_type: 0,
        });
    }

    pub fn register_model(&mut self, name: &str, info: ModelInfo) {
        self.hull_planes
            .insert(name.to_string(), info.hull_planes.clone());
        self.models.insert(name.to_string(), info);
    }

    /// The player leaves: whatever the physgun held is released, the entry goes; props,
    /// constraints and their ownership stay (the game keeps a leaver's props).
    pub fn remove_player(&mut self, id: u32) -> bool {
        self.players.remove(&id).is_some()
    }

    pub fn add_player(&mut self, id: u32, origin: Vec3, view_angles: Vec3) {
        self.players.insert(id, Player::new(origin, view_angles));
        if self.request_hook.is_some() {
            self.give_loadout(id);
        }
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
        for (i, (model, _, _, frozen)) in props.iter().enumerate() {
            self.sandbox.set_frozen(i as u32 + 1, *frozen);
            let health = self.models.get(model).map_or(0, |m| m.prop_data.health);
            self.sandbox.set_health(i as u32 + 1, health);
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
    /// The player's hull origin and height for the +use reach and carry clamps.
    fn player_hull(&self, player: u32) -> (Vec3, f32) {
        self.players.get(&player).map_or((Vec3::ZERO, 0.0), |p| {
            let h = if p.state.ducked {
                gm_physics::pickup::DUCK_HULL_HEIGHT
            } else {
                gm_physics::pickup::HULL_HEIGHT
            };
            (p.state.origin, h)
        })
    }

    /// `FindUseEntity`: the prop under the view line within reach, else the first one a 16 u
    /// box finds in 72 u sweeps tilted down from the view (props at the feet).
    fn find_use_entity(&self, player: u32, eye: Vec3, view: Vec3) -> Option<u32> {
        use gm_physics::pickup::{
            box_sweep_convex, sweep_dirs, view_vectors, within_use_reach, world_convex, SWEEP_HALF,
            SWEEP_LEN,
        };
        let (forward, up) = view_vectors(view.x, view.y);
        let (origin, hull) = self.player_hull(player);
        let bodies = self.trace_bodies();
        if let Some(h) = gm_sandbox::tool_trace(self.collision, &bodies, eye, forward) {
            if let Some(ent) = h.ent {
                // the game's trace stops at the convex's skin, `REST_GAP` short of the hull
                let hit = h.pos - forward * gm_physics::REST_GAP;
                if within_use_reach(eye, hit, origin, hull) {
                    return Some(ent);
                }
            }
        }
        let convexes: Vec<(u32, Vec<WorldConvex>)> = self
            .sandbox
            .props()
            .iter()
            .filter_map(|p| {
                let b = *self.bodies.get(&p.id)?;
                let info = self.models.get(&p.model)?;
                let st = self.physics.state(b);
                let cs = info
                    .hull_planes
                    .iter()
                    .zip(&info.hulls)
                    .map(|(pl, pts)| world_convex(pl, pts, st.rotation, st.position))
                    .collect();
                Some((p.id, cs))
            })
            .collect();
        for dir in sweep_dirs(forward, up) {
            let wall = self
                .collision
                .trace_ray(eye, eye + dir * SWEEP_LEN, gm_collision::MASK_SOLID)
                .fraction
                * SWEEP_LEN;
            let mut best: Option<(f32, u32)> = None;
            for (ent, cs) in &convexes {
                for (planes, aabb) in cs {
                    if let Some(t) =
                        box_sweep_convex(eye, dir, SWEEP_LEN, SWEEP_HALF, planes, *aabb)
                    {
                        if t <= wall && best.is_none_or(|(bt, _)| t < bt) {
                            best = Some((t, *ent));
                        }
                    }
                }
            }
            if let Some((t, ent)) = best {
                if within_use_reach(eye, eye + dir * t, origin, hull) {
                    return Some(ent);
                }
            }
        }
        None
    }

    /// A used prop becomes the player's carry when it is light and small enough
    /// (`CanPickupObject(35, 128)`), nobody physguns it and `Request::PlayerPickup` allows.
    fn try_pickup(&mut self, player: u32, ent: u32, yaw: f32) {
        let Some(model) = self
            .sandbox
            .props()
            .iter()
            .find(|p| p.id == ent)
            .map(|p| p.model.clone())
        else {
            return;
        };
        let Some(info) = self.models.get(&model) else {
            return;
        };
        let size = info.bounds.max - info.bounds.min;
        if info.mass > gm_physics::pickup::MAX_MASS
            || size.max_element() > gm_physics::pickup::MAX_SIZE
        {
            return;
        }
        if self.players.values().any(|p| {
            p.held.is_some_and(|(e, _)| e == ent) || p.carry.is_some_and(|(e, _)| e == ent)
        }) {
            return;
        }
        let Some(b) = self.bodies.get(&ent).copied() else {
            return;
        };
        if self.ask(Request::PlayerPickup { player, ent }) == Verdict::Cancel {
            return;
        }
        let carry = gm_physics::pickup::Carry::new(self.physics.state(b).rotation, yaw);
        if let Some(p) = self.players.get_mut(&player) {
            p.carry = Some((ent, carry));
        }
        self.game_events.push(GameEvent::Pickup { player, ent });
    }

    /// Let the carried prop go where it is, at rest (the game drops it with no velocity).
    fn drop_carry(&mut self, player: u32) {
        let Some((ent, _)) = self.players.get_mut(&player).and_then(|p| p.carry.take()) else {
            return;
        };
        if let Some(b) = self.bodies.get(&ent).copied() {
            self.physics.release(b);
            self.physics.set_velocity(b, Vec3::ZERO, Vec3::ZERO);
        }
        self.game_events.push(GameEvent::Drop { player, ent });
    }

    /// Drive the carried prop to its hold point this tick; the carry ends when the player dies,
    /// the prop is gone or a physgun took it.
    fn carry_tick(&mut self, player: u32, eye: Vec3, view: Vec3) {
        let Some((ent, carry)) = self.players.get(&player).and_then(|p| p.carry) else {
            return;
        };
        let alive = self.players.get(&player).is_some_and(|p| p.alive);
        let physgunned = self
            .players
            .values()
            .any(|p| p.held.is_some_and(|(e, _)| e == ent));
        let model = self
            .sandbox
            .props()
            .iter()
            .find(|p| p.id == ent)
            .map(|p| p.model.clone());
        let body = self.bodies.get(&ent).copied();
        let (Some(b), Some(info), true, false) = (
            body,
            model.and_then(|m| self.models.get(&m)),
            alive,
            physgunned,
        ) else {
            self.drop_carry(player);
            return;
        };
        let (origin, hull) = self.player_hull(player);
        let collision = self.collision;
        let brush = |a: Vec3, b: Vec3| collision.trace_ray(a, b, gm_collision::MASK_SOLID).fraction;
        let st = self.physics.state(b);
        let (tp, tr) = carry.target(
            eye,
            view,
            &info.hulls,
            info.mass_center,
            st.rotation,
            origin,
            hull,
            &brush,
        );
        self.physics.hold(
            b,
            tp,
            tr,
            &gm_physics::pickup::params(gm_core::tick::DEFAULT_TICK_INTERVAL),
        );
    }

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
        if let Some(id) = id {
            self.sandbox.set_health(id, info.prop_data.health);
        }
        self.sync_physics();
        id
    }

    /// Duplicator left click: the hit prop and everything constrained to it (transitively) go
    /// to the player's clipboard with positions relative to the hit prop and world angles kept
    /// (pasted as copied, the game's default).
    pub fn duplicator_copy(&mut self, player: u32, hit: &ToolHit) -> bool {
        let Some(root) = hit.ent else {
            return false;
        };
        let props = self.sandbox.props();
        let Some(anchor) = props.iter().find(|p| p.id == root) else {
            return false;
        };
        let cons = self.sandbox.constraints();
        let mut group = vec![root];
        let mut i = 0;
        while i < group.len() {
            let e = group[i];
            for c in cons {
                for (x, y) in [(c.ent1, c.ent2), (c.ent2, c.ent1)] {
                    if x == Some(e) {
                        if let Some(o) = y {
                            if !group.contains(&o) && props.iter().any(|p| p.id == o) {
                                group.push(o);
                            }
                        }
                    }
                }
            }
            i += 1;
        }
        let anchor_pos = anchor.pos;
        let index = |e: Option<u32>| e.and_then(|id| group.iter().position(|g| *g == id));
        let saved: Vec<save::SavedProp> = group
            .iter()
            .filter_map(|id| props.iter().find(|p| p.id == *id))
            .map(|p| save::SavedProp {
                model: p.model.clone(),
                pos: (p.pos - anchor_pos).to_array(),
                angles: p.angles.to_array(),
                frozen: p.frozen,
                skin: p.skin,
                body: p.body.clone(),
            })
            .collect();
        let constraints = cons
            .iter()
            .filter(|c| {
                let in_group = |e: Option<u32>| match e {
                    None => true,
                    Some(id) => group.contains(&id),
                };
                in_group(c.ent1) && in_group(c.ent2) && (c.ent1.is_some() || c.ent2.is_some())
            })
            .map(|c| save::SavedConstraint {
                a: index(c.ent1),
                b: index(c.ent2),
                kind: save::SavedKind::from(&c.kind),
            })
            .collect();
        self.clipboards.insert(
            player,
            save::Save {
                format: 1,
                map: String::new(),
                props: saved,
                constraints,
            },
        );
        true
    }

    /// Duplicator right click: the clipboard group at the hit point (the copied prop's origin on
    /// the hit, orientations as copied), each prop under the player's prop limit, one undo entry.
    pub fn duplicator_paste(
        &mut self,
        player: u32,
        hit: &ToolHit,
        events: &mut Vec<gm_sandbox::Event>,
    ) -> bool {
        let Some(clip) = self.clipboards.get(&player).cloned() else {
            return false;
        };
        if clip.props.is_empty() {
            return false;
        }
        let mut ids: Vec<Option<u32>> = Vec::new();
        for p in &clip.props {
            if !self.sandbox.check_limit(player, "props", events) {
                break;
            }
            if !self.models.contains_key(&p.model) {
                match self.model_loader.as_mut().and_then(|f| f(&p.model)) {
                    Some(info) => self.register_model(&p.model, info),
                    None => {
                        ids.push(None);
                        continue;
                    }
                }
            }
            let id = self.sandbox.restore_prop(
                player,
                &p.model,
                hit.pos + Vec3::from(p.pos),
                Vec3::from(p.angles),
                p.frozen,
            );
            let health = self.models.get(&p.model).map_or(0, |m| m.prop_data.health);
            self.sandbox.set_health(id, health);
            self.sandbox.set_skin(id, p.skin);
            self.sandbox.set_body(id, p.body.clone());
            self.sandbox.add_count(player, "props", id);
            self.sandbox.add_cleanup(player, "props", id);
            ids.push(Some(id));
        }
        if ids.is_empty() {
            return false;
        }
        let mut made: Vec<u32> = ids.iter().flatten().copied().collect();
        for c in &clip.constraints {
            let ent = |i: Option<usize>| match i {
                None => Some(None),
                Some(i) => ids.get(i).copied().flatten().map(Some),
            };
            let (Some(a), Some(b)) = (ent(c.a), ent(c.b)) else {
                continue;
            };
            let id = self.sandbox.restore_constraint(
                player,
                a,
                b,
                gm_sandbox::ConstraintKind::from(&c.kind),
            );
            self.sandbox.add_count(player, "constraints", id);
            self.sandbox.add_cleanup(player, "constraints", id);
            made.push(id);
        }
        self.sandbox.push_undo(
            player,
            "Duplicator",
            "#tool.duplicator.name".into(),
            Some("Undone #tool.duplicator.name".into()),
            made,
        );
        true
    }

    /// A tool gun click of `player` with the current view.
    pub fn tool_click(&mut self, player: u32, tool: &str, right: bool) -> bool {
        matches!(
            self.tool_click_full(player, tool, right),
            ToolClick::Done(true)
        )
    }

    /// The tool gun's click was taken by a stool: the fire event for its sound and gestures.
    pub fn tool_fired(&mut self, player: u32) {
        if let Some(p) = self.players.get_mut(&player) {
            p.inventory.tool_fired();
        }
    }

    /// Like `tool_click`, telling the caller when the stool is not a Rust one.
    pub fn tool_click_full(&mut self, player: u32, tool: &str, right: bool) -> ToolClick {
        match self.tool_hit(player) {
            Some(hit) => self.tool_click_at(player, tool, right, hit),
            None => ToolClick::Miss,
        }
    }

    /// The tool gun's trace from the player's eye: what a click would hit.
    pub fn tool_hit(&self, player: u32) -> Option<ToolHit> {
        let (eye, eye_angles) = self.eye(player)?;
        let bodies = self.trace_bodies();
        let dir = gm_sandbox::forward(eye_angles.x, eye_angles.y);
        gm_sandbox::tool_trace(self.collision, &bodies, eye, dir)
    }

    /// `tool_click_full` with the trace already made (`tool_hit`).
    pub fn tool_click_at(
        &mut self,
        player: u32,
        tool: &str,
        right: bool,
        hit: ToolHit,
    ) -> ToolClick {
        if !Sandbox::knows_tool(tool) {
            return ToolClick::Unknown(hit);
        }
        let mut events = Vec::new();
        if tool == "duplicator" {
            let ret = if right {
                self.duplicator_paste(player, &hit, &mut events)
            } else {
                self.duplicator_copy(player, &hit)
            };
            self.events.extend(events);
            self.sync_physics();
            return ToolClick::Done(ret);
        }
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
        let Some(name) = self.sandbox.undos(player).last().map(|u| u.name.clone()) else {
            return;
        };
        if self.ask(Request::Undo { player, name }) == Verdict::Cancel {
            return;
        }
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
                    if !p.gravity {
                        self.physics.set_gravity(body, false);
                    }
                    if !p.collide {
                        self.physics.set_world_only(body, true);
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
        let use_pressed = use_key
            && self
                .players
                .get(&player)
                .is_some_and(|p| !p.use_down && p.alive);
        if let Some(p) = self.players.get_mut(&player) {
            p.use_down = use_key;
        }
        // Measured (`lua_use_hold`): while +use is held over an entity the game asks
        // `PlayerUse` every tick (41 calls for a 40-tick hold on a frozen crate, 5 for a
        // 5-tick one, also while already carrying); the pickup itself happens on the press
        // (picked the tick after), the next press drops.
        let use_alive = use_key && self.players.get(&player).is_some_and(|p| p.alive);
        if use_alive {
            if use_pressed && self.players.get(&player).is_some_and(|p| p.carry.is_some()) {
                self.drop_carry(player);
            } else if let Some(ent) = self.find_use_entity(player, eye, view) {
                if self.ask(Request::PlayerUse { player, ent }) != Verdict::Cancel {
                    self.game_events.push(GameEvent::Use { player, ent });
                    if use_pressed {
                        self.try_pickup(player, ent, view.y);
                    }
                }
            }
        }
        let reload = pc.cmd.buttons & weapons::IN_RELOAD != 0;
        let reload_pressed = reload
            && self
                .players
                .get(&player)
                .is_some_and(|p| !p.reload_down && p.alive);
        if let Some(p) = self.players.get_mut(&player) {
            p.reload_down = reload;
        }
        if reload_pressed && pc.physgun {
            // `PhysgunUnfreeze`: every frozen prop of the player, each asked
            let frozen: Vec<u32> = self
                .sandbox
                .props()
                .iter()
                .filter(|p| p.frozen && p.owner == player)
                .map(|p| p.id)
                .collect();
            let mut count = 0;
            for ent in frozen {
                if self.ask(Request::Unfreeze { player, ent }) != Verdict::Cancel
                    && self.set_prop_frozen(ent, false)
                {
                    count += 1;
                }
            }
            if count > 0 {
                self.game_events.push(GameEvent::Unfroze { player, count });
            }
        }
        let held = self.players.get(&player).and_then(|p| p.held);
        let had = held.is_some();
        let held = match held {
            None if attack && pc.physgun => {
                let bodies = self.trace_bodies();
                let hit = gm_sandbox::tool_trace(self.collision, &bodies, eye, forward);
                match hit.and_then(|h| h.ent.map(|e| (e, h))) {
                    Some((ent, h))
                        if !self.sandbox.props().iter().any(|p| p.id == ent && p.frozen) =>
                    {
                        let allowed = !matches!(
                            self.ask(Request::PhysgunPickup { player, ent }),
                            Verdict::Cancel
                        );
                        let body = allowed.then(|| self.bodies.get(&ent).copied()).flatten();
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
                self.game_events
                    .push(GameEvent::PhysgunDrop { player, ent });
                None
            }
            Some((ent, _)) if attack2 => {
                if let Some(b) = self.bodies.get(&ent).copied() {
                    self.physics.freeze(b);
                }
                self.sandbox.set_frozen(ent, true);
                self.game_events
                    .push(GameEvent::PhysgunFreeze { player, ent });
                None
            }
            other => other,
        };
        if !had {
            if let Some((ent, _)) = &held {
                self.game_events
                    .push(GameEvent::PhysgunPickup { player, ent: *ent });
            }
        }
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
        self.carry_tick(player, eye, view);
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
        let now = self.tick_count as u32;
        self.fire_tick();
        let upright: Vec<(BodyHandle, Vec3)> = self
            .sandbox
            .props()
            .iter()
            .filter_map(|p| Some((*self.bodies.get(&p.id)?, p.upright?)))
            .collect();
        for (b, angles) in upright {
            self.physics.keep_upright(
                b,
                gm_sandbox::rotation(angles),
                gm_core::tick::DEFAULT_TICK_INTERVAL,
            );
        }
        for p in self.players.values_mut() {
            p.history.push_back((now, p.state.origin, p.state.ducked));
            if p.history.len() > 64 {
                p.history.pop_front();
            }
        }
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
                if tick >= d + p.respawn_ticks {
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
        // weapons first: a hit pushes and hurts the target in the tick of the shot, before the
        // target's own move (recordings `hit_*`); a prop takes the damage impulse before this
        // tick's physics step (`shoot_prop_357_drum`: the floor eats its downward part in the hit tick)
        let mut shots = Vec::new();
        let mut swings = Vec::new();
        let mut launches = Vec::new();
        for (id, p) in self.players.iter_mut() {
            if !p.alive {
                continue;
            }
            if let Some(pc) = cmds.get(id) {
                let shot = p.inventory.command(&p.active_weapon, pc.cmd.buttons);
                let eye = p.state.origin + Vec3::new(0.0, 0.0, self.mover.eye_height(&p.state));
                if let Some(shot) = shot {
                    let forward = gm_sandbox::forward(pc.cmd.view_angles.x, pc.cmd.view_angles.y);
                    let class = weapons::WEAPONS
                        .iter()
                        .find(|w| w.class == p.active_weapon.as_str())
                        .map_or("player", |w| w.class);
                    shots.push((*id, eye, forward, shot, class, pc.seen_tick));
                }
                if p.inventory.swung {
                    let forward = gm_sandbox::forward(pc.cmd.view_angles.x, pc.cmd.view_angles.y);
                    swings.push((*id, eye, forward));
                }
                if let Some(kind) = p.inventory.launched.take() {
                    launches.push((*id, kind, eye, pc.cmd.view_angles));
                }
                p.inventory.tick(&p.active_weapon);
            }
        }
        for (attacker, eye, forward, shot, class, seen) in shots {
            self.resolve_shot(attacker, eye, forward, shot, class, seen);
        }
        for (attacker, eye, forward) in swings {
            self.resolve_melee(attacker, eye, forward);
        }
        let mut steps: Vec<(Vec3, gm_move::Step)> = Vec::new();
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
                // footsteps see the state entering the tick, at the feet (the origin)
                let feet = p.state.origin;
                let step = p.steps.before_move(
                    &p.state,
                    self.mover.step_max_speed(&cmd),
                    gm_core::tick::DEFAULT_TICK_INTERVAL * 1000.0,
                );
                self.mover.tick(&mut p.state, &cmd);
                p.prev_vz = prev_vz;
                p.prev_on_ground = !was_airborne;
                let jumped = p.state.boost_pending && !was_airborne;
                if let Some(step) = step.or_else(|| jumped.then(|| p.steps.on_jump())) {
                    steps.push((feet, step));
                }
                // a landing: the hurting ones also voice `Player.FallDamage` (`move_step_fall_*`)
                if p.state.on_ground && was_airborne {
                    if -prev_vz >= FALL_SAFE_SPEED {
                        self.sounds.push(WorldSound {
                            name: "Player.FallDamage".to_owned(),
                            pos: p.state.origin,
                            volume: 1.0,
                        });
                    }
                    if let Some(step) = p.steps.on_land(-prev_vz) {
                        steps.push((p.state.origin, step));
                    }
                }
            }
        }
        for (feet, step) in steps {
            self.footstep(feet, step);
        }
        // entities after the players' moves: a projectile's hit or blast this tick pushes and
        // hurts a target that has already moved (its velocity shows the push raw)
        self.tick_projectiles(launches);
        // the physics step after bullets, moves and projectiles: a prop hit by any of them takes
        // the impulse before the floor acts on it in the same tick (`shoot_prop_crossbow_drum`)
        self.physics.step();
        self.impact_sounds();
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
            let dmg = match self.ask(Request::FallDamage {
                player: id,
                speed,
                amount: dmg,
            }) {
                Verdict::Cancel => continue,
                Verdict::Amount(a) => a,
                Verdict::Allow => dmg,
                Verdict::Text(_) => dmg,
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

    /// The context menu's properties on the prop under the player's crosshair, as the game
    /// offers them for a prop (measured, docs/spec/sandbox.md): `remove`, `gravity` (toggle),
    /// `collision_off` / `collision_on`, `keepupright` / `keepupright_stop`. `CanProperty` is
    /// asked first; a property that does not apply (collision already off, ...) is not offered
    /// and does nothing. Returns whether it was applied.
    /// The context-menu entries the game offers for the prop under `player`'s cursor, in the
    /// game's order (`properties.Add` order values: skin 601, bodygroups 600, keepupright 700,
    /// ignite 999, remove 1000, gravity 1001, collision 1500): `skin <n>` per family when the
    /// model has more than one, `bodygroups <part> <model>` per choice, then the toggles by
    /// state. Empty when nothing is under the cursor or it is not a prop.
    pub fn offered_properties(&self, player: u32) -> Vec<String> {
        let Some(ent) = self.tool_hit(player).and_then(|h| h.ent) else {
            return Vec::new();
        };
        let Some(prop) = self.sandbox.props().iter().find(|p| p.id == ent) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Some(info) = self.models.get(&prop.model) {
            for (part, &n) in info.body_groups.iter().enumerate() {
                if n > 1 {
                    out.extend((0..n).map(|m| format!("bodygroups {part} {m}")));
                }
            }
            if info.skins > 1 {
                out.extend((0..info.skins).map(|n| format!("skin {n}")));
            }
        }
        out.push(if prop.upright.is_none() {
            "keepupright".into()
        } else {
            "keepupright_stop".into()
        });
        out.push(if prop.fire.is_none() {
            "ignite".into()
        } else {
            "extinguish".into()
        });
        out.push("remove".into());
        out.push("gravity".into());
        out.push(if prop.collide {
            "collision_off".into()
        } else {
            "collision_on".into()
        });
        out
    }

    pub fn apply_property(&mut self, player: u32, name: &str) -> bool {
        // `skin <n>`: the game's property opens a submenu of the model's skins
        let (name, arg) = match name.split_once(' ') {
            Some((n, a)) => (n, a.trim()),
            None => (name, ""),
        };
        let Some(ent) = self.tool_hit(player).and_then(|h| h.ent) else {
            return false;
        };
        let Some(prop) = self.sandbox.props().iter().find(|p| p.id == ent).cloned() else {
            return false;
        };
        let offered = match name {
            "remove" | "gravity" => true,
            "collision_off" => prop.collide,
            "collision_on" => !prop.collide,
            "keepupright" => prop.upright.is_none(),
            "keepupright_stop" => prop.upright.is_some(),
            "ignite" => prop.fire.is_none(),
            "extinguish" => prop.fire.is_some(),
            "skin" => {
                let skins = self.models.get(&prop.model).map_or(1, |m| m.skins);
                skins > 1 && arg.parse::<u8>().is_ok_and(|n| n < skins)
            }
            // the game's filter: some body part with more than one model; `<part> <model>`
            "bodygroups" => {
                let groups = self.models.get(&prop.model).map(|m| m.body_groups.clone());
                let Some(groups) = groups.filter(|g| g.iter().any(|&n| n > 1)) else {
                    return false;
                };
                let mut it = arg.split_whitespace();
                let part = it.next().and_then(|p| p.parse::<usize>().ok());
                let model = it.next().and_then(|m| m.parse::<u8>().ok());
                match (part, model) {
                    (Some(p), Some(m)) => groups.get(p).is_some_and(|&n| m < n),
                    _ => false,
                }
            }
            _ => false,
        };
        if !offered
            || self.ask(Request::Property {
                player,
                ent,
                name: name.to_string(),
            }) == Verdict::Cancel
        {
            return false;
        }
        let body = self.bodies.get(&ent).copied();
        match name {
            "remove" => {
                self.remove_prop(ent);
            }
            "gravity" => {
                self.sandbox.set_gravity(ent, !prop.gravity);
                if let Some(b) = body {
                    self.physics.set_gravity(b, !prop.gravity);
                }
            }
            "collision_off" | "collision_on" => {
                let on = name == "collision_on";
                self.sandbox.set_collide(ent, on);
                if let Some(b) = body {
                    self.physics.set_world_only(b, !on);
                }
            }
            "keepupright" => {
                let angles = body.map_or(prop.angles, |b| self.physics.state(b).angles.into());
                self.sandbox.set_upright(ent, Some(angles));
            }
            "ignite" => {
                self.ignite(ent, PROPERTY_IGNITE_SECONDS);
            }
            "skin" => {
                self.sandbox.set_skin(ent, arg.parse().unwrap_or(0));
            }
            "bodygroups" => {
                let mut it = arg.split_whitespace();
                let part: usize = it.next().and_then(|p| p.parse().ok()).unwrap_or(0);
                let model = it.next().and_then(|m| m.chars().next()).unwrap_or('0');
                let mut body: Vec<char> = prop.body.chars().collect();
                while body.len() <= part {
                    body.push('0');
                }
                body[part] = model;
                self.sandbox.set_body(ent, body.into_iter().collect());
            }
            "extinguish" => {
                self.extinguish(ent);
            }
            _ => self.sandbox.set_upright(ent, None),
        }
        self.game_events.push(GameEvent::Property {
            player,
            ent,
            name: name.to_string(),
        });
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

    /// Asks the `request_hook`, if any; `Allow` without one.
    fn ask(&mut self, req: Request) -> Verdict {
        let Some(mut hook) = self.request_hook.take() else {
            return Verdict::Allow;
        };
        let v = hook(self, &req);
        if self.request_hook.is_none() {
            self.request_hook = Some(hook);
        }
        v
    }

    /// `noclip` toggled by the player (the game's `noclip` command under `sbox_noclip`): walk <->
    /// fly through everything; refused (false) when the cvar is 0 or the player is dead.
    pub fn set_noclip(&mut self, id: u32, on: bool) -> bool {
        if self.cvar("sbox_noclip").is_some_and(|v| v.trim() == "0") {
            return false;
        }
        let Some(p) = self.players.get_mut(&id) else {
            return false;
        };
        if !p.alive {
            return false;
        }
        let want = if on {
            gm_move::MoveType::Noclip
        } else {
            gm_move::MoveType::Walk
        };
        if p.state.move_type == want {
            return true;
        }
        p.state.move_type = want;
        self.game_events.push(GameEvent::Noclip { player: id, on });
        true
    }

    /// A script plays a sound script at a world position (`sound.play`).
    /// Adds models to a spawn menu category (created at the end when new; duplicates within a
    /// category are dropped). Returns how many models were added.
    pub fn add_spawn_list(&mut self, category: &str, models: &[String]) -> usize {
        let idx = match self.spawn_lists.iter().position(|(c, _)| c == category) {
            Some(i) => i,
            None => {
                self.spawn_lists.push((category.to_owned(), Vec::new()));
                self.spawn_lists.len() - 1
            }
        };
        let list = &mut self.spawn_lists[idx].1;
        let before = list.len();
        for m in models {
            if !m.is_empty() && !list.contains(m) {
                list.push(m.clone());
            }
        }
        let added = list.len() - before;
        if added > 0 || before == 0 {
            self.spawn_lists_version += 1;
        }
        added
    }

    /// Sets (or replaces) the setting schema of a scripted stool; a no-op when unchanged.
    pub fn set_tool_schema(&mut self, tool: &str, settings: Vec<ToolSetting>) {
        match self.tool_schemas.iter_mut().find(|(t, _)| t == tool) {
            Some((_, cur)) if *cur == settings => return,
            Some((_, cur)) => *cur = settings,
            None => self.tool_schemas.push((tool.to_owned(), settings)),
        }
        self.tool_schemas_version += 1;
    }

    pub fn play_sound(&mut self, name: String, pos: Vec3, volume: f32) {
        self.sounds.push(WorldSound { name, pos, volume });
    }

    /// The world sounds of the last tick.
    pub fn take_sounds(&mut self) -> Vec<WorldSound> {
        std::mem::take(&mut self.sounds)
    }

    /// Prop impact sounds of this tick's physics step: the impacting prop's own `impacthard`
    /// script at the contact point, one per prop per tick (its first contact: the crate that
    /// lands at 116 u/s and takes a second 188 u/s contact in the same tick plays 116).
    /// A footstep at `feet`: the wave set is the `stepleft`/`stepright` of the brush side under
    /// the feet (measured `move_step_*`: the server emits the surface's step wave, level 75).
    fn footstep(&mut self, feet: Vec3, step: gm_move::Step) {
        let material = self.collision.ground_material(feet).map(str::to_owned);
        let key = material.clone().unwrap_or_default();
        if !self.step_sounds.contains_key(&key) {
            let loaded = self
                .step_loader
                .as_mut()
                .and_then(|f| f(material.as_deref()));
            self.step_sounds.insert(key.clone(), loaded);
        }
        let Some(Some(sounds)) = self.step_sounds.get(&key) else {
            return;
        };
        let name = if step.right {
            &sounds.right
        } else {
            &sounds.left
        };
        self.sounds.push(WorldSound {
            name: name.clone(),
            pos: feet,
            volume: step.volume,
        });
    }

    fn impact_sounds(&mut self) {
        let impacts = self.physics.take_impacts();
        if impacts.is_empty() {
            return;
        }
        let mut heard: Vec<u32> = Vec::new();
        for imp in impacts {
            let prop_of = |body: BodyHandle| -> Option<u32> {
                if body == self.world_body {
                    return Some(0);
                }
                self.bodies
                    .iter()
                    .find(|(_, b)| **b == body)
                    .map(|(id, _)| *id)
            };
            let ids = [prop_of(imp.bodies[0]), prop_of(imp.bodies[1])];
            for (me, other) in [(ids[0], ids[1]), (ids[1], ids[0])] {
                if let (Some(ent), Some(other)) = (me, other) {
                    if ent != 0 {
                        self.game_events.push(GameEvent::Collide {
                            ent,
                            other,
                            point: imp.point,
                            speed: imp.speed,
                        });
                    }
                }
            }
            let Some(volume) = impact_volume(imp.speed) else {
                continue;
            };
            for body in imp.bodies {
                let Some((&id, _)) = self.bodies.iter().find(|(_, b)| **b == body) else {
                    continue;
                };
                if heard.contains(&id) {
                    continue;
                }
                let Some(model) = self
                    .sandbox
                    .props()
                    .iter()
                    .find(|p| p.id == id)
                    .map(|p| p.model.clone())
                else {
                    continue;
                };
                let Some(name) = self.models.get(&model).and_then(|m| m.impact_hard.clone()) else {
                    continue;
                };
                heard.push(id);
                self.sounds.push(WorldSound {
                    name,
                    pos: imp.point,
                    volume,
                });
            }
        }
    }
}

#[cfg(test)]
mod impact_sound_tests {
    use super::impact_volume;

    /// The game's crate drops (`phys_sound_drop_crate_*`, script volume 0.6).
    #[test]
    fn impact_volume_follows_the_measured_law() {
        let heard = |speed: f32| impact_volume(speed).map(|v| v * 0.6);
        assert_eq!(heard(62.85), None);
        assert_eq!(heard(69.1), None);
        for (speed, vol) in [
            (71.79, 0.0302),
            (80.7, 0.0381),
            (107.32, 0.0674),
            (177.03, 0.1835),
            (292.29, 0.5003),
            (422.78, 0.6),
            (529.61, 0.6),
        ] {
            let got = heard(speed).unwrap();
            assert!((got - vol).abs() < 0.0005, "{speed} u/s: {got} vs {vol}");
        }
    }
}
