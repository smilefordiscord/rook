//! MemoryStoreService for scripts: the objects it hands out and their
//! methods, over the session's structures in [`crate::memory_stores`]
//! (`docs/behavior/scripting/memory-stores.md`).
//!
//! Each call answers at once (Approximation: Roblox's go to its servers
//! and yield for the round trip, as Rook's DataStores don't either).
//! `UpdateAsync` and `ReadAsync` are written in Luau: one calls the
//! script's transform function, the other waits for items.

use mlua::{Lua, MultiValue, Table, Value as LuaValue};
use rook_dom::InstanceId;
use serde_json::Value as Json;

use super::RawMethod;
use super::http::{from_json, to_json};
use super::pages::{self, Source};
use crate::api::instance::push_instance;
use crate::memory_stores::{
    self as stores, Bound, Handle, MAX_PAGE_COUNT, MAX_RANGE_COUNT, MAX_READ_COUNT,
    MemoryStoreError, MemoryStores, SortKey, Time,
};
use crate::runtime::ctx::args::{Args, number_of};
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, api_fn, with_world};

impl From<MemoryStoreError> for ScriptError {
    fn from(e: MemoryStoreError) -> Self {
        Self(e.to_string())
    }
}

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("MemoryStoreService", "GetSortedMap") => get_sorted_map,
        ("MemoryStoreService", "GetQueue") => get_queue,
        ("MemoryStoreService", "GetHashMap") => get_hash_map,
        ("MemoryStoreSortedMap", "GetAsync") => sorted_get,
        ("MemoryStoreSortedMap", "SetAsync") => sorted_set,
        ("MemoryStoreSortedMap", "RemoveAsync") => sorted_remove,
        ("MemoryStoreSortedMap", "GetRangeAsync") => sorted_range,
        ("MemoryStoreSortedMap", "GetSizeAsync") => sorted_size,
        ("MemoryStoreQueue", "AddAsync") => queue_add,
        ("MemoryStoreQueue", "RemoveAsync") => queue_remove,
        ("MemoryStoreQueue", "GetSizeAsync") => queue_size,
        ("MemoryStoreHashMap", "GetAsync") => hash_get,
        ("MemoryStoreHashMap", "SetAsync") => hash_set,
        ("MemoryStoreHashMap", "RemoveAsync") => hash_remove,
        ("MemoryStoreHashMap", "ListItemsAsync") => hash_list,
        _ => return None,
    })
}

pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    Some(match (owner, name) {
        // The transform gets the old value (and sort key) and returns the
        // new ones; nil cancels (creator-docs). One session has no other
        // server to conflict with, so it runs once.
        ("MemoryStoreSortedMap", "UpdateAsync") => {
            r#"
            local std = ...
            local ms, pcall, error = std.memorystore, std.pcall, std.error
            return function(self, key, transform, expiration)
                ms.expiration(key, transform, expiration)
                local old, oldSort = self:GetAsync(key)
                local ok, new, newSort = pcall(transform, old, oldSort)
                if not ok then
                    error(ms.transform_failed, 0)
                end
                if new == nil then
                    return nil
                end
                self:SetAsync(key, new, expiration, newSort)
                return new, newSort
            end
            "#
        }
        ("MemoryStoreHashMap", "UpdateAsync") => {
            r#"
            local std = ...
            local ms, pcall, error = std.memorystore, std.pcall, std.error
            return function(self, key, transform, expiration)
                ms.expiration(key, transform, expiration)
                local ok, new = pcall(transform, (self:GetAsync(key)))
                if not ok then
                    error(ms.transform_failed, 0)
                end
                if new == nil then
                    return nil
                end
                self:SetAsync(key, new, expiration)
                return new
            end
            "#
        }
        // Reads are tried again every two seconds until `waitTimeout`
        // runs out, forever at -1, its default (creator-docs
        // MemoryStoreQueue:ReadAsync). Nothing read returns an empty
        // array and no id (Inference).
        ("MemoryStoreQueue", "ReadAsync") => {
            r#"
            local std = ...
            local ms, wait = std.memorystore, std.task.wait
            return function(self, count, allOrNothing, waitTimeout)
                if waitTimeout == nil then
                    waitTimeout = -1
                end
                local items, id = ms.read(self, count, allOrNothing)
                local waited = 0
                while items == nil do
                    local step = 2
                    if waitTimeout >= 0 and waitTimeout - waited < step then
                        step = waitTimeout - waited
                    end
                    if step <= 0 then
                        return {}, nil
                    end
                    waited += wait(step)
                    items, id = ms.read(self, count, allOrNothing)
                end
                return items, id
            end
            "#
        }
        _ => return None,
    })
}

