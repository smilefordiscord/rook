//! `DataStore`'s versions and listings: reading a key's older versions
//! by string or by time, removing one for good, and listing keys and
//! versions (creator-docs DataStore).

use mlua::Lua;
use rook_dom::InstanceId;

use super::entries::found;
use super::pages::{open, page_size};
use super::{clock, target};
use crate::data_stores::{
    KEYS_PAGE, Listing, Scope, StoreError, StoreId, StoreName, VERSIONS_PAGE,
};
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, with_world};

/// How far ahead of now `GetVersionAtTimeAsync` may ask, in
/// milliseconds (creator-docs: "not more than ten minutes in the future").
const FUTURE_LIMIT: i64 = 10 * 60 * 1000;

/// A standard store and the stored key a request names.
pub(super) fn standard_key(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<(StoreName, Scope, String)> {
    let key = a.string(0)?;
    match target(lua, this)? {
        StoreId::Standard { name, scope } => {
            let full = scope.full_key(&key)?;
            Ok((name, scope, full))
        }
        StoreId::Ordered { .. } => Err(ScriptError("Ordered data stores have no versions".into())),
    }
}

/// `GetVersionAsync(key, version)`: that version's value and key info, or
/// nil for a version removed or deleted (Inference: as `GetAsync` reads a
/// deleted key).
pub(super) fn get_version(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (name, _, full) = standard_key(lua, this, a)?;
    let version = a.string(1)?;
    let hit = with_world(lua, |w| {
        let now = clock(w).1;
        (w.data_stores.standard.get(&name)).and_then(|s| s.version(&full, &version, now))
    });
    found(lua, hit)
}

/// The timestamp `GetVersionAtTimeAsync` takes, checked.
pub(super) fn timestamp(lua: &Lua, a: &Args) -> ApiResult<i64> {
    let time = a.number(1)? as i64;
    let now = with_world(lua, |w| clock(w).1);
    if time <= 0 || time > now + FUTURE_LIMIT {
        return Err(StoreError::BadTimestamp.into());
    }
    Ok(time)
}

/// `GetVersionAtTimeAsync(key, timestamp)`: the version current at that
/// Unix time in milliseconds, or nil (creator-docs).
pub(super) fn get_version_at_time(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (name, _, full) = standard_key(lua, this, a)?;
    let time = timestamp(lua, a)?;
    let hit = with_world(lua, |w| {
        let now = clock(w).1;
        (w.data_stores.standard.get(&name)).and_then(|s| s.version_at(&full, time, now))
    });
    found(lua, hit)
}

/// `RemoveVersionAsync(key, version)`: that version is gone for good,
/// with no tombstone (creator-docs).
pub(super) fn remove_version(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (name, _, full) = standard_key(lua, this, a)?;
    let version = a.string(1)?;
    with_world(lua, |w| {
        let now = clock(w).1;
        w.data_stores
            .standard
            .get_mut(&name)
            .purge(&full, &version, now);
    });
    Outcome::ret(lua, ())
}

/// `ListKeysAsync(prefix, pageSize, cursor, excludeDeleted)`: the store's
/// keys in its scope, or every scope's as `scope/key` for `AllScopes`
/// (creator-docs).
pub(super) fn list_keys(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let StoreId::Standard { name, scope } = target(lua, this)? else {
        return Err(ScriptError(
            "Ordered data stores have no key listing".into(),
        ));
    };
    let prefix = a.opt_string(0)?.unwrap_or_default();
    let size = page_size(a, 1, KEYS_PAGE)?;
    let after = (a.opt_string(2)?)
        .filter(|c| !c.is_empty())
        .map(|c| format!("{}{c}", scope.prefix()));
    let listing = Listing::Keys {
        store: name,
        scope,
        prefix,
        size,
        exclude_deleted: a.truthy(3),
        after,
    };
    open(lua, "DataStoreKeyPages", listing)
}

/// `ListVersionsAsync(key, sortDirection, minDate, maxDate, pageSize)`:
/// the key's versions, oldest or newest first, between the dates (Unix
/// milliseconds, 0 for no bound), deleted ones included and marked.
pub(super) fn list_versions(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (name, _, full) = standard_key(lua, this, a)?;
    let descending = match a.get(1) {
        mlua::Value::Nil => false,
        _ => a.enum_value(lua, 1, "SortDirection")? == 1,
    };
    let date = |i| -> ApiResult<Option<i64>> {
        let n = a.number_or(i, 0.0)? as i64;
        Ok((n > 0).then_some(n))
    };
    let listing = Listing::Versions {
        store: name,
        key: full,
        descending,
        min: date(2)?,
        max: date(3)?,
        size: page_size(a, 4, VERSIONS_PAGE)?,
        after: None,
    };
    open(lua, "DataStoreVersionPages", listing)
}
