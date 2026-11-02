//! The host API an actor sees (server realm, v1): `events.on`, `world:get/find/players/queue`,
//! `game.tick`, `print`. Writes are queued and applied in the serial phase.

use crate::snapshot::Snapshot;
use glam::Vec3;
use gm_server::Server;
use mlua::{FromLua, Lua, Result, Table, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

/// Event payload: small typed fields (no Lua values cross actors).
#[derive(Clone, Debug, Default)]
pub struct Payload {
    pub ints: Vec<(&'static str, i64)>,
    pub floats: Vec<(&'static str, f64)>,
    pub vecs: Vec<(&'static str, Vec3)>,
    /// Tool click: the client's tool settings, `ctx.settings[key] = value` (strings).
    pub settings: Vec<(String, String)>,
    pub strs: Vec<(&'static str, String)>,
    /// Tool click: (entity or 0 for the world, position, normal).
    pub hit: Option<(u32, Vec3, Vec3)>,
    pub objects: Vec<(u32, Vec3, Vec3)>,
    /// `net.message`: name and wire payload (`netval`), decoded into `name` / `data`.
    pub net: Option<(String, Vec<u8>)>,
    pub bools: Vec<(&'static str, bool)>,
}

/// What a scripted stool asks the sandbox to do after a click (applied at once, in order).
#[derive(Clone, Debug, PartialEq)]
pub enum ToolOp {
    SetObject,
    Clear,
    /// The scripted stool did something of its own (GLua `TOOL:LeftClick` returned true): the
    /// click counts as fired (effect, `tool.after` done).
    Done,
    Weld {
        ent1: u32,
        ent2: u32,
        forcelimit: f32,
        nocollide: bool,
        undo: String,
    },
    NoCollide {
        ent1: u32,
        ent2: u32,
        undo: String,
    },
    /// `constraint.Rope` between the two stored tool objects (length = their distance now).
    Rope,
    /// `constraint.Axis` between the two stored tool objects (align, 0.2 u offset, hinge).
    Axis,
    /// The remover stool: `ent` (0 = the hit entity) leaves the table, `EntityRemoved` fires.
    Remove {
        ent: u32,
    },
}

/// A write an actor asked for; applied in the serial phase.
#[derive(Clone, Debug, PartialEq)]
pub enum QueuedWrite {
    SetPos {
        ent: u32,
        pos: Vec3,
    },
    SetAngles {
        ent: u32,
        angles: Vec3,
    },
    Freeze {
        ent: u32,
        frozen: bool,
    },
    Remove {
        ent: u32,
    },
    /// `set_overlay_text`: the world tip clients draw when aiming at a scripted entity.
    SetOverlayText {
        ent: u32,
        text: String,
    },
    /// `sound.play`: a sound script at a world position, `volume` multiplies the script's.
    Sound {
        name: String,
        pos: Vec3,
        volume: f32,
        /// Non-zero: the sound belongs to this entity (`Entity:EmitSound`).
        ent: u32,
    },
    /// `spawnmenu.add`: models under a spawn menu category for every client.
    SpawnList {
        category: String,
        models: Vec<String>,
    },
    /// `tool.register` with `settings`: the stool's setting schema for every client.
    ToolSchema {
        tool: String,
        settings: Vec<gm_server::ToolSetting>,
    },
    /// `npc_goto` / `npc_chase` / `npc_stop`: `ent` is the NPC id; the server's own
    /// navigation (measured gaits, no game schedules).
    NpcGoto {
        ent: u32,
        pos: Vec3,
        run: bool,
    },
    NpcChase {
        ent: u32,
        player: u32,
        run: bool,
    },
    NpcStop {
        ent: u32,
    },
    /// `npc_target`: `{player}` engages that player, no value drops the target and keeps the
    /// NPC passive until the next `npc_target`.
    /// `NPC:SetArrivalDistance`: how close the chase mover gets before it stops.
    NpcMaxYaw {
        ent: u32,
        rate: f32,
    },
    NpcArrival {
        ent: u32,
        dist: f32,
    },
    NpcTarget {
        ent: u32,
        player: Option<u32>,
    },
    SetVelocity {
        ent: u32,
        velocity: Vec3,
        angular: Vec3,
    },
    /// `ents.Create("prop_physics")` + `Spawn`: a prop at a pose; `owner` 0 is the server's,
    /// a player id goes through that player's limits and cleanup; `tag` comes back in
    /// `entity.spawned`.
    Spawn {
        owner: u32,
        /// A scripted entity class instead of a plain prop (its model comes from the class).
        class: Option<String>,
        model: String,
        pos: Vec3,
        angles: Vec3,
        frozen: bool,
        tag: String,
        /// `ENT:PhysicsInit*` called before `Spawn`.
        physics: Option<gm_server::PhysShape>,
    },
    /// `ENT:PhysicsInit*` / `PhysicsDestroy` on a live scripted entity.
    EntityPhysics {
        ent: u32,
        shape: gm_server::PhysShape,
    },
    /// `Entity:SetSequence` / `ResetSequence` by sequence name.
    EntitySequence {
        ent: u32,
        name: String,
    },
    /// `Entity:SetCycle`.
    EntityCycle {
        ent: u32,
        cycle: f32,
    },
    /// `Entity:SetPlaybackRate`.
    EntityPlaybackRate {
        ent: u32,
        rate: f32,
    },
    /// `Entity:ResetSequenceInfo`.
    EntityResetSequence {
        ent: u32,
    },
    /// `Entity:FrameAdvance(dt)`.
    EntityFrameAdvance {
        ent: u32,
        dt: f32,
    },
    /// `NPC:MarkTookDamageFromEnemy`.
    NpcTookDamage {
        ent: u32,
    },
    /// `Entity:SetPoseParameter` / `ClearPoseParameters`.
    EntityPose {
        ent: u32,
        name: String,
        value: f32,
    },
    EntityPoseClear {
        ent: u32,
    },
    /// `Entity:SetCollisionBounds`.
    EntityBounds {
        ent: u32,
        mins: glam::Vec3,
        maxs: glam::Vec3,
    },
    /// `Entity:AddGestureSequence` / `SetLayer*` / `RemoveLayer` on the gesture layer.
    EntityGesture {
        ent: u32,
        op: GestureOp,
    },
    SetSkin {
        ent: u32,
        skin: u8,
    },
    /// One body part's model digit (`SetBodyGroups`).
    SetBody {
        ent: u32,
        part: usize,
        model: char,
    },
    SetGravity {
        ent: u32,
        on: bool,
    },
    SetCollide {
        ent: u32,
        on: bool,
    },
    KeepUpright {
        ent: u32,
        on: bool,
    },
    /// `Entity:Ignite(seconds)`.
    Ignite {
        ent: u32,
        seconds: f32,
    },
    Extinguish {
        ent: u32,
    },
    /// `Entity:SetHealth` on a prop (its `prop_data` health).
    SetHealth {
        ent: u32,
        health: i32,
    },
    /// `weapon.register`: a scripted weapon class for the server and every client.
    EntityRegister {
        def: gm_server::ScriptedEntity,
    },
    /// `ENT:NextThink(CurTime() + seconds)` of a scripted entity instance.
    NextThink {
        ent: u32,
        seconds: f32,
    },
    WeaponRegister {
        def: gm_server::ScriptedWeapon,
    },
    /// `fire_bullet` from a scripted weapon's `primary`: hitscan bullets from the player's eyes.
    FireBullet {
        player: u32,
        damage: f32,
        pellets: u8,
        spread: f32,
    },
    /// `constraint.Weld` owned by the server (owner 0: no player limits, no undo entry).
    Weld {
        ent: u32,
        ent2: u32,
        forcelimit: f32,
        nocollide: bool,
        /// 0: owned by the server (no limits, undo or cleanup); a player id: made as that
        /// player's (`sbox_maxconstraints`, an undo entry, the player's cleanup list).
        owner: u32,
    },
    /// `constraint.NoCollide` owned by the server (`ent2` 0 = the world).
    NoCollide {
        ent: u32,
        ent2: u32,
        owner: u32,
    },
    /// `constraint.Rope` / `Axis` / `Ballsocket` / `Slider` / `Elastic`: a constraint record of
    /// that kind between `ent` and `ent2` (0 = the world); `owner` as for `Weld`.
    Constrain {
        ent: u32,
        ent2: u32,
        owner: u32,
        kind: gm_sandbox::ConstraintKind,
    },
    /// `constraint.Motor`: an axis from `lpos1` of `ent` towards `axis_point` (its frame) plus
    /// a motor device driven by the two numpad keys.
    Motor {
        ent: u32,
        ent2: u32,
        owner: u32,
        lpos1: Vec3,
        lpos2: Vec3,
        axis_point: Vec3,
        friction: f32,
        torque: f32,
        forcetime: f32,
        nocollide: bool,
        toggle: bool,
        forcelimit: f32,
        key_f: i32,
        key_r: i32,
        direction: f32,
    },
    PlayerTeleport {
        player: u32,
        pos: Vec3,
    },
    PlayerSetHealth {
        player: u32,
        health: i32,
    },
    PlayerSetArmor {
        player: u32,
        armor: i32,
    },
    /// `Entity:TakeDamage` from the world (`type` = DMG_* bits).
    PlayerDamage {
        player: u32,
        amount: f32,
        dmg_type: u32,
    },
    PlayerKill {
        player: u32,
    },
    PlayerGiveWeapon {
        player: u32,
        class: String,
    },
    PlayerSelectWeapon {
        player: u32,
        class: String,
    },
    /// `None` strips every weapon.
    SetMass {
        ent: u32,
        mass: f32,
    },
    /// `PhysObj:SetInertia` (kg u^2, Source axes).
    SetInertia {
        ent: u32,
        inertia: Vec3,
    },
    SetDamping {
        ent: u32,
        linear: f32,
        angular: f32,
    },
    /// `PhysObj:EnableDrag`: air drag on or off for the entity's body.
    EnableDrag {
        ent: u32,
        on: bool,
    },
    /// `PhysObj:Wake` (`asleep` false) / `Sleep` (true).
    SetAsleep {
        ent: u32,
        asleep: bool,
    },
    /// `Entity:SetParent`; `None` unparents.
    SetParent {
        ent: u32,
        parent: Option<u32>,
    },
    /// Render state parts to replace (`SetColor` / `SetMaterial` / `SetRenderMode` / `SetRenderFX`).
    SetRender {
        ent: u32,
        material: Option<String>,
        color: Option<[u8; 4]>,
        mode: Option<u8>,
        fx: Option<u8>,
    },
    PlayerSetFlashlight {
        player: u32,
        on: bool,
    },
    /// `ViewPunch` (`add`) / `SetViewPunchAngles` / `SetViewPunchVelocity` / `ViewPunchReset`.
    PlayerPunch {
        player: u32,
        angle: Option<Vec3>,
        vel: Option<Vec3>,
        add: bool,
    },
    /// `Player:PickupObject`: carry `ent` as +use would.
    PlayerPickupObject {
        player: u32,
        ent: u32,
    },
    /// `Player:PlayStepSound`.
    PlayerStepSound {
        player: u32,
        volume: f32,
    },
    /// `Player:ExitLadder`.
    PlayerExitLadder {
        player: u32,
    },
    /// `Player:SetEyeAngles`.
    PlayerSetEyeAngles {
        player: u32,
        angles: Vec3,
    },
    /// `Player:EquipSuit` / `RemoveSuit`.
    PlayerSetSuit {
        player: u32,
        on: bool,
    },
    /// `Player:SetWalkSpeed` family: movement tunable `which`, `None` clears it.
    PlayerMoveParam {
        player: u32,
        which: u8,
        value: Option<f32>,
    },
    /// Some addon listens to the move hooks (`Server::move_hooks`).
    MoveHooks {
        on: bool,
    },
    /// `Player:SetLastHitGroup`.
    PlayerSetLastHitGroup {
        player: u32,
        group: u8,
    },
    /// `Player:ShouldDropWeapon`.
    PlayerShouldDropWeapon {
        player: u32,
        on: bool,
    },
    /// `SetFrags` / `SetDeaths`: `None` keeps that counter.
    PlayerSetScore {
        player: u32,
        frags: Option<u32>,
        deaths: Option<u32>,
    },
    /// `ENTITY:UpdateTransmitState` result (`TRANSMIT_*`).
    TransmitMode {
        ent: u32,
        mode: u8,
    },
    /// `Entity:SetPreventTransmit(ply, stop)`.
    PreventTransmit {
        ent: u32,
        player: u32,
        prevent: bool,
    },
    /// `ENT:SetTrigger(on)`.
    SentTrigger {
        ent: u32,
        on: bool,
    },
    /// `ENT:UseTriggerBounds(on, padding)`.
    SentTriggerBounds {
        ent: u32,
        on: bool,
        pad: f32,
    },
    /// `Player:SendHint(name, delay)`.
    PlayerHint {
        player: u32,
        name: String,
        delay: f32,
    },
    /// `Player:ScreenFade(flags, color, time, hold)`.
    PlayerScreenFade {
        player: u32,
        flags: u32,
        color: [u8; 4],
        time: f32,
        hold: f32,
    },
    /// `Player:SetFOV(fov, time)`.
    PlayerFov {
        player: u32,
        fov: f32,
        secs: f32,
    },
    /// `Entity:SetModelScale(scale)`.
    ModelScale {
        ent: u32,
        scale: f32,
    },
    /// `CLuaLocomotion` ops on a scripted NextBot.
    NextBot {
        ent: u32,
        op: gm_server::NextBotOp,
    },
    /// `Player:AnimRestartGesture(slot, act, autokill)`.
    PlayerGestureAct {
        player: u32,
        act: String,
        autokill: bool,
    },
    /// `Player:AnimSetGestureSequence` / `AnimSetGestureWeight`.
    PlayerGestureSet {
        player: u32,
        sequence: Option<String>,
        weight: Option<f32>,
    },
    /// `Player:AnimResetGestureSlot`.
    PlayerGestureReset {
        player: u32,
    },
    /// `Player:LimitHit(kind)`: the client shows the limit notice.
    PlayerLimitHit {
        player: u32,
        limit: String,
    },
    /// `Player:Ban`: dropped with the identity banned for the server's lifetime.
    PlayerBan {
        player: u32,
    },
    PlayerKick {
        player: u32,
    },
    /// `EnterVehicle(ent)` / `ExitVehicle` (`None`).
    /// `Vehicle:SetThrottle` / `SetSteering` / `SetHandbrake` (`None` leaves one alone).
    VehicleControls {
        ent: u32,
        throttle: Option<f32>,
        steer: Option<f32>,
        handbrake: Option<bool>,
    },
    /// `Vehicle:SetBoost`.
    /// `Vehicle:SetMaxThrottle` / `SetSteeringDegrees` / `SetWheelFriction` / `SetSpringLength` / `SetVehicleParams`.
    VehicleTuning {
        ent: u32,
        max_throttle: Option<f32>,
        max_rev_throttle: Option<f32>,
        deg_slow: Option<f32>,
        deg_fast: Option<f32>,
        tyre_mu: Option<f32>,
        max_fwd: Option<f32>,
        max_rev: Option<f32>,
        rest: Option<f32>,
    },
    /// Suspension constants of one wheel (`SetVehicleParams` axles).
    VehicleWheel {
        ent: u32,
        idx: u32,
        spring: Option<f32>,
        damping: Option<f32>,
        damping_compression: Option<f32>,
    },
    VehicleBoost {
        ent: u32,
        boost: f32,
    },
    /// `Vehicle:StartEngine` / `EnableEngine`.
    VehicleEngine {
        ent: u32,
        on: bool,
    },
    PlayerVehicle {
        player: u32,
        vehicle: Option<u32>,
    },
    /// `ManipulateBoneScale` / `ManipulateBoneAngles`: each `Some` replaces that part.
    SetBoneManip {
        ent: u32,
        bone: u32,
        scale: Option<Vec3>,
        angles: Option<Vec3>,
    },
    /// `SetFlexWeight` (`Some(id)`) / `SetFlexScale` (`None`).
    SetFlex {
        ent: u32,
        id: Option<u32>,
        value: f32,
    },
    SetEyeTarget {
        ent: u32,
        target: Option<Vec3>,
    },
    PlayerDropWeapon {
        player: u32,
        class: Option<String>,
    },
    PlayerDropObject {
        player: u32,
    },
    PlayerNoTarget {
        player: u32,
        on: bool,
    },
    /// `SetPlayerColor` / `SetWeaponColor` (0..1 RGB).
    PlayerColors {
        player: u32,
        body: Option<Vec3>,
        weapon: Option<Vec3>,
    },
    ForceDrop {
        ent: u32,
    },
    PlayerLock {
        player: u32,
        locked: bool,
    },
    SetSurface {
        ent: u32,
        friction: Option<f32>,
        elasticity: Option<f32>,
    },
    PlayerMaxArmor {
        player: u32,
        max: i32,
    },
    PlayerSay {
        player: u32,
        text: String,
    },
    /// `NPC:Give` / `SelectWeapon`; empty class drops the weapon.
    NpcWeapon {
        ent: u32,
        class: String,
    },
    /// `NPC:NavSetWanderGoal`: a random navmesh goal `min..max` away.
    NpcWander {
        ent: u32,
        min: f32,
        max: f32,
    },
    /// `NPC:SetMaxLookDistance`.
    NpcSight {
        ent: u32,
        dist: f32,
    },
    /// `NPC:PickupWeapon`: take world weapon `weapon`.
    NpcPickupWeapon {
        ent: u32,
        weapon: u32,
    },
    /// `NPC:IgnoreEnemyUntil`: skip `player` as a target before tick `until`.
    NpcIgnore {
        ent: u32,
        player: u32,
        until: u64,
    },
    /// `Entity:SetBloodColor` (NPCs).
    SetBlood {
        ent: u32,
        blood: i8,
    },
    PlayerStripWeapon {
        player: u32,
        class: Option<String>,
    },
    /// Reserve ammo of one type; `add` accumulates instead of replacing.
    PlayerSetAmmo {
        player: u32,
        ammo: String,
        count: i32,
        add: bool,
    },
    PlayerSetClip {
        player: u32,
        class: String,
        slot: u8,
        count: i32,
    },
    /// `CreateConVar` (rejected when the name exists; needs no capability).
    CreateCvar {
        name: String,
        cvar: gm_server::Cvar,
    },
    /// `ConVar:SetString` on a known variable (rejected when unknown or unchanged).
    SetCvar {
        name: String,
        value: String,
    },
    /// `undo.Create/AddEntity/Finish` for a player: the entities become undoable and cleanable by
    /// that player and count against the player's limits (rejected when the player is unknown
    /// or an entity is gone).
    UndoPush {
        player: u32,
        name: String,
        text: String,
        entities: Vec<u32>,
    },
}

/// A font's layout data as the client measured it (`UiRenderer::font_metrics`): line height
/// and the advance of every glyph it rasterised.
#[derive(Clone, Debug, Default)]
pub struct TextMetrics {
    pub line_height: f32,
    pub advances: HashMap<char, f32>,
    pub fallback: f32,
}

/// Fonts the client has rasterised, by name, `""` for the default font. One client per
/// process, so the table is process-wide; a headless runtime has none and measures 0.
static FONT_METRICS: std::sync::LazyLock<std::sync::RwLock<HashMap<String, TextMetrics>>> =
    std::sync::LazyLock::new(Default::default);

/// The client reports a font's metrics (`""` for the default) for `draw.text_size`.
pub fn set_font_metrics(name: &str, metrics: TextMetrics) {
    FONT_METRICS
        .write()
        .unwrap()
        .insert(name.to_string(), metrics);
}

/// `surface.GetTextSize`: the width of `text` in `font` (the default font for an unknown
/// name) and the line height.
pub fn text_size(text: &str, font: Option<&str>) -> (f32, f32) {
    let table = FONT_METRICS.read().unwrap();
    let Some(m) = font.and_then(|f| table.get(f)).or_else(|| table.get("")) else {
        return (0.0, 0.0);
    };
    let w = text
        .chars()
        .map(|c| m.advances.get(&c).copied().unwrap_or(m.fallback))
        .sum();
    (w, m.line_height)
}

/// A 2D draw command of a client actor's `render.hud` handler, screen pixels, colors 0..1.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawCmd {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: [f32; 4],
    },
    /// `font` is a name given to `draw.font`, the client's default font when `None`.
    Text {
        x: f32,
        y: f32,
        text: String,
        color: [f32; 4],
        font: Option<String>,
    },
    /// `draw.texture`: a VTF (a material's `$basetexture`) over the rectangle, tinted.
    Image {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        material: String,
        color: [f32; 4],
    },
    /// `draw.font(name, { family, size })`: the client rasterises the family at `size`
    /// pixels under `name` (`surface.CreateFont`); emitted once per call, in order.
    Font {
        name: String,
        family: String,
        size: f32,
    },
    /// `draw.start3d2d(pos, angles, scale)`: the 2D calls up to `draw.end3d2d` go onto a plane
    /// in the world (`cam.Start3D2D`): 2D x along the angles' forward, 2D y along their right,
    /// `scale` world units per pixel.
    Begin3D2D {
        pos: Vec3,
        angles: Vec3,
        scale: f32,
    },
    End3D2D,
    /// `draw.model`: a model drawn in the world, translucent (alpha 0..1); tool ghosts.
    Model {
        model: String,
        pos: Vec3,
        angles: Vec3,
        alpha: f32,
    },
}

/// Shared between the Rust side of an actor and the closures its VM calls.
/// Host lookup behind `SoundDuration`: sound length in seconds by name.
pub type SoundDuration = Arc<dyn Fn(&str) -> Option<f32> + Send + Sync>;

pub struct Shared {
    pub snapshot: RwLock<Arc<Snapshot>>,
    pub queue: Mutex<Vec<(u64, QueuedWrite)>>,
    pub queued: Mutex<u64>,
    pub prints: Mutex<Vec<String>>,
    pub can_write: bool,
    /// Outgoing addon messages: (player or 0 for everyone / the server, name, payload).
    pub net_out: Mutex<Vec<(u32, String, Vec<u8>)>>,
    /// This actor's addon and every addon loaded in the runtime (`engine.GetAddons`).
    pub addon: String,
    pub addons: Mutex<Vec<String>>,
    /// `draw.*` calls of the current `render.hud` dispatch (client realm).
    pub draws: Mutex<Vec<DrawCmd>>,
    /// The addon's store when it has the `storage` capability.
    pub store: Option<Arc<Mutex<crate::storage::Store>>>,
    /// `SoundDuration`: sound length by name from the host content, if the host offers it.
    pub sound_duration: Option<SoundDuration>,
}

fn player_table(lua: &Lua, p: &crate::snapshot::PlayerSnap) -> Result<Table> {
    let t = lua.create_table()?;
    t.set("id", p.id)?;
    t.set("origin", vec3_table(lua, p.origin)?)?;
    t.set("velocity", vec3_table(lua, p.velocity)?)?;
    t.set("view_angles", vec3_table(lua, p.view_angles)?)?;
    t.set("held", p.held)?;
    t.set("carry", p.carry)?;
    t.set("water_level", p.water_level)?;
    t.set("ping", p.ping)?;
    t.set("loss", p.loss)?;
    t.set("timeout", p.timeout)?;
    t.set("addr", p.addr.clone())?;
    t.set("alive", p.alive)?;
    t.set("health", p.health)?;
    t.set("armor", p.armor)?;
    t.set("fov", p.fov)?;
    t.set("max_health", p.max_health)?;
    t.set("identity", p.identity.as_deref())?;
    t.set("model", p.model.as_str())?;
    t.set("skin", p.skin)?;
    t.set("body", p.body.as_str())?;
    t.set("weapons", p.weapons.clone())?;
    t.set("active_weapon", p.active_weapon.as_str())?;
    t.set("previous_weapon", p.previous_weapon.as_str())?;
    t.set("buttons", p.buttons)?;
    t.set("buttons_last", p.buttons_last)?;
    let counts = lua.create_table()?;
    for (k, v) in &p.counts {
        counts.set(k.as_str(), *v)?;
    }
    t.set("counts", counts)?;
    t.set("cmd_move", vec3_table(lua, p.cmd_move)?)?;
    t.set("cmd_mouse_x", p.cmd_mouse_x)?;
    t.set("cmd_mouse_y", p.cmd_mouse_y)?;
    t.set("cmd_impulse", p.cmd_impulse)?;
    t.set("cmd_tick", p.cmd_tick)?;
    t.set("on_ground", p.on_ground)?;
    t.set("ducked", p.ducked)?;
    t.set("vehicle", p.vehicle)?;
    t.set("driving", p.driving)?;
    t.set("use_target", p.use_target)?;
    t.set("flashlight", p.flashlight)?;
    t.set("punch", vec3_table(lua, p.punch)?)?;
    t.set("punch_vel", vec3_table(lua, p.punch_vel)?)?;
    t.set("ladder", p.ladder)?;
    t.set("suit", p.suit)?;
    t.set("walk_speed", p.walk_speed)?;
    t.set("ladder_speed", p.ladder_speed)?;
    t.set("lagged_movement", p.lagged_movement)?;
    t.set("sprint_enabled", p.sprint_enabled)?;
    t.set("run_speed", p.run_speed)?;
    t.set("jump_power", p.jump_power)?;
    t.set("step_size", p.step_size)?;
    t.set("crouched_walk_speed", p.crouched_walk_speed)?;
    t.set("duck_time", p.duck_time)?;
    t.set("unduck_time", p.unduck_time)?;
    t.set("max_velocity", p.max_velocity)?;
    t.set("stand_eye", p.stand_eye)?;
    t.set("duck_eye", p.duck_eye)?;
    t.set("hull_half", p.hull_half)?;
    t.set("stand_height", p.stand_height)?;
    t.set("duck_height", p.duck_height)?;
    t.set(
        "ammo",
        p.ammo
            .iter()
            .cloned()
            .collect::<std::collections::HashMap<String, i32>>(),
    )?;
    t.set(
        "clips1",
        p.clips
            .iter()
            .map(|(c, a, _)| (c.clone(), *a))
            .collect::<std::collections::HashMap<String, i32>>(),
    )?;
    t.set(
        "clips2",
        p.clips
            .iter()
            .map(|(c, _, b)| (c.clone(), *b))
            .collect::<std::collections::HashMap<String, i32>>(),
    )?;
    t.set("frags", p.frags)?;
    t.set("deaths", p.deaths)?;
    t.set("max_armor", p.max_armor)?;
    t.set("no_target", p.no_target)?;
    t.set("player_color", vec3_table(lua, p.player_color)?)?;
    t.set("weapon_color", vec3_table(lua, p.weapon_color)?)?;
    t.set("locked", p.locked)?;
    t.set("joined_tick", p.joined_tick)?;
    t.set("last_hitgroup", p.last_hitgroup)?;
    Ok(t)
}

fn cvar_string(v: &Value) -> Result<String> {
    match v {
        Value::String(s) => Ok(s.to_str()?.to_string()),
        Value::Integer(i) => Ok(i.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Boolean(b) => Ok((*b as i32).to_string()),
        other => Err(mlua::Error::runtime(format!(
            "cvar value must be a string, number or boolean, got {}",
            other.type_name()
        ))),
    }
}

/// `{ kind = "model" | "box" | "sphere" | "static" | "none", mins, maxs, radius }`.
/// What a script does to an entity's gesture layer.
#[derive(Clone, Debug, PartialEq)]
pub enum GestureOp {
    Add { sequence: String, autokill: bool },
    Remove,
    Set(gm_server::GesturePatch),
}

/// A field of a table argument (`nil` / not a table -> `None`).
/// A `{x, y, z}` table argument (missing fields are 0).
fn tbl_vec3(v: &Value) -> Result<glam::Vec3> {
    Ok(glam::Vec3::new(
        tbl_field::<f32>(v, "x")?.unwrap_or(0.0),
        tbl_field::<f32>(v, "y")?.unwrap_or(0.0),
        tbl_field::<f32>(v, "z")?.unwrap_or(0.0),
    ))
}

fn tbl_field<T: mlua::FromLua>(v: &Value, key: &str) -> Result<Option<T>> {
    match v {
        Value::Table(t) => t.get::<Option<T>>(key),
        _ => Ok(None),
    }
}

fn f32_arg(v: &Value) -> f32 {
    match v {
        Value::Number(n) => *n as f32,
        Value::Integer(i) => *i as f32,
        _ => 0.0,
    }
}

fn gesture_op(t: &Table) -> Result<GestureOp> {
    let op: String = t.get::<Option<String>>("op")?.unwrap_or_default();
    Ok(match op.as_str() {
        "add" => GestureOp::Add {
            sequence: t.get::<Option<String>>("sequence")?.unwrap_or_default(),
            autokill: t.get::<Option<bool>>("autokill")?.unwrap_or(true),
        },
        "remove" => GestureOp::Remove,
        _ => GestureOp::Set(gm_server::GesturePatch {
            sequence: t.get("sequence")?,
            cycle: t.get("cycle")?,
            rate: t.get("rate")?,
            weight: t.get("weight")?,
            looping: t.get("looping")?,
            autokill: t.get("autokill")?,
            priority: t.get("priority")?,
        }),
    })
}

fn phys_shape(t: &Table) -> Result<gm_server::PhysShape> {
    let kind: String = t.get::<Option<String>>("kind")?.unwrap_or_default();
    Ok(match kind.as_str() {
        "box" => gm_server::PhysShape::Box {
            mins: table_vec3(&t.get::<Table>("mins")?)?,
            maxs: table_vec3(&t.get::<Table>("maxs")?)?,
        },
        "sphere" => gm_server::PhysShape::Sphere {
            radius: t.get::<Option<f32>>("radius")?.unwrap_or(1.0),
        },
        "static" => gm_server::PhysShape::Static,
        "none" => gm_server::PhysShape::None,
        _ => gm_server::PhysShape::Model,
    })
}

fn int_arg(v: &Value) -> Result<i32> {
    match v {
        Value::Integer(i) => Ok(*i as i32),
        Value::Number(n) => Ok(*n as i32),
        other => Err(mlua::Error::runtime(format!(
            "expected a number, got {}",
            other.type_name()
        ))),
    }
}

fn str_arg(v: &Value) -> Result<String> {
    match v {
        Value::String(s) => Ok(s.to_str()?.to_string()),
        other => Err(mlua::Error::runtime(format!(
            "expected a string, got {}",
            other.type_name()
        ))),
    }
}

fn vec3_table(lua: &Lua, v: Vec3) -> Result<Table> {
    let t = lua.create_table()?;
    t.set("x", v.x)?;
    t.set("y", v.y)?;
    t.set("z", v.z)?;
    Ok(t)
}

/// A `{x, y, z}` table argument of a queued write (anything else is the zero vector).
fn value_vec3(v: &Value) -> mlua::Result<Vec3> {
    match v {
        Value::Table(t) => table_vec3(t),
        _ => Ok(Vec3::ZERO),
    }
}

fn table_vec3(t: &Table) -> Result<Vec3> {
    Ok(Vec3::new(t.get("x")?, t.get("y")?, t.get("z")?))
}

fn entity_table(lua: &Lua, e: &crate::snapshot::EntitySnap) -> Result<Table> {
    let t = lua.create_table()?;
    t.set("id", e.id)?;
    t.set("model", e.model.as_str())?;
    t.set("pos", vec3_table(lua, e.pos)?)?;
    t.set("angles", vec3_table(lua, e.angles)?)?;
    t.set("frozen", e.frozen)?;
    t.set("skin", e.skin)?;
    t.set("body", e.body.as_str())?;
    t.set("owner", e.owner)?;
    t.set("class", e.class.as_str())?;
    t.set("health", e.health)?;
    t.set("max_health", e.max_health)?;
    t.set("npc_target", e.npc_target)?;
    t.set("npc_chase", e.npc_chase)?;
    if let Some(g) = e.npc_goal {
        t.set("npc_goal", vec3_table(lua, g)?)?;
    }
    t.set("velocity", vec3_table(lua, e.velocity)?)?;
    t.set("ang_velocity", vec3_table(lua, e.ang_velocity)?)?;
    t.set("mass", e.mass)?;
    t.set("damping_linear", e.damping.0)?;
    t.set("inertia", vec3_table(lua, e.inertia)?)?;
    t.set("damping_angular", e.damping.1)?;
    t.set("asleep", e.asleep)?;
    t.set("drag", e.drag)?;
    t.set("parent", e.parent)?;
    let color = lua.create_table()?;
    color.set("r", e.color[0])?;
    color.set("g", e.color[1])?;
    color.set("b", e.color[2])?;
    color.set("a", e.color[3])?;
    t.set("color", color)?;
    t.set("material", e.material.as_str())?;
    t.set("render_mode", e.render_mode)?;
    t.set("render_fx", e.render_fx)?;
    t.set("sequence", e.sequence.as_str())?;
    t.set("cycle", e.cycle)?;
    t.set("playback_rate", e.playback_rate)?;
    t.set("sequence_finished", e.sequence_finished)?;
    let pose = lua.create_table()?;
    for (n, v) in &e.pose {
        pose.set(n.as_str(), *v)?;
    }
    t.set("pose", pose)?;
    t.set("mins", vec3_table(lua, e.mins)?)?;
    t.set("maxs", vec3_table(lua, e.maxs)?)?;
    if let Some(g) = &e.gesture {
        let gt = lua.create_table()?;
        gt.set("sequence", g.sequence.as_str())?;
        gt.set("cycle", g.cycle)?;
        gt.set("rate", g.rate)?;
        gt.set("weight", g.weight)?;
        gt.set("looping", g.looping)?;
        gt.set("autokill", g.autokill)?;
        gt.set("priority", g.priority)?;
        gt.set("finished", g.finished)?;
        t.set("gesture", gt)?;
    }
    t.set("weapon", e.weapon.as_str())?;
    t.set("blood", e.blood)?;
    t.set("npc_target_seen", e.npc_target_seen)?;
    t.set("npc_target_first_seen", e.npc_target_first_seen)?;
    t.set("npc_stuck", e.npc_stuck)?;
    t.set("npc_chase_reach", e.npc_chase_reach)?;
    t.set("npc_max_yaw", e.npc_max_yaw)?;
    t.set("npc_move_speed", e.npc_move_speed)?;
    if let Some(g) = e.npc_next_goal {
        t.set("npc_next_goal", vec3_table(lua, g)?)?;
    }
    t.set("npc_caps", e.npc_caps)?;
    t.set("model_scale", e.model_scale)?;
    t.set("transmit_block", e.transmit_block.clone())?;
    if let Some(nb) = &e.nextbot {
        let n = lua.create_table()?;
        n.set("desired_speed", nb.desired_speed)?;
        n.set("accel", nb.accel)?;
        n.set("decel", nb.decel)?;
        n.set("step_height", nb.step_height)?;
        n.set("jump_height", nb.jump_height)?;
        n.set("max_yaw_rate", nb.max_yaw_rate)?;
        n.set("gravity", nb.gravity)?;
        n.set("on_ground", nb.on_ground)?;
        n.set("stuck", nb.stuck)?;
        n.set("velocity", vec3_table(lua, nb.velocity)?)?;
        t.set("nextbot", n)?;
    }
    t.set("npc_enemy_damage", e.npc_enemy_damage)?;
    t.set("npc_sight", e.npc_sight)?;
    t.set("friction", e.friction)?;
    t.set("elasticity", e.elasticity)?;
    t.set("on_fire", e.on_fire)?;
    let bs = lua.create_table()?;
    for (b, v) in &e.bone_scales {
        bs.set(*b, vec3_table(lua, *v)?)?;
    }
    t.set("bone_scales", bs)?;
    let ba = lua.create_table()?;
    for (b, v) in &e.bone_angles {
        ba.set(*b, vec3_table(lua, *v)?)?;
    }
    t.set("bone_angles", ba)?;
    if let Some((w, sc)) = &e.flex {
        t.set("flex_weights", w.clone())?;
        t.set("flex_scale", *sc)?;
    }
    if let Some(et) = e.eye_target {
        t.set("eye_target", vec3_table(lua, et)?)?;
    }
    Ok(t)
}

/// The prelude every actor runs: event registry and dispatch in Luau itself.
pub const PRELUDE: &str = r#"
local handlers = {}
events = {}
function events.on(name: string, fn, priority: number?)
    local list = handlers[name]
    if list == nil then list = {}; handlers[name] = list end
    table.insert(list, { fn = fn, priority = priority or 0, order = #list })
    table.sort(list, function(a, b)
        if a.priority ~= b.priority then return a.priority < b.priority end
        return a.order < b.order
    end)
end
-- timers: the game's schedule measured on srcds (script_timers): a timer made in a tick's
-- handlers is due `delay` after that tick, due times accumulate (next due = last due + delay,
-- so a 0.02 s repeater at 0.015 s ticks fires on ticks 2, 3, 4), timers run after the tick.pre
-- handlers and before the server tick, one shot (Simple) timers first in creation order then
-- the named ones in name order; a callback that errors destroys its timer (the error is printed)
timer = {}
local timers = {}
local anon = 0
local seq = 0
local run_id = 0
local function ticks_of(delay: number): number
    local n = delay / game.interval()
    if n < 0 then n = 0 end
    return n
end
function timer.Create(name: string, delay: number, reps: number, fn)
    seq += 1
    local d = ticks_of(delay)
    timers[name] = { fn = fn, delay = d, at = 0, due = d, reps = reps, paused = false, seq = seq, simple = false, born = run_id }
end
function timer.Simple(delay: number, fn)
    anon += 1
    timer.Create("__simple" .. anon, delay, 1, fn)
    timers["__simple" .. anon].simple = true
end
function timer.Exists(name: string): boolean
    return timers[name] ~= nil
end
function timer.Remove(name: string)
    timers[name] = nil
end
timer.Destroy = timer.Remove
function timer.Pause(name: string): boolean
    local t = timers[name]
    if t == nil or t.paused then return false end
    t.paused = true
    return true
end
function timer.UnPause(name: string): boolean
    local t = timers[name]
    if t == nil or not t.paused then return false end
    t.paused = false
    return true
end
function timer.IsPaused(name: string): boolean
    local t = timers[name]
    return t ~= nil and t.paused
end
function timer.Toggle(name: string): boolean
    local t = timers[name]
    if t == nil then return false end
    t.paused = not t.paused
    return t.paused
end
function timer.Start(name: string): boolean
    local t = timers[name]
    if t == nil then return false end
    t.due = t.at + t.delay
    t.paused = false
    return true
end
function timer.Stop(name: string): boolean
    -- GLua: stop and rewind; `Start` runs it from the full delay again
    local t = timers[name]
    if t == nil then return false end
    t.due = t.at + t.delay
    t.paused = true
    return true
end
function timer.Adjust(name: string, delay: number, reps: number?, fn): boolean
    local t = timers[name]
    if t == nil then return false end
    local d = ticks_of(delay)
    t.due = t.due - t.delay + d
    t.delay = d
    if reps ~= nil then t.reps = reps end
    if fn ~= nil then t.fn = fn end
    return true
end
function timer.TimeLeft(name: string): number?
    local t = timers[name]
    if t == nil then return nil end
    return (t.due - t.at) * game.interval()
end
function timer.RepsLeft(name: string): number?
    local t = timers[name]
    if t == nil then return nil end
    return t.reps
end
local function run_timers()
    local due = {}
    for name, t in pairs(timers) do
        if not t.paused then
            -- a timer made during this tick's handlers counts its first tick from now
            if t.born ~= run_id then t.at += 1 end
            if t.at + 1e-6 >= t.due then table.insert(due, name) end
        end
    end
    table.sort(due, function(a, b)
        local ta, tb = timers[a], timers[b]
        if ta.simple ~= tb.simple then return ta.simple end
        if ta.simple then return ta.seq < tb.seq end
        return a < b
    end)
    for _, name in ipairs(due) do
        local t = timers[name]
        if t ~= nil then
            t.due += t.delay
            if t.reps > 0 then
                t.reps -= 1
                if t.reps == 0 then timers[name] = nil end
            end
            local ok, err = pcall(t.fn)
            if not ok then
                timers[name] = nil
                print("timer " .. name .. ": " .. tostring(err))
            end
        end
    end
end
function __dispatch(name: string, payload)
    if name == "tick.pre" then run_id += 1 end
    local list = handlers[name]
    local result = nil
    if list ~= nil then
        for _, h in ipairs(list) do
            local r = h.fn(payload)
            if r == false then result = false; break end
            -- a number answers a request (`player.damage`: the amount to deal); later handlers
            -- of the same actor see it in the payload
            if type(r) == "number" then result = r; payload.amount = r end
            if type(r) == "string" then result = r; payload.text = r end
        end
    end
    -- timers run after the Think handlers and before the server tick, like the game's
    if name == "tick.pre" then run_timers() end
    return result
end
state = state or {}
net = net or {}
hud = hud or {}
-- `hud.register(name, fn)`: fn(screen) runs every frame with { w, h } and draws with `draw.*`
function hud.register(name: string, fn, priority: number?)
    events.on("render.hud", fn, priority)
end
function net.receive(name: string, fn, priority: number?)
    events.on("net.message", function(p)
        if p.name == name then return fn(p) end
    end, priority)
end
spawnmenu = {}
function spawnmenu.add(category: string, models: { string })
    world.queue("spawnmenu.add", 0, { category = category, models = models })
end
entity = {}
local sents = {}
-- a scripted entity class: `def` carries model, think_interval (seconds, optional), use_type
-- ("continuous" default, "simple", "onoff") and the callbacks `init`, `think`, `start_touch`,
-- `touch`, `end_touch`, `use`, `use_off`, `remove`, `damage` (each gets the event payload)
function entity.register(class: string, def)
    sents[class] = def
    world.queue("entity.register", 0, {
        class = class, model = def.model, think = def.think_interval, use_type = def.use_type,
        print_name = def.print_name, category = def.category,
    })
end
for _, kind in ipairs({"init", "think", "start_touch", "touch", "end_touch", "use", "use_off", "remove", "damage"}) do
    events.on("entity." .. kind, function(p)
        local d = sents[p.class]
        if d ~= nil and d[kind] ~= nil then
            local r = d[kind](p)
            -- `think` returning seconds is `self:NextThink(CurTime() + r) return true`
            if kind == "think" and type(r) == "number" then
                world.queue("next_think", p.entity, r)
                return nil
            end
            return r
        end
    end)
end

weapon = {}
local sweps = {}
-- a scripted weapon: `def` carries the ScriptedWeapon fields plus the callbacks `primary`,
-- `secondary`, `reload`, `deploy`, `holster` (each gets the event payload)
function weapon.register(class: string, def)
    sweps[class] = def
    local d = { class = class }
    for _, k in ipairs({ "print_name", "slot", "slot_pos", "hold_type", "view_model", "world_model",
        "clip_size", "default_clip", "ammo", "automatic", "primary_delay", "secondary_delay", "reload_time" }) do
        d[k] = def[k]
    end
    world.queue("weapon.register", 0, d)
end
for _, kind in ipairs({"primary", "secondary", "reload", "deploy", "holster", "think"}) do
    events.on("weapon." .. kind, function(p)
        local d = sweps[p.weapon]
        if d ~= nil and d[kind] ~= nil then return d[kind](p) end
    end)
end
tool = {}
local tools = {}
function tool.register(name: string, def)
    tools[name] = def
    if def.settings ~= nil then
        world.queue("tool.schema", 0, { name = name, settings = def.settings })
    end
end
function __has_tool(name: string)
    return tools[name] ~= nil
end
function __tool(name: string, button: string, ctx)
    local d = tools[name]
    if d == nil then return nil end
    local f = d[button]
    if f == nil then return nil end
    return f(ctx)
end
-- client realm: the selected stool's `ghost(ctx)` runs every frame with the player's trace
-- (`ctx.hit`, `ctx.settings`) and returns { model, pos?, angles?, alpha? } or nil
events.on("render.ghost", function(p)
    local d = tools[p.tool]
    if d == nil or d.ghost == nil then return end
    local g = d.ghost(p)
    if g ~= nil and g.model ~= nil then
        draw.model(g.model, g.pos or p.hit.pos, g.angles, g.alpha)
    end
end)
"#;

/// Installs the sandbox and the host namespaces into a fresh VM.
pub fn install(lua: &Lua, shared: Arc<Shared>) -> Result<()> {
    let g = lua.globals();
    // nothing reaches the process: no os, io, require, loadstring of files
    for k in [
        "os",
        "io",
        "require",
        "loadstring",
        "dofile",
        "loadfile",
        "package",
        "debug",
        "getfenv",
        "setfenv",
    ] {
        g.set(k, Value::Nil)?;
    }
    let sh = shared.clone();
    g.set(
        "print",
        lua.create_function(move |_, args: mlua::Variadic<Value>| {
            let line = args
                .iter()
                .map(|v| match v {
                    Value::String(s) => s.to_string_lossy().to_string(),
                    other => format!("{other:?}"),
                })
                .collect::<Vec<_>>()
                .join("\t");
            sh.prints.lock().unwrap().push(line);
            Ok(())
        })?,
    )?;
    // `draw.rect(x, y, w, h, color?)` / `draw.text(x, y, text, color?)`: color is a table
    // {r, g, b, a?} in 0..255 (the game's Color), white when omitted; the client draws the list
    // after the handler returned, in call order
    let draw = lua.create_table()?;
    let sh = shared.clone();
    draw.set(
        "rect",
        lua.create_function(
            move |_, (x, y, w, h, color): (f32, f32, f32, f32, Option<Table>)| {
                let color = color_of(color.as_ref())?;
                sh.draws
                    .lock()
                    .unwrap()
                    .push(DrawCmd::Rect { x, y, w, h, color });
                Ok(())
            },
        )?,
    )?;
    let sh = shared.clone();
    draw.set(
        "model",
        lua.create_function(
            move |_, (model, pos, angles, alpha): (String, Table, Option<Table>, Option<f32>)| {
                let pos = table_vec3(&pos)?;
                let angles = angles
                    .map(|t| table_vec3(&t))
                    .transpose()?
                    .unwrap_or(Vec3::ZERO);
                let alpha = (alpha.unwrap_or(150.0) / 255.0).clamp(0.0, 1.0);
                sh.draws.lock().unwrap().push(DrawCmd::Model {
                    model,
                    pos,
                    angles,
                    alpha,
                });
                Ok(())
            },
        )?,
    )?;
    let sh = shared.clone();
    draw.set(
        "text",
        lua.create_function(
            move |_, (x, y, text, color, font): (f32, f32, String, Option<Table>, Option<String>)| {
                let color = color_of(color.as_ref())?;
                sh.draws.lock().unwrap().push(DrawCmd::Text {
                    x,
                    y,
                    text,
                    color,
                    font,
                });
                Ok(())
            },
        )?,
    )?;
    // `draw.texture(x, y, w, h, material, color?)`: a VTF stretched over the rectangle
    let sh = shared.clone();
    draw.set(
        "texture",
        lua.create_function(
            move |_, (x, y, w, h, material, color): (f32, f32, f32, f32, String, Option<Table>)| {
                let color = color_of(color.as_ref())?;
                sh.draws.lock().unwrap().push(DrawCmd::Image {
                    x,
                    y,
                    w,
                    h,
                    material,
                    color,
                });
                Ok(())
            },
        )?,
    )?;
    // `draw.start3d2d(pos, angles, scale)` / `draw.end3d2d()`: `cam.Start3D2D` / `cam.End3D2D`
    let sh = shared.clone();
    draw.set(
        "start3d2d",
        lua.create_function(
            move |_, (pos, angles, scale): (Table, Option<Table>, Option<f32>)| {
                let pos = table_vec3(&pos)?;
                let angles = angles
                    .map(|t| table_vec3(&t))
                    .transpose()?
                    .unwrap_or(Vec3::ZERO);
                sh.draws.lock().unwrap().push(DrawCmd::Begin3D2D {
                    pos,
                    angles,
                    scale: scale.unwrap_or(1.0),
                });
                Ok(())
            },
        )?,
    )?;
    let sh = shared.clone();
    draw.set(
        "end3d2d",
        lua.create_function(move |_, ()| {
            sh.draws.lock().unwrap().push(DrawCmd::End3D2D);
            Ok(())
        })?,
    )?;
    // `draw.font(name, { family, size })`: `surface.CreateFont`; the client builds the
    // atlas when the command reaches it, so a font declared at load is ready for the first
    // frame
    let sh = shared.clone();
    draw.set(
        "font",
        lua.create_function(move |_, (name, spec): (String, Table)| {
            let family: String = spec.get::<Option<String>>("family")?.unwrap_or_default();
            let size: f32 = spec.get::<Option<f32>>("size")?.unwrap_or(16.0);
            sh.draws.lock().unwrap().push(DrawCmd::Font {
                name,
                family,
                size: size.clamp(4.0, 256.0),
            });
            Ok(())
        })?,
    )?;
    // `draw.text_size(text, font?) -> w, h`: `surface.GetTextSize` from the metrics the
    // client reported for the font (the default font's until it has)
    draw.set(
        "text_size",
        lua.create_function(move |_, (text, font): (String, Option<String>)| {
            let (w, h) = text_size(&text, font.as_deref());
            Ok((w, h))
        })?,
    )?;
    g.set("draw", draw)?;
    let world = lua.create_table()?;
    let sh = shared.clone();
    world.set(
        "get",
        lua.create_function(move |lua, id: u32| {
            let snap = sh.snapshot.read().unwrap().clone();
            match snap.entities.get(&id) {
                Some(e) => Ok(Value::Table(entity_table(lua, e)?)),
                None => Ok(Value::Nil),
            }
        })?,
    )?;
    let sh = shared.clone();
    world.set(
        "find",
        lua.create_function(move |lua, model: Option<String>| {
            let snap = sh.snapshot.read().unwrap().clone();
            let out = lua.create_table()?;
            for e in snap.entities.values() {
                if model.as_deref().is_none_or(|m| m == e.model) {
                    out.push(entity_table(lua, e)?)?;
                }
            }
            Ok(out)
        })?,
    )?;
    let sh = shared.clone();
    world.set(
        "players",
        lua.create_function(move |lua, ()| {
            let snap = sh.snapshot.read().unwrap().clone();
            let out = lua.create_table()?;
            for p in snap.players.values() {
                out.push(player_table(lua, p)?)?;
            }
            Ok(out)
        })?,
    )?;
    let sh = shared.clone();
    world.set(
        "player",
        lua.create_function(move |lua, id: u32| {
            let snap = sh.snapshot.read().unwrap().clone();
            match snap.players.get(&id) {
                Some(p) => Ok(Value::Table(player_table(lua, p)?)),
                None => Ok(Value::Nil),
            }
        })?,
    )?;
    let sh = shared.clone();
    world.set(
        "count",
        lua.create_function(move |_, ()| {
            let snap = sh.snapshot.read().unwrap().clone();
            Ok((snap.prop_count, snap.constraint_count))
        })?,
    )?;
    let sh = shared.clone();
    world.set(
        "queue",
        lua.create_function(move |lua, (op, ent, arg): (String, u32, Value)| {
            if !sh.can_write {
                return Err(mlua::Error::runtime(
                    "this actor has no world-write capability",
                ));
            }
            let w = match op.as_str() {
                "set_pos" => QueuedWrite::SetPos {
                    ent,
                    pos: table_vec3(&table_arg(&arg)?)?,
                },
                "set_angles" => QueuedWrite::SetAngles {
                    ent,
                    angles: table_vec3(&table_arg(&arg)?)?,
                },
                "freeze" => QueuedWrite::Freeze {
                    ent,
                    frozen: !matches!(arg, Value::Boolean(false)),
                },
                "remove" => QueuedWrite::Remove { ent },
                "npc_goto" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::NpcGoto {
                        ent,
                        pos: table_vec3(&t)?,
                        run: t.get::<Option<bool>>("run")?.unwrap_or(false),
                    }
                }
                "npc_chase" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::NpcChase {
                        ent,
                        player: t.get::<u32>("player")?,
                        run: t.get::<Option<bool>>("run")?.unwrap_or(false),
                    }
                }
                "npc_stop" => QueuedWrite::NpcStop { ent },
                "npc_max_yaw" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::NpcMaxYaw {
                        ent,
                        rate: t.get::<Option<f32>>("rate")?.unwrap_or(0.0),
                    }
                }
                "npc_arrival" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::NpcArrival {
                        ent,
                        dist: t.get::<Option<f32>>("dist")?.unwrap_or(0.0),
                    }
                }
                "npc_target" => QueuedWrite::NpcTarget {
                    ent,
                    player: match &arg {
                        Value::Table(t) => Some(t.get::<u32>("player")?),
                        _ => None,
                    },
                },
                "set_overlay_text" => QueuedWrite::SetOverlayText {
                    ent,
                    text: match &arg {
                        Value::String(s) => s.to_str()?.to_string(),
                        Value::Nil => String::new(),
                        _ => return Err(mlua::Error::runtime("set_overlay_text wants a string")),
                    },
                },
                "spawnmenu.add" => {
                    let t = table_arg(&arg)?;
                    let models: Table = t.get("models")?;
                    QueuedWrite::SpawnList {
                        category: t.get("category")?,
                        models: models
                            .sequence_values::<String>()
                            .collect::<Result<Vec<_>>>()?,
                    }
                }
                "tool.schema" => {
                    let t = table_arg(&arg)?;
                    let list: Table = t.get("settings")?;
                    let mut settings = Vec::new();
                    for s in list.sequence_values::<Table>() {
                        let s = s?;
                        let kind = match s.get::<Option<String>>("type")?.as_deref() {
                            Some("bool") => gm_server::ToolSettingKind::Bool,
                            Some("text") => gm_server::ToolSettingKind::Text,
                            _ => gm_server::ToolSettingKind::Number,
                        };
                        let default = match s.get::<Value>("default")? {
                            Value::Nil => String::new(),
                            Value::Boolean(b) => (b as u8).to_string(),
                            Value::Integer(i) => i.to_string(),
                            Value::Number(n) => n.to_string(),
                            Value::String(s) => s.to_str()?.to_string(),
                            _ => String::new(),
                        };
                        settings.push(gm_server::ToolSetting {
                            name: s.get("name")?,
                            kind,
                            min: s.get::<Option<f32>>("min")?.unwrap_or(f32::MIN),
                            max: s.get::<Option<f32>>("max")?.unwrap_or(f32::MAX),
                            default,
                        });
                    }
                    QueuedWrite::ToolSchema {
                        tool: t.get("name")?,
                        settings,
                    }
                }
                "sound.play" => {
                    let t = table_arg(&arg)?;
                    let pos: Table = t.get("pos")?;
                    QueuedWrite::Sound {
                        name: t.get("name")?,
                        pos: table_vec3(&pos)?,
                        volume: t.get::<Option<f32>>("volume")?.unwrap_or(1.0),
                        ent: t.get::<Option<u32>>("ent")?.unwrap_or(0),
                    }
                }
                "set_velocity" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::SetVelocity {
                        ent,
                        velocity: table_vec3(&t)?,
                        angular: match t.get::<Option<Table>>("angular")? {
                            Some(a) => table_vec3(&a)?,
                            None => Vec3::ZERO,
                        },
                    }
                }
                "next_think" => QueuedWrite::NextThink {
                    ent,
                    seconds: f32::from_lua(arg, lua)?,
                },
                "entity.register" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::EntityRegister {
                        def: gm_server::ScriptedEntity {
                            class: t.get("class")?,
                            model: t.get("model")?,
                            print_name: t.get::<Option<String>>("print_name")?,
                            category: t.get::<Option<String>>("category")?,
                            think: t.get::<Option<f32>>("think")?,
                            use_type: match t.get::<Option<String>>("use_type")?.as_deref() {
                                None | Some("continuous") => gm_server::UseType::Continuous,
                                Some("simple") => gm_server::UseType::Simple,
                                Some("onoff") => gm_server::UseType::OnOff,
                                Some(other) => {
                                    return Err(mlua::Error::runtime(format!(
                                        "entity.register: unknown use_type {other}"
                                    )))
                                }
                            },
                        },
                    }
                }
                "weapon.register" => {
                    let t = table_arg(&arg)?;
                    let class: String = t.get("class")?;
                    if class.is_empty() || class.len() > 64 {
                        return Err(mlua::Error::runtime("weapon.register: bad class"));
                    }
                    let num = |k: &str, d: f32| -> Result<f32> {
                        Ok(t.get::<Option<f32>>(k)?.unwrap_or(d))
                    };
                    QueuedWrite::WeaponRegister {
                        def: gm_server::ScriptedWeapon {
                            print_name: t
                                .get::<Option<String>>("print_name")?
                                .unwrap_or_else(|| class.clone()),
                            slot: t.get::<Option<u8>>("slot")?.unwrap_or(1),
                            slot_pos: t.get::<Option<u8>>("slot_pos")?.unwrap_or(0),
                            hold_type: t
                                .get::<Option<String>>("hold_type")?
                                .unwrap_or_else(|| "pistol".into()),
                            view_model: t
                                .get::<Option<String>>("view_model")?
                                .unwrap_or_else(|| "models/weapons/c_pistol.mdl".into()),
                            world_model: t
                                .get::<Option<String>>("world_model")?
                                .unwrap_or_else(|| "models/weapons/w_pistol.mdl".into()),
                            clip_size: t.get::<Option<i32>>("clip_size")?.unwrap_or(-1),
                            default_clip: t.get::<Option<i32>>("default_clip")?.unwrap_or(-1),
                            ammo: t
                                .get::<Option<String>>("ammo")?
                                .unwrap_or_else(|| "Pistol".into()),
                            automatic: t.get::<Option<bool>>("automatic")?.unwrap_or(false),
                            primary_delay: num("primary_delay", 0.2)?,
                            secondary_delay: num("secondary_delay", 0.5)?,
                            reload_time: num("reload_time", gm_server::SWEP_DEFAULT_RELOAD)?,
                            class,
                        },
                    }
                }
                "fire_bullet" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::FireBullet {
                        player: ent,
                        damage: t.get::<Option<f32>>("damage")?.unwrap_or(0.0),
                        pellets: t.get::<Option<u8>>("pellets")?.unwrap_or(1),
                        spread: t.get::<Option<f32>>("spread")?.unwrap_or(0.0),
                    }
                }
                "spawn" => {
                    let t = table_arg(&arg)?;
                    let pos: Table = t.get("pos")?;
                    QueuedWrite::Spawn {
                        owner: t.get::<Option<u32>>("owner")?.unwrap_or(0),
                        class: t.get::<Option<String>>("class")?,
                        model: t.get::<Option<String>>("model")?.unwrap_or_default(),
                        pos: table_vec3(&pos)?,
                        angles: match t.get::<Option<Table>>("angles")? {
                            Some(a) => table_vec3(&a)?,
                            None => Vec3::ZERO,
                        },
                        frozen: t.get::<Option<bool>>("frozen")?.unwrap_or(false),
                        tag: t.get::<Option<String>>("tag")?.unwrap_or_default(),
                        physics: t
                            .get::<Option<Table>>("physics")?
                            .map(|p| phys_shape(&p))
                            .transpose()?,
                    }
                }
                "set_skin" => QueuedWrite::SetSkin {
                    ent,
                    skin: u8::try_from(i64::from_lua(arg, lua)?).unwrap_or(u8::MAX),
                },
                "set_body" => {
                    let t = table_arg(&arg)?;
                    let model: i64 = t.get("model")?;
                    QueuedWrite::SetBody {
                        ent,
                        part: t.get::<Option<usize>>("part")?.unwrap_or(0),
                        model: char::from_digit(model.clamp(0, 10) as u32, 10).unwrap_or('a'),
                    }
                }
                "set_gravity" => QueuedWrite::SetGravity {
                    ent,
                    on: bool::from_lua(arg, lua)?,
                },
                "set_collide" => QueuedWrite::SetCollide {
                    ent,
                    on: bool::from_lua(arg, lua)?,
                },
                "keep_upright" => QueuedWrite::KeepUpright {
                    ent,
                    on: bool::from_lua(arg, lua)?,
                },
                "ignite" => QueuedWrite::Ignite {
                    ent,
                    seconds: match arg {
                        Value::Nil => gm_server::PROPERTY_IGNITE_SECONDS,
                        v => f32::from_lua(v, lua)?,
                    },
                },
                "extinguish" => QueuedWrite::Extinguish { ent },
                "set_health" => QueuedWrite::SetHealth {
                    ent,
                    health: i64::from_lua(arg, lua)?.clamp(0, i32::MAX as i64) as i32,
                },
                "weld" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::Weld {
                        ent,
                        ent2: t.get::<Option<u32>>("ent2")?.unwrap_or(0),
                        forcelimit: t.get::<Option<f32>>("forcelimit")?.unwrap_or(0.0),
                        nocollide: t.get::<Option<bool>>("nocollide")?.unwrap_or(false),
                        owner: t.get::<Option<u32>>("owner")?.unwrap_or(0),
                    }
                }
                "nocollide" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::NoCollide {
                        ent,
                        ent2: t.get::<Option<u32>>("ent2")?.unwrap_or(0),
                        owner: t.get::<Option<u32>>("owner")?.unwrap_or(0),
                    }
                }
                "constrain" => {
                    let t = table_arg(&arg)?;
                    let v3 = |n: &str| -> Result<Vec3> {
                        Ok(match t.get::<Option<Table>>(n)? {
                            Some(v) => Vec3::new(
                                v.get::<Option<f32>>("x")?.unwrap_or(0.0),
                                v.get::<Option<f32>>("y")?.unwrap_or(0.0),
                                v.get::<Option<f32>>("z")?.unwrap_or(0.0),
                            ),
                            None => Vec3::ZERO,
                        })
                    };
                    let num = |n: &str, d: f32| -> Result<f32> {
                        Ok(t.get::<Option<f32>>(n)?.unwrap_or(d))
                    };
                    let flag = |n: &str| -> Result<bool> {
                        Ok(t.get::<Option<bool>>(n)?.unwrap_or(false))
                    };
                    let text = |n: &str, d: &str| -> Result<String> {
                        Ok(t.get::<Option<String>>(n)?.unwrap_or_else(|| d.to_string()))
                    };
                    use gm_sandbox::ConstraintKind as K;
                    let kind = match t.get::<Option<String>>("kind")?.as_deref() {
                        Some("Rope") => K::Rope {
                            lpos1: v3("lpos1")?,
                            lpos2: v3("lpos2")?,
                            length: num("length", 0.0)?,
                            addlength: num("addlength", 0.0)?,
                            width: num("width", 2.0)?,
                            material: text("material", "cable/rope")?,
                            rigid: flag("rigid")?,
                        },
                        Some("Axis") => K::Axis {
                            lpos1: v3("lpos1")?,
                            lpos2: v3("lpos2")?,
                            local_axis: v3("localaxis")?,
                            forcelimit: num("forcelimit", 0.0)?,
                            torquelimit: num("torquelimit", 0.0)?,
                            friction: num("friction", 0.0)?,
                            nocollide: flag("nocollide")?,
                        },
                        Some("Ballsocket") => K::Ballsocket {
                            lpos: v3("lpos")?,
                            forcelimit: num("forcelimit", 0.0)?,
                            torquelimit: num("torquelimit", 0.0)?,
                            nocollide: flag("nocollide")?,
                        },
                        Some("Slider") => K::Slider {
                            lpos1: v3("lpos1")?,
                            lpos2: v3("lpos2")?,
                            width: num("width", 1.5)?,
                            material: text("material", "cable/cable")?,
                            part_of: None,
                        },
                        Some("Elastic") => K::Elastic {
                            lpos1: v3("lpos1")?,
                            lpos2: v3("lpos2")?,
                            constant: num("constant", 500.0)?,
                            damping: num("damping", 3.0)?,
                            rdamping: num("rdamping", 0.01)?,
                            width: num("width", 2.0)?,
                            material: text("material", "cable/cable")?,
                            length: num("length", 0.0)?,
                            stretchonly: flag("stretchonly")?,
                        },
                        Some("Pulley") => K::Pulley {
                            lpos1: v3("lpos1")?,
                            lpos4: v3("lpos4")?,
                            wpos2: v3("wpos2")?,
                            wpos3: v3("wpos3")?,
                            forcelimit: num("forcelimit", 0.0)?,
                            rigid: flag("rigid")?,
                            width: num("width", 2.0)?,
                            material: text("material", "cable/rope")?,
                            addlength: num("addlength", 0.0)?,
                        },
                        Some("Hydraulic") => {
                            let (l1, l2) = (num("length1", 0.0)?, num("length2", 0.0)?);
                            let (speed, toggle) = (num("speed", 64.0)?, flag("toggle")?);
                            K::Hydraulic {
                                lpos1: v3("lpos1")?,
                                lpos2: v3("lpos2")?,
                                length1: l1,
                                length2: l2,
                                width: num("width", 3.0)?,
                                material: text("material", "cable/rope")?,
                                key: num("key", -1.0)? as i32,
                                fixed: flag("fixed")?,
                                speed,
                                toggle,
                                constant: 0.0,
                                damping: 0.0,
                                controller: gm_sandbox::LengthController::new(
                                    l1,
                                    l1.min(l2),
                                    Some(l1.max(l2)),
                                    speed,
                                    speed,
                                    toggle,
                                ),
                            }
                        }
                        Some("Winch") => {
                            let (fwd, bwd) = (num("fwd_speed", 64.0)?, num("bwd_speed", 64.0)?);
                            let toggle = flag("toggle")?;
                            K::Winch {
                                lpos1: v3("lpos1")?,
                                lpos2: v3("lpos2")?,
                                width: num("width", 3.0)?,
                                material: text("material", "cable/rope")?,
                                fwd_key: num("fwd_key", -1.0)? as i32,
                                bwd_key: num("bwd_key", -1.0)? as i32,
                                fwd_speed: fwd,
                                bwd_speed: bwd,
                                toggle,
                                constant: 0.0,
                                damping: 0.0,
                                controller: gm_sandbox::LengthController::new(
                                    num("length", 0.0)?,
                                    1.0,
                                    None,
                                    fwd,
                                    bwd,
                                    toggle,
                                ),
                            }
                        }
                        Some("Muscle") => {
                            let (l1, l2) = (num("length1", 0.0)?, num("length2", 0.0)?);
                            let (period, amplitude) = (num("period", 1.0)?, num("amplitude", 0.0)?);
                            let starton = flag("starton")?;
                            K::Muscle {
                                lpos1: v3("lpos1")?,
                                lpos2: v3("lpos2")?,
                                length1: l1,
                                length2: l2,
                                width: num("width", 3.0)?,
                                material: text("material", "cable/rope")?,
                                key: num("key", -1.0)? as i32,
                                fixed: flag("fixed")?,
                                period,
                                amplitude,
                                starton,
                                constant: 0.0,
                                damping: 0.0,
                                controller: gm_sandbox::LengthController::muscle(
                                    l1, l1, l2, period, amplitude, starton,
                                ),
                            }
                        }
                        _ => return Err(mlua::Error::runtime("unknown constraint kind")),
                    };
                    QueuedWrite::Constrain {
                        ent,
                        ent2: t.get::<Option<u32>>("ent2")?.unwrap_or(0),
                        owner: t.get::<Option<u32>>("owner")?.unwrap_or(0),
                        kind,
                    }
                }
                "motor" => {
                    let t = table_arg(&arg)?;
                    let v3 = |n: &str| -> Result<Vec3> {
                        Ok(match t.get::<Option<Table>>(n)? {
                            Some(v) => Vec3::new(
                                v.get::<Option<f32>>("x")?.unwrap_or(0.0),
                                v.get::<Option<f32>>("y")?.unwrap_or(0.0),
                                v.get::<Option<f32>>("z")?.unwrap_or(0.0),
                            ),
                            None => Vec3::ZERO,
                        })
                    };
                    let num = |n: &str, d: f32| -> Result<f32> {
                        Ok(t.get::<Option<f32>>(n)?.unwrap_or(d))
                    };
                    QueuedWrite::Motor {
                        ent,
                        ent2: t.get::<Option<u32>>("ent2")?.unwrap_or(0),
                        owner: t.get::<Option<u32>>("owner")?.unwrap_or(0),
                        lpos1: v3("lpos1")?,
                        lpos2: v3("lpos2")?,
                        axis_point: v3("axis_point")?,
                        friction: num("friction", 0.0)?,
                        torque: num("torque", 0.0)?,
                        forcetime: num("forcetime", 0.0)?,
                        nocollide: t.get::<Option<bool>>("nocollide")?.unwrap_or(false),
                        toggle: t.get::<Option<bool>>("toggle")?.unwrap_or(false),
                        forcelimit: num("forcelimit", 0.0)?,
                        key_f: num("key_f", -1.0)? as i32,
                        key_r: num("key_r", -1.0)? as i32,
                        direction: num("direction", 1.0)?,
                    }
                }
                "undo.push" => {
                    let t = table_arg(&arg)?;
                    let name = t
                        .get::<Option<String>>("name")?
                        .unwrap_or_else(|| "Prop".into());
                    QueuedWrite::UndoPush {
                        player: ent,
                        text: t
                            .get::<Option<String>>("text")?
                            .unwrap_or_else(|| format!("#tool.{}.name", name.to_lowercase())),
                        name,
                        entities: t.get::<Option<Vec<u32>>>("entities")?.unwrap_or_default(),
                    }
                }
                "player.teleport" => QueuedWrite::PlayerTeleport {
                    player: ent,
                    pos: table_vec3(&table_arg(&arg)?)?,
                },
                "player.set_health" => QueuedWrite::PlayerSetHealth {
                    player: ent,
                    health: int_arg(&arg)?,
                },
                "player.set_armor" => QueuedWrite::PlayerSetArmor {
                    player: ent,
                    armor: int_arg(&arg)?,
                },
                "player.damage" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::PlayerDamage {
                        player: ent,
                        amount: t.get::<Option<f32>>("amount")?.unwrap_or(0.0),
                        dmg_type: t.get::<Option<u32>>("type")?.unwrap_or(0),
                    }
                }
                "player.kill" => QueuedWrite::PlayerKill { player: ent },
                "player.give_weapon" => QueuedWrite::PlayerGiveWeapon {
                    player: ent,
                    class: str_arg(&arg)?,
                },
                "player.select_weapon" => QueuedWrite::PlayerSelectWeapon {
                    player: ent,
                    class: str_arg(&arg)?,
                },
                "set_mass" => QueuedWrite::SetMass {
                    ent,
                    mass: match &arg {
                        Value::Number(n) => *n as f32,
                        Value::Integer(i) => *i as f32,
                        _ => return Err(mlua::Error::runtime("set_mass needs a number")),
                    },
                },
                "set_inertia" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::SetInertia {
                        ent,
                        inertia: Vec3::new(
                            t.get::<f32>("x")?,
                            t.get::<f32>("y")?,
                            t.get::<f32>("z")?,
                        ),
                    }
                }
                "set_damping" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::SetDamping {
                        ent,
                        linear: t.get::<f32>("linear")?,
                        angular: t.get::<f32>("angular")?,
                    }
                }
                "enable_drag" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::EnableDrag {
                        ent,
                        on: t.get::<Option<bool>>("on")?.unwrap_or(true),
                    }
                }
                "set_asleep" => QueuedWrite::SetAsleep {
                    ent,
                    asleep: matches!(arg, Value::Boolean(true)),
                },
                "set_parent" => QueuedWrite::SetParent {
                    ent,
                    parent: match &arg {
                        Value::Nil => None,
                        Value::Integer(i) => Some(*i as u32),
                        Value::Number(n) => Some(*n as u32),
                        _ => return Err(mlua::Error::runtime("set_parent needs an entity id")),
                    },
                },
                "set_color" => {
                    let t = table_arg(&arg)?;
                    let c = |k: &str, d: u8| -> mlua::Result<u8> {
                        Ok(t.get::<Option<f64>>(k)?
                            .map_or(d, |v| v.clamp(0.0, 255.0) as u8))
                    };
                    QueuedWrite::SetRender {
                        ent,
                        material: None,
                        color: Some([c("r", 255)?, c("g", 255)?, c("b", 255)?, c("a", 255)?]),
                        mode: None,
                        fx: None,
                    }
                }
                "set_material" => QueuedWrite::SetRender {
                    ent,
                    material: Some(str_arg(&arg)?),
                    color: None,
                    mode: None,
                    fx: None,
                },
                "set_render_mode" => QueuedWrite::SetRender {
                    ent,
                    material: None,
                    color: None,
                    mode: Some(int_arg(&arg)?.clamp(0, 10) as u8),
                    fx: None,
                },
                "set_render_fx" => QueuedWrite::SetRender {
                    ent,
                    material: None,
                    color: None,
                    mode: None,
                    fx: Some(int_arg(&arg)?.clamp(0, 255) as u8),
                },
                "player.view_punch" => QueuedWrite::PlayerPunch {
                    player: ent,
                    angle: Some(value_vec3(&arg)?),
                    vel: None,
                    add: true,
                },
                "player.set_view_punch" => QueuedWrite::PlayerPunch {
                    player: ent,
                    angle: Some(value_vec3(&arg)?),
                    vel: None,
                    add: false,
                },
                "player.set_view_punch_vel" => QueuedWrite::PlayerPunch {
                    player: ent,
                    angle: None,
                    vel: Some(value_vec3(&arg)?),
                    add: false,
                },
                "player.view_punch_reset" => QueuedWrite::PlayerPunch {
                    player: ent,
                    angle: Some(Vec3::ZERO),
                    vel: Some(Vec3::ZERO),
                    add: false,
                },
                "player.pickup_object" => QueuedWrite::PlayerPickupObject {
                    player: ent,
                    ent: match &arg {
                        Value::Integer(i) => *i as u32,
                        Value::Number(n) => *n as u32,
                        _ => 0,
                    },
                },
                "player.step_sound" => QueuedWrite::PlayerStepSound {
                    player: ent,
                    volume: match &arg {
                        Value::Integer(i) => *i as f32,
                        Value::Number(n) => *n as f32,
                        _ => 1.0,
                    },
                },
                "player.exit_ladder" => QueuedWrite::PlayerExitLadder { player: ent },
                "player.set_eye_angles" => QueuedWrite::PlayerSetEyeAngles {
                    player: ent,
                    angles: value_vec3(&arg)?,
                },
                "move_hooks" => QueuedWrite::MoveHooks {
                    on: matches!(arg, Value::Boolean(true)),
                },
                "player.move_param" => {
                    let v = value_vec3(&arg)?;
                    QueuedWrite::PlayerMoveParam {
                        player: ent,
                        which: v.x.clamp(0.0, 255.0) as u8,
                        value: (v.z > 0.5).then_some(v.y),
                    }
                }
                "player.set_suit" => QueuedWrite::PlayerSetSuit {
                    player: ent,
                    on: matches!(arg, Value::Boolean(true)),
                },
                "player.set_last_hit_group" => QueuedWrite::PlayerSetLastHitGroup {
                    player: ent,
                    group: match &arg {
                        Value::Integer(i) => (*i).clamp(0, 255) as u8,
                        Value::Number(n) => n.clamp(0.0, 255.0) as u8,
                        _ => 0,
                    },
                },
                "player.should_drop_weapon" => QueuedWrite::PlayerShouldDropWeapon {
                    player: ent,
                    on: matches!(arg, Value::Boolean(true)),
                },
                "player.set_flashlight" => QueuedWrite::PlayerSetFlashlight {
                    player: ent,
                    on: matches!(arg, Value::Boolean(true)),
                },
                "player.set_frags" => QueuedWrite::PlayerSetScore {
                    player: ent,
                    frags: Some(int_arg(&arg)?.max(0) as u32),
                    deaths: None,
                },
                "player.set_deaths" => QueuedWrite::PlayerSetScore {
                    player: ent,
                    frags: None,
                    deaths: Some(int_arg(&arg)?.max(0) as u32),
                },
                "player.kick" => QueuedWrite::PlayerKick { player: ent },
                "player.ban" => QueuedWrite::PlayerBan { player: ent },
                "player.gesture_act" => QueuedWrite::PlayerGestureAct {
                    player: ent,
                    act: tbl_field::<String>(&arg, "act")?.unwrap_or_default(),
                    autokill: tbl_field::<bool>(&arg, "autokill")?.unwrap_or(true),
                },
                "player.gesture_set" => QueuedWrite::PlayerGestureSet {
                    player: ent,
                    sequence: tbl_field::<String>(&arg, "sequence")?,
                    weight: tbl_field::<f32>(&arg, "weight")?,
                },
                "player.gesture_reset" => QueuedWrite::PlayerGestureReset { player: ent },
                "player.limit_hit" => QueuedWrite::PlayerLimitHit {
                    player: ent,
                    limit: str_arg(&arg)?,
                },
                "vehicle.controls" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::VehicleControls {
                        ent,
                        throttle: t.get::<Option<f32>>("throttle")?,
                        steer: t.get::<Option<f32>>("steer")?,
                        handbrake: t.get::<Option<bool>>("handbrake")?,
                    }
                }
                "vehicle.tuning" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::VehicleTuning {
                        ent,
                        max_throttle: t.get::<Option<f32>>("max_throttle")?,
                        max_rev_throttle: t.get::<Option<f32>>("max_rev_throttle")?,
                        deg_slow: t.get::<Option<f32>>("deg_slow")?,
                        deg_fast: t.get::<Option<f32>>("deg_fast")?,
                        tyre_mu: t.get::<Option<f32>>("tyre_mu")?,
                        max_fwd: t.get::<Option<f32>>("max_fwd")?,
                        max_rev: t.get::<Option<f32>>("max_rev")?,
                        rest: t.get::<Option<f32>>("rest")?,
                    }
                }
                "vehicle.wheel" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::VehicleWheel {
                        ent,
                        idx: t.get::<Option<u32>>("idx")?.unwrap_or(0),
                        spring: t.get::<Option<f32>>("spring")?,
                        damping: t.get::<Option<f32>>("damping")?,
                        damping_compression: t.get::<Option<f32>>("damping_compression")?,
                    }
                }
                "vehicle.boost" => QueuedWrite::VehicleBoost {
                    ent,
                    boost: match &arg {
                        Value::Number(n) => *n as f32,
                        Value::Integer(i) => *i as f32,
                        _ => 0.0,
                    },
                },
                "ent.physics" => QueuedWrite::EntityPhysics {
                    ent,
                    shape: phys_shape(&table_arg(&arg)?)?,
                },
                "ent.sequence" => QueuedWrite::EntitySequence {
                    ent,
                    name: str_arg(&arg)?,
                },
                "ent.cycle" => QueuedWrite::EntityCycle {
                    ent,
                    cycle: f32_arg(&arg),
                },
                "ent.rate" => QueuedWrite::EntityPlaybackRate {
                    ent,
                    rate: f32_arg(&arg),
                },
                "ent.reset_sequence" => QueuedWrite::EntityResetSequence { ent },
                "ent.frame_advance" => QueuedWrite::EntityFrameAdvance {
                    ent,
                    dt: f32_arg(&arg),
                },
                "npc.took_damage" => QueuedWrite::NpcTookDamage { ent },
                "ent.transmit" => QueuedWrite::TransmitMode {
                    ent,
                    mode: f32_arg(&arg).clamp(0.0, 255.0) as u8,
                },
                "ent.prevent_transmit" => QueuedWrite::PreventTransmit {
                    ent,
                    player: tbl_field::<u32>(&arg, "player")?.unwrap_or(0),
                    prevent: tbl_field::<bool>(&arg, "prevent")?.unwrap_or(true),
                },
                "ent.trigger" => QueuedWrite::SentTrigger {
                    ent,
                    on: matches!(arg, Value::Boolean(true)),
                },
                "ent.trigger_bounds" => QueuedWrite::SentTriggerBounds {
                    ent,
                    on: tbl_field::<bool>(&arg, "on")?.unwrap_or(true),
                    pad: tbl_field::<f32>(&arg, "pad")?.unwrap_or(0.0),
                },
                "player.hint" => QueuedWrite::PlayerHint {
                    player: ent,
                    name: tbl_field::<String>(&arg, "name")?.unwrap_or_default(),
                    delay: tbl_field::<f32>(&arg, "delay")?.unwrap_or(0.0),
                },
                "player.screen_fade" => QueuedWrite::PlayerScreenFade {
                    player: ent,
                    flags: tbl_field::<u32>(&arg, "flags")?.unwrap_or(2),
                    color: [
                        tbl_field::<f32>(&arg, "r")?
                            .unwrap_or(0.0)
                            .clamp(0.0, 255.0) as u8,
                        tbl_field::<f32>(&arg, "g")?
                            .unwrap_or(0.0)
                            .clamp(0.0, 255.0) as u8,
                        tbl_field::<f32>(&arg, "b")?
                            .unwrap_or(0.0)
                            .clamp(0.0, 255.0) as u8,
                        tbl_field::<f32>(&arg, "a")?
                            .unwrap_or(255.0)
                            .clamp(0.0, 255.0) as u8,
                    ],
                    time: tbl_field::<f32>(&arg, "time")?.unwrap_or(0.0),
                    hold: tbl_field::<f32>(&arg, "hold")?.unwrap_or(0.0),
                },
                "player.fov" => QueuedWrite::PlayerFov {
                    player: ent,
                    fov: tbl_field::<f32>(&arg, "fov")?.unwrap_or(0.0),
                    secs: tbl_field::<f32>(&arg, "time")?.unwrap_or(0.0),
                },
                "ent.model_scale" => QueuedWrite::ModelScale {
                    ent,
                    scale: f32_arg(&arg),
                },
                "nextbot.approach" => QueuedWrite::NextBot {
                    ent,
                    op: gm_server::NextBotOp::Approach {
                        pos: tbl_vec3(&arg)?,
                    },
                },
                "nextbot.face" => QueuedWrite::NextBot {
                    ent,
                    op: gm_server::NextBotOp::Face {
                        pos: tbl_vec3(&arg)?,
                    },
                },
                "nextbot.jump_gap" => QueuedWrite::NextBot {
                    ent,
                    op: gm_server::NextBotOp::JumpAcrossGap {
                        pos: tbl_vec3(&arg)?,
                    },
                },
                "nextbot.velocity" => QueuedWrite::NextBot {
                    ent,
                    op: gm_server::NextBotOp::SetVelocity(tbl_vec3(&arg)?),
                },
                "nextbot.speed" => QueuedWrite::NextBot {
                    ent,
                    op: gm_server::NextBotOp::DesiredSpeed(f32_arg(&arg)),
                },
                "nextbot.accel" => QueuedWrite::NextBot {
                    ent,
                    op: gm_server::NextBotOp::Accel(f32_arg(&arg)),
                },
                "nextbot.decel" => QueuedWrite::NextBot {
                    ent,
                    op: gm_server::NextBotOp::Decel(f32_arg(&arg)),
                },
                "nextbot.step" => QueuedWrite::NextBot {
                    ent,
                    op: gm_server::NextBotOp::StepHeight(f32_arg(&arg)),
                },
                "nextbot.jump_height" => QueuedWrite::NextBot {
                    ent,
                    op: gm_server::NextBotOp::JumpHeight(f32_arg(&arg)),
                },
                "nextbot.yaw_rate" => QueuedWrite::NextBot {
                    ent,
                    op: gm_server::NextBotOp::MaxYawRate(f32_arg(&arg)),
                },
                "nextbot.gravity" => QueuedWrite::NextBot {
                    ent,
                    op: gm_server::NextBotOp::Gravity(f32_arg(&arg)),
                },
                "nextbot.jump" => QueuedWrite::NextBot {
                    ent,
                    op: gm_server::NextBotOp::Jump,
                },
                "nextbot.clear_stuck" => QueuedWrite::NextBot {
                    ent,
                    op: gm_server::NextBotOp::ClearStuck,
                },
                "ent.pose" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::EntityPose {
                        ent,
                        name: t.get::<Option<String>>("name")?.unwrap_or_default(),
                        value: t.get::<Option<f32>>("value")?.unwrap_or(0.0),
                    }
                }
                "ent.pose_clear" => QueuedWrite::EntityPoseClear { ent },
                "ent.bounds" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::EntityBounds {
                        ent,
                        mins: table_vec3(&t.get::<Table>("mins")?)?,
                        maxs: table_vec3(&t.get::<Table>("maxs")?)?,
                    }
                }
                "ent.gesture" => QueuedWrite::EntityGesture {
                    ent,
                    op: gesture_op(&table_arg(&arg)?)?,
                },
                "vehicle.engine" => QueuedWrite::VehicleEngine {
                    ent,
                    on: matches!(arg, Value::Boolean(true)),
                },
                "player.vehicle" => QueuedWrite::PlayerVehicle {
                    player: ent,
                    vehicle: match &arg {
                        Value::Nil => None,
                        _ => Some(int_arg(&arg)?.max(0) as u32),
                    },
                },
                "set_bone_manip" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::SetBoneManip {
                        ent,
                        bone: t.get::<u32>("bone")?,
                        scale: match t.get::<Option<Table>>("scale")? {
                            Some(v) => Some(table_vec3(&v)?),
                            None => None,
                        },
                        angles: match t.get::<Option<Table>>("angles")? {
                            Some(v) => Some(table_vec3(&v)?),
                            None => None,
                        },
                    }
                }
                "set_flex" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::SetFlex {
                        ent,
                        id: t.get::<Option<u32>>("id")?,
                        value: t.get::<f32>("value")?,
                    }
                }
                "set_eye_target" => QueuedWrite::SetEyeTarget {
                    ent,
                    target: match &arg {
                        Value::Nil => None,
                        _ => Some(table_vec3(&table_arg(&arg)?)?),
                    },
                },
                "player.drop_weapon" => QueuedWrite::PlayerDropWeapon {
                    player: ent,
                    class: match &arg {
                        Value::Nil => None,
                        v => Some(str_arg(v)?),
                    },
                },
                "player.drop_object" => QueuedWrite::PlayerDropObject { player: ent },
                "player.no_target" => QueuedWrite::PlayerNoTarget {
                    player: ent,
                    on: matches!(arg, Value::Boolean(true)),
                },
                "player.colors" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::PlayerColors {
                        player: ent,
                        body: t
                            .get::<Option<Table>>("body")?
                            .map(|v| table_vec3(&v))
                            .transpose()?,
                        weapon: t
                            .get::<Option<Table>>("weapon")?
                            .map(|v| table_vec3(&v))
                            .transpose()?,
                    }
                }
                "force_drop" => QueuedWrite::ForceDrop { ent },
                "player.lock" => QueuedWrite::PlayerLock {
                    player: ent,
                    locked: matches!(arg, Value::Boolean(true)),
                },
                "set_surface" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::SetSurface {
                        ent,
                        friction: t.get("friction")?,
                        elasticity: t.get("elasticity")?,
                    }
                }
                "player.max_armor" => QueuedWrite::PlayerMaxArmor {
                    player: ent,
                    max: int_arg(&arg)?,
                },
                "player.say" => QueuedWrite::PlayerSay {
                    player: ent,
                    text: str_arg(&arg)?,
                },
                "npc_wander" => {
                    let v = value_vec3(&arg)?;
                    QueuedWrite::NpcWander {
                        ent,
                        min: v.x.max(0.0),
                        max: v.y.max(v.x),
                    }
                }
                "npc_sight" => QueuedWrite::NpcSight {
                    ent,
                    dist: match &arg {
                        Value::Integer(i) => *i as f32,
                        Value::Number(n) => *n as f32,
                        _ => 0.0,
                    },
                },
                "npc_pickup_weapon" => QueuedWrite::NpcPickupWeapon {
                    ent,
                    weapon: match &arg {
                        Value::Integer(i) => (*i).max(0) as u32,
                        Value::Number(n) => n.max(0.0) as u32,
                        _ => 0,
                    },
                },
                "npc_ignore" => {
                    let v = value_vec3(&arg)?;
                    QueuedWrite::NpcIgnore {
                        ent,
                        player: v.x.max(0.0) as u32,
                        until: v.y.max(0.0) as u64,
                    }
                }
                "entity.set_blood" => QueuedWrite::SetBlood {
                    ent,
                    blood: match &arg {
                        Value::Integer(i) => (*i).clamp(-1, 127) as i8,
                        Value::Number(n) => n.clamp(-1.0, 127.0) as i8,
                        _ => -1,
                    },
                },
                "npc_weapon" => QueuedWrite::NpcWeapon {
                    ent,
                    class: str_arg(&arg)?,
                },
                "player.strip_weapon" => QueuedWrite::PlayerStripWeapon {
                    player: ent,
                    class: match &arg {
                        Value::Nil => None,
                        _ => Some(str_arg(&arg)?),
                    },
                },
                "player.set_ammo" | "player.give_ammo" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::PlayerSetAmmo {
                        player: ent,
                        ammo: t.get::<String>("type")?,
                        count: t.get::<i32>("count")?,
                        add: op == "player.give_ammo",
                    }
                }
                "player.set_clip" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::PlayerSetClip {
                        player: ent,
                        class: t.get::<String>("class")?,
                        slot: t.get::<Option<u8>>("slot")?.unwrap_or(0),
                        count: t.get::<i32>("count")?,
                    }
                }
                other => return Err(mlua::Error::runtime(format!("unknown write {other}"))),
            };
            let mut n = sh.queued.lock().unwrap();
            *n += 1;
            sh.queue.lock().unwrap().push((*n, w));
            Ok(())
        })?,
    )?;
    g.set("world", world)?;
    let net: Table = match lua.globals().get::<Option<Table>>("net")? {
        Some(t) => t,
        None => lua.create_table()?,
    };
    let sh = shared.clone();
    net.set(
        "send",
        lua.create_function(
            move |_, (name, value, player): (String, Value, Option<u32>)| {
                if name.is_empty() || name.len() > 64 {
                    return Err(mlua::Error::runtime("net message name must be 1..64 bytes"));
                }
                let data = crate::netval::encode(&value)?;
                let mut out = sh.net_out.lock().unwrap();
                if out.len() >= 256 {
                    return Err(mlua::Error::runtime(
                        "net: over 256 messages queued in one run",
                    ));
                }
                out.push((player.unwrap_or(0), name, data));
                Ok(())
            },
        )?,
    )?;
    lua.globals().set("net", net)?;
    let cvar = lua.create_table()?;
    let sh = shared.clone();
    cvar.set(
        "get",
        lua.create_function(move |_, name: String| {
            Ok(sh.snapshot.read().unwrap().cvars.get(&name).cloned())
        })?,
    )?;
    let sh = shared.clone();
    cvar.set(
        "get_number",
        lua.create_function(move |_, name: String| {
            Ok(sh
                .snapshot
                .read()
                .unwrap()
                .cvars
                .get(&name)
                .map(|v| v.trim().parse::<f64>().unwrap_or(0.0)))
        })?,
    )?;
    let sh = shared.clone();
    cvar.set(
        "get_bool",
        lua.create_function(move |_, name: String| {
            Ok(sh
                .snapshot
                .read()
                .unwrap()
                .cvars
                .get(&name)
                .map(|v| v.trim().parse::<f64>().unwrap_or(0.0) != 0.0))
        })?,
    )?;
    let sh = shared.clone();
    cvar.set(
        "create",
        lua.create_function(
            move |_, (name, default, opts): (String, Value, Option<Table>)| {
                if name.is_empty() || name.len() > 64 {
                    return Err(mlua::Error::runtime("cvar name must be 1..64 bytes"));
                }
                let default = cvar_string(&default)?;
                let mut c = gm_server::Cvar {
                    value: default.clone(),
                    default,
                    ..Default::default()
                };
                if let Some(o) = opts {
                    c.replicated = o.get::<Option<bool>>("replicated")?.unwrap_or(false);
                    c.archive = o.get::<Option<bool>>("archive")?.unwrap_or(false);
                    c.min = o.get::<Option<f32>>("min")?;
                    c.max = o.get::<Option<f32>>("max")?;
                    c.help = o.get::<Option<String>>("help")?.unwrap_or_default();
                }
                let mut n = sh.queued.lock().unwrap();
                *n += 1;
                sh.queue
                    .lock()
                    .unwrap()
                    .push((*n, QueuedWrite::CreateCvar { name, cvar: c }));
                Ok(())
            },
        )?,
    )?;
    let sh = shared.clone();
    cvar.set(
        "set",
        lua.create_function(move |_, (name, value): (String, Value)| {
            if !sh.can_write {
                return Err(mlua::Error::runtime(
                    "this actor has no world-write capability",
                ));
            }
            let value = cvar_string(&value)?;
            let mut n = sh.queued.lock().unwrap();
            *n += 1;
            sh.queue
                .lock()
                .unwrap()
                .push((*n, QueuedWrite::SetCvar { name, value }));
            Ok(())
        })?,
    )?;
    lua.globals().set("cvar", cvar)?;
    let storage = lua.create_table()?;
    let no_store = || mlua::Error::runtime("this addon has no storage capability");
    let sh = shared.clone();
    storage.set(
        "get",
        lua.create_function(move |lua, key: String| {
            let st = sh.store.as_ref().ok_or_else(no_store)?.lock().unwrap();
            match st.map.get(&key) {
                Some(b) => crate::netval::decode(lua, b),
                None => Ok(Value::Nil),
            }
        })?,
    )?;
    let sh = shared.clone();
    storage.set(
        "set",
        lua.create_function(move |_, (key, value): (String, Value)| {
            if key.is_empty() || key.len() > crate::storage::MAX_KEY {
                return Err(mlua::Error::runtime("storage key must be 1..64 bytes"));
            }
            let bytes = match value {
                Value::Nil => Vec::new(),
                v => crate::netval::encode(&v)?,
            };
            let mut st = sh.store.as_ref().ok_or_else(no_store)?.lock().unwrap();
            if !st.set(&key, bytes) {
                return Err(mlua::Error::runtime("storage over 1 MiB for this addon"));
            }
            Ok(())
        })?,
    )?;
    let sh = shared.clone();
    storage.set(
        "keys",
        lua.create_function(move |_, ()| {
            let st = sh.store.as_ref().ok_or_else(no_store)?.lock().unwrap();
            Ok(st.map.keys().cloned().collect::<Vec<String>>())
        })?,
    )?;
    lua.globals().set("storage", storage)?;
    let sh = shared.clone();
    lua.globals().set(
        "__mounted",
        lua.create_function(move |_, ()| Ok(sh.snapshot.read().unwrap().mounted.clone()))?,
    )?;
    let sh = shared.clone();
    lua.globals().set(
        "__sound_duration",
        lua.create_function(move |_, name: String| {
            Ok(sh.sound_duration.as_ref().and_then(|f| f(&name)))
        })?,
    )?;
    // `CompileString` / `RunString`: a chunk from a string, compiled in this VM's sandbox
    // (no file access; `include` / `CompileFile` are not this).
    lua.globals().set(
        "__compile",
        lua.create_function(|lua, (src, name): (mlua::LuaString, Option<String>)| {
            let src = String::from_utf8_lossy(&src.as_bytes()).into_owned();
            let chunk = lua
                .load(src)
                .set_name(name.unwrap_or_else(|| "RunString".into()));
            match chunk.into_function() {
                Ok(f) => Ok((Value::Function(f), Value::Nil)),
                Err(e) => Ok((Value::Nil, Value::String(lua.create_string(e.to_string())?))),
            }
        })?,
    )?;
    lua.globals().set(
        "__sha256",
        lua.create_function(|_, s: mlua::LuaString| {
            use sha2::Digest;
            Ok(format!("{:x}", sha2::Sha256::digest(s.as_bytes())))
        })?,
    )?;
    let game = lua.create_table()?;
    game.set(
        "interval",
        lua.create_function(|_, ()| Ok(gm_core::tick::DEFAULT_TICK_INTERVAL as f64))?,
    )?;
    let sh = shared.clone();
    game.set(
        "tick",
        lua.create_function(move |_, ()| Ok(sh.snapshot.read().unwrap().tick))?,
    )?;
    g.set("game", game)?;
    let phys = lua.create_table()?;
    let sh = shared.clone();
    phys.set(
        "trace",
        lua.create_function(move |lua, (start, end): (Table, Table)| {
            let snap = sh.snapshot.read().unwrap().clone();
            let t = lua.create_table()?;
            match snap.trace(table_vec3(&start)?, table_vec3(&end)?) {
                Some(h) => {
                    t.set("hit", true)?;
                    t.set("pos", vec3_table(lua, h.pos)?)?;
                    t.set("normal", vec3_table(lua, h.normal)?)?;
                    t.set("ent", h.ent.unwrap_or(0))?;
                    t.set("fraction", h.fraction)?;
                }
                None => {
                    t.set("hit", false)?;
                    t.set("fraction", 1.0)?;
                }
            }
            Ok(t)
        })?,
    )?;
    let nav = lua.create_table()?;
    let sh = shared.clone();
    nav.set(
        "loaded",
        lua.create_function(move |_, ()| Ok(sh.snapshot.read().unwrap().nav_loaded()))?,
    )?;
    let sh = shared.clone();
    nav.set(
        "poly_count",
        lua.create_function(move |_, ()| Ok(sh.snapshot.read().unwrap().nav_poly_count()))?,
    )?;
    let sh = shared.clone();
    nav.set(
        "vertices",
        lua.create_function(move |lua, ()| {
            let snap = sh.snapshot.read().unwrap().clone();
            let t = lua.create_table()?;
            for (i, v) in snap.nav_vertices().iter().enumerate() {
                t.set(i + 1, vec3_table(lua, *v)?)?;
            }
            Ok(t)
        })?,
    )?;
    let sh = shared.clone();
    nav.set(
        "polys",
        lua.create_function(move |lua, ()| {
            let snap = sh.snapshot.read().unwrap().clone();
            let t = lua.create_table()?;
            for (i, p) in snap.nav_polys().iter().enumerate() {
                let pt = lua.create_table()?;
                pt.set("v", p.verts.iter().map(|v| v + 1).collect::<Vec<u32>>())?;
                pt.set(
                    "n",
                    p.neis
                        .iter()
                        .map(|n| n.map_or(0, |n| n + 1))
                        .collect::<Vec<u32>>(),
                )?;
                t.set(i + 1, pt)?;
            }
            Ok(t)
        })?,
    )?;
    let sh = shared.clone();
    nav.set(
        "nearest",
        lua.create_function(move |lua, (pos, ext): (Table, Option<Table>)| {
            let snap = sh.snapshot.read().unwrap().clone();
            let ext = ext.map_or(Ok(Vec3::new(64.0, 64.0, 128.0)), |e| table_vec3(&e))?;
            match snap.nav_nearest(table_vec3(&pos)?, ext) {
                Some(p) => Ok(Value::Table(vec3_table(lua, p)?)),
                None => Ok(Value::Nil),
            }
        })?,
    )?;
    let sh = shared.clone();
    nav.set(
        "path",
        lua.create_function(move |lua, (a, b, ext): (Table, Table, Option<Table>)| {
            let snap = sh.snapshot.read().unwrap().clone();
            let ext = ext.map_or(Ok(Vec3::new(64.0, 64.0, 128.0)), |e| table_vec3(&e))?;
            let t = lua.create_table()?;
            match snap.nav_path(table_vec3(&a)?, table_vec3(&b)?, ext) {
                Some(path) => {
                    let pts = lua.create_table()?;
                    for (i, p) in path.points.iter().enumerate() {
                        pts.set(i + 1, vec3_table(lua, *p)?)?;
                    }
                    t.set("points", pts)?;
                    t.set("partial", path.partial)?;
                    t.set("ok", true)?;
                }
                None => t.set("ok", false)?,
            }
            Ok(t)
        })?,
    )?;
    g.set("nav", nav)?;
    // surfaceproperties: index = position in the sorted name list
    let surf = lua.create_table()?;
    let sh = shared.clone();
    surf.set(
        "names",
        lua.create_function(move |_, ()| {
            let snap = sh.snapshot.read().unwrap().clone();
            Ok(snap.surface_table().map_or_else(Vec::new, |t| t.names()))
        })?,
    )?;
    let sh = shared.clone();
    surf.set(
        "data",
        lua.create_function(move |lua, name: String| {
            let snap = sh.snapshot.read().unwrap().clone();
            let Some(t) = snap.surface_table() else {
                return Ok(Value::Nil);
            };
            if !t.names().contains(&name) {
                return Ok(Value::Nil);
            }
            let p = t.get(&name);
            let d = lua.create_table()?;
            d.set("name", name.as_str())?;
            d.set("friction", p.friction)?;
            d.set("elasticity", p.elasticity)?;
            d.set("density", p.density)?;
            d.set("dampening", p.dampening)?;
            for key in [
                "stepleft",
                "stepright",
                "impactsoft",
                "impacthard",
                "scraperough",
                "scrapesmooth",
                "bulletimpact",
                "break",
                "strain",
                "rolling",
            ] {
                if let Some(v) = t.sound(&name, key) {
                    d.set(key, v)?;
                }
            }
            Ok(Value::Table(d))
        })?,
    )?;
    g.set("surf", surf)?;
    let sh = shared.clone();
    g.set(
        "__addons",
        lua.create_function(move |_, ()| Ok(sh.addons.lock().unwrap().clone()))?,
    )?;
    g.set(
        "__weapon_def",
        lua.create_function(|lua, class: String| {
            let Some(d) = gm_server::weapons::def(&class) else {
                return Ok(Value::Nil);
            };
            let t = lua.create_table()?;
            t.set("slot", d.slot)?;
            t.set("slot_pos", d.slot_pos)?;
            t.set("clip1", d.clip1)?;
            t.set("clip2", d.clip2)?;
            t.set("ammo1", d.ammo1)?;
            t.set("ammo2", d.ammo2)?;
            t.set("weight", d.weight)?;
            t.set("hold_type", d.hold_type)?;
            Ok(Value::Table(t))
        })?,
    )?;
    let sh = shared.clone();
    g.set(
        "__map_version",
        lua.create_function(move |_, ()| Ok(sh.snapshot.read().unwrap().map_revision))?,
    )?;
    let sh = shared.clone();
    g.set(
        "__sun_dir",
        lua.create_function(move |lua, ()| {
            let snap = sh.snapshot.read().unwrap().clone();
            match snap.sun_direction {
                Some(d) => Ok(Some(vec3_table(lua, d)?)),
                None => Ok(None),
            }
        })?,
    )?;
    let sh = shared.clone();
    g.set(
        "__pvs_visible",
        lua.create_function(move |_, (a, b): (Table, Table)| {
            let snap = sh.snapshot.read().unwrap().clone();
            let (a, b) = (table_vec3(&a)?, table_vec3(&b)?);
            let Some(w) = snap.collision() else {
                return Ok(true);
            };
            Ok(w.visible(w.cluster_at(a), w.cluster_at(b)))
        })?,
    )?;
    let sh = shared.clone();
    g.get::<Table>("game")?.set(
        "__start_spot",
        lua.create_function(move |lua, ()| {
            let snap = sh.snapshot.read().unwrap().clone();
            match snap.start_spot {
                Some((pos, ang)) => Ok((Some(vec3_table(lua, pos)?), Some(vec3_table(lua, ang)?))),
                None => Ok((None, None)),
            }
        })?,
    )?;
    let model = lua.create_table()?;
    let sh = shared.clone();
    model.set(
        "info",
        lua.create_function(move |lua, name: String| {
            let snap = sh.snapshot.read().unwrap().clone();
            let Some(m) = snap.model_info(&name) else {
                return Ok(Value::Nil);
            };
            let t = lua.create_table()?;
            let bg = lua.create_table()?;
            for (i, (n, c)) in m.body_groups.iter().enumerate() {
                let e = lua.create_table()?;
                e.set("name", n.as_str())?;
                e.set("num", *c)?;
                bg.set(i + 1, e)?;
            }
            t.set("bodygroups", bg)?;
            t.set("skins", m.skins)?;
            t.set("bones", m.bones.clone())?;
            t.set("bone_parents", m.bone_parents.clone())?;
            t.set("phys_bones", m.phys_bones.clone())?;
            let bb = lua.create_table()?;
            for (i, (pos, ang)) in m.bone_bind.iter().enumerate() {
                let e = lua.create_table()?;
                e.set("pos", vec3_table(lua, *pos)?)?;
                e.set("ang", vec3_table(lua, *ang)?)?;
                bb.set(i + 1, e)?;
            }
            t.set("bone_bind", bb)?;
            t.set("sequences", m.sequences.clone())?;
            let sa = lua.create_table()?;
            for (i, (fps, frames, flags)) in m.sequence_anim.iter().enumerate() {
                let e = lua.create_table()?;
                e.set("fps", *fps)?;
                e.set("frames", *frames)?;
                e.set("flags", *flags)?;
                sa.set(i + 1, e)?;
            }
            t.set("sequence_anim", sa)?;
            let sm = lua.create_table()?;
            for (i, mv) in m.sequence_move.iter().enumerate() {
                sm.set(i + 1, vec3_table(lua, glam::Vec3::from_array(*mv))?)?;
            }
            t.set("sequence_move", sm)?;
            let fi = lua.create_table()?;
            for (i, (name, min, max, kind)) in m.flex_info.iter().enumerate() {
                let e = lua.create_table()?;
                e.set("name", name.as_str())?;
                e.set("min", *min)?;
                e.set("max", *max)?;
                e.set("kind", kind.as_str())?;
                fi.set(i + 1, e)?;
            }
            t.set("flex_info", fi)?;
            t.set("materials", m.materials.clone())?;
            let bp = lua.create_table()?;
            for (i, (surface, contents)) in m.bone_props.iter().enumerate() {
                let e = lua.create_table()?;
                e.set("surface", surface.as_str())?;
                e.set("contents", *contents)?;
                bp.set(i + 1, e)?;
            }
            t.set("bone_props", bp)?;
            let sact = lua.create_table()?;
            for (i, (act, weight)) in m.sequence_activity.iter().enumerate() {
                let e = lua.create_table()?;
                e.set("name", act.as_str())?;
                e.set("weight", *weight)?;
                sact.set(i + 1, e)?;
            }
            t.set("sequence_activity", sact)?;
            let pp = lua.create_table()?;
            for (i, (name, start, end, wrap)) in m.pose_params.iter().enumerate() {
                let e = lua.create_table()?;
                e.set("name", name.as_str())?;
                e.set("start", *start)?;
                e.set("end", *end)?;
                e.set("loop", *wrap)?;
                pp.set(i + 1, e)?;
            }
            t.set("pose_params", pp)?;
            let an = lua.create_table()?;
            for (i, (name, fps, frames, flags)) in m.anims.iter().enumerate() {
                let e = lua.create_table()?;
                e.set("name", name.as_str())?;
                e.set("fps", *fps)?;
                e.set("frames", *frames)?;
                e.set("flags", *flags)?;
                an.set(i + 1, e)?;
            }
            t.set("anims", an)?;
            t.set("hitboxes", m.hitboxes)?;
            t.set("hitbox_sets", m.hitbox_sets)?;
            let hb = lua.create_table()?;
            for (i, (b, g, mins, maxs)) in m.hitbox_list.iter().enumerate() {
                let e = lua.create_table()?;
                e.set("bone", *b)?;
                e.set("group", *g)?;
                e.set("mins", vec3_table(lua, *mins)?)?;
                e.set("maxs", vec3_table(lua, *maxs)?)?;
                hb.set(i + 1, e)?;
            }
            t.set("hitbox_list", hb)?;
            t.set("flexes", m.flexes.clone())?;
            t.set("mins", vec3_table(lua, m.mins)?)?;
            t.set("maxs", vec3_table(lua, m.maxs)?)?;
            t.set("friction", m.friction)?;
            t.set("elasticity", m.elasticity)?;
            t.set("phys_mins", vec3_table(lua, m.phys_mins)?)?;
            t.set("phys_maxs", vec3_table(lua, m.phys_maxs)?)?;
            t.set("volume", m.volume)?;
            t.set("surface_area", m.surface_area)?;
            t.set("mass_center", vec3_table(lua, m.mass_center)?)?;
            t.set("inertia", vec3_table(lua, m.inertia)?)?;
            let mesh = lua.create_table()?;
            for (i, convex) in m.mesh.iter().enumerate() {
                let ct = lua.create_table()?;
                for (j, tri) in convex.iter().enumerate() {
                    for (k, v) in tri.iter().enumerate() {
                        ct.set(j * 3 + k + 1, vec3_table(lua, *v)?)?;
                    }
                }
                mesh.set(i + 1, ct)?;
            }
            t.set("mesh", mesh)?;
            let at = lua.create_table()?;
            for (i, (n, b, p, f)) in m.attachments.iter().enumerate() {
                let e = lua.create_table()?;
                e.set("name", n.as_str())?;
                e.set("bone", *b)?;
                e.set("pos", vec3_table(lua, *p)?)?;
                e.set("forward", vec3_table(lua, *f)?)?;
                at.set(i + 1, e)?;
            }
            t.set("attachments", at)?;
            Ok(Value::Table(t))
        })?,
    )?;
    g.set("model", model)?;
    let vehicle = lua.create_table()?;
    let sh = shared.clone();
    vehicle.set(
        "info",
        lua.create_function(move |lua, id: u32| {
            let snap = sh.snapshot.read().unwrap().clone();
            let Some(v) = snap.vehicle_info(id) else {
                return Ok(Value::Nil);
            };
            let t = lua.create_table()?;
            t.set("key", v.key.as_str())?;
            t.set("class", v.class.as_str())?;
            t.set("mass", v.mass)?;
            t.set("add_gravity", v.add_gravity)?;
            t.set("driver", v.driver.unwrap_or(0))?;
            t.set("health", v.health)?;
            t.set("max_health", v.max_health)?;
            let c = &v.controls;
            t.set("throttle", c.throttle)?;
            t.set("steer", c.steer)?;
            t.set("handbrake", c.handbrake)?;
            t.set("boost", c.boost)?;
            t.set("engine", c.engine_on)?;
            t.set("steer_angle", c.steer_angle)?;
            t.set("speed", c.speed)?;
            t.set("contacts", c.wheels_in_contact)?;
            t.set("gear", c.gear)?;
            t.set("max_throttle", v.tuning.max_throttle)?;
            t.set("max_rev_throttle", v.tuning.max_rev_throttle)?;
            t.set(
                "rest",
                v.tuning
                    .rest
                    .or_else(|| gm_server::vehicle_script(&v.key).map(|s| s.rest))
                    .unwrap_or(0.0),
            )?;
            if let Some(spec) = v.spec() {
                t.set("rpm", c.rpm(&spec))?;
                t.set("tyre_mu", spec.tyre_mu)?;
                t.set(
                    "cone",
                    gm_server::VehicleControls::steer_cone(c.speed, &spec),
                )?;
                let d = lua.create_table()?;
                d.set("max_fwd", spec.max_fwd)?;
                d.set("max_rev", spec.max_rev)?;
                d.set("boost_max", spec.boost_max)?;
                d.set("deg_slow", spec.deg_slow)?;
                d.set("deg_fast", spec.deg_fast)?;
                d.set("slow_speed", spec.slow_speed)?;
                d.set("fast_speed", spec.fast_speed)?;
                d.set("steer_exp", spec.steer_exp)?;
                d.set("axle_ratio", spec.axle_ratio)?;
                d.set("shift_up", spec.shift_up_rpm)?;
                d.set("shift_down", spec.shift_down_rpm)?;
                d.set("gears", spec.gears.to_vec())?;
                t.set("drive", d)?;
            }
            let ws = lua.create_table()?;
            for (i, w) in v.wheels.iter().enumerate() {
                let e = lua.create_table()?;
                e.set("hub", vec3_table(lua, w.hub)?)?;
                e.set("radius", w.radius)?;
                e.set("spring", w.spring)?;
                e.set("damping", w.damping)?;
                e.set("damping_compression", w.damping_compression)?;
                if let Some(ct) = v.controls.contacts.get(i) {
                    e.set("contact", vec3_table(lua, ct.point)?)?;
                    e.set("touching", ct.touching)?;
                }
                ws.set(i + 1, e)?;
            }
            t.set("wheels", ws)?;
            Ok(Value::Table(t))
        })?,
    )?;
    g.set("vehicle", vehicle)?;
    g.set("phys", phys)?;
    lua.load(PRELUDE).set_name("prelude").exec()?;
    Ok(())
}

