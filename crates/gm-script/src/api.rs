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
    /// `sound.play`: a sound script at a world position, `volume` multiplies the script's.
    Sound {
        name: String,
        pos: Vec3,
        volume: f32,
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
        model: String,
        pos: Vec3,
        angles: Vec3,
        frozen: bool,
        tag: String,
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
pub struct Shared {
    pub snapshot: RwLock<Arc<Snapshot>>,
    pub queue: Mutex<Vec<(u64, QueuedWrite)>>,
    pub queued: Mutex<u64>,
    pub prints: Mutex<Vec<String>>,
    pub can_write: bool,
    /// Outgoing addon messages: (player or 0 for everyone / the server, name, payload).
    pub net_out: Mutex<Vec<(u32, String, Vec<u8>)>>,
    /// `draw.*` calls of the current `render.hud` dispatch (client realm).
    pub draws: Mutex<Vec<DrawCmd>>,
    /// The addon's store when it has the `storage` capability.
    pub store: Option<Arc<Mutex<crate::storage::Store>>>,
}

fn player_table(lua: &Lua, p: &crate::snapshot::PlayerSnap) -> Result<Table> {
    let t = lua.create_table()?;
    t.set("id", p.id)?;
    t.set("origin", vec3_table(lua, p.origin)?)?;
    t.set("velocity", vec3_table(lua, p.velocity)?)?;
    t.set("view_angles", vec3_table(lua, p.view_angles)?)?;
    t.set("held", p.held)?;
    t.set("alive", p.alive)?;
    t.set("health", p.health)?;
    t.set("armor", p.armor)?;
    t.set("max_health", p.max_health)?;
    t.set("identity", p.identity.as_deref())?;
    t.set("model", p.model.as_str())?;
    t.set("skin", p.skin)?;
    t.set("body", p.body.as_str())?;
    t.set("weapons", p.weapons.clone())?;
    t.set("active_weapon", p.active_weapon.as_str())?;
    t.set("frags", p.frags)?;
    t.set("deaths", p.deaths)?;
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
for _, kind in ipairs({ "primary", "secondary", "reload", "deploy", "holster" }) do
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
                        model: t.get("model")?,
                        pos: table_vec3(&pos)?,
                        angles: match t.get::<Option<Table>>("angles")? {
                            Some(a) => table_vec3(&a)?,
                            None => Vec3::ZERO,
                        },
                        frozen: t.get::<Option<bool>>("frozen")?.unwrap_or(false),
                        tag: t.get::<Option<String>>("tag")?.unwrap_or_default(),
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
    if !p.settings.is_empty() {
        let st = lua.create_table()?;
        for (k, v) in &p.settings {
            st.set(k.as_str(), v.as_str())?;
        }
        t.set("settings", st)?;
    }
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
            ToolOp::SetObject => {
                let n = server.sandbox.set_object(player, hit);
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
        QueuedWrite::Sound { name, pos, volume } => {
            server.play_sound(name.clone(), *pos, *volume);
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
            model,
            pos,
            angles,
            frozen,
            tag,
        } => match server.script_spawn_prop(*owner, model, *pos, *angles, *frozen) {
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
