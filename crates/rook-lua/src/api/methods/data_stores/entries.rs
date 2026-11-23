//! Reading and writing keys (`GlobalDataStore`'s methods) and an ordered
//! store's sorted pages. A standard store's reads return the value and a
//! `DataStoreKeyInfo`; an ordered store's have no key info (creator-docs
//! GlobalDataStore).

use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;
use serde_json::{Map, Value as Json};

use super::info::{key_info, options_of};
use super::{clock, target};
use crate::data_stores::{
    Content, Found, Listing, StoreError, StoreId, check_key, metadata_of, ordered_value,
    stored_value, user_ids_of,
};
use crate::json;
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, with_world};

/// Keys `BatchGetAsync` takes (creator-docs: "default 100").
const BATCH_LIMIT: usize = 100;

/// The key the read cache knows a store's key by: stores sharing data
/// (a scope and `AllScopes`) share it.
pub(super) fn cache_key(store: &StoreId, key: &str) -> Result<String, StoreError> {
    Ok(match store {
        StoreId::Standard { name, scope } => format!("{name:?}\0{}", scope.full_key(key)?),
        StoreId::Ordered { name, scope } => {
            check_key(key)?;
            format!("ordered\0{name}\0{scope}\0{key}")
        }
    })
}

/// Whether a `GetAsync` is answered from the cache: its options allow it
/// (`DataStoreGetOptions.UseCache`, true by default) and the key is
/// cached.
pub(super) fn from_cache(lua: &Lua, store: &StoreId, a: &Args) -> ApiResult<bool> {
    let key = cache_key(store, &a.string(0)?)?;
    let use_cache = match a.opt_instance(1)? {
        Some(options) => {
            with_world(lua, |w| w.dom.bool_property(options, "UseCache")).unwrap_or(true)
        }
        None => true,
    };
    Ok(use_cache && with_world(lua, |w| w.data_stores.cache.is_fresh(&key, clock(w).0)))
}

/// A standard read's two results.
pub(super) fn found(lua: &Lua, found: Option<Found>) -> ApiResult<Outcome> {
    match found {
        Some((value, info)) => Outcome::ret(
            lua,
            (json::to_lua(lua, &value)?, key_info(lua, Some(info))?),
        ),
        None => Outcome::ret(lua, (LuaValue::Nil, LuaValue::Nil)),
    }
}

fn ordered_result(lua: &Lua, value: Option<i64>) -> ApiResult<Outcome> {
    Outcome::ret(lua, value)
}

/// `GetAsync(key, options)`: the latest value, nil for a key removed or
/// never written. A read from the backend caches the key.
pub(super) fn get(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let store = target(lua, this)?;
    let key = a.string(0)?;
    let cached = from_cache(lua, &store, a)?;
    let ck = cache_key(&store, &key)?;
    with_world(lua, |w| {
        if !cached {
            w.data_stores.cache.touch(&ck, clock(w).0);
        }
    });
    match &store {
        StoreId::Standard { name, scope } => {
            let full = scope.full_key(&key)?;
            let latest = with_world(lua, |w| {
                (w.data_stores.standard.get(name)).and_then(|s| s.latest(&full))
            });
            found(lua, latest)
        }
        StoreId::Ordered { name, scope } => {
            let value = with_world(lua, |w| {
                (w.data_stores.ordered.get(name, scope)).and_then(|s| s.get(&key))
            });
            ordered_result(lua, value)
        }
    }
}

/// A write's value: nil isn't one (Inference: `Argument 2 missing or
/// nil`, as for any missing argument).
fn value_arg(a: &Args, i: usize) -> ApiResult<&LuaValue> {
    match a.get(i) {
        LuaValue::Nil => Err(a.missing(i)),
        v => Ok(v),
    }
}

/// `SetAsync(key, value, userIds, options)`: the new version's string.
/// Ordered stores take no user IDs (creator-docs: they don't support
/// them; Inference: they're ignored) and have no versions.
pub(super) fn set(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let store = target(lua, this)?;
    let key = a.string(0)?;
    let value = value_arg(a, 1)?;
    let ck = cache_key(&store, &key)?;
    match &store {
        StoreId::Standard { name, scope } => {
            let full = scope.full_key(&key)?;
            let json = stored_value(value)?;
            let user_ids = user_ids_of(a.get(2))?;
            let metadata = options_of(lua, a.opt_instance(3)?)?;
            let info = with_world(lua, |w| {
                let (now, ms) = clock(w);
                w.data_stores.cache.touch(&ck, now);
                (w.data_stores.standard.get_mut(name)).write(
                    &full,
                    Content::Value(json),
                    user_ids,
                    metadata,
                    ms,
                )
            });
            Outcome::ret(lua, info.version)
        }
        StoreId::Ordered { name, scope } => {
            let value = ordered_value(value)?;
            with_world(lua, |w| {
                w.data_stores.cache.touch(&ck, clock(w).0);
                w.data_stores.ordered.get_mut(name, scope).set(&key, value);
            });
            Outcome::ret(lua, ())
        }
    }
}