/// A player-owned constraint from a script goes through the player's `sbox_maxconstraints`
/// (owner 0 is the server: always allowed, nothing counted).
fn owner_may_constrain(server: &mut Server<'_>, owner: u32) -> bool {
    if owner == 0 {
        return true;
    }
    if !server.players.contains_key(&owner) {
        return false;
    }
    let mut events = Vec::new();
    let ok = server
        .sandbox
        .check_limit(owner, "constraints", &mut events);
    server.events.extend(events);
    ok
}

/// The bookkeeping the tool does for a player's constraint: an undo entry, the count and the
/// cleanup list.
fn own_constraint(server: &mut Server<'_>, owner: u32, tool: &str, id: u32) {
    if owner == 0 {
        return;
    }
    server.sandbox.push_undo(
        owner,
        tool,
        format!("#tool.{tool}.name"),
        Some(format!("Undone #tool.{tool}.name")),
        vec![id],
    );
    server.sandbox.add_count(owner, "constraints", id);
    server.sandbox.add_cleanup(owner, "constraints", id);
}

fn table_arg(v: &Value) -> Result<Table> {
    match v {
        Value::Table(t) => Ok(t.clone()),
        _ => Err(mlua::Error::runtime("expected a {x, y, z} table")),
    }
}

/// Lua table for an event payload.
/// The payload as a JSON object (WASM guests): numbers, vectors as `{x, y, z}`, `hit` as
/// `{ent, pos, normal}`, `settings` as an object, `net` as `{name}` (the data stays binary).
pub fn payload_json(p: &Payload) -> String {
    serde_json::Value::Object(payload_json_map(p)).to_string()
}

