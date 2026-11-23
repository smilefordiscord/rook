//! Luau values as JSON: the shape rules `HttpService:JSONEncode` follows
//! (measured: docs/behavior/scripting/json.md), shared with data stores,
//! which keep values as the JSON they serialize to (creator-docs "Data
//! store error codes and limits": the value "is also stored as a string").

use mlua::{Lua, Table, Value as LuaValue};
use serde_json::Value as Json;

use crate::runtime::ctx::args::{format_number, number_of};

/// A number as JSON: a whole number that a double holds exactly is
/// written without a fraction.
pub(crate) fn number(n: f64) -> Json {
    if n.fract() == 0.0 && n.abs() < 9.0e15 {
        Json::Number((n as i64).into())
    } else {
        serde_json::Number::from_f64(n).map_or(Json::Null, Json::Number)
    }
}

/// How many entries a table writes as an array, or `None` for an object.
/// Measured in Studio 0.748: a table with a length (`#t`) is an array of
/// 1 to that length, nil as null and other keys dropped; an empty one is
/// `[]`; the rest are objects.
pub(crate) fn array_length(t: &Table) -> Option<usize> {
    let len = t.raw_len();
    (len > 0 || t.is_empty()).then_some(len)
}

/// An object's key: a string as it is, a number as `tostring` writes it;
/// any other key has no JSON form.
pub(crate) fn object_key(k: &LuaValue) -> Option<String> {
    match k {
        LuaValue::String(s) => Some(s.to_string_lossy()),
        LuaValue::Integer(_) | LuaValue::Number(_) => Some(format_number(number_of(k)?)),
        _ => None,
    }
}

/// JSON as Luau values: null is nil, arrays and objects are tables.
pub(crate) fn to_lua(lua: &Lua, v: &Json) -> mlua::Result<LuaValue> {
    Ok(match v {
        Json::Null => LuaValue::Nil,
        Json::Bool(b) => LuaValue::Boolean(*b),
        Json::Number(n) => LuaValue::Number(n.as_f64().unwrap_or(0.0)),
        Json::String(s) => LuaValue::String(lua.create_string(s)?),
        Json::Array(a) => {
            let t = lua.create_table_with_capacity(a.len(), 0)?;
            for (i, x) in a.iter().enumerate() {
                t.raw_set(i + 1, to_lua(lua, x)?)?;
            }
            LuaValue::Table(t)
        }
        Json::Object(o) => {
            let t = lua.create_table_with_capacity(0, o.len())?;
            for (k, x) in o {
                t.raw_set(k.as_str(), to_lua(lua, x)?)?;
            }
            LuaValue::Table(t)
        }
    })
}
