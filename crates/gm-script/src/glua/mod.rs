//! GLua-compatible surface for actors: the documented Garry's Mod Lua API (libraries, classes,
//! globals) installed into every actor VM on top of the own `world` / `events` API. Server and
//! shared realms first; client / menu realm functions are catalogued in
//! `tools/glua/manifest.json` and arrive with the frontend. Coverage is tracked function by
//! function: `examples/glua_list.rs` lists what a VM really has, `tools/glua/build_manifest.py`
//! writes the statuses, `tests/glua_coverage.rs` refuses a manifest that claims more.
//!
//! Everything here is written from the documented behaviour; no Garry's Mod Lua is copied.

use std::sync::Arc;
use std::time::Instant;

use mlua::{Lua, Result, Table, Value};

use crate::api::Shared;

/// The core libraries in Luau: Vector / Angle / Color, math / string / table / bit / utf8
/// extensions, hook, timer and the time globals.
const CORE: &str = include_str!("core.luau");
/// Entity / Player / NPC / Vehicle / Weapon / PhysObj, ents / player / game / engine / util and
/// the entity globals over the host snapshot and write queue.
const ENTITY: &str = include_str!("entity.luau");
/// net, ConVar / cvars, concommand, file, CTakeDamageInfo, sound, undo / cleanup / constraint
/// and the GM hook bridge from the own server events.
const SERVER: &str = include_str!("server.luau");
/// math.ease, VMatrix, CEffectData.
const CLASSES: &str = include_str!("classes.luau");
/// scripted_ents / weapons / baseclass: ENT and SWEP tables over the host scripted classes.
const SCRIPTED: &str = include_str!("scripted.luau");

/// Registers a class metatable under `__glua_classes[name]` so the coverage listing (and
/// `FindMetaTable`) can reach it.
pub fn register_class(lua: &Lua, name: &str, meta: &Table) -> Result<()> {
    let classes = classes(lua)?;
    classes.set(name, meta.clone())
}

fn classes(lua: &Lua) -> Result<Table> {
    let g = lua.globals();
    Ok(match g.get::<Value>("__glua_classes")? {
        Value::Table(t) => t,
        _ => {
            let t = lua.create_table()?;
            g.set("__glua_classes", t.clone())?;
            t
        }
    })
}

/// A documented function that exists but cannot do its job in this realm / build yet: calling
/// it raises a clear error, and the coverage listing reports it as a stub, never as implemented.
pub fn stub(lua: &Lua, address: &str) -> Result<mlua::Function> {
    let g = lua.globals();
    let stubs: Table = match g.get::<Value>("__glua_stubs")? {
        Value::Table(t) => t,
        _ => {
            let t = lua.create_table()?;
            g.set("__glua_stubs", t.clone())?;
            t
        }
    };
    stubs.set(address, true)?;
    let address = address.to_owned();
    lua.create_function(move |_, _: mlua::Variadic<Value>| -> Result<()> {
        Err(mlua::Error::runtime(format!(
            "{address}: not available in this realm yet"
        )))
    })
}

pub fn install(lua: &Lua, _shared: Arc<Shared>) -> Result<()> {
    let g = lua.globals();
    let start = Instant::now();
    g.set(
        "__glua_systime",
        lua.create_function(move |_, ()| Ok(start.elapsed().as_secs_f64()))?,
    )?;
    classes(lua)?;
    g.set(
        "__glua_stub",
        lua.create_function(|lua, address: String| stub(lua, &address))?,
    )?;
    lua.load(CORE).set_name("glua/core").exec()?;
    lua.load(ENTITY).set_name("glua/entity").exec()?;
    lua.load(SERVER).set_name("glua/server").exec()?;
    lua.load(CLASSES).set_name("glua/classes").exec()?;
    lua.load(SCRIPTED).set_name("glua/scripted").exec()?;
    let classes = classes(lua)?;
    g.set(
        "FindMetaTable",
        lua.create_function(move |_, name: String| classes.get::<Value>(name.as_str()))?,
    )?;
    Ok(())
}

/// Every documented address the VM really provides: `lib.fn` for functions reachable from a
/// global table, `Class:method` for the registered class metatables, `Global/fn` for global
/// functions. Stubs are listed separately.
pub fn list(lua: &Lua) -> Result<(Vec<String>, Vec<String>)> {
    let g = lua.globals();
    let mut present = Vec::new();
    for pair in g.pairs::<String, Value>() {
        let (name, v) = pair?;
        match v {
            Value::Function(_) => present.push(format!("Global.{name}")),
            Value::Table(t) if !name.starts_with("__glua") => {
                for inner in t.pairs::<String, Value>() {
                    let (k, iv) = inner?;
                    if !matches!(iv, Value::Nil | Value::Table(_)) {
                        present.push(format!("{name}.{k}"));
                    }
                }
            }
            _ => {}
        }
    }
    // Hooks are not Lua values: a GM hook counts as present when a bridge raises it
    // (`hook.Run("Name"`), a scripted ENTITY / WEAPON callback when the dispatcher that calls it
    // is annotated `-- @hook CLASS:Name`.
    for src in [CORE, ENTITY, SERVER, CLASSES, SCRIPTED] {
        for (marker, prefix) in [("hook.Run(\"", "GM:"), ("@hook ", "")] {
            let mut rest = src;
            while let Some(i) = rest.find(marker) {
                rest = &rest[i + marker.len()..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == ':')
                    .collect();
                if !name.is_empty() && name != "event" {
                    present.push(format!("{prefix}{name}"));
                }
            }
        }
    }
    present.sort();
    present.dedup();
    if let Value::Table(classes) = g.get::<Value>("__glua_classes")? {
        for pair in classes.pairs::<String, Table>() {
            let (class, meta) = pair?;
            for inner in meta.pairs::<String, Value>() {
                let (k, iv) = inner?;
                if matches!(iv, Value::Function(_)) && !k.starts_with("__") {
                    present.push(format!("{class}:{k}"));
                }
            }
        }
    }
    let mut stubs = Vec::new();
    if let Value::Table(t) = g.get::<Value>("__glua_stubs")? {
        for pair in t.pairs::<String, bool>() {
            stubs.push(pair?.0);
        }
    }
    // A gamemode default on `GM` whose trigger no bridge raises is only a stub.
    if let Value::Table(gm) = g.get::<Value>("GM")? {
        for pair in gm.pairs::<String, Value>() {
            let (k, v) = pair?;
            let addr = format!("GM:{k}");
            if matches!(v, Value::Function(_)) && !present.contains(&addr) {
                stubs.push(addr);
            }
        }
    }
    present.sort();
    present.dedup();
    stubs.sort();
    Ok((present, stubs))
}