/// `payload_json` as the object, for callers that add fields (a WASM actor adds the decoded
/// net `data`).
pub fn payload_json_map(p: &Payload) -> serde_json::Map<String, serde_json::Value> {
    use serde_json::{json, Map, Value as J};
    let v3 = |v: Vec3| json!({ "x": v.x, "y": v.y, "z": v.z });
    let mut m = Map::new();
    for (k, v) in &p.ints {
        m.insert(k.to_string(), json!(v));
    }
    for (k, v) in &p.floats {
        m.insert(k.to_string(), json!(v));
    }
    for (k, v) in &p.vecs {
        m.insert(k.to_string(), v3(*v));
    }
    for (k, v) in &p.bools {
        m.insert(k.to_string(), json!(v));
    }
    for (k, v) in &p.strs {
        m.insert(k.to_string(), json!(v));
    }
    if !p.settings.is_empty() {
        let s: Map<String, J> = p
            .settings
            .iter()
            .map(|(k, v)| (k.clone(), json!(v)))
            .collect();
        m.insert("settings".into(), J::Object(s));
    }
    if let Some((ent, pos, normal)) = p.hit {
        m.insert(
            "hit".into(),
            json!({ "ent": ent, "pos": v3(pos), "normal": v3(normal) }),
        );
    }
    if let Some((name, _)) = &p.net {
        m.insert("name".into(), json!(name));
    }
    m
}

