//! What data stores accept, and the error each limit raises: names,
//! scopes and keys, values as the JSON they're kept as, metadata and user
//! IDs (creator-docs "Data store error codes and limits").

use mlua::{Table, Value as LuaValue};
use rustc_hash::FxHashSet;
use serde_json::{Map, Value as Json};

use super::StoreError;
use crate::json;
use crate::runtime::ctx::args::{number_of, type_name};

/// Longest name, scope or key, in bytes as `string.len` counts them
/// (creator-docs: "Use `string.len()` to check their length").
const NAME_LIMIT: usize = 50;
/// Longest serialized value of one key.
const VALUE_LIMIT: usize = 4_194_304;
/// Longest metadata as a whole, one metadata key and one metadata value.
const METADATA_LIMIT: usize = 300;
const METADATA_KEY_LIMIT: usize = 50;
const METADATA_VALUE_LIMIT: usize = 250;
/// Most user IDs on one key. Inference: older creator docs' limits table
/// gave 4; today's leaves the number out of its error 512.
const USER_ID_LIMIT: usize = 4;
/// Deepest nesting a stored value may have (creator-docs server error
/// `InvalidJson`).
const DEPTH_LIMIT: usize = 400;

pub(crate) fn check_key(key: &str) -> Result<(), StoreError> {
    match key.len() {
        0 => Err(StoreError::KeyEmpty),
        n if n > NAME_LIMIT => Err(StoreError::KeyTooLong),
        _ => Ok(()),
    }
}

pub(crate) fn check_name(name: &str) -> Result<(), StoreError> {
    match name.len() {
        0 => Err(StoreError::NameEmpty),
        n if n > NAME_LIMIT => Err(StoreError::NameTooLong),
        _ => Ok(()),
    }
}

pub(crate) fn check_scope(scope: &str) -> Result<(), StoreError> {
    match scope.len() {
        0 => Err(StoreError::ScopeEmpty),
        n if n > NAME_LIMIT => Err(StoreError::ScopeTooLong),
        _ => Ok(()),
    }
}

/// A value as a standard store keeps it. Anything JSON can't hold
/// (instances and other datatypes, functions, strings that aren't UTF-8,
/// NaN and infinities, cycles) is error 104, named by the value's type,
/// or for a table by its shape (`Array` or `Dictionary`) as Roblox names
/// tables in it (Inference).
pub(crate) fn stored_value(value: &LuaValue) -> Result<Json, StoreError> {
    let json = convert(value, &mut FxHashSet::default(), 0)
        .map_err(|Unstorable| StoreError::CantStore(stored_type(value)))?;
    if serialized_len(&json) > VALUE_LIMIT {
        return Err(StoreError::TooLarge);
    }
    Ok(json)
}

/// A value as an ordered store keeps it: an integer (creator-docs
/// OrderedDataStore). Anything else is error 103, a fractional number
/// named `double` as Roblox's engine names its numbers (Inference).
pub(crate) fn ordered_value(value: &LuaValue) -> Result<i64, StoreError> {
    match value {
        LuaValue::Integer(i) => Ok(*i),
        LuaValue::Number(n) if n.fract() == 0.0 && n.abs() < 9.2e18 => Ok(*n as i64),
        LuaValue::Number(_) => Err(StoreError::NotAllowed("double".into())),
        other => Err(StoreError::NotAllowed(type_name(other))),
    }
}

/// Metadata set with `SetMetadata`: string keys and storable values,
/// within the metadata limits.
pub(crate) fn metadata_of(t: &Table) -> Result<Map<String, Json>, StoreError> {
    let mut out = Map::new();
    for pair in t.clone().pairs::<LuaValue, LuaValue>() {
        let (k, v) = pair.map_err(|_| StoreError::BadMetadata)?;
        let LuaValue::String(k) = k else {
            return Err(StoreError::BadMetadata);
        };
        let k = k.to_str().map_err(|_| StoreError::BadMetadata)?.to_owned();
        let v = convert(&v, &mut FxHashSet::default(), 0).map_err(|_| StoreError::BadMetadata)?;
        if k.len() > METADATA_KEY_LIMIT || serialized_len(&v) > METADATA_VALUE_LIMIT {
            return Err(StoreError::MetadataTooLarge);
        }
        out.insert(k, v);
    }
    if serialized_len(&Json::Object(out.clone())) > METADATA_LIMIT {
        return Err(StoreError::MetadataTooLarge);
    }
    Ok(out)
}

