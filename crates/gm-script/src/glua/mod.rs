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
/// `Enums/ACT` from the wiki: every activity id (models tag their sequences with the names).
const ACT: &str = include_str!("act.luau");
/// net, ConVar / cvars, concommand, file, CTakeDamageInfo, sound, undo / cleanup / constraint
/// and the GM hook bridge from the own server events.
const SERVER: &str = include_str!("server.luau");
/// math.ease, VMatrix, CEffectData.
const CLASSES: &str = include_str!("classes.luau");
/// scripted_ents / weapons / baseclass: ENT and SWEP tables over the host scripted classes.
const SCRIPTED: &str = include_str!("scripted.luau");
const LIBS: &str = include_str!("libs.luau");
const ENUMS: &str = include_str!("enums.luau");
const CLIENT: &str = include_str!("client.luau");
const DERMA: &str = include_str!("derma.luau");
/// The rest of the documented surface that needs no host object: debug / jit / sql / usermessage /
/// gameevent, IMaterial / ITexture / ISave / IRestore / SurfaceInfo, the leftover globals.
const COMPAT: &str = include_str!("compat.luau");
const HOOKS: &str = include_str!("hooks.luau");

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
    let fns: Table = match g.get::<Value>("__glua_stub_fns")? {
        Value::Table(t) => t,
        _ => {
            let t = lua.create_table()?;
            g.set("__glua_stub_fns", t.clone())?;
            t
        }
    };
    let owned = address.to_owned();
    let f = lua.create_function(move |_, _: mlua::Variadic<Value>| -> Result<()> {
        Err(mlua::Error::runtime(format!(
            "{owned}: not available in this realm yet"
        )))
    })?;
    fns.set(address, f.clone())?;
    Ok(f)
}

/// What a documented address resolves to in this VM: `Class:Name` through `__glua_classes`
/// (and the `__index` chain), `lib.Name` / `Global.Name` through the globals.
fn resolve_address(g: &Table, address: &str) -> Option<Value> {
    if let Some((class, name)) = address.split_once(':') {
        let Ok(Value::Table(classes)) = g.get::<Value>("__glua_classes") else {
            return None;
        };
        let mut table = classes.get::<Value>(class).ok().and_then(|v| match v {
            Value::Table(t) => Some(t),
            _ => None,
        });
        let mut depth = 0;
        while let Some(t) = table.take() {
            match t.get::<Value>(name) {
                Ok(Value::Nil) | Err(_) => {}
                Ok(v) => return Some(v),
            }
            depth += 1;
            if depth > 16 {
                break;
            }
            table = t
                .metatable()
                .and_then(|m| m.get::<Value>("__index").ok())
                .and_then(|v| match v {
                    Value::Table(b) => Some(b),
                    _ => None,
                });
        }
        return None;
    }
    let (lib, name) = address.split_once('.')?;
    if lib == "Global" {
        return g.get::<Value>(name).ok();
    }
    match g.get::<Value>(lib).ok()? {
        Value::Table(t) => t.get::<Value>(name).ok(),
        _ => None,
    }
}