/// The type error 103 names for a stored value that isn't an integer.
fn json_type(value: &Json) -> &'static str {
    match value {
        Json::Null => "nil",
        Json::Bool(_) => "boolean",
        Json::Number(_) => "double",
        Json::String(_) => "string",
        Json::Array(_) => "Array",
        Json::Object(_) => "Dictionary",
    }
}

/// `IncrementAsync(key, delta = 1, userIds, options)`: the new value and,
/// for a standard store, its key info. Both must be integers
/// (creator-docs); a fractional delta is cut to one, as Roblox passes it
/// as an `int` (Inference). A sum past a 64-bit integer raises, leaving
/// the value as it was (Inference: Roblox's limit isn't documented).
pub(super) fn increment(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let store = target(lua, this)?;
    let key = a.string(0)?;
    let delta = a.number_or(1, 1.0)?.trunc() as i64;
    match &store {
        StoreId::Standard { name, scope } => {
            let full = scope.full_key(&key)?;
            let user_ids = user_ids_of(a.get(2))?;
            let metadata = options_of(lua, a.opt_instance(3)?)?;
            let result: ApiResult<Found> = with_world(lua, |w| {
                let (_, ms) = clock(w);
                let s = w.data_stores.standard.get_mut(name);
                let current = match s.latest(&full) {
                    None => 0,
                    Some((Json::Number(n), _)) if n.as_i64().is_some() => n.as_i64().unwrap_or(0),
                    Some((other, _)) => {
                        return Err(StoreError::NotAllowed(json_type(&other).into()).into());
                    }
                };
                let sum = i64::checked_add(current, delta).ok_or(StoreError::IncrementOverflow)?;
                let value = Json::from(sum);
                let info = s.write(&full, Content::Value(value.clone()), user_ids, metadata, ms);
                Ok((value, info))
            });
            found(lua, Some(result?))
        }
        StoreId::Ordered { name, scope } => {
            check_key(&key)?;
            let value = with_world(lua, |w| {
                let s = w.data_stores.ordered.get_mut(name, scope);
                let value = (s.get(&key).unwrap_or(0))
                    .checked_add(delta)
                    .ok_or(StoreError::IncrementOverflow)?;
                s.set(&key, value);
                Ok::<_, StoreError>(value)
            })?;
            ordered_result(lua, Some(value))
        }
    }
}

/// `RemoveAsync(key)`: the value it had and that version's key info; a
/// standard key stays as a deleted version, an ordered one is gone
/// (creator-docs).
pub(super) fn remove(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let store = target(lua, this)?;
    let key = a.string(0)?;
    match &store {
        StoreId::Standard { name, scope } => {
            let full = scope.full_key(&key)?;
            let old = with_world(lua, |w| {
                let (_, ms) = clock(w);
                w.data_stores.standard.get_mut(name).remove(&full, ms)
            });
            found(lua, old)
        }
        StoreId::Ordered { name, scope } => {
            check_key(&key)?;
            let old = with_world(lua, |w| {
                w.data_stores.ordered.get_mut(name, scope).remove(&key)
            });
            ordered_result(lua, old)
        }
    }
}

/// `UpdateAsync`'s read: the value and key info its transform gets.
pub(super) fn update_read(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    a.function(1)?;
    let store = target(lua, this)?;
    let key = a.string(0)?;
    match &store {
        StoreId::Standard { name, scope } => {
            let full = scope.full_key(&key)?;
            let latest = with_world(lua, |w| {
                (w.data_stores.standard.get(name)).and_then(|s| s.latest(&full))
            });
            found(lua, latest)
        }
        StoreId::Ordered { name, scope } => {
            let value = with_world(lua, |w| {
                (w.data_stores.ordered.get(name, scope)).and_then(|s| s.get(&key))
            });
            ordered_result(lua, value)
        }
    }
}