/// `std.memorystore`: what the Luau methods above call.
pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let std: Table = lua.named_registry_value("rook.std")?;
    let ms = lua.create_table()?;
    ms.set("read", api_fn(lua, read)?)?;
    ms.set(
        "expiration",
        api_fn(lua, |lua, args| {
            let a = Args::new(args, "UpdateAsync");
            stores::expiration(a.number(2)?)?;
            Outcome::ret(lua, ())
        })?,
    )?;
    ms.set("transform_failed", MemoryStoreError::Transform.to_string())?;
    std.set("memorystore", ms)
}

/// One object per structure name (and queue invisibility timeout), as
/// DataStoreService does (Inference).
fn object(lua: &Lua, class: &str, handle: Handle) -> ApiResult<Outcome> {
    let cache: Table = match lua.named_registry_value::<Option<Table>>("rook.memorystores")? {
        Some(t) => t,
        None => {
            let t = lua.create_table()?;
            lua.set_named_registry_value("rook.memorystores", t.clone())?;
            t
        }
    };
    let key = format!("{handle:?}");
    if let Some(u) = cache.raw_get::<Option<mlua::AnyUserData>>(key.as_str())? {
        return Outcome::ret(lua, u);
    }
    let name = match &handle {
        Handle::SortedMap(n) | Handle::Queue { name: n, .. } | Handle::HashMap(n) => n.clone(),
    };
    let id = with_world(lua, |w| -> Result<_, rook_dom::DomError> {
        let id = w.dom.create_by_name(class)?;
        w.dom.set_name(id, name)?;
        w.memory_stores.objects.insert(id, handle);
        Ok(id)
    })?;
    let ud = push_instance(lua, id)?;
    cache.raw_set(key, ud.clone())?;
    Outcome::ret(lua, ud)
}

fn get_sorted_map(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    object(lua, "MemoryStoreSortedMap", Handle::SortedMap(a.string(0)?))
}

fn get_queue(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let invisibility = a.number_or(1, 30.0)?.trunc().max(0.0);
    object(
        lua,
        "MemoryStoreQueue",
        Handle::Queue { name, invisibility },
    )
}

fn get_hash_map(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    object(lua, "MemoryStoreHashMap", Handle::HashMap(a.string(0)?))
}

/// Run `f` on the session's stores at the clock's time. Only servers
/// reach them (creator-docs "Memory stores": `InvalidClientAccess`).
fn with_stores<R>(
    lua: &Lua,
    this: InstanceId,
    f: impl FnOnce(&mut MemoryStores, &Handle, Time) -> Result<R, MemoryStoreError>,
) -> ApiResult<R> {
    with_world(lua, |w| {
        if w.is_client {
            return Err(MemoryStoreError::Client.into());
        }
        let now = Time(w.scheduler.now());
        let stores = &mut w.memory_stores;
        let handle = stores
            .objects
            .get(&this)
            .cloned()
            .ok_or(MemoryStoreError::Unavailable)?;
        f(stores, &handle, now).map_err(ScriptError::from)
    })
}

fn name(handle: &Handle) -> &str {
    match handle {
        Handle::SortedMap(n) | Handle::Queue { name: n, .. } | Handle::HashMap(n) => n,
    }
}

/// A value to store, as the JSON Roblox converts it to.
fn value(a: &Args, i: usize) -> ApiResult<Json> {
    match a.get(i) {
        LuaValue::Nil => Err(a.missing(i)),
        v => to_json(v, 0, true).map_err(|_| MemoryStoreError::NotJson.into()),
    }
}

fn sort_key(v: &LuaValue) -> Result<Option<SortKey>, MemoryStoreError> {
    match v {
        LuaValue::Nil => Ok(None),
        LuaValue::String(s) => SortKey::string(s.to_string_lossy()).map(Some),
        LuaValue::Integer(_) | LuaValue::Number(_) => {
            SortKey::number(number_of(v).unwrap_or(f64::NAN)).map(Some)
        }
        _ => Err(MemoryStoreError::SortKeyType),
    }
}

fn push_sort_key(lua: &Lua, sort: Option<SortKey>) -> mlua::Result<LuaValue> {
    Ok(match sort {
        None => LuaValue::Nil,
        Some(SortKey::Number(n)) => LuaValue::Number(n),
        Some(SortKey::String(s)) => LuaValue::String(lua.create_string(s)?),
    })
}

fn sorted_get(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let key = a.string(0)?;
    let found = with_stores(lua, this, |s, h, now| s.sorted_map(name(h)).get(now, &key))?;
    let (value, sort) = found.unwrap_or((Json::Null, None));
    Outcome::ret(lua, (from_json(lua, &value)?, push_sort_key(lua, sort)?))
}

fn sorted_set(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let key = a.string(0)?;
    let value = value(a, 1)?;
    let expiration = stores::expiration(a.number(2)?)?;
    let sort = sort_key(a.get(3))?;
    let existed = with_stores(lua, this, |s, h, now| {
        s.sorted_map(name(h))
            .set(now, &key, value, expiration, sort)
    })?;
    Outcome::ret(lua, existed)
}

fn sorted_remove(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let key = a.string(0)?;
    with_stores(lua, this, |s, h, now| {
        s.sorted_map(name(h)).remove(now, &key)
    })?;
    Outcome::ret(lua, ())
}

