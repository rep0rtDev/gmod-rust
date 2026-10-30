//! Wire form of a Lua value for addon messages: nil, boolean, number, string and tables of
//! those (string or integer keys), nested at most `MAX_DEPTH` deep, at most `MAX_BYTES` long.

use mlua::{Lua, Result, Table, Value};

pub const MAX_BYTES: usize = 64 * 1024;
pub const MAX_DEPTH: usize = 16;

const T_NIL: u8 = 0;
const T_FALSE: u8 = 1;
const T_TRUE: u8 = 2;
const T_INT: u8 = 3;
const T_NUM: u8 = 4;
const T_STR: u8 = 5;
const T_TABLE: u8 = 6;

fn err(msg: &str) -> mlua::Error {
    mlua::Error::runtime(msg.to_string())
}

pub fn encode(v: &Value) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    put(&mut out, v, 0)?;
    if out.len() > MAX_BYTES {
        return Err(err("net message over 64 KiB"));
    }
    Ok(out)
}

fn put(out: &mut Vec<u8>, v: &Value, depth: usize) -> Result<()> {
    match v {
        Value::Nil => out.push(T_NIL),
        Value::Boolean(false) => out.push(T_FALSE),
        Value::Boolean(true) => out.push(T_TRUE),
        Value::Integer(i) => {
            out.push(T_INT);
            out.extend_from_slice(&i.to_le_bytes());
        }
        Value::Number(n) => {
            out.push(T_NUM);
            out.extend_from_slice(&n.to_le_bytes());
        }
        Value::String(s) => {
            let b = s.as_bytes();
            if b.len() > u16::MAX as usize {
                return Err(err("net string over 64 KiB"));
            }
            out.push(T_STR);
            out.extend_from_slice(&(b.len() as u16).to_le_bytes());
            out.extend_from_slice(&b);
        }
        Value::Table(t) => {
            if depth >= MAX_DEPTH {
                return Err(err("net table nested too deep"));
            }
            out.push(T_TABLE);
            let at = out.len();
            out.extend_from_slice(&0u16.to_le_bytes());
            let mut n = 0u16;
            for pair in t.clone().pairs::<Value, Value>() {
                let (k, val) = pair?;
                match k {
                    Value::Integer(_) | Value::String(_) => {}
                    Value::Number(f) if f.fract() == 0.0 => {}
                    other => {
                        return Err(err(&format!(
                            "net table key must be a string or integer, got {}",
                            other.type_name()
                        )))
                    }
                }
                put(out, &k, depth + 1)?;
                put(out, &val, depth + 1)?;
                n = n
                    .checked_add(1)
                    .ok_or_else(|| err("net table over 65535 entries"))?;
            }
            out[at..at + 2].copy_from_slice(&n.to_le_bytes());
        }
        other => return Err(err(&format!("net cannot send a {}", other.type_name()))),
    }
    Ok(())
}

pub fn decode(lua: &Lua, data: &[u8]) -> Result<Value> {
    let mut pos = 0;
    let v = get(lua, data, &mut pos, 0)?;
    if pos != data.len() {
        return Err(err("net message has trailing bytes"));
    }
    Ok(v)
}

fn take<'a>(data: &'a [u8], pos: &mut usize, n: usize) -> Result<&'a [u8]> {
    if *pos + n > data.len() {
        return Err(err("net message truncated"));
    }
    let s = &data[*pos..*pos + n];
    *pos += n;
    Ok(s)
}

fn get(lua: &Lua, data: &[u8], pos: &mut usize, depth: usize) -> Result<Value> {
    Ok(match take(data, pos, 1)?[0] {
        T_NIL => Value::Nil,
        T_FALSE => Value::Boolean(false),
        T_TRUE => Value::Boolean(true),
        T_INT => Value::Integer(i64::from_le_bytes(take(data, pos, 8)?.try_into().unwrap())),
        T_NUM => Value::Number(f64::from_le_bytes(take(data, pos, 8)?.try_into().unwrap())),
        T_STR => {
            let n = u16::from_le_bytes(take(data, pos, 2)?.try_into().unwrap()) as usize;
            Value::String(lua.create_string(take(data, pos, n)?)?)
        }
        T_TABLE => {
            if depth >= MAX_DEPTH {
                return Err(err("net table nested too deep"));
            }
            let n = u16::from_le_bytes(take(data, pos, 2)?.try_into().unwrap());
            let t: Table = lua.create_table()?;
            for _ in 0..n {
                let k = get(lua, data, pos, depth + 1)?;
                let v = get(lua, data, pos, depth + 1)?;
                t.raw_set(k, v)?;
            }
            Value::Table(t)
        }
        _ => return Err(err("net message has an unknown tag")),
    })
}