pub fn payload_table(lua: &Lua, p: &Payload) -> Result<Table> {
    let t = lua.create_table()?;
    if let Some((name, data)) = &p.net {
        t.set("name", name.as_str())?;
        t.set("data", crate::netval::decode(lua, data)?)?;
    }
    for (k, v) in &p.ints {
        t.set(*k, *v)?;
    }
    for (k, v) in &p.floats {
        t.set(*k, *v)?;
    }
    for (k, v) in &p.vecs {
        t.set(*k, vec3_table(lua, *v)?)?;
    }
    // always a table: a click without client settings reads `ctx.settings.x` as nil
    let st = lua.create_table()?;
    for (k, v) in &p.settings {
        st.set(k.as_str(), v.as_str())?;
    }
    t.set("settings", st)?;
    for (k, v) in &p.bools {
        t.set(*k, *v)?;
    }
    for (k, v) in &p.strs {
        t.set(*k, v.as_str())?;
    }
    if let Some((ent, pos, normal)) = p.hit {
        let h = lua.create_table()?;
        h.set("ent", ent)?;
        h.set("pos", vec3_table(lua, pos)?)?;
        h.set("normal", vec3_table(lua, normal)?)?;
        t.set("hit", h)?;
    }
    if p.hit.is_some() {
        let objs = lua.create_table()?;
        for (ent, pos, normal) in &p.objects {
            let o = lua.create_table()?;
            o.set("ent", *ent)?;
            o.set("pos", vec3_table(lua, *pos)?)?;
            o.set("normal", vec3_table(lua, *normal)?)?;
            objs.push(o)?;
        }
        t.set("objects", objs)?;
    }
    Ok(t)
}

