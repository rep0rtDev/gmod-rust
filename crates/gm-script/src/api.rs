//! The host API an actor sees (server realm, v1): `events.on`, `world:get/find/players/queue`,
//! `game.tick`, `print`. Writes are queued and applied in the serial phase.

use crate::snapshot::Snapshot;
use glam::Vec3;
use gm_server::Server;
use mlua::{Lua, Result, Table, Value};
use std::sync::{Arc, Mutex, RwLock};

/// Event payload: small typed fields (no Lua values cross actors).
#[derive(Clone, Debug, Default)]
pub struct Payload {
    pub ints: Vec<(&'static str, i64)>,
    pub strs: Vec<(&'static str, String)>,
    /// Tool click: (entity or 0 for the world, position, normal).
    pub hit: Option<(u32, Vec3, Vec3)>,
    pub objects: Vec<(u32, Vec3, Vec3)>,
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
    SetVelocity {
        ent: u32,
        velocity: Vec3,
        angular: Vec3,
    },
    /// `constraint.Weld` owned by the server (owner 0: no player limits, no undo entry).
    Weld {
        ent: u32,
        ent2: u32,
        forcelimit: f32,
        nocollide: bool,
    },
    /// `constraint.NoCollide` owned by the server (`ent2` 0 = the world).
    NoCollide {
        ent: u32,
        ent2: u32,
    },
}

/// Shared between the Rust side of an actor and the closures its VM calls.
pub struct Shared {
    pub snapshot: RwLock<Arc<Snapshot>>,
    pub queue: Mutex<Vec<(u64, QueuedWrite)>>,
    pub queued: Mutex<u64>,
    pub prints: Mutex<Vec<String>>,
    pub can_write: bool,
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
-- timers: delays in seconds, counted in ticks of game.interval(), run at the start of every
-- tick.pre before the handlers; a callback that errors destroys its timer (the error is printed)
timer = {}
local timers = {}
local anon = 0
local function ticks_of(delay: number): number
    local n = math.ceil(delay / game.interval() - 1e-6)
    if n < 1 then n = 1 end
    return n
end
local seq = 0
function timer.Create(name: string, delay: number, reps: number, fn)
    seq += 1
    timers[name] = { fn = fn, every = ticks_of(delay), left = ticks_of(delay), reps = reps, paused = false, seq = seq }
end
function timer.Simple(delay: number, fn)
    anon += 1
    timer.Create("__simple" .. anon, delay, 1, fn)
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
    t.left = t.every
    t.paused = false
    return true
end
function timer.Adjust(name: string, delay: number, reps: number?, fn): boolean
    local t = timers[name]
    if t == nil then return false end
    local every = ticks_of(delay)
    t.left = math.max(1, t.left + every - t.every)
    t.every = every
    if reps ~= nil then t.reps = reps end
    if fn ~= nil then t.fn = fn end
    return true
end
function timer.TimeLeft(name: string): number?
    local t = timers[name]
    if t == nil then return nil end
    return t.left * game.interval()
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
            t.left -= 1
            if t.left <= 0 then table.insert(due, name) end
        end
    end
    table.sort(due, function(a, b) return timers[a].seq < timers[b].seq end)
    for _, name in ipairs(due) do
        local t = timers[name]
        if t ~= nil then
            t.left = t.every
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
    if name == "tick.pre" then run_timers() end
    local list = handlers[name]
    if list == nil then return nil end
    for _, h in ipairs(list) do
        local r = h.fn(payload)
        if r == false then return false end
    end
    return nil
end
state = state or {}
tool = {}
local tools = {}
function tool.register(name: string, def)
    tools[name] = def
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
                let t = lua.create_table()?;
                t.set("id", p.id)?;
                t.set("origin", vec3_table(lua, p.origin)?)?;
                t.set("velocity", vec3_table(lua, p.velocity)?)?;
                t.set("view_angles", vec3_table(lua, p.view_angles)?)?;
                t.set("held", p.held)?;
                out.push(t)?;
            }
            Ok(out)
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
        lua.create_function(move |_, (op, ent, arg): (String, u32, Value)| {
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
                "weld" => {
                    let t = table_arg(&arg)?;
                    QueuedWrite::Weld {
                        ent,
                        ent2: t.get::<Option<u32>>("ent2")?.unwrap_or(0),
                        forcelimit: t.get::<Option<f32>>("forcelimit")?.unwrap_or(0.0),
                        nocollide: t.get::<Option<bool>>("nocollide")?.unwrap_or(false),
                    }
                }
                "nocollide" => QueuedWrite::NoCollide {
                    ent,
                    ent2: table_arg(&arg)?.get::<Option<u32>>("ent2")?.unwrap_or(0),
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

fn table_arg(v: &Value) -> Result<Table> {
    match v {
        Value::Table(t) => Ok(t.clone()),
        _ => Err(mlua::Error::runtime("expected a {x, y, z} table")),
    }
}

/// Lua table for an event payload.
pub fn payload_table(lua: &Lua, p: &Payload) -> Result<Table> {
    let t = lua.create_table()?;
    for (k, v) in &p.ints {
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
        QueuedWrite::Weld {
            ent,
            ent2,
            forcelimit,
            nocollide,
        } => {
            let world = |e: u32| if e == 0 { None } else { Some(e) };
            if !exists(server, *ent) || (*ent2 != 0 && !exists(server, *ent2)) {
                return false;
            }
            let made = server
                .sandbox
                .weld(0, world(*ent), world(*ent2), *forcelimit, *nocollide)
                .is_some();
            if made {
                server.sync_after_script();
            }
            made
        }
        QueuedWrite::NoCollide { ent, ent2 } => {
            let world = |e: u32| if e == 0 { None } else { Some(e) };
            if !exists(server, *ent) || (*ent2 != 0 && !exists(server, *ent2)) {
                return false;
            }
            let made = server
                .sandbox
                .nocollide(0, world(*ent), world(*ent2))
                .is_some();
            if made {
                server.sync_after_script();
            }
            made
        }
    }
}
