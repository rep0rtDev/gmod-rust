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
}

/// A write an actor asked for; applied in the serial phase.
#[derive(Clone, Debug, PartialEq)]
pub enum QueuedWrite {
    SetPos { ent: u32, pos: Vec3 },
    SetAngles { ent: u32, angles: Vec3 },
    Freeze { ent: u32, frozen: bool },
    Remove { ent: u32 },
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
function __dispatch(name: string, payload)
    local list = handlers[name]
    if list == nil then return nil end
    for _, h in ipairs(list) do
        local r = h.fn(payload)
        if r == false then return false end
    end
    return nil
end
state = state or {}
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
    let sh = shared.clone();
    game.set(
        "tick",
        lua.create_function(move |_, ()| Ok(sh.snapshot.read().unwrap().tick))?,
    )?;
    g.set("game", game)?;
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
    Ok(t)
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
    }
}