/// The ops a stool handler returned: an array of `{ op = "...", ... }` tables (or nothing).
pub fn tool_ops(v: Value) -> Result<Vec<ToolOp>> {
    let Value::Table(list) = v else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for item in list.sequence_values::<Table>() {
        let t = item?;
        let op: String = t.get("op")?;
        out.push(match op.as_str() {
            "set_object" => ToolOp::SetObject,
            "clear" => ToolOp::Clear,
            "done" => ToolOp::Done,
            "weld" => ToolOp::Weld {
                ent1: t.get::<Option<u32>>("ent1")?.unwrap_or(0),
                ent2: t.get::<Option<u32>>("ent2")?.unwrap_or(0),
                forcelimit: t.get::<Option<f32>>("forcelimit")?.unwrap_or(0.0),
                nocollide: t.get::<Option<bool>>("nocollide")?.unwrap_or(false),
                undo: t
                    .get::<Option<String>>("undo")?
                    .unwrap_or_else(|| "Weld".into()),
            },
            "rope" => ToolOp::Rope,
            "axis" => ToolOp::Axis,
            "remove" => ToolOp::Remove {
                ent: t.get::<Option<u32>>("ent")?.unwrap_or(0),
            },
            "nocollide" => ToolOp::NoCollide {
                ent1: t.get::<Option<u32>>("ent1")?.unwrap_or(0),
                ent2: t.get::<Option<u32>>("ent2")?.unwrap_or(0),
                undo: t
                    .get::<Option<String>>("undo")?
                    .unwrap_or_else(|| "NoCollide".into()),
            },
            other => return Err(mlua::Error::runtime(format!("unknown tool op {other}"))),
        });
    }
    Ok(out)
}