/// `UpdateAsync`'s write of what its transform returned (a value, then
/// user IDs and metadata, which replace the key's: creator-docs), giving
/// the stored value and its key info.
pub(super) fn update_write(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let store = target(lua, this)?;
    let key = a.string(0)?;
    let value = value_arg(a, 1)?;
    let ck = cache_key(&store, &key)?;
    match &store {
        StoreId::Standard { name, scope } => {
            let full = scope.full_key(&key)?;
            let json = stored_value(value)?;
            let user_ids = user_ids_of(a.get(2))?;
            let metadata = match a.get(3) {
                LuaValue::Nil => Map::new(),
                LuaValue::Table(t) => metadata_of(t)?,
                _ => return Err(StoreError::BadMetadata.into()),
            };
            let info = with_world(lua, |w| {
                let (now, ms) = clock(w);
                w.data_stores.cache.touch(&ck, now);
                (w.data_stores.standard.get_mut(name)).write(
                    &full,
                    Content::Value(json.clone()),
                    user_ids,
                    metadata,
                    ms,
                )
            });
            found(lua, Some((json, info)))
        }
        StoreId::Ordered { name, scope } => {
            let value = ordered_value(value)?;
            with_world(lua, |w| {
                w.data_stores.cache.touch(&ck, clock(w).0);
                w.data_stores.ordered.get_mut(name, scope).set(&key, value);
            });
            ordered_result(lua, Some(value))
        }
    }
}

/// The keys of a `BatchGetAsync`, checked.
pub(super) fn batch_keys(store: &StoreId, a: &Args) -> ApiResult<Vec<String>> {
    if !matches!(store, StoreId::Ordered { .. }) {
        return Err(StoreError::BatchOnStandard.into());
    }
    let LuaValue::Table(t) = a.get(0) else {
        return Err(a.wrong(0, "table"));
    };
    let keys = (t.sequence_values::<String>()).collect::<mlua::Result<Vec<_>>>()?;
    if keys.is_empty() || keys.len() > BATCH_LIMIT {
        return Err(StoreError::BatchSize.into());
    }
    for k in &keys {
        check_key(k)?;
    }
    Ok(keys)
}

/// `BatchGetAsync(keys)`: each key that has a value, as `{value = v}`
/// (creator-docs).
pub(super) fn batch_get(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let store = target(lua, this)?;
    let keys = batch_keys(&store, a)?;
    let StoreId::Ordered { name, scope } = &store else {
        return Err(StoreError::BatchOnStandard.into());
    };
    let values: Vec<(String, i64)> = with_world(lua, |w| {
        let s = w.data_stores.ordered.get(name, scope);
        (keys.into_iter())
            .filter_map(|k| Some((k.clone(), s?.get(&k)?)))
            .collect()
    });
    let out = lua.create_table()?;
    for (k, v) in values {
        let entry = lua.create_table_with_capacity(0, 1)?;
        entry.raw_set("value", v)?;
        out.raw_set(k, entry)?;
    }
    Outcome::ret(lua, out)
}

/// An optional bound of `GetSortedAsync`: nil, or an integer.
fn bound(a: &Args, i: usize, not_integer: StoreError) -> ApiResult<Option<i64>> {
    match a.get(i) {
        LuaValue::Nil => Ok(None),
        LuaValue::Integer(n) => Ok(Some(*n)),
        LuaValue::Number(n) if n.fract() == 0.0 => Ok(Some(*n as i64)),
        _ => Err(not_integer.into()),
    }
}

/// `GetSortedAsync(ascending, pageSize, minValue, maxValue)`.
pub(super) fn get_sorted(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let listing = sorted_listing(lua, this, a)?;
    super::pages::open(lua, "DataStorePages", listing)
}

/// `GetSortedAsync`'s arguments, checked: a page size from 1 to 100,
/// integer bounds with the minimum at most the maximum (creator-docs).
pub(super) fn sorted_listing(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Listing> {
    let StoreId::Ordered { name, scope } = target(lua, this)? else {
        return Err(ScriptError(
            "GetSortedAsync is a member of OrderedDataStore".into(),
        ));
    };
    let ascending = a.truthy(0);
    let size = a.number(1)?;
    if !(1.0..=100.0).contains(&size) {
        return Err(StoreError::PageSize.into());
    }
    let min = bound(a, 2, StoreError::MinNotInteger)?;
    let max = bound(a, 3, StoreError::MaxNotInteger)?;
    if let (Some(min), Some(max)) = (min, max)
        && max < min
    {
        return Err(StoreError::MinAboveMax.into());
    }
    Ok(Listing::Sorted {
        name,
        scope,
        ascending,
        min,
        max,
        size: size as usize,
        after: None,
    })
}