fn sorted_size(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let n = with_stores(lua, this, |s, h, now| Ok(s.sorted_map(name(h)).len(now)))?;
    Outcome::ret(lua, n)
}

/// A `GetRangeAsync` bound: nil, or `{key = ..., sortKey = ...}`.
fn bound(a: &Args, i: usize) -> ApiResult<Bound> {
    match a.get(i) {
        LuaValue::Nil => Ok(Bound::default()),
        LuaValue::Table(t) => Ok(Bound {
            key: t.get::<Option<String>>("key")?,
            sort: sort_key(&t.get::<LuaValue>("sortKey")?)?,
        }),
        _ => Err(a.wrong(i, "table")),
    }
}

/// `GetRangeAsync(direction, count, lower, upper)`: an array of
/// `{key, value, sortKey}`, `sortKey` only where the item has one
/// (creator-docs "Memory store sorted map").
fn sorted_range(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let ascending = a.enum_value(lua, 0, "SortDirection")? == 0;
    let count = stores::count(a.number(1)?, MAX_RANGE_COUNT)?;
    let (lower, upper) = (bound(a, 2)?, bound(a, 3)?);
    let items = with_stores(lua, this, |s, h, now| {
        Ok(s.sorted_map(name(h))
            .range(now, ascending, count, &lower, &upper))
    })?;
    let list = lua.create_table_with_capacity(items.len(), 0)?;
    for item in items {
        let t = lua.create_table_with_capacity(0, 3)?;
        t.raw_set("key", item.key)?;
        t.raw_set("value", from_json(lua, &item.value)?)?;
        t.raw_set("sortKey", push_sort_key(lua, item.sort)?)?;
        list.raw_push(t)?;
    }
    Outcome::ret(lua, list)
}

fn queue_add(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let value = value(a, 0)?;
    let expiration = stores::expiration(a.number(1)?)?;
    let priority = a.number_or(2, 0.0)?;
    with_stores(lua, this, |s, h, now| {
        s.queue(name(h)).add(now, value, expiration, priority)
    })?;
    Outcome::ret(lua, ())
}

/// `std.memorystore.read(queue, count, allOrNothing)`: one attempt of
/// `ReadAsync`, the values and the read's id, or nil.
fn read(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let (this, a) = Args::method(args, "ReadAsync")?;
    let count = stores::count(a.number(0)?, MAX_READ_COUNT)?;
    let all_or_nothing = a.truthy(1);
    let read = with_stores(lua, this, |s, h, now| {
        let Handle::Queue { name, invisibility } = h else {
            return Err(MemoryStoreError::Unavailable);
        };
        let read = s.next_read();
        Ok(s.queue(name)
            .read(now, (count, all_or_nothing), *invisibility, read))
    })?;
    let Some((values, id)) = read else {
        return Outcome::ret(lua, LuaValue::Nil);
    };
    let list = lua.create_table_with_capacity(values.len(), 0)?;
    for v in &values {
        list.raw_push(from_json(lua, v)?)?;
    }
    Outcome::ret(lua, (list, id))
}

fn queue_remove(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let id = a.string(0)?;
    with_stores(lua, this, |s, h, now| {
        s.queue(name(h)).remove(now, &id);
        Ok(())
    })?;
    Outcome::ret(lua, ())
}

fn queue_size(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let exclude_invisible = a.truthy(0);
    let n = with_stores(lua, this, |s, h, now| {
        Ok(s.queue(name(h)).len(now, exclude_invisible))
    })?;
    Outcome::ret(lua, n)
}

fn hash_get(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let key = a.string(0)?;
    let found = with_stores(lua, this, |s, h, now| s.hash_map(name(h)).get(now, &key))?;
    Outcome::ret(lua, from_json(lua, &found.unwrap_or(Json::Null))?)
}

fn hash_set(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let key = a.string(0)?;
    let value = value(a, 1)?;
    let expiration = stores::expiration(a.number(2)?)?;
    let existed = with_stores(lua, this, |s, h, now| {
        s.hash_map(name(h)).set(now, &key, value, expiration)
    })?;
    Outcome::ret(lua, existed)
}

fn hash_remove(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let key = a.string(0)?;
    with_stores(lua, this, |s, h, now| s.hash_map(name(h)).remove(now, &key))?;
    Outcome::ret(lua, ())
}

/// `ListItemsAsync(count)`: a MemoryStoreHashMapPages whose pages hold
/// `{key, value}` entries (creator-docs MemoryStoreHashMapPages).
fn hash_list(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let count = stores::count(a.number(0)?, MAX_PAGE_COUNT)?;
    let map = with_stores(lua, this, |_, h, _| Ok(name(h).to_owned()))?;
    let source = Source::HashMap {
        name: map,
        count,
        after: None,
    };
    pages::create(lua, "MemoryStoreHashMapPages", source)
}
