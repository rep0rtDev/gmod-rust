//! Messages. A packet carries any number of them; reliable ones are resent until acked.

use crate::wire::{Reader, Truncated, Writer};
use glam::Vec3;

/// One usercmd as sent by the client (66 Hz, the last 8 in every packet).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetCmd {
    pub tick: u32,
    pub buttons: u32,
    pub view_angles: Vec3,
    pub forward_move: f32,
    pub side_move: f32,
    pub up_move: f32,
    pub mouse_x: f32,
    pub mouse_y: f32,
    /// Mouse wheel lines this command (the physgun's hold distance).
    pub wheel: f32,
    /// Active weapon: 0 none, 1 physgun, 2 tool gun.
    pub weapon: u8,
    /// Console `impulse N` fired this command (100 flashlight), 0 none.
    pub impulse: u8,
    /// The server tick the client was rendering when it made this command (lag compensation
    /// rewinds the other players to it when a shot of this command is resolved); 0 = unknown.
    pub seen_tick: u32,
}

/// Sandbox commands (reliable): console commands of the real game.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Spawn {
        model: String,
    },
    Undo,
    Cleanup,
    ToolClick {
        tool: String,
        right: bool,
        reload: bool,
        /// The client's values of the tool's settings (`tool <name> <key> <value>`), handed
        /// to a scripted stool as `ctx.settings`; at most 255 pairs.
        settings: Vec<(String, String)>,
    },
    Cvar {
        name: String,
        value: String,
    },
    /// Admin: hot reload of a loaded addon by id (its files are re-read on the server).
    Reload {
        addon: String,
    },
    /// Admin: drop a player; `ban` also remembers its identity (`ServerNet::bans`).
    Kick {
        player: u32,
        ban: bool,
    },
    /// Admin console line (`status`, `kick`, `ban`, `unban`, `cvar`, `reload`, `say`, `admins`,
    /// `help`); the replies come back as `ServerMsg::Event` lines.
    /// Make a carried weapon the active one (`weapon_pistol`, `gmod_tool`, ...).
    SelectWeapon {
        class: String,
    },
    /// `gm_giveswep`: get (and switch to) a spawnable weapon from the Weapons tab.
    GiveWeapon {
        class: String,
    },
    /// `gm_spawnsent`: spawn a scripted entity from the Entities tab where the player looks.
    SpawnSent {
        class: String,
    },
    /// `gm_spawnswep`: drop a weapon entity where the player looks (Weapons tab middle click).
    SpawnWeapon {
        class: String,
    },
    Console {
        line: String,
    },
    /// Chat line of any player: `player.chat` may cancel or rewrite it, then every welcomed
    /// client gets `<name>: <text>` as an event.
    Chat {
        text: String,
    },
    /// The game's `noclip` command: fly through everything while `sbox_noclip` allows it.
    Noclip {
        on: bool,
    },
    /// A context-menu property (`remove`, `gravity`, `collision_off`, ...) on the prop under
    /// the crosshair.
    Property {
        name: String,
    },
    /// Asks which context-menu properties the prop under the cursor offers right now; the
    /// server answers with [`ServerMsg::Properties`].
    PropertyList,
    /// `editentity`: one row of an editor entity's property sheet (`edit_fog`, ...) changed.
    EditEntity {
        ent: u32,
        key: String,
        value: String,
    },
    /// A numpad key edge (`numpad.Activate` / `Deactivate`): drives the stools bound to it.
    Numpad {
        key: u8,
        down: bool,
    },
    /// `cl_playermodel`: the model name (the game's table) to wear from the next spawn.
    /// `cl_playerskin` / `cl_playerbodygroups`: worn from the next spawn.
    PlayerSkin {
        skin: u8,
        body: String,
    },
    /// The `kill` console command: the player dies and respawns.
    Kill,
    /// `ent_fire <target> <input> [param]`: an input at the map entities `target` names.
    EntFire {
        target: String,
        input: String,
        param: String,
    },
    PlayerModel {
        name: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClientMsg {
    Hello {
        name: String,
        protocol: u16,
        /// Ephemeral X25519 public key: the packet encryption key is derived from it and the
        /// server's one in `Challenge`.
        pubkey: [u8; 32],
    },
    Cmds {
        cmds: Vec<NetCmd>,
    },
    Command {
        seq: u32,
        cmd: Command,
    },
    /// The newest snapshot tick the client holds (the delta baseline for the server).
    /// Reliable: an addon message from a client actor to the server's actors.
    Addon {
        name: String,
        data: Vec<u8>,
    },
    /// Reliable: please stream the published addon package with this sha256.
    WantAddon {
        sha: [u8; 32],
    },
    /// Answer to `ServerMsg::Challenge`: the client's identity (ed25519 public key) and its
    /// signature over `AUTH_PREFIX ++ nonce`.
    Auth {
        pubkey: [u8; 32],
        sig: Vec<u8>,
    },
    /// The client leaves; the server frees the slot at once instead of after the timeout.
    Disconnect,
    AckSnapshot {
        tick: u32,
    },
    /// Unreliable: one 20 ms Opus frame of the player's voice (`gm_audio::voice`).
    Voice {
        seq: u16,
        data: Vec<u8>,
    },
}

/// A constraint's rope (rope, elastic, slider, hydraulic, muscle, winch, pulley segment):
/// endpoints in their entities' frames (entity 0: the world, world space), the rest length,
/// width, material and colour of its `keyframe_rope`.
#[derive(Clone, Debug, PartialEq)]
pub struct RopeSnap {
    pub ent1: u32,
    pub lpos1: Vec3,
    pub ent2: u32,
    pub lpos2: Vec3,
    pub length: f32,
    pub width: f32,
    pub material: String,
    pub color: [u8; 3],
}

/// A `gmod_thruster` that is on with an effect: the client draws it from `offset` (the
/// model-space `GetOffset()`) away from the model's origin.
#[derive(Clone, Debug, PartialEq)]
pub struct ThrusterSnap {
    pub ent: u32,
    pub offset: Vec3,
    pub effect: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityState {
    pub id: u32,
    /// Resolved from `model_id` through the connection's model table (`ServerMsg::Models`);
    /// not on the wire.
    pub model: String,
    /// Index into the model table (0 = none), what the snapshot carries.
    pub model_id: u32,
    pub pos: Vec3,
    pub angles: Vec3,
    /// Source units per second and body-local degrees per second (for interpolation and the
    /// client's extrapolation when a snapshot is late).
    pub vel: Vec3,
    pub angvel: Vec3,
    pub frozen: bool,
    /// `Entity:GetSkin` of the prop.
    pub skin: u8,
    /// `Entity:GetBodyGroups` digits of the prop.
    pub body: String,
    pub owner: u32,
    /// `SetMaterial` override, `SetColor` rgba, render mode and fx (material / colour stools)
    pub material: String,
    pub color: [u8; 4],
    pub render_mode: u8,
    pub render_fx: u8,
    /// `Entity:SetSubMaterial` overrides: (material index, path).
    pub sub_materials: Vec<(u8, String)>,
    /// `Entity:SetModelScale` (render only).
    pub model_scale: f32,
    /// prop_ragdoll: pose of every physics object (`GetPhysicsObjectNum(i)`), empty on a prop.
    pub bones: Vec<BonePose>,
    /// Manipulated model bones (inflator scale, finger poser angles); only the touched ones.
    pub manips: Vec<BoneManip>,
    /// Flex weights and scale (faceposer) of a model with flex controllers.
    pub flex: Option<FlexState>,
    /// Eye target (eyeposer), local to the `eyes` attachment.
    pub eye_target: Option<Vec3>,
    /// Sequence the entity plays (an NPC's AI sequence or a script's `SetSequence`); empty for
    /// props that do not animate.
    pub sequence: String,
    /// The server animator's cycle `[0, 1]` of that sequence at this snapshot and its rate.
    pub cycle: f32,
    pub playback_rate: f32,
    /// The sequence reached its end since it was (re)started.
    pub sequence_finished: bool,
    /// Gesture layer played on top (`Entity:AddGestureSequence`), if any.
    pub gesture: Option<GestureState>,
    /// Pose parameter values a script set, by name (`Entity:SetPoseParameter`).
    pub pose: Vec<(String, f32)>,
    /// Collision bounds (`Entity:GetCollisionBounds`): an NPC's hull, a prop's model box.
    pub mins: Vec3,
    pub maxs: Vec3,
}

/// A gesture layer on the wire (the server's `GestureLayer`).
#[derive(Clone, Debug, PartialEq)]
pub struct GestureState {
    pub sequence: String,
    pub cycle: f32,
    pub rate: f32,
    pub weight: f32,
    pub looping: bool,
    pub autokill: bool,
    pub priority: i32,
    pub finished: bool,
}

pub use gm_server::BonePose;

/// `ManipulateBoneScale` / `ManipulateBoneAngles` of one model bone.
#[derive(Clone, Debug, PartialEq)]
pub struct BoneManip {
    pub bone: u16,
    pub scale: Vec3,
    pub angles: Vec3,
    pub pos: Vec3,
}

/// `SetFlexWeight` per controller and `SetFlexScale`.
#[derive(Clone, Debug, PartialEq)]
pub struct FlexState {
    pub weights: Vec<f32>,
    pub scale: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerSnap {
    pub id: u32,
    pub origin: Vec3,
    pub velocity: Vec3,
    pub view_angles: Vec3,
    pub ducked: bool,
    /// Standing on something (false in the air: jump / fall animation).
    pub on_ground: bool,
    /// `Player:FlashlightIsOn` (impulse 100).
    pub flashlight: bool,
    /// The tool gun's stage (`TOOL:GetStage`), for the HUD's Information rows.
    pub tool_stage: u8,
    /// `gm_server::player_models` index of the player model (255: not in the table).
    pub model: u8,
    /// Skin family and `SetBodyGroups` digits of the player model, as set (unclamped).
    pub skin: u8,
    pub body: String,
    /// The player's name as given at connect (`Player:Nick`).
    pub name: String,
    pub held: Option<u32>,
    /// The physgun grab point on `held` in world space (the beam's end).
    pub held_pos: Vec3,
    /// Gravity gun claws open (its `m_bOpen` element state).
    pub claws: bool,
    /// The prop the player is driving (the `drive` property), if any.
    pub driving: Option<u32>,
    pub alive: bool,
    /// `Player:Frags()` / `Player:Deaths()` for the scoreboard.
    pub frags: u16,
    pub deaths: u16,
    /// Entity of the player's death ragdoll (`hl2mp_ragdoll`), 0 for none; protocol 15.
    pub ragdoll: u32,
    pub health: i16,
    pub armor: i16,
    /// `Player:SetFOV`: the server-set field of view (0: the client's own) and its blend time.
    pub fov: f32,
    pub fov_time: f32,
    /// Weapons carried as a bit per entry of `Server::weapon_classes` (`LOADOUT`, then the
    /// scripted weapons of `ServerMsg::WeaponDefs` in order).
    pub weapons: u64,
    /// Index into `Server::weapon_classes` of the weapon out, 255 for none.
    pub active: u8,
    /// Rounds in the active weapon's clip, -1 without a clip; reserve of its primary ammo.
    pub clip: i16,
    pub reserve: i16,
    /// Secondary ammo (AR2 balls, SMG grenades), -1 for weapons without one.
    pub alt: i16,
    /// Server-decided viewmodel event (`gm_server::weapons::VmAct` as u8) and its serial: the
    /// client plays the sequence when the serial changes.
    pub vm_act: u8,
    pub vm_serial: u8,
    /// The server-side gesture layer (`Player:AnimRestartGesture`), on top of the client pose.
    pub gesture: Option<GestureState>,
}

/// Weapon class names to the registry bitmask and back.
pub fn weapon_mask(names: &[String], classes: &[String]) -> u64 {
    let mut m = 0u64;
    for (i, w) in classes.iter().enumerate().take(64) {
        if names.iter().any(|n| n == w) {
            m |= 1 << i;
        }
    }
    m
}

pub fn weapon_names(mask: u64, classes: &[String]) -> Vec<String> {
    classes
        .iter()
        .enumerate()
        .take(64)
        .filter(|(i, _)| mask & (1 << i) != 0)
        .map(|(_, w)| w.clone())
        .collect()
}

pub fn weapon_index(name: &str, classes: &[String]) -> u8 {
    classes
        .iter()
        .position(|w| w == name)
        .filter(|&i| i < 255)
        .map(|i| i as u8)
        .unwrap_or(255)
}

/// `LOADOUT` followed by the scripted weapon classes a client knows (`WeaponDefs`).
pub fn weapon_classes(defs: &[gm_server::ScriptedWeapon]) -> Vec<String> {
    gm_server::LOADOUT
        .iter()
        .map(|w| w.to_string())
        .chain(defs.iter().map(|d| d.class.clone()))
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct UndoEntry {
    pub id: u32,
    pub name: String,
}

/// The receiving client's own movement state after its last run usercmd, for reconciliation
/// (all of `gm_move::PlayerState`, so a replay from it is bit-exact).
#[derive(Clone, Debug, PartialEq)]
pub struct LocalPlayer {
    pub state: gm_move::PlayerState,
}

/// What the receiving client's own sandbox looks like (count, undo stack): per client.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OwnSandbox {
    pub count: u32,
    pub constraints: u32,
    pub undos: Vec<UndoEntry>,
    pub max_props: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ServerMsg {
    Welcome {
        player_id: u32,
        tick_interval: f32,
        map: String,
        tick: u32,
    },
    /// Delta against `baseline` (0 = full); entities changed since, ids removed since.
    Snapshot {
        tick: u32,
        baseline: u32,
        last_cmd_tick: u32,
        players: Vec<PlayerSnap>,
        entities: Vec<EntityState>,
        ropes: Vec<RopeSnap>,
        thrusters: Vec<ThrusterSnap>,
        /// Named light styles (32..) at their brightness: `light` TurnOn / TurnOff / SetPattern.
        lightstyles: Vec<(u8, f32)>,
        removed: Vec<u32>,
        own: OwnSandbox,
        local: Option<LocalPlayer>,
    },
    /// Reliable: the server tells the client about a sandbox event (LimitHit etc.).
    Event {
        seq: u32,
        text: String,
    },
    CommandAck {
        seq: u32,
    },
    /// Reliable: an addon message from a server actor to the client's actors.
    Addon {
        seq: u32,
        name: String,
        data: Vec<u8>,
    },
    /// Reliable: a replicated console variable (all of them on welcome, then every change).
    Cvar {
        seq: u32,
        name: String,
        value: String,
    },
    /// Reliable, after `Welcome`: the addon packages the server publishes for the client realm.
    Addons {
        list: Vec<AddonInfo>,
    },
    /// Reliable: a piece of a published package a client asked for (`offset` grows in order).
    AddonChunk {
        sha: [u8; 32],
        offset: u64,
        total: u64,
        data: Vec<u8>,
    },
    /// Reliable: the player's undo list whenever it changes (it is not part of the snapshot, a
    /// long list would not fit the packet budget).
    Undos {
        seq: u32,
        entries: Vec<UndoEntry>,
    },
    /// Reply to [`Command::PropertyList`]: the entries offered for `ent` (0: nothing under
    /// the cursor), as `property` command names in the game's menu order.
    Properties {
        ent: u32,
        names: Vec<String>,
    },
    /// Unreliable: a frame of another player's voice, relayed as received.
    Voice {
        player: u32,
        seq: u16,
        data: Vec<u8>,
    },
    /// Reliable: new entries of the connection's model table, `names[i]` is model `from + i`
    /// (ids start at 1); sent before the first snapshot that uses them.
    /// Reliable, right after `Hello`: a random nonce the client signs to prove its identity
    /// (`ClientMsg::Auth`). With `ServerNet::require_auth` the `Welcome` waits for it.
    Challenge {
        nonce: [u8; 32],
        /// The server's ephemeral X25519 public key (see `ClientMsg::Hello`).
        pubkey: [u8; 32],
        /// The server's long-lived ed25519 identity and its signature over
        /// `server_message(nonce, pubkey)`; `None` when the server runs without a key.
        identity: Option<([u8; 32], [u8; 64])>,
    },
    /// Reliable, last message of a connection: refused (`full`, `banned`), kicked or timed out.
    Disconnect {
        reason: String,
    },
    Models {
        from: u32,
        names: Vec<String>,
    },
    /// Unreliable, with the snapshot: sounds the server played in the world this tick (prop
    /// impacts); `volume` multiplies the script's own.
    Sounds {
        tick: u32,
        list: Vec<SoundEvent>,
    },
    /// Unreliable, with the snapshot: visual effects the server dispatched this tick.
    Effects {
        tick: u32,
        list: Vec<EffectEvent>,
    },
    /// Reliable: the addons' spawn menu categories, whole, whenever the server's version changes.
    SpawnLists {
        version: u32,
        lists: Vec<(String, Vec<String>)>,
    },
    /// Reliable: the scripted stools' setting schemas, whole, whenever the version changes.
    ToolSchemas {
        version: u32,
        tools: Vec<(String, Vec<gm_server::ToolSetting>)>,
    },
    /// Reliable: the scripted weapon classes, whole, whenever the version changes; their
    /// wire indices follow `LOADOUT` in this order.
    WeaponDefs {
        version: u32,
        defs: Vec<gm_server::ScriptedWeapon>,
    },
    /// Reliable: the spawn menu's Weapons / Entities content, whole, whenever the server's
    /// version changes.
    ContentLists {
        version: u32,
        weapons: Vec<gm_server::ContentEntry>,
        entities: Vec<gm_server::ContentEntry>,
    },
    /// The scripted entities' world tips (`SetOverlayText`), latest version whole; sent
    /// reliably whenever one changes.
    EntityOverlays {
        version: u32,
        items: Vec<(u32, String)>,
    },
    /// The classes of the scripted / sandbox entities (`sent_ball` draws as a sprite), by
    /// entity id, whole; sent reliably whenever the set changes.
    EntityClasses {
        items: Vec<(u32, String)>,
    },
    /// The editor entities' `Edit` vars (`edit_fog`, `edit_sky`, `edit_sun`) by entity id,
    /// key / value, whole; sent reliably whenever one changes.
    EntityEdits {
        items: EditVars,
    },
}

/// A world sound: script name, where, and a volume factor on the script's own.
#[derive(Clone, Debug, PartialEq)]
pub enum EffectEvent {
    /// Bullet / crowbar hit on the world: decal and impact particles.
    Impact {
        pos: Vec3,
        normal: Vec3,
        surface: String,
        melee: bool,
    },
    /// Tracer from `player`'s muzzle to `end`.
    Tracer {
        player: u32,
        end: Vec3,
    },
    Explosion {
        pos: Vec3,
    },
    Splash {
        pos: Vec3,
    },
    /// `PhyscannonImpact`: `player`'s gravity gun punted at `pos` (blast at the gun, beam to it).
    PhyscannonImpact {
        player: u32,
        pos: Vec3,
    },
    /// The tool gun's shot effect: tracer from `player`'s muzzle to `pos`, selection
    /// indicator in the plane of `normal`.
    ToolTracer {
        player: u32,
        pos: Vec3,
        normal: Vec3,
    },
    /// Bullet hit on a prop: a decal in the entity's frame, `surface` its surface property.
    PropImpact {
        ent: u32,
        lpos: Vec3,
        lnormal: Vec3,
        surface: String,
    },
    /// The remover's `entity_remove`: sparks over the removed entity's world-space box.
    EntityRemove {
        mins: Vec3,
        maxs: Vec3,
    },
    /// A bullet in flesh at `pos` going `dir`: the spray there and the blood decals the
    /// server's `TraceBleed` traces landed (position, normal). `color`: 0 red, 1 yellow,
    /// 2 green.
    Blood {
        pos: Vec3,
        dir: Vec3,
        color: u8,
        decals: Vec<(Vec3, Vec3)>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SoundEvent {
    pub name: String,
    pub pos: Vec3,
    pub volume: f32,
    /// Owner entity (0: the world); the client follows it and keys channel replacement by it.
    pub ent: u32,
    /// Loop until stopped (the game's `CreateSound` patches).
    pub looping: bool,
    /// Stop what `ent` plays instead of starting `name`, fading out over `fade` seconds.
    pub stop: bool,
    pub fade: f32,
    /// Pitch in percent, 0 for the script's own.
    pub pitch: f32,
}

/// A published addon package: identity is the sha256 of the `.gmr` bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AddonInfo {
    pub id: String,
    pub version: String,
    pub sha: [u8; 32],
    pub size: u64,
}

pub fn sha_hex(sha: &[u8; 32]) -> String {
    sha.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha(r: &mut Reader) -> Result<[u8; 32], Truncated> {
    let mut out = [0u8; 32];
    for b in out.iter_mut() {
        *b = r.u8()?;
    }
    Ok(out)
}

/// The editor entities' vars by entity id: key / value pairs (`ServerMsg::EntityEdits`).
pub type EditVars = Vec<(u32, Vec<(String, String)>)>;

pub const PROTOCOL: u16 = 31;

/// Usercmds are coded as deltas against the previous one in the batch (the first against the
/// default): a change mask byte, then only the changed fields. Ticks are consecutive, so only the
/// first tick is sent.
const CMD_BUTTONS: u16 = 1;
const CMD_VIEW: u16 = 2;
const CMD_FORWARD: u16 = 4;
const CMD_SIDE: u16 = 8;
const CMD_UP: u16 = 16;
const CMD_MOUSE: u16 = 32;
const CMD_WEAPON: u16 = 64;
const CMD_SEEN: u16 = 128;
const CMD_IMPULSE: u16 = 256;
const CMD_WHEEL: u16 = 512;

fn w_cmd(w: &mut Writer, c: &NetCmd, prev: &NetCmd) {
    let mut mask = 0u16;
    if c.buttons != prev.buttons {
        mask |= CMD_BUTTONS;
    }
    if c.view_angles != prev.view_angles {
        mask |= CMD_VIEW;
    }
    if c.forward_move != prev.forward_move {
        mask |= CMD_FORWARD;
    }
    if c.side_move != prev.side_move {
        mask |= CMD_SIDE;
    }
    if c.up_move != prev.up_move {
        mask |= CMD_UP;
    }
    if c.mouse_x != prev.mouse_x || c.mouse_y != prev.mouse_y {
        mask |= CMD_MOUSE;
    }
    if c.weapon != prev.weapon {
        mask |= CMD_WEAPON;
    }
    if c.impulse != 0 {
        mask |= CMD_IMPULSE;
    }
    if c.wheel != 0.0 {
        mask |= CMD_WHEEL;
    }
    if c.seen_tick != prev.seen_tick {
        mask |= CMD_SEEN;
    }
    w.u16(mask);
    if mask & CMD_BUTTONS != 0 {
        w.varint(c.buttons as u64);
    }
    if mask & CMD_VIEW != 0 {
        w.vec3(c.view_angles);
    }
    if mask & CMD_FORWARD != 0 {
        w.f32(c.forward_move);
    }
    if mask & CMD_SIDE != 0 {
        w.f32(c.side_move);
    }
    if mask & CMD_UP != 0 {
        w.f32(c.up_move);
    }
    if mask & CMD_MOUSE != 0 {
        w.f32(c.mouse_x);
        w.f32(c.mouse_y);
    }
    if mask & CMD_WEAPON != 0 {
        w.u8(c.weapon);
    }
    if mask & CMD_WHEEL != 0 {
        w.f32(c.wheel);
    }
    if mask & CMD_IMPULSE != 0 {
        w.u8(c.impulse);
    }
    if mask & CMD_SEEN != 0 {
        w.varint(c.seen_tick as u64);
    }
}

fn r_cmd(r: &mut Reader, prev: &NetCmd, tick: u32) -> Result<NetCmd, Truncated> {
    let mask = r.u16()?;
    let mut c = prev.clone();
    c.tick = tick;
    if mask & CMD_BUTTONS != 0 {
        c.buttons = r.varint()? as u32;
    }
    if mask & CMD_VIEW != 0 {
        c.view_angles = r.vec3()?;
    }
    if mask & CMD_FORWARD != 0 {
        c.forward_move = r.f32()?;
    }
    if mask & CMD_SIDE != 0 {
        c.side_move = r.f32()?;
    }
    if mask & CMD_UP != 0 {
        c.up_move = r.f32()?;
    }
    if mask & CMD_MOUSE != 0 {
        c.mouse_x = r.f32()?;
        c.mouse_y = r.f32()?;
    }
    if mask & CMD_WEAPON != 0 {
        c.weapon = r.u8()?;
    }
    c.impulse = if mask & CMD_IMPULSE != 0 { r.u8()? } else { 0 };
    c.wheel = if mask & CMD_WHEEL != 0 { r.f32()? } else { 0.0 };
    if mask & CMD_SEEN != 0 {
        c.seen_tick = r.varint()? as u32;
    }
    Ok(c)
}

fn w_state(w: &mut Writer, s: &gm_move::PlayerState) {
    w.vec3(s.origin);
    w.vec3(s.velocity);
    w.bool(s.on_ground);
    w.vec3(s.ground_normal);
    w.bool(s.ducked);
    w.bool(s.ducking);
    w.bool(s.unducking);
    w.f32(s.duck_timer);
    w.bool(s.jump_held);
    w.bool(s.boost_pending);
    w.f32(s.boost_forward);
    w.f32(s.boost_yaw);
    w.bool(s.ducked_at_tick_start);
    w.f32(s.surface_friction);
    w.u8(match s.move_type {
        gm_move::MoveType::Walk => 0,
        gm_move::MoveType::Noclip => 1,
        gm_move::MoveType::LadderMount => 2,
        gm_move::MoveType::Ladder => 3,
    });
    w.u8(s.water_level);
    w.u16(s.ladder);
    w.u32(s.ground_ent);
    w.f32(s.ground_friction);
    w.f32(s.punch.x);
    w.f32(s.punch.y);
    w.f32(s.punch.z);
    w.f32(s.punch_vel.x);
    w.f32(s.punch_vel.y);
    w.f32(s.punch_vel.z);
}

fn r_state(r: &mut Reader) -> Result<gm_move::PlayerState, Truncated> {
    Ok(gm_move::PlayerState {
        ground_push: false,
        phys_lift: false,
        game_moved: false,
        origin: r.vec3()?,
        velocity: r.vec3()?,
        impulse: glam::Vec3::ZERO,
        on_ground: r.bool()?,
        ground_normal: r.vec3()?,
        ducked: r.bool()?,
        ducking: r.bool()?,
        unducking: r.bool()?,
        duck_timer: r.f32()?,
        jump_held: r.bool()?,
        boost_pending: r.bool()?,
        boost_forward: r.f32()?,
        boost_yaw: r.f32()?,
        ducked_at_tick_start: r.bool()?,
        surface_friction: r.f32()?,
        move_type: match r.u8()? {
            0 => gm_move::MoveType::Walk,
            1 => gm_move::MoveType::Noclip,
            2 => gm_move::MoveType::LadderMount,
            _ => gm_move::MoveType::Ladder,
        },
        water_level: r.u8()?,
        ladder: r.u16()?,
        ground_ent: r.u32()?,
        ground_friction: r.f32()?,
        punch: Vec3::new(r.f32()?, r.f32()?, r.f32()?),
        punch_vel: Vec3::new(r.f32()?, r.f32()?, r.f32()?),
    })
}

impl ClientMsg {
    pub fn reliable(&self) -> bool {
        matches!(
            self,
            ClientMsg::Hello { .. }
                | ClientMsg::Command { .. }
                | ClientMsg::Addon { .. }
                | ClientMsg::WantAddon { .. }
        )
    }

    pub fn write(&self, w: &mut Writer) {
        match self {
            ClientMsg::Hello {
                name,
                protocol,
                pubkey,
            } => {
                w.u8(1);
                w.str(name);
                w.u16(*protocol);
                w.buf.extend_from_slice(pubkey);
            }
            ClientMsg::Cmds { cmds } => {
                w.u8(2);
                w.u8(cmds.len() as u8);
                w.u32(cmds.first().map(|c| c.tick).unwrap_or(0));
                let mut prev = NetCmd::default();
                for c in cmds {
                    w_cmd(w, c, &prev);
                    prev = c.clone();
                }
            }
            ClientMsg::Command { seq, cmd } => {
                w.u8(3);
                w.u32(*seq);
                match cmd {
                    Command::Spawn { model } => {
                        w.u8(1);
                        w.str(model);
                    }
                    Command::Undo => w.u8(2),
                    Command::Cleanup => w.u8(3),
                    Command::ToolClick {
                        tool,
                        right,
                        reload,
                        settings,
                    } => {
                        w.u8(4);
                        w.str(tool);
                        w.bool(*right);
                        w.bool(*reload);
                        let n = settings.len().min(255);
                        w.u8(n as u8);
                        for (k, v) in &settings[..n] {
                            w.str(k);
                            w.str(v);
                        }
                    }
                    Command::Cvar { name, value } => {
                        w.u8(5);
                        w.str(name);
                        w.str(value);
                    }
                    Command::Reload { addon } => {
                        w.u8(6);
                        w.str(addon);
                    }
                    Command::Kick { player, ban } => {
                        w.u8(7);
                        w.u32(*player);
                        w.bool(*ban);
                    }
                    Command::SelectWeapon { class } => {
                        w.u8(9);
                        w.str(class);
                    }
                    Command::GiveWeapon { class } => {
                        w.u8(18);
                        w.str(class);
                    }
                    Command::SpawnSent { class } => {
                        w.u8(19);
                        w.str(class);
                    }
                    Command::SpawnWeapon { class } => {
                        w.u8(20);
                        w.str(class);
                    }
                    Command::Console { line } => {
                        w.u8(8);
                        w.str(line);
                    }
                    Command::Chat { text } => {
                        w.u8(10);
                        w.str(text);
                    }
                    Command::Noclip { on } => {
                        w.u8(11);
                        w.bool(*on);
                    }
                    Command::Property { name } => {
                        w.u8(12);
                        w.str(name);
                    }
                    Command::PlayerModel { name } => {
                        w.u8(13);
                        w.str(name);
                    }
                    Command::PlayerSkin { skin, body } => {
                        w.u8(14);
                        w.u8(*skin);
                        w.str(body);
                    }
                    Command::Kill => w.u8(15),
                    Command::EntFire {
                        target,
                        input,
                        param,
                    } => {
                        w.u8(22);
                        w.str(target);
                        w.str(input);
                        w.str(param);
                    }
                    Command::PropertyList => w.u8(16),
                    Command::EditEntity { ent, key, value } => {
                        w.u8(21);
                        w.u32(*ent);
                        w.str(key);
                        w.str(value);
                    }
                    Command::Numpad { key, down } => {
                        w.u8(17);
                        w.u8(*key);
                        w.bool(*down);
                    }
                }
            }
            ClientMsg::AckSnapshot { tick } => {
                w.u8(4);
                w.u32(*tick);
            }
            ClientMsg::Voice { seq, data } => {
                w.u8(9);
                w.u16(*seq);
                w.bytes(data);
            }
            ClientMsg::Addon { name, data } => {
                w.u8(5);
                w.str(name);
                w.bytes(data);
            }
            ClientMsg::WantAddon { sha } => {
                w.u8(6);
                w.buf.extend_from_slice(sha);
            }
            ClientMsg::Auth { pubkey, sig } => {
                w.u8(7);
                w.buf.extend_from_slice(pubkey);
                w.bytes(sig);
            }
            ClientMsg::Disconnect => w.u8(8),
        }
    }

    pub fn read(r: &mut Reader) -> Result<Self, Truncated> {
        Ok(match r.u8()? {
            1 => ClientMsg::Hello {
                name: r.str()?,
                protocol: r.u16()?,
                pubkey: sha(r)?,
            },
            2 => {
                let n = r.u8()? as usize;
                let first = r.u32()?;
                let mut cmds = Vec::with_capacity(n);
                let mut prev = NetCmd::default();
                for i in 0..n {
                    let c = r_cmd(r, &prev, first + i as u32)?;
                    prev = c.clone();
                    cmds.push(c);
                }
                ClientMsg::Cmds { cmds }
            }
            3 => {
                let seq = r.u32()?;
                let cmd = match r.u8()? {
                    1 => Command::Spawn { model: r.str()? },
                    2 => Command::Undo,
                    3 => Command::Cleanup,
                    4 => {
                        let (tool, right, reload) = (r.str()?, r.bool()?, r.bool()?);
                        let n = r.u8()?;
                        let mut settings = Vec::with_capacity(n as usize);
                        for _ in 0..n {
                            settings.push((r.str()?, r.str()?));
                        }
                        Command::ToolClick {
                            tool,
                            right,
                            reload,
                            settings,
                        }
                    }
                    5 => Command::Cvar {
                        name: r.str()?,
                        value: r.str()?,
                    },
                    6 => Command::Reload { addon: r.str()? },
                    7 => Command::Kick {
                        player: r.u32()?,
                        ban: r.bool()?,
                    },
                    8 => Command::Console { line: r.str()? },
                    9 => Command::SelectWeapon { class: r.str()? },
                    18 => Command::GiveWeapon { class: r.str()? },
                    19 => Command::SpawnSent { class: r.str()? },
                    20 => Command::SpawnWeapon { class: r.str()? },
                    10 => Command::Chat { text: r.str()? },
                    11 => Command::Noclip { on: r.bool()? },
                    12 => Command::Property { name: r.str()? },
                    13 => Command::PlayerModel { name: r.str()? },
                    14 => Command::PlayerSkin {
                        skin: r.u8()?,
                        body: r.str()?,
                    },
                    15 => Command::Kill,
                    16 => Command::PropertyList,
                    21 => Command::EditEntity {
                        ent: r.u32()?,
                        key: r.str()?,
                        value: r.str()?,
                    },
                    22 => Command::EntFire {
                        target: r.str()?,
                        input: r.str()?,
                        param: r.str()?,
                    },
                    17 => Command::Numpad {
                        key: r.u8()?,
                        down: r.bool()?,
                    },
                    _ => return Err(Truncated),
                };
                ClientMsg::Command { seq, cmd }
            }
            4 => ClientMsg::AckSnapshot { tick: r.u32()? },
            9 => ClientMsg::Voice {
                seq: r.u16()?,
                data: r.bytes()?.to_vec(),
            },
            5 => ClientMsg::Addon {
                name: r.str()?,
                data: r.bytes()?.to_vec(),
            },
            6 => ClientMsg::WantAddon { sha: sha(r)? },
            7 => ClientMsg::Auth {
                pubkey: sha(r)?,
                sig: r.bytes()?.to_vec(),
            },
            8 => ClientMsg::Disconnect,
            _ => return Err(Truncated),
        })
    }
}

impl ServerMsg {
    pub fn reliable(&self) -> bool {
        matches!(
            self,
            ServerMsg::Welcome { .. }
                | ServerMsg::Event { .. }
                | ServerMsg::CommandAck { .. }
                | ServerMsg::Addon { .. }
                | ServerMsg::Cvar { .. }
                | ServerMsg::Addons { .. }
                | ServerMsg::AddonChunk { .. }
                | ServerMsg::SpawnLists { .. }
                | ServerMsg::ToolSchemas { .. }
                | ServerMsg::WeaponDefs { .. }
                | ServerMsg::ContentLists { .. }
                | ServerMsg::EntityOverlays { .. }
                | ServerMsg::EntityClasses { .. }
                | ServerMsg::EntityEdits { .. }
        )
    }

    pub fn write(&self, w: &mut Writer) {
        match self {
            ServerMsg::Welcome {
                player_id,
                tick_interval,
                map,
                tick,
            } => {
                w.u8(1);
                w.u32(*player_id);
                w.f32(*tick_interval);
                w.str(map);
                w.u32(*tick);
            }
            ServerMsg::Snapshot {
                tick,
                baseline,
                last_cmd_tick,
                players,
                entities,
                ropes,
                thrusters,
                lightstyles,
                removed,
                own,
                local,
            } => {
                w.u8(2);
                w.u32(*tick);
                w.u32(*baseline);
                w.u32(*last_cmd_tick);
                w.varint(players.len() as u64);
                for p in players {
                    w.u32(p.id);
                    w.qpos(p.origin);
                    w.qvel(p.velocity, VEL_SCALE);
                    w.qang(p.view_angles);
                    w.bool(p.ducked);
                    w.bool(p.on_ground);
                    w.bool(p.flashlight);
                    w.u8(p.tool_stage);
                    w.u8(p.model);
                    w.u8(p.skin);
                    w.str(&p.body);
                    w.str(&p.name);
                    w.u32(p.held.unwrap_or(0));
                    w.vec3(p.held_pos);
                    w.bool(p.claws);
                    w.u32(p.driving.unwrap_or(0));
                    w.bool(p.alive);
                    w.u16(p.frags);
                    w.u16(p.deaths);
                    w.u32(p.ragdoll);
                    w.i16(p.health);
                    w.i16(p.armor);
                    w.f32(p.fov);
                    w.f32(p.fov_time);
                    w.u64(p.weapons);
                    w.u8(p.active);
                    w.i16(p.clip);
                    w.i16(p.reserve);
                    w.i16(p.alt);
                    w.u8(p.vm_act);
                    w.u8(p.vm_serial);
                }
                w.varint(entities.len() as u64);
                for e in entities {
                    w.u32(e.id);
                    w.varint(e.model_id as u64);
                    w.qpos(e.pos);
                    w.qang(e.angles);
                    w.qvel(e.vel, VEL_SCALE);
                    w.qvel(e.angvel, ANGVEL_SCALE);
                    w.bool(e.frozen);
                    w.u8(e.skin);
                    w.str(&e.body);
                    w.u32(e.owner);
                    w.str(&e.material);
                    for c in e.color {
                        w.u8(c);
                    }
                    w.u8(e.render_mode);
                    w.u8(e.render_fx);
                    w.varint(e.sub_materials.len() as u64);
                    for (i, m) in &e.sub_materials {
                        w.u8(*i);
                        w.str(m);
                    }
                    w.f32(e.model_scale);
                    w.varint(e.bones.len() as u64);
                    for b in &e.bones {
                        w.qpos(b.pos);
                        w.qang(b.angles);
                        w.bool(b.frozen);
                    }
                    w.varint(e.manips.len() as u64);
                    for m in &e.manips {
                        w.u16(m.bone);
                        for v in [m.scale, m.angles, m.pos] {
                            w.f32(v.x);
                            w.f32(v.y);
                            w.f32(v.z);
                        }
                    }
                    w.bool(e.flex.is_some());
                    if let Some(f) = &e.flex {
                        w.varint(f.weights.len() as u64);
                        for v in &f.weights {
                            w.f32(*v);
                        }
                        w.f32(f.scale);
                    }
                    w.bool(e.eye_target.is_some());
                    if let Some(t) = e.eye_target {
                        w.f32(t.x);
                        w.f32(t.y);
                        w.f32(t.z);
                    }
                    w.str(&e.sequence);
                    w.f32(e.cycle);
                    w.f32(e.playback_rate);
                    w.bool(e.sequence_finished);
                    w.bool(e.gesture.is_some());
                    if let Some(g) = &e.gesture {
                        w.str(&g.sequence);
                        w.f32(g.cycle);
                        w.f32(g.rate);
                        w.f32(g.weight);
                        w.bool(g.looping);
                        w.bool(g.autokill);
                        w.i32(g.priority);
                        w.bool(g.finished);
                    }
                    w.varint(e.pose.len() as u64);
                    for (n, v) in &e.pose {
                        w.str(n);
                        w.f32(*v);
                    }
                    for v in [e.mins, e.maxs] {
                        w.f32(v.x);
                        w.f32(v.y);
                        w.f32(v.z);
                    }
                }
                w.varint(ropes.len() as u64);
                for r in ropes {
                    w.u32(r.ent1);
                    w.vec3(r.lpos1);
                    w.u32(r.ent2);
                    w.vec3(r.lpos2);
                    w.f32(r.length);
                    w.f32(r.width);
                    w.str(&r.material);
                    w.u8(r.color[0]);
                    w.u8(r.color[1]);
                    w.u8(r.color[2]);
                }
                w.varint(thrusters.len() as u64);
                for t in thrusters {
                    w.u32(t.ent);
                    w.vec3(t.offset);
                    w.str(&t.effect);
                }
                w.varint(lightstyles.len() as u64);
                for (style, brightness) in lightstyles {
                    w.u8(*style);
                    w.f32(*brightness);
                }
                w.varint(removed.len() as u64);
                for id in removed {
                    w.u32(*id);
                }
                w.u32(own.count);
                w.u32(own.constraints);
                w.u32(own.max_props as u32);
                match local {
                    None => w.u8(0),
                    Some(l) => {
                        w.u8(1);
                        w_state(w, &l.state);
                    }
                }
            }
            ServerMsg::Event { seq, text } => {
                w.u8(3);
                w.u32(*seq);
                w.str(text);
            }
            ServerMsg::CommandAck { seq } => {
                w.u8(4);
                w.u32(*seq);
            }
            ServerMsg::Addon { seq, name, data } => {
                w.u8(5);
                w.u32(*seq);
                w.str(name);
                w.bytes(data);
            }
            ServerMsg::Cvar { seq, name, value } => {
                w.u8(6);
                w.u32(*seq);
                w.str(name);
                w.str(value);
            }
            ServerMsg::Addons { list } => {
                w.u8(7);
                w.varint(list.len() as u64);
                for a in list {
                    w.str(&a.id);
                    w.str(&a.version);
                    w.buf.extend_from_slice(&a.sha);
                    w.varint(a.size);
                }
            }
            ServerMsg::AddonChunk {
                sha,
                offset,
                total,
                data,
            } => {
                w.u8(8);
                w.buf.extend_from_slice(sha);
                w.varint(*offset);
                w.varint(*total);
                w.bytes(data);
            }
            ServerMsg::Undos { seq, entries } => {
                w.u8(9);
                w.u32(*seq);
                w.varint(entries.len() as u64);
                for u in entries {
                    w.u32(u.id);
                    w.str(&u.name);
                }
            }
            ServerMsg::Properties { ent, names } => {
                w.u8(16);
                w.u32(*ent);
                w.varint(names.len() as u64);
                for n in names {
                    w.str(n);
                }
            }
            ServerMsg::Disconnect { reason } => {
                w.u8(12);
                w.str(reason);
            }
            ServerMsg::Challenge {
                nonce,
                pubkey,
                identity,
            } => {
                w.u8(11);
                w.buf.extend_from_slice(nonce);
                w.buf.extend_from_slice(pubkey);
                match identity {
                    Some((key, sig)) => {
                        w.u8(1);
                        w.buf.extend_from_slice(key);
                        w.buf.extend_from_slice(sig);
                    }
                    None => w.u8(0),
                }
            }
            ServerMsg::Models { from, names } => {
                w.u8(10);
                w.varint(*from as u64);
                w.varint(names.len() as u64);
                for n in names {
                    w.str(n);
                }
            }
            ServerMsg::SpawnLists { version, lists } => {
                w.u8(14);
                w.u32(*version);
                w.varint(lists.len() as u64);
                for (category, models) in lists {
                    w.str(category);
                    w.varint(models.len() as u64);
                    for m in models {
                        w.str(m);
                    }
                }
            }
            ServerMsg::ContentLists {
                version,
                weapons,
                entities,
            } => {
                w.u8(18);
                w.u32(*version);
                for list in [weapons, entities] {
                    w.varint(list.len() as u64);
                    for e in list {
                        w.str(&e.class);
                        w.str(&e.name);
                        w.str(&e.category);
                    }
                }
            }
            ServerMsg::Voice { player, seq, data } => {
                w.u8(20);
                w.u32(*player);
                w.u16(*seq);
                w.bytes(data);
            }
            ServerMsg::EntityOverlays { version, items } => {
                w.u8(19);
                w.u32(*version);
                w.varint(items.len() as u64);
                for (ent, text) in items {
                    w.u32(*ent);
                    w.str(text);
                }
            }
            ServerMsg::EntityClasses { items } => {
                w.u8(22);
                w.varint(items.len() as u64);
                for (ent, class) in items {
                    w.u32(*ent);
                    w.str(class);
                }
            }
            ServerMsg::EntityEdits { items } => {
                w.u8(23);
                w.varint(items.len() as u64);
                for (ent, vars) in items {
                    w.u32(*ent);
                    w.varint(vars.len() as u64);
                    for (k, v) in vars {
                        w.str(k);
                        w.str(v);
                    }
                }
            }
            ServerMsg::WeaponDefs { version, defs } => {
                w.u8(17);
                w.u32(*version);
                w.varint(defs.len() as u64);
                for d in defs {
                    w.str(&d.class);
                    w.str(&d.print_name);
                    w.u8(d.slot);
                    w.u8(d.slot_pos);
                    w.str(&d.hold_type);
                    w.str(&d.view_model);
                    w.str(&d.world_model);
                    w.i32(d.clip_size);
                    w.i32(d.default_clip);
                    w.str(&d.ammo);
                    w.bool(d.automatic);
                    w.f32(d.primary_delay);
                    w.f32(d.secondary_delay);
                    w.f32(d.reload_time);
                }
            }
            ServerMsg::ToolSchemas { version, tools } => {
                w.u8(15);
                w.u32(*version);
                w.varint(tools.len() as u64);
                for (tool, settings) in tools {
                    w.str(tool);
                    w.u8(settings.len().min(255) as u8);
                    for s in settings.iter().take(255) {
                        w.str(&s.name);
                        w.u8(s.kind.as_u8());
                        w.f32(s.min);
                        w.f32(s.max);
                        w.str(&s.default);
                        w.str(&s.label);
                        w.u8(s.options.len().min(255) as u8);
                        for (l, v) in s.options.iter().take(255) {
                            w.str(l);
                            w.str(v);
                        }
                    }
                }
            }
            ServerMsg::Sounds { tick, list } => {
                w.u8(13);
                w.u32(*tick);
                w.varint(list.len() as u64);
                for s in list {
                    w.str(&s.name);
                    w.vec3(s.pos);
                    w.f32(s.volume);
                    w.varint(s.ent as u64);
                    w.u8(u8::from(s.looping) | (u8::from(s.stop) << 1));
                    w.f32(s.fade);
                    w.f32(s.pitch);
                }
            }
            ServerMsg::Effects { tick, list } => {
                w.u8(21);
                w.u32(*tick);
                w.varint(list.len() as u64);
                for e in list {
                    match e {
                        EffectEvent::Impact {
                            pos,
                            normal,
                            surface,
                            melee,
                        } => {
                            w.u8(0);
                            w.vec3(*pos);
                            w.vec3(*normal);
                            w.str(surface);
                            w.u8(u8::from(*melee));
                        }
                        EffectEvent::Tracer { player, end } => {
                            w.u8(1);
                            w.varint(*player as u64);
                            w.vec3(*end);
                        }
                        EffectEvent::Explosion { pos } => {
                            w.u8(2);
                            w.vec3(*pos);
                        }
                        EffectEvent::Splash { pos } => {
                            w.u8(3);
                            w.vec3(*pos);
                        }
                        EffectEvent::PhyscannonImpact { player, pos } => {
                            w.u8(7);
                            w.varint(*player as u64);
                            w.vec3(*pos);
                        }
                        EffectEvent::ToolTracer {
                            player,
                            pos,
                            normal,
                        } => {
                            w.u8(8);
                            w.varint(*player as u64);
                            w.vec3(*pos);
                            w.vec3(*normal);
                        }
                        EffectEvent::PropImpact {
                            ent,
                            lpos,
                            lnormal,
                            surface,
                        } => {
                            w.u8(4);
                            w.u32(*ent);
                            w.vec3(*lpos);
                            w.vec3(*lnormal);
                            w.str(surface);
                        }
                        EffectEvent::EntityRemove { mins, maxs } => {
                            w.u8(6);
                            w.vec3(*mins);
                            w.vec3(*maxs);
                        }
                        EffectEvent::Blood {
                            pos,
                            dir,
                            color,
                            decals,
                        } => {
                            w.u8(5);
                            w.vec3(*pos);
                            w.vec3(*dir);
                            w.u8(*color);
                            w.varint(decals.len() as u64);
                            for (p, n) in decals {
                                w.vec3(*p);
                                w.vec3(*n);
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn read(r: &mut Reader) -> Result<Self, Truncated> {
        Ok(match r.u8()? {
            1 => ServerMsg::Welcome {
                player_id: r.u32()?,
                tick_interval: r.f32()?,
                map: r.str()?,
                tick: r.u32()?,
            },
            2 => {
                let tick = r.u32()?;
                let baseline = r.u32()?;
                let last_cmd_tick = r.u32()?;
                let n = r.varint()? as usize;
                let mut players = Vec::with_capacity(n);
                for _ in 0..n {
                    players.push(PlayerSnap {
                        id: r.u32()?,
                        origin: r.qpos()?,
                        velocity: r.qvel(VEL_SCALE)?,
                        view_angles: r.qang()?,
                        ducked: r.bool()?,
                        on_ground: r.bool()?,
                        flashlight: r.bool()?,
                        tool_stage: r.u8()?,
                        model: r.u8()?,
                        skin: r.u8()?,
                        body: r.str()?,
                        name: r.str()?,
                        held: match r.u32()? {
                            0 => None,
                            h => Some(h),
                        },
                        held_pos: r.vec3()?,
                        claws: r.bool()?,
                        driving: match r.u32()? {
                            0 => None,
                            h => Some(h),
                        },
                        alive: r.bool()?,
                        frags: r.u16()?,
                        deaths: r.u16()?,
                        ragdoll: r.u32()?,
                        health: r.i16()?,
                        armor: r.i16()?,
                        fov: r.f32()?,
                        fov_time: r.f32()?,
                        weapons: r.u64()?,
                        active: r.u8()?,
                        clip: r.i16()?,
                        reserve: r.i16()?,
                        alt: r.i16()?,
                        vm_act: r.u8()?,
                        vm_serial: r.u8()?,
                        gesture: None,
                    });
                }
                let n = r.varint()? as usize;
                let mut entities = Vec::with_capacity(n);
                for _ in 0..n {
                    entities.push(EntityState {
                        id: r.u32()?,
                        model: String::new(),
                        model_id: r.varint()? as u32,
                        pos: r.qpos()?,
                        angles: r.qang()?,
                        vel: r.qvel(VEL_SCALE)?,
                        angvel: r.qvel(ANGVEL_SCALE)?,
                        frozen: r.bool()?,
                        skin: r.u8()?,
                        body: r.str()?,
                        owner: r.u32()?,
                        material: r.str()?,
                        color: [r.u8()?, r.u8()?, r.u8()?, r.u8()?],
                        render_mode: r.u8()?,
                        render_fx: r.u8()?,
                        sub_materials: {
                            let n = r.varint()? as usize;
                            let mut v = Vec::with_capacity(n);
                            for _ in 0..n {
                                v.push((r.u8()?, r.str()?));
                            }
                            v
                        },
                        model_scale: r.f32()?,
                        bones: {
                            let n = r.varint()? as usize;
                            let mut bones = Vec::with_capacity(n);
                            for _ in 0..n {
                                bones.push(BonePose {
                                    pos: r.qpos()?,
                                    angles: r.qang()?,
                                    frozen: r.bool()?,
                                });
                            }
                            bones
                        },
                        manips: {
                            let n = r.varint()? as usize;
                            let mut manips = Vec::with_capacity(n);
                            for _ in 0..n {
                                manips.push(BoneManip {
                                    bone: r.u16()?,
                                    scale: Vec3::new(r.f32()?, r.f32()?, r.f32()?),
                                    angles: Vec3::new(r.f32()?, r.f32()?, r.f32()?),
                                    pos: Vec3::new(r.f32()?, r.f32()?, r.f32()?),
                                });
                            }
                            manips
                        },
                        flex: if r.bool()? {
                            let n = r.varint()? as usize;
                            let mut weights = Vec::with_capacity(n);
                            for _ in 0..n {
                                weights.push(r.f32()?);
                            }
                            Some(FlexState {
                                weights,
                                scale: r.f32()?,
                            })
                        } else {
                            None
                        },
                        eye_target: if r.bool()? {
                            Some(Vec3::new(r.f32()?, r.f32()?, r.f32()?))
                        } else {
                            None
                        },
                        sequence: r.str()?,
                        cycle: r.f32()?,
                        playback_rate: r.f32()?,
                        sequence_finished: r.bool()?,
                        gesture: if r.bool()? {
                            Some(GestureState {
                                sequence: r.str()?,
                                cycle: r.f32()?,
                                rate: r.f32()?,
                                weight: r.f32()?,
                                looping: r.bool()?,
                                autokill: r.bool()?,
                                priority: r.i32()?,
                                finished: r.bool()?,
                            })
                        } else {
                            None
                        },
                        pose: {
                            let n = r.varint()? as usize;
                            let mut v = Vec::with_capacity(n.min(64));
                            for _ in 0..n {
                                v.push((r.str()?, r.f32()?));
                            }
                            v
                        },
                        mins: Vec3::new(r.f32()?, r.f32()?, r.f32()?),
                        maxs: Vec3::new(r.f32()?, r.f32()?, r.f32()?),
                    });
                }
                let n = r.varint()? as usize;
                let mut ropes = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    ropes.push(RopeSnap {
                        ent1: r.u32()?,
                        lpos1: r.vec3()?,
                        ent2: r.u32()?,
                        lpos2: r.vec3()?,
                        length: r.f32()?,
                        width: r.f32()?,
                        material: r.str()?,
                        color: [r.u8()?, r.u8()?, r.u8()?],
                    });
                }
                let n = r.varint()? as usize;
                let mut thrusters = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    thrusters.push(ThrusterSnap {
                        ent: r.u32()?,
                        offset: r.vec3()?,
                        effect: r.str()?,
                    });
                }
                let n = r.varint()? as usize;
                let mut lightstyles = Vec::with_capacity(n.min(256));
                for _ in 0..n {
                    lightstyles.push((r.u8()?, r.f32()?));
                }
                let n = r.varint()? as usize;
                let mut removed = Vec::with_capacity(n);
                for _ in 0..n {
                    removed.push(r.u32()?);
                }
                let count = r.u32()?;
                let constraints = r.u32()?;
                let max_props = r.u32()? as i32;
                let undos = Vec::new();
                let local = if r.u8()? != 0 {
                    Some(LocalPlayer { state: r_state(r)? })
                } else {
                    None
                };
                ServerMsg::Snapshot {
                    tick,
                    baseline,
                    last_cmd_tick,
                    players,
                    entities,
                    ropes,
                    thrusters,
                    lightstyles,
                    removed,
                    own: OwnSandbox {
                        count,
                        constraints,
                        undos,
                        max_props,
                    },
                    local,
                }
            }
            3 => ServerMsg::Event {
                seq: r.u32()?,
                text: r.str()?,
            },
            4 => ServerMsg::CommandAck { seq: r.u32()? },
            5 => ServerMsg::Addon {
                seq: r.u32()?,
                name: r.str()?,
                data: r.bytes()?.to_vec(),
            },
            6 => ServerMsg::Cvar {
                seq: r.u32()?,
                name: r.str()?,
                value: r.str()?,
            },
            7 => {
                let n = r.varint()? as usize;
                let mut list = Vec::with_capacity(n.min(1024));
                for _ in 0..n {
                    list.push(AddonInfo {
                        id: r.str()?,
                        version: r.str()?,
                        sha: sha(r)?,
                        size: r.varint()?,
                    });
                }
                ServerMsg::Addons { list }
            }
            8 => ServerMsg::AddonChunk {
                sha: sha(r)?,
                offset: r.varint()?,
                total: r.varint()?,
                data: r.bytes()?.to_vec(),
            },
            9 => {
                let seq = r.u32()?;
                let n = r.varint()?.min(4096) as usize;
                let mut entries = Vec::with_capacity(n);
                for _ in 0..n {
                    entries.push(UndoEntry {
                        id: r.u32()?,
                        name: r.str()?,
                    });
                }
                ServerMsg::Undos { seq, entries }
            }
            10 => {
                let from = r.varint()? as u32;
                let n = r.varint()?.min(65536) as usize;
                let mut names = Vec::with_capacity(n);
                for _ in 0..n {
                    names.push(r.str()?);
                }
                ServerMsg::Models { from, names }
            }
            16 => {
                let ent = r.u32()?;
                let n = r.varint()?.min(4096) as usize;
                let mut names = Vec::with_capacity(n);
                for _ in 0..n {
                    names.push(r.str()?);
                }
                ServerMsg::Properties { ent, names }
            }
            13 => {
                let tick = r.u32()?;
                let n = r.varint()?.min(1024) as usize;
                let mut list = Vec::with_capacity(n);
                for _ in 0..n {
                    let name = r.str()?;
                    let pos = r.vec3()?;
                    let volume = r.f32()?;
                    let ent = r.varint()? as u32;
                    let flags = r.u8()?;
                    let fade = r.f32()?;
                    let pitch = r.f32()?;
                    list.push(SoundEvent {
                        name,
                        pos,
                        volume,
                        ent,
                        looping: flags & 1 != 0,
                        stop: flags & 2 != 0,
                        fade,
                        pitch,
                    });
                }
                ServerMsg::Sounds { tick, list }
            }
            14 => {
                let version = r.u32()?;
                let n = r.varint()?.min(256) as usize;
                let mut lists = Vec::with_capacity(n);
                for _ in 0..n {
                    let category = r.str()?;
                    let k = r.varint()?.min(4096) as usize;
                    let mut models = Vec::with_capacity(k);
                    for _ in 0..k {
                        models.push(r.str()?);
                    }
                    lists.push((category, models));
                }
                ServerMsg::SpawnLists { version, lists }
            }
            21 => {
                let tick = r.u32()?;
                let n = r.varint()?.min(4096) as usize;
                let mut list = Vec::with_capacity(n);
                for _ in 0..n {
                    list.push(match r.u8()? {
                        0 => EffectEvent::Impact {
                            pos: r.vec3()?,
                            normal: r.vec3()?,
                            surface: r.str()?,
                            melee: r.u8()? != 0,
                        },
                        1 => EffectEvent::Tracer {
                            player: r.varint()? as u32,
                            end: r.vec3()?,
                        },
                        2 => EffectEvent::Explosion { pos: r.vec3()? },
                        3 => EffectEvent::Splash { pos: r.vec3()? },
                        7 => EffectEvent::PhyscannonImpact {
                            player: r.varint()? as u32,
                            pos: r.vec3()?,
                        },
                        8 => EffectEvent::ToolTracer {
                            player: r.varint()? as u32,
                            pos: r.vec3()?,
                            normal: r.vec3()?,
                        },
                        4 => EffectEvent::PropImpact {
                            ent: r.u32()?,
                            lpos: r.vec3()?,
                            lnormal: r.vec3()?,
                            surface: r.str()?,
                        },
                        5 => {
                            let pos = r.vec3()?;
                            let dir = r.vec3()?;
                            let color = r.u8()?;
                            let n = r.varint()? as usize;
                            let mut decals = Vec::with_capacity(n.min(64));
                            for _ in 0..n {
                                decals.push((r.vec3()?, r.vec3()?));
                            }
                            EffectEvent::Blood {
                                pos,
                                dir,
                                color,
                                decals,
                            }
                        }
                        6 => EffectEvent::EntityRemove {
                            mins: r.vec3()?,
                            maxs: r.vec3()?,
                        },
                        _ => return Err(Truncated),
                    });
                }
                ServerMsg::Effects { tick, list }
            }
            20 => ServerMsg::Voice {
                player: r.u32()?,
                seq: r.u16()?,
                data: r.bytes()?.to_vec(),
            },
            19 => {
                let version = r.u32()?;
                let n = r.varint()?.min(4096) as usize;
                let mut items = Vec::with_capacity(n);
                for _ in 0..n {
                    items.push((r.u32()?, r.str()?));
                }
                ServerMsg::EntityOverlays { version, items }
            }
            22 => {
                let n = r.varint()?.min(4096) as usize;
                let mut items = Vec::with_capacity(n);
                for _ in 0..n {
                    items.push((r.u32()?, r.str()?));
                }
                ServerMsg::EntityClasses { items }
            }
            23 => {
                let n = r.varint()?.min(4096) as usize;
                let mut items = Vec::with_capacity(n);
                for _ in 0..n {
                    let ent = r.u32()?;
                    let k = r.varint()?.min(256) as usize;
                    let mut vars = Vec::with_capacity(k);
                    for _ in 0..k {
                        vars.push((r.str()?, r.str()?));
                    }
                    items.push((ent, vars));
                }
                ServerMsg::EntityEdits { items }
            }
            18 => {
                let version = r.u32()?;
                let mut lists: [Vec<gm_server::ContentEntry>; 2] = [Vec::new(), Vec::new()];
                for list in &mut lists {
                    let n = r.varint()?.min(4096) as usize;
                    for _ in 0..n {
                        list.push(gm_server::ContentEntry {
                            class: r.str()?,
                            name: r.str()?,
                            category: r.str()?,
                        });
                    }
                }
                let [weapons, entities] = lists;
                ServerMsg::ContentLists {
                    version,
                    weapons,
                    entities,
                }
            }
            17 => {
                let version = r.u32()?;
                let n = r.varint()?.min(64) as usize;
                let mut defs = Vec::with_capacity(n);
                for _ in 0..n {
                    defs.push(gm_server::ScriptedWeapon {
                        class: r.str()?,
                        print_name: r.str()?,
                        slot: r.u8()?,
                        slot_pos: r.u8()?,
                        hold_type: r.str()?,
                        view_model: r.str()?,
                        world_model: r.str()?,
                        clip_size: r.i32()?,
                        default_clip: r.i32()?,
                        ammo: r.str()?,
                        automatic: r.bool()?,
                        primary_delay: r.f32()?,
                        secondary_delay: r.f32()?,
                        reload_time: r.f32()?,
                    });
                }
                ServerMsg::WeaponDefs { version, defs }
            }
            15 => {
                let version = r.u32()?;
                let n = r.varint()?.min(1024) as usize;
                let mut tools = Vec::with_capacity(n);
                for _ in 0..n {
                    let tool = r.str()?;
                    let k = r.u8()? as usize;
                    let mut settings = Vec::with_capacity(k);
                    for _ in 0..k {
                        let name = r.str()?;
                        let kind = gm_server::ToolSettingKind::from_u8(r.u8()?);
                        let min = r.f32()?;
                        let max = r.f32()?;
                        let default = r.str()?;
                        let label = r.str()?;
                        let mut options = Vec::new();
                        for _ in 0..r.u8()? {
                            options.push((r.str()?, r.str()?));
                        }
                        settings.push(gm_server::ToolSetting {
                            name,
                            kind,
                            min,
                            max,
                            default,
                            label,
                            options,
                        });
                    }
                    tools.push((tool, settings));
                }
                ServerMsg::ToolSchemas { version, tools }
            }
            11 => {
                let nonce = sha(r)?;
                let pubkey = sha(r)?;
                let identity = if r.u8()? == 1 {
                    let key = sha(r)?;
                    let (a, b) = (sha(r)?, sha(r)?);
                    let mut sig = [0u8; 64];
                    sig[..32].copy_from_slice(&a);
                    sig[32..].copy_from_slice(&b);
                    Some((key, sig))
                } else {
                    None
                };
                ServerMsg::Challenge {
                    nonce,
                    pubkey,
                    identity,
                }
            }
            12 => ServerMsg::Disconnect { reason: r.str()? },
            _ => return Err(Truncated),
        })
    }
}

/// Snapshot quantization: positions 1/32 u, angles 1/64 degree, velocities 1/8 u/s (+-4096),
/// angular velocities 1/4 degree/s (+-8192). The local player's own state stays f32.
pub const VEL_SCALE: f32 = 8.0;
pub const ANGVEL_SCALE: f32 = 4.0;

/// Domain separator of the authentication signature: `AUTH_PREFIX ++ nonce ++ binding`, where
/// `binding` is the hash of this connection's packet encryption key (`crypto::Session::binding`),
/// so a signature only authenticates the connection it was made on.
pub const AUTH_PREFIX: &[u8] = b"gmr-auth1";

/// What a server signs in `ServerMsg::Challenge` to prove its identity: the nonce it issued and
/// its ephemeral key, so a relay cannot reuse the signature with its own key exchange.
pub fn server_message(nonce: &[u8; 32], eph_pubkey: &[u8; 32]) -> Vec<u8> {
    let mut m = b"gmr-srv1".to_vec();
    m.extend_from_slice(nonce);
    m.extend_from_slice(eph_pubkey);
    m
}

pub fn auth_message(nonce: &[u8; 32], binding: &[u8; 32]) -> Vec<u8> {
    let mut m = AUTH_PREFIX.to_vec();
    m.extend_from_slice(nonce);
    m.extend_from_slice(binding);
    m
}