/// The `userIds` argument: nil or an array of numbers.
pub(crate) fn user_ids_of(value: &LuaValue) -> Result<Vec<i64>, StoreError> {
    let t = match value {
        LuaValue::Nil => return Ok(Vec::new()),
        LuaValue::Table(t) => t,
        _ => return Err(StoreError::BadUserId),
    };
    let mut ids = Vec::new();
    for v in t.sequence_values::<LuaValue>() {
        let v = v.map_err(|_| StoreError::BadUserId)?;
        let id = match v {
            LuaValue::Integer(_) | LuaValue::Number(_) => number_of(&v),
            _ => None,
        };
        // A user id is a whole number (Inference: Roblox's check isn't
        // documented; NaN or a fraction would be stored as another id).
        let id = id.filter(|n| n.is_finite() && n.fract() == 0.0);
        ids.push(id.ok_or(StoreError::BadUserId)? as i64);
    }
    if ids.len() > USER_ID_LIMIT {
        return Err(StoreError::TooManyUserIds);
    }
    Ok(ids)
}

/// A value JSON can't hold.
struct Unstorable;

fn convert(v: &LuaValue, seen: &mut FxHashSet<usize>, depth: usize) -> Result<Json, Unstorable> {
    Ok(match v {
        LuaValue::Nil => Json::Null,
        LuaValue::Boolean(b) => Json::Bool(*b),
        LuaValue::Integer(i) => Json::from(*i),
        LuaValue::Number(n) if n.is_finite() => json::number(*n),
        LuaValue::String(s) => Json::String(s.to_str().map_err(|_| Unstorable)?.to_owned()),
        LuaValue::Table(t) => {
            let at = t.to_pointer() as usize;
            if depth >= DEPTH_LIMIT || !seen.insert(at) {
                return Err(Unstorable);
            }
            let json = table(t, seen, depth).map_err(|_| Unstorable)?;
            seen.remove(&at);
            json
        }
        _ => return Err(Unstorable),
    })
}

fn table(t: &Table, seen: &mut FxHashSet<usize>, depth: usize) -> Result<Json, Unstorable> {
    if let Some(len) = json::array_length(t) {
        let mut out = Vec::with_capacity(len);
        for i in 1..=len {
            let v = t.raw_get::<LuaValue>(i).map_err(|_| Unstorable)?;
            out.push(convert(&v, seen, depth + 1)?);
        }
        return Ok(Json::Array(out));
    }
    let mut out = Map::new();
    for pair in t.clone().pairs::<LuaValue, LuaValue>() {
        let (k, v) = pair.map_err(|_| Unstorable)?;
        if let LuaValue::String(s) = &k
            && s.to_str().is_err()
        {
            return Err(Unstorable);
        }
        let key = json::object_key(&k).ok_or(Unstorable)?;
        out.insert(key, convert(&v, seen, depth + 1)?);
    }
    Ok(Json::Object(out))
}

/// The type error 104 names.
fn stored_type(value: &LuaValue) -> String {
    match value {
        LuaValue::Table(t) if json::array_length(t).is_some() => "Array".into(),
        LuaValue::Table(_) => "Dictionary".into(),
        other => type_name(other),
    }
}

/// How long a value's JSON is, without writing it out.
fn serialized_len(json: &Json) -> usize {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0 += buf.len();
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    // Writing a `Value` to a sink that never fails can't fail.
    let _ = serde_json::to_writer(&mut count, json);
    count.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_count_bytes() {
        assert_eq!(check_key(""), Err(StoreError::KeyEmpty));
        assert_eq!(check_key(&"a".repeat(50)), Ok(()));
        assert_eq!(check_key(&"a".repeat(51)), Err(StoreError::KeyTooLong));
        // 25 two-byte characters are 50 bytes.
        assert_eq!(check_key(&"é".repeat(25)), Ok(()));
        assert_eq!(check_key(&"é".repeat(26)), Err(StoreError::KeyTooLong));
    }

    #[test]
    fn serialized_len_counts_the_json() {
        let json = serde_json::json!({"a": [1, "b"]});
        assert_eq!(serialized_len(&json), json.to_string().len());
    }
}