/// Applies a scripted stool's ops with the same bookkeeping as the Rust stools.
pub fn apply_tool_ops(
    server: &mut Server<'_>,
    player: u32,
    hit: &gm_sandbox::ToolHit,
    ops: &[ToolOp],
) -> bool {
    let ent = |e: u32| if e == 0 { None } else { Some(e) };
    let mut events = Vec::new();
    let mut did = false;
    for op in ops {
        match op {
            ToolOp::Done => did = true,
            ToolOp::SetObject => {
                let n = server.sandbox.set_object(player, "script", hit);
                server.sandbox.set_stage(player, n as u32);
                did = true;
            }
            ToolOp::Clear => {
                server.sandbox.clear_objects(player);
                did = true;
            }
            ToolOp::Weld {
                ent1,
                ent2,
                forcelimit,
                nocollide,
                undo,
            } => {
                did = true;
                if !server
                    .sandbox
                    .check_limit(player, "constraints", &mut events)
                {
                    continue;
                }
                if let Some(id) =
                    server
                        .sandbox
                        .weld(player, ent(*ent1), ent(*ent2), *forcelimit, *nocollide)
                {
                    let key = undo.to_lowercase();
                    server.sandbox.push_undo(
                        player,
                        undo,
                        format!("#tool.{key}.name"),
                        Some(format!("Undone #tool.{key}.name")),
                        vec![id],
                    );
                    server.sandbox.add_count(player, "constraints", id);
                    server.sandbox.add_cleanup(player, "constraints", id);
                }
            }
            ToolOp::Rope => {
                did = true;
                server.sandbox.finish_rope(player, &mut events);
            }
            ToolOp::Axis => {
                did = true;
                server.sandbox.finish_axis(player, &mut events);
            }
            ToolOp::Remove { ent } => {
                let target = if *ent == 0 {
                    hit.ent.unwrap_or(0)
                } else {
                    *ent
                };
                if target != 0 && server.remove_prop(target) {
                    did = true;
                    events.push(gm_sandbox::Event::Removed { prop: target });
                }
            }
            ToolOp::NoCollide { ent1, ent2, undo } => {
                did = true;
                if !server
                    .sandbox
                    .check_limit(player, "constraints", &mut events)
                {
                    continue;
                }
                if let Some(id) = server.sandbox.nocollide(player, ent(*ent1), ent(*ent2)) {
                    let key = undo.to_lowercase();
                    server.sandbox.push_undo(
                        player,
                        undo,
                        format!("#tool.{key}.name"),
                        Some(format!("Undone #tool.{key}.name")),
                        vec![id],
                    );
                    server.sandbox.add_count(player, "constraints", id);
                    server.sandbox.add_cleanup(player, "nocollide", id);
                }
            }
        }
    }
    server.events.extend(events);
    server.sync_after_script();
    did
}