pub fn install(lua: &Lua, shared: Arc<Shared>) -> Result<()> {
    let g = lua.globals();
    // realm globals: `CLIENT` / `SERVER` / `MENU_DLL` as the game defines them
    g.set("SERVER", shared.realm == "server")?;
    g.set("CLIENT", shared.realm == "client" || shared.realm == "menu")?;
    g.set("MENU_DLL", shared.realm == "menu")?;
    let start = Instant::now();
    g.set(
        "__glua_datestamp",
        lua.create_function(|_, ()| {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            let (y, mo, d, h, mi, s) = civil_from_unix(secs);
            Ok(format!("{y:04}-{mo:02}-{d:02} {h:02}-{mi:02}-{s:02}"))
        })?,
    )?;
    g.set(
        "__glua_unixtime",
        lua.create_function(|_, ()| {
            Ok(std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0.0, |d| d.as_secs_f64()))
        })?,
    )?;
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
    lua.load(ENUMS).set_name("glua/enums").exec()?;
    lua.load(ACT).set_name("glua/act").exec()?;
    lua.load(ENTITY).set_name("glua/entity").exec()?;
    lua.load(SERVER).set_name("glua/server").exec()?;
    lua.load(CLASSES).set_name("glua/classes").exec()?;
    lua.load(SCRIPTED).set_name("glua/scripted").exec()?;
    lua.load(LIBS).set_name("glua/libs").exec()?;
    lua.load(CLIENT).set_name("glua/client").exec()?;
    lua.load(DERMA).set_name("glua/derma").exec()?;
    lua.load(COMPAT).set_name("glua/compat").exec()?;
    lua.load(HOOKS).set_name("glua/hooks").exec()?;
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
                    match iv {
                        Value::Nil => {}
                        // one nested level: `math.ease.InBack`
                        Value::Table(inner_t) => {
                            for p in inner_t.pairs::<String, Value>() {
                                let (k2, v2) = p?;
                                if matches!(v2, Value::Function(_)) {
                                    present.push(format!("{name}.{k}.{k2}"));
                                }
                            }
                        }
                        _ => present.push(format!("{name}.{k}")),
                    }
                }
            }
            _ => {}
        }
    }
    // Hooks are not Lua values: a GM hook counts as present when a bridge raises it
    // (`hook.Run("Name"`), a scripted ENTITY / WEAPON callback when the dispatcher that calls it
    // is annotated `-- @hook CLASS:Name`.
    for src in [
        CORE, ENUMS, ACT, ENTITY, SERVER, CLASSES, SCRIPTED, LIBS, CLIENT, DERMA, COMPAT, HOOKS,
    ] {
        for (marker, prefix) in [
            ("hook.Run(\"", "GM:"),
            ("@hook ", ""),
            ("@enum ", "Enums/"),
            ("@struct ", "Structures/"),
            ("@field ", ""),
        ] {
            let mut rest = src;
            while let Some(i) = rest.find(marker) {
                rest = &rest[i + marker.len()..];
                let name: String = rest
                    .chars()
                    .take_while(|c| {
                        c.is_ascii_alphanumeric() || *c == '_' || *c == ':' || *c == '.'
                    })
                    .collect();
                if !name.is_empty() && name != "event" {
                    present.push(format!("{prefix}{name}"));
                    if prefix == "GM:" {
                        // the sandbox gamemode documents its hooks under SANDBOX: too
                        present.push(format!("SANDBOX:{name}"));
                    }
                }
            }
        }
    }
    present.sort();
    present.dedup();
    if let Value::Table(classes) = g.get::<Value>("__glua_classes")? {
        for pair in classes.pairs::<String, Table>() {
            let (class, meta) = pair?;
            // a control inherits its base's methods through the metatable's `__index` chain
            // (DFrame:SetPos is Panel's): those count for the class too
            let mut table = Some(meta);
            let mut depth = 0;
            while let Some(t) = table.take() {
                for inner in t.pairs::<String, Value>() {
                    let (k, iv) = inner?;
                    if matches!(iv, Value::Function(_)) && !k.starts_with("__") {
                        present.push(format!("{class}:{k}"));
                    }
                }
                depth += 1;
                if depth > 16 {
                    break;
                }
                table = t
                    .metatable()
                    .and_then(|m| m.get::<Value>("__index").ok())
                    .and_then(|v| match v {
                        Value::Table(b) => Some(b),
                        _ => None,
                    });
            }
        }
        present.sort();
        present.dedup();
    }
    // a stub an implementation later replaced at its address is no stub any more
    let mut stubs = Vec::new();
    let stub_fns = match g.get::<Value>("__glua_stub_fns")? {
        Value::Table(t) => Some(t),
        _ => None,
    };
    if let Value::Table(t) = g.get::<Value>("__glua_stubs")? {
        for pair in t.pairs::<String, bool>() {
            let addr = pair?.0;
            let replaced = match (&stub_fns, resolve_address(&g, &addr)) {
                (Some(fns), Some(Value::Function(cur))) => match fns.get::<Value>(addr.as_str())? {
                    Value::Function(orig) => orig != cur,
                    _ => false,
                },
                _ => false,
            };
            if !replaced {
                stubs.push(addr);
            }
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

/// Civil date from unix seconds (UTC), for `util.DateStamp`: `(year, month, day, h, m, s)`.
pub fn civil_from_unix(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (
        y,
        m,
        d,
        (rem / 3600) as u32,
        (rem % 3600 / 60) as u32,
        (rem % 60) as u32,
    )
}