/// Serial phase: one queued write onto the server; false when the entity is gone or the write
/// is not allowed (the world body, another actor's removed entity).
pub fn apply(server: &mut Server<'_>, w: &QueuedWrite) -> bool {
    let exists = |server: &Server<'_>, ent: u32| server.sandbox.props().iter().any(|p| p.id == ent);
    match w {
        QueuedWrite::SetOverlayText { ent, text } => server.set_overlay_text(*ent, text),
        QueuedWrite::SetPos { ent, pos } => {
            if !exists(server, *ent) {
                return false;
            }
            let ang = server
                .sandbox
                .props()
                .iter()
                .find(|p| p.id == *ent)
                .map(|p| p.angles)
                .unwrap_or_default();
            server.teleport_prop(*ent, *pos, ang)
        }
        QueuedWrite::SetAngles { ent, angles } => {
            if !exists(server, *ent) {
                return false;
            }
            let pos = server
                .sandbox
                .props()
                .iter()
                .find(|p| p.id == *ent)
                .map(|p| p.pos)
                .unwrap_or_default();
            server.teleport_prop(*ent, pos, *angles)
        }
        QueuedWrite::Freeze { ent, frozen } => server.set_prop_frozen(*ent, *frozen),
        QueuedWrite::Remove { ent } => server.remove_prop(*ent),
        QueuedWrite::NpcGoto { ent, pos, run } => server.npc_goto(*ent, *pos, *run),
        QueuedWrite::NpcChase { ent, player, run } => server.npc_chase(*ent, *player, *run),
        QueuedWrite::NpcMaxYaw { ent, rate } => server.npc_set_max_yaw(*ent, *rate),
        QueuedWrite::NpcArrival { ent, dist } => server.npc_set_arrival(*ent, *dist),
        QueuedWrite::NpcTarget { ent, player } => server.npc_target(*ent, *player),
        QueuedWrite::NpcStop { ent } => {
            if !server.npcs.contains_key(ent) {
                return false;
            }
            server.npc_stop(*ent);
            true
        }
        QueuedWrite::Sound {
            name,
            pos,
            volume,
            ent,
        } => {
            if *ent == 0 || !server.emit_sound(*ent, name.clone(), *volume) {
                server.play_sound(name.clone(), *pos, *volume);
            }
            true
        }
        QueuedWrite::SpawnList { category, models } => {
            server.add_spawn_list(category, models);
            true
        }
        QueuedWrite::ToolSchema { tool, settings } => {
            server.set_tool_schema(tool, settings.clone());
            true
        }
        QueuedWrite::SetVelocity {
            ent,
            velocity,
            angular,
        } => match server.body_of(*ent) {
            Some(body) => {
                server.physics.set_velocity(body, *velocity, *angular);
                true
            }
            None => false,
        },
        QueuedWrite::Spawn {
            owner,
            class,
            model,
            pos,
            angles,
            frozen,
            tag,
            physics,
        } => match match class {
            Some(c) => {
                server.script_spawn_entity(*owner, c, *pos, *angles, *frozen, physics.clone())
            }
            None => server.script_spawn_prop(*owner, model, *pos, *angles, *frozen),
        } {
            Some(ent) => {
                server
                    .game_events
                    .push(gm_server::GameEvent::ScriptSpawned {
                        owner: *owner,
                        ent,
                        tag: tag.clone(),
                    });
                true
            }
            None => false,
        },
        QueuedWrite::EntityRegister { def } => {
            server.register_entity(def.clone());
            true
        }
        QueuedWrite::NextThink { ent, seconds } => server.set_next_think(*ent, *seconds),
        QueuedWrite::WeaponRegister { def } => {
            server.register_weapon(def.clone());
            true
        }
        QueuedWrite::FireBullet {
            player,
            damage,
            pellets,
            spread,
        } => server.script_bullets(*player, *damage, *pellets, *spread),
        QueuedWrite::SetSkin { ent, skin } => server.set_prop_skin(*ent, *skin),
        QueuedWrite::SetBody { ent, part, model } => server.set_prop_body_part(*ent, *part, *model),
        QueuedWrite::SetGravity { ent, on } => server.set_prop_gravity(*ent, *on),
        QueuedWrite::SetCollide { ent, on } => server.set_prop_collide(*ent, *on),
        QueuedWrite::KeepUpright { ent, on } => server.set_prop_upright(*ent, *on),
        QueuedWrite::Ignite { ent, seconds } => server.ignite(*ent, *seconds),
        QueuedWrite::Extinguish { ent } => server.extinguish(*ent),
        QueuedWrite::SetHealth { ent, health } => {
            if !exists(server, *ent) {
                return false;
            }
            server.sandbox.set_health(*ent, *health);
            true
        }
        QueuedWrite::Weld {
            ent,
            ent2,
            forcelimit,
            nocollide,
            owner,
        } => {
            let world = |e: u32| if e == 0 { None } else { Some(e) };
            if !exists(server, *ent) || (*ent2 != 0 && !exists(server, *ent2)) {
                return false;
            }
            if !owner_may_constrain(server, *owner) {
                return false;
            }
            let made =
                server
                    .sandbox
                    .weld(*owner, world(*ent), world(*ent2), *forcelimit, *nocollide);
            if let Some(id) = made {
                own_constraint(server, *owner, "weld", id);
                server.sync_after_script();
            }
            made.is_some()
        }
        QueuedWrite::Constrain {
            ent,
            ent2,
            owner,
            kind,
        } => {
            let world = |e: u32| if e == 0 { None } else { Some(e) };
            if (*ent != 0 && !exists(server, *ent)) || (*ent2 != 0 && !exists(server, *ent2)) {
                return false;
            }
            if !owner_may_constrain(server, *owner) {
                return false;
            }
            use gm_sandbox::ConstraintKind as K;
            let tool = match kind {
                K::Rope { .. } => "rope",
                K::Axis { .. } => "axis",
                K::Ballsocket { .. } => "ballsocket",
                K::Slider { .. } => "slider",
                K::Elastic { .. } => "elastic",
                K::Hydraulic { .. } => "hydraulic",
                K::Winch { .. } => "winch",
                K::Muscle { .. } => "muscle",
                K::Pulley { .. } => "pulley",
                _ => "constraint",
            };
            let made = server
                .sandbox
                .constrain(*owner, world(*ent), world(*ent2), kind.clone());
            if let Some(id) = made {
                own_constraint(server, *owner, tool, id);
                server.sync_after_script();
            }
            made.is_some()
        }
        QueuedWrite::Motor {
            ent,
            ent2,
            owner,
            lpos1,
            lpos2,
            axis_point,
            friction,
            torque,
            forcetime,
            nocollide,
            toggle,
            forcelimit,
            key_f,
            key_r,
            direction,
        } => {
            if *ent == 0 || !exists(server, *ent) || (*ent2 != 0 && !exists(server, *ent2)) {
                return false;
            }
            if !owner_may_constrain(server, *owner) {
                return false;
            }
            let ent2 = if *ent2 == 0 { None } else { Some(*ent2) };
            let made = server.sandbox.motor(
                *owner,
                *ent,
                ent2,
                *lpos1,
                *lpos2,
                *axis_point,
                *friction,
                *torque,
                *forcetime,
                *nocollide,
                *toggle,
                *forcelimit,
                *key_f,
                *key_r,
                *direction,
            );
            if let Some((motor, axis)) = made {
                if *owner != 0 {
                    server.sandbox.push_undo(
                        *owner,
                        "motor",
                        "#tool.motor.name".into(),
                        Some("Undone #tool.motor.name".into()),
                        vec![motor, axis],
                    );
                    server.sandbox.add_count(*owner, "constraints", motor);
                    server.sandbox.add_cleanup(*owner, "constraints", motor);
                    server.sandbox.add_cleanup(*owner, "constraints", axis);
                }
                server.sync_after_script();
            }
            made.is_some()
        }
        QueuedWrite::PlayerTeleport { player, pos } => {
            if !server.players.contains_key(player) {
                return false;
            }
            server.teleport_player(*player, *pos);
            true
        }
        QueuedWrite::PlayerSetHealth { player, health } => {
            if !server.players.contains_key(player) {
                return false;
            }
            server.set_player_health(*player, *health);
            true
        }
        QueuedWrite::PlayerSetArmor { player, armor } => {
            if !server.players.contains_key(player) {
                return false;
            }
            server.set_player_armor(*player, *armor);
            true
        }
        QueuedWrite::PlayerDamage {
            player,
            amount,
            dmg_type,
        } => {
            if !server.players.contains_key(player) {
                return false;
            }
            server.damage_player(*player, *amount, *dmg_type);
            true
        }
        QueuedWrite::PlayerKill { player } => {
            if !server.players.get(player).is_some_and(|p| p.alive) {
                return false;
            }
            server.kill_player(*player);
            true
        }
        QueuedWrite::PlayerGiveWeapon { player, class } => server.give_weapon(*player, class),
        QueuedWrite::PlayerSelectWeapon { player, class } => server.select_weapon(*player, class),
        QueuedWrite::SetMass { ent, mass } => match server.body_of(*ent) {
            Some(body) => {
                server.physics.set_mass(body, *mass);
                true
            }
            None => false,
        },
        QueuedWrite::SetInertia { ent, inertia } => match server.body_of(*ent) {
            Some(body) => {
                server.physics.set_inertia(body, *inertia);
                true
            }
            None => false,
        },
        QueuedWrite::SetDamping {
            ent,
            linear,
            angular,
        } => match server.body_of(*ent) {
            Some(body) => {
                server.physics.set_damping(body, *linear, *angular);
                true
            }
            None => false,
        },
        QueuedWrite::EnableDrag { ent, on } => match server.body_of(*ent) {
            Some(body) => {
                server.physics.set_drag(body, *on);
                true
            }
            None => false,
        },
        QueuedWrite::SetAsleep { ent, asleep } => match server.body_of(*ent) {
            Some(body) => {
                server.physics.set_asleep(body, *asleep);
                true
            }
            None => false,
        },
        QueuedWrite::SetParent { ent, parent } => server.set_parent(*ent, *parent),
        QueuedWrite::SetRender {
            ent,
            material,
            color,
            mode,
            fx,
        } => server.set_prop_render(*ent, material.as_deref(), *color, *mode, *fx),
        QueuedWrite::PlayerSetFlashlight { player, on } => {
            server.set_player_flashlight(*player, *on)
        }
        QueuedWrite::PlayerPunch {
            player,
            angle,
            vel,
            add,
        } => server.view_punch(*player, *angle, *vel, *add),
        QueuedWrite::PlayerPickupObject { player, ent } => server.pickup_object(*player, *ent),
        QueuedWrite::PlayerStepSound { player, volume } => server.play_step_sound(*player, *volume),
        QueuedWrite::PlayerExitLadder { player } => server.exit_ladder(*player),
        QueuedWrite::PlayerSetEyeAngles { player, angles } => {
            server.set_eye_angles(*player, *angles)
        }
        QueuedWrite::PlayerSetSuit { player, on } => server.set_suit(*player, *on),
        QueuedWrite::PlayerMoveParam {
            player,
            which,
            value,
        } => server.set_move_param(*player, *which, *value),
        QueuedWrite::MoveHooks { on } => {
            server.move_hooks = *on;
            true
        }
        QueuedWrite::PlayerSetLastHitGroup { player, group } => {
            server.set_last_hit_group(*player, *group)
        }
        QueuedWrite::PlayerShouldDropWeapon { player, on } => {
            server.set_drop_weapon_on_death(*player, *on)
        }
        QueuedWrite::PlayerSetScore {
            player,
            frags,
            deaths,
        } => server.set_player_score(*player, *frags, *deaths),
        QueuedWrite::PlayerKick { player } => server.remove_player(*player),
        QueuedWrite::PlayerGestureAct {
            player,
            act,
            autokill,
        } => server.player_add_gesture_by_activity(*player, act, *autokill),
        QueuedWrite::PlayerGestureSet {
            player,
            sequence,
            weight,
        } => server.player_set_gesture(*player, sequence.as_deref(), *weight),
        QueuedWrite::PlayerGestureReset { player } => server.player_remove_gesture(*player),
        QueuedWrite::PlayerBan { player } => {
            server.pending_bans.push(*player);
            true
        }
        QueuedWrite::PlayerLimitHit { player, limit } => {
            server.notices.push((*player, format!("LimitHit {limit}")));
            true
        }
        QueuedWrite::VehicleControls {
            ent,
            throttle,
            steer,
            handbrake,
        } => server.set_vehicle_controls(*ent, *throttle, *steer, *handbrake),
        QueuedWrite::VehicleTuning {
            ent,
            max_throttle,
            max_rev_throttle,
            deg_slow,
            deg_fast,
            tyre_mu,
            max_fwd,
            max_rev,
            rest,
        } => server.set_vehicle_tuning(
            *ent,
            *max_throttle,
            *max_rev_throttle,
            *deg_slow,
            *deg_fast,
            *tyre_mu,
            *max_fwd,
            *max_rev,
            *rest,
        ),
        QueuedWrite::VehicleWheel {
            ent,
            idx,
            spring,
            damping,
            damping_compression,
        } => server.set_vehicle_wheel(*ent, *idx as usize, *spring, *damping, *damping_compression),
        QueuedWrite::VehicleBoost { ent, boost } => server.set_vehicle_boost(*ent, *boost),
        QueuedWrite::EntityPhysics { ent, shape } => server.set_script_shape(*ent, shape.clone()),
        QueuedWrite::EntitySequence { ent, name } => server.set_sequence(*ent, name),
        QueuedWrite::EntityCycle { ent, cycle } => server.set_cycle(*ent, *cycle),
        QueuedWrite::EntityPlaybackRate { ent, rate } => server.set_playback_rate(*ent, *rate),
        QueuedWrite::EntityResetSequence { ent } => server.reset_sequence_info(*ent),
        QueuedWrite::EntityFrameAdvance { ent, dt } => server.frame_advance(*ent, *dt),
        QueuedWrite::NpcTookDamage { ent } => server.npc_mark_damaged(*ent),
        QueuedWrite::PlayerFov { player, fov, secs } => server.set_player_fov(*player, *fov, *secs),
        QueuedWrite::PlayerHint {
            player,
            name,
            delay,
        } => {
            if !server.players.contains_key(player) {
                return false;
            }
            let name: String = name.chars().filter(|c| !c.is_whitespace()).collect();
            server
                .notices
                .push((*player, format!("Hint {name} {delay}")));
            true
        }
        QueuedWrite::PlayerScreenFade {
            player,
            flags,
            color,
            time,
            hold,
        } => {
            if !server.players.contains_key(player) {
                return false;
            }
            let [r, g, b, a] = color;
            server.notices.push((
                *player,
                format!("ScreenFade {r} {g} {b} {a} {time} {hold} {flags}"),
            ));
            true
        }
        QueuedWrite::PreventTransmit {
            ent,
            player,
            prevent,
        } => server.set_prevent_transmit(*ent, *player, *prevent),
        QueuedWrite::TransmitMode { ent, mode } => server.set_transmit_mode(*ent, *mode),
        QueuedWrite::SentTrigger { ent, on } => server.set_sent_trigger(*ent, *on),
        QueuedWrite::SentTriggerBounds { ent, on, pad } => {
            server.set_sent_trigger_bounds(*ent, *on, *pad)
        }
        QueuedWrite::ModelScale { ent, scale } => server.set_model_scale(*ent, *scale),
        QueuedWrite::NextBot { ent, op } => server.nextbot_op(*ent, op.clone()),
        QueuedWrite::EntityPose { ent, name, value } => {
            server.set_pose_parameter(*ent, name, *value)
        }
        QueuedWrite::EntityPoseClear { ent } => server.clear_pose_parameters(*ent),
        QueuedWrite::EntityBounds { ent, mins, maxs } => {
            server.set_collision_bounds(*ent, *mins, *maxs)
        }
        QueuedWrite::EntityGesture { ent, op } => match op {
            GestureOp::Add { sequence, autokill } => server.add_gesture(*ent, sequence, *autokill),
            GestureOp::Remove => server.remove_gesture(*ent),
            GestureOp::Set(patch) => server.set_gesture(*ent, patch),
        },
        QueuedWrite::VehicleEngine { ent, on } => server.set_vehicle_engine(*ent, *on),
        QueuedWrite::PlayerVehicle { player, vehicle } => match vehicle {
            Some(v) => server.enter_vehicle(*player, *v),
            None => server.exit_vehicle(*player),
        },
        QueuedWrite::SetBoneManip {
            ent,
            bone,
            scale,
            angles,
        } => server.set_bone_manip(*ent, *bone as usize, *scale, *angles),
        QueuedWrite::SetFlex { ent, id, value } => {
            server.set_flex(*ent, id.map(|i| i as usize), *value)
        }
        QueuedWrite::SetEyeTarget { ent, target } => server.set_script_eye_target(*ent, *target),
        QueuedWrite::PlayerSay { player, text } => server.say(*player, text),
        QueuedWrite::PlayerDropWeapon { player, class } => {
            server.drop_weapon(*player, class.as_deref()).is_some()
        }
        QueuedWrite::PlayerDropObject { player } => server.drop_object(*player),
        QueuedWrite::PlayerNoTarget { player, on } => server.set_no_target(*player, *on),
        QueuedWrite::PlayerColors {
            player,
            body,
            weapon,
        } => server.set_player_colors(*player, *body, *weapon),
        QueuedWrite::PlayerMaxArmor { player, max } => server.set_max_armor(*player, *max),
        QueuedWrite::PlayerLock { player, locked } => server.set_locked(*player, *locked),
        QueuedWrite::ForceDrop { ent } => server.force_player_drop(*ent),
        QueuedWrite::SetSurface {
            ent,
            friction,
            elasticity,
        } => server.set_entity_surface(*ent, *friction, *elasticity),
        QueuedWrite::NpcWeapon { ent, class } => server.set_npc_weapon(*ent, class),
        QueuedWrite::SetBlood { ent, blood } => server.set_blood(*ent, *blood),
        QueuedWrite::NpcIgnore { ent, player, until } => {
            server.npc_ignore_until(*ent, *player, *until)
        }
        QueuedWrite::NpcSight { ent, dist } => server.set_npc_sight(*ent, *dist),
        QueuedWrite::NpcPickupWeapon { ent, weapon } => server.npc_pickup_weapon(*ent, *weapon),
        QueuedWrite::NpcWander { ent, min, max } => server.npc_wander_goal(*ent, *min, *max),
        QueuedWrite::PlayerStripWeapon { player, class } => {
            server.strip_weapon(*player, class.as_deref())
        }
        QueuedWrite::PlayerSetAmmo {
            player,
            ammo,
            count,
            add,
        } => {
            server.set_ammo(*player, ammo, *count, *add);
            true
        }
        QueuedWrite::PlayerSetClip {
            player,
            class,
            slot,
            count,
        } => server.set_clip(*player, class, *slot as usize, *count),
        QueuedWrite::UndoPush {
            player,
            name,
            text,
            entities,
        } => {
            if !server.players.contains_key(player)
                || entities.is_empty()
                || entities.iter().any(|e| !exists(server, *e))
            {
                return false;
            }
            server
                .sandbox
                .push_undo(*player, name, text.clone(), None, entities.clone());
            for e in entities {
                server.sandbox.add_count(*player, "props", *e);
                server.sandbox.add_cleanup(*player, "props", *e);
            }
            true
        }
        QueuedWrite::CreateCvar { name, cvar } => server.create_cvar(name, cvar.clone()),
        QueuedWrite::SetCvar { name, value } => server.set_cvar(name, value),
        QueuedWrite::NoCollide { ent, ent2, owner } => {
            let world = |e: u32| if e == 0 { None } else { Some(e) };
            if !exists(server, *ent) || (*ent2 != 0 && !exists(server, *ent2)) {
                return false;
            }
            if !owner_may_constrain(server, *owner) {
                return false;
            }
            let made = server.sandbox.nocollide(*owner, world(*ent), world(*ent2));
            if let Some(id) = made {
                own_constraint(server, *owner, "nocollide", id);
                server.sync_after_script();
            }
            made.is_some()
        }
    }
}

/// `{r, g, b, a?}` in 0..255 to 0..1; nil is opaque white.
fn color_of(t: Option<&Table>) -> Result<[f32; 4]> {
    let Some(t) = t else {
        return Ok([1.0, 1.0, 1.0, 1.0]);
    };
    let c = |k: &str, d: f32| -> Result<f32> {
        Ok(t.get::<Option<f32>>(k)?.unwrap_or(d).clamp(0.0, 255.0) / 255.0)
    };
    Ok([
        c("r", 255.0)?,
        c("g", 255.0)?,
        c("b", 255.0)?,
        c("a", 255.0)?,
    ])
}
