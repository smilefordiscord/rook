//! `SharedTable`: table-like storage that every Actor reaches by reference
//! (creator-docs SharedTable; docs/behavior/scripting/parallel-luau.md).
//!
//! Its contents live outside any Luau VM, behind a lock, so the same table
//! can be read and changed from several Actors' threads: keys and values
//! are converted on the way in and out, never kept as Luau values. Every
//! operation takes the lock once, so each is atomic, and Luau code (an
//! `update` function) never runs while it's held. Roblox's `update` calls
//! its function again when another script changed the table meanwhile;
//! Rook's Actors take turns on one thread, so only the function itself
//! can, and that is an error rather than a retry that never ends.

use std::collections::BTreeMap;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use mlua::{
    Function, Lua, MetaMethod, Table, UserData, UserDataFields, UserDataMethods, UserDataRef,
    Value as LuaValue,
};
use rook_types::Value;
use rustc_hash::FxHashMap;

use crate::runtime::ctx::args::type_name;
use crate::runtime::ctx::convert::{datatype_value, to_lua};
use crate::types::err;

/// Keys are strings or whole numbers from 0 to 2³² − 1 (Confirmed:
/// creator-docs). Whole numbers iterate first, in order, then strings in
/// byte order (Inference: Roblox doesn't say).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Index(u32),
    Name(Vec<u8>),
}

/// Values are booleans, numbers, vectors, strings, SharedTables and
/// Roblox's serializable datatypes (Confirmed: creator-docs).
#[derive(Debug, Clone)]
enum Entry {
    Bool(bool),
    Number(f64),
    Vector([f32; 3]),
    String(Vec<u8>),
    Table(LSharedTable),
    Data(Value),
}

/// How deep SharedTables may nest where Rook walks them by recursion:
/// converting a Luau table and copying deeply. Approximation: Roblox's
/// limit isn't documented; without one a script's deeply nested table
/// overflows the stack.
const MAX_DEPTH: usize = 1000;

#[derive(Debug, Default)]
struct Contents {
    frozen: bool,
    /// Counts changes, so `update` can tell the table changed under it.
    version: u64,
    entries: BTreeMap<Key, Entry>,
}

/// Dropped table by table, not by recursion: a script can chain tables
/// (each holding the next) as long as memory allows, and dropping such a
/// chain by recursion overflows the stack.
impl Drop for Contents {
    fn drop(&mut self) {
        let mut pending = tables(std::mem::take(&mut self.entries));
        while let Some(table) = pending.pop() {
            if let Ok(lock) = Arc::try_unwrap(table.0) {
                let mut contents = lock.into_inner().unwrap_or_else(PoisonError::into_inner);
                pending.extend(tables(std::mem::take(&mut contents.entries)));
            }
        }
    }
}

fn tables(entries: BTreeMap<Key, Entry>) -> Vec<LSharedTable> {
    (entries.into_values())
        .filter_map(|e| match e {
            Entry::Table(t) => Some(t),
            _ => None,
        })
        .collect()
}

#[derive(Debug, Clone, Default)]
pub(crate) struct LSharedTable(Arc<RwLock<Contents>>);

impl LSharedTable {
    /// Nothing runs while the lock is held that could panic and poison it,
    /// so a poisoned lock still holds a whole table.
    fn read(&self) -> RwLockReadGuard<'_, Contents> {
        self.0.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> mlua::Result<RwLockWriteGuard<'_, Contents>> {
        let contents = self.0.write().unwrap_or_else(PoisonError::into_inner);
        if contents.frozen {
            // Inference: Luau's own words for a frozen table.
            return Err(err("attempt to modify a readonly table"));
        }
        Ok(contents)
    }

    fn set(&self, key: Key, entry: Option<Entry>) -> mlua::Result<()> {
        let mut contents = self.write()?;
        match entry {
            Some(e) => contents.entries.insert(key, e),
            None => contents.entries.remove(&key),
        };
        contents.version += 1;
        Ok(())
    }

    /// A copy: of this table only, or (`deep`) of every SharedTable it
    /// reaches, each copied once however often it's reached. Each table is
    /// copied in one read, so it's a consistent snapshot of that table.
    fn clone_table(&self, deep: bool, freeze: bool) -> mlua::Result<Self> {
        let mut copies = FxHashMap::default();
        self.clone_into((deep, freeze), 0, &mut copies)
    }

    fn clone_into(
        &self,
        (deep, freeze): (bool, bool),
        depth: usize,
        copies: &mut FxHashMap<*const RwLock<Contents>, Self>,
    ) -> mlua::Result<Self> {
        if let Some(copy) = copies.get(&Arc::as_ptr(&self.0)) {
            return Ok(copy.clone());
        }
        if depth >= MAX_DEPTH {
            return Err(too_deep());
        }
        let copy = Self::default();
        copies.insert(Arc::as_ptr(&self.0), copy.clone());
        let mut entries = self.read().entries.clone();
        if deep {
            for entry in entries.values_mut() {
                if let Entry::Table(t) = entry {
                    *t = t.clone_into((deep, freeze), depth + 1, copies)?;
                }
            }
        }
        let mut contents = copy.0.write().unwrap_or_else(PoisonError::into_inner);
        contents.entries = entries;
        contents.frozen = freeze;
        drop(contents);
        Ok(copy)
    }
}

/// Inference: the wording.
fn too_deep() -> mlua::Error {
    err(format!(
        "SharedTables can't nest more than {MAX_DEPTH} deep"
    ))
}

impl UserData for LSharedTable {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "SharedTable");
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Index, |lua, t, key: LuaValue| {
            let key = key_of(&key)?;
            let entry = t.read().entries.get(&key).cloned();
            entry.map_or(Ok(LuaValue::Nil), |e| entry_to_lua(lua, e))
        });
        m.add_meta_method(
            MetaMethod::NewIndex,
            |lua, t, (key, value): (LuaValue, LuaValue)| {
                let key = key_of(&key)?;
                let entry = entry_of(lua, &value)?;
                t.set(key, entry)
            },
        );
        // A snapshot taken as the loop starts.
        m.add_meta_method(MetaMethod::Iter, |lua, t, ()| {
            let mut entries = t.read().entries.clone().into_iter();
            lua.create_function_mut(move |lua, _: mlua::MultiValue| match entries.next() {
                Some((key, entry)) => Ok((key_to_lua(lua, key)?, entry_to_lua(lua, entry)?)),
                None => Ok((LuaValue::Nil, LuaValue::Nil)),
            })
        });
        // Distinct tables never compare equal, whatever they hold
        // (Confirmed: creator-docs).
        m.add_meta_method(MetaMethod::Eq, |_, a, b: UserDataRef<Self>| {
            Ok(Arc::ptr_eq(&a.0, &b.0))
        });
        // Inference: the type's name, as other datatypes without a
        // printed form show.
        m.add_meta_method(MetaMethod::ToString, |_, _, ()| Ok("SharedTable"));
    }
}

fn key_of(key: &LuaValue) -> mlua::Result<Key> {
    let index = |n: f64| {
        (n.fract() == 0.0 && (0.0..4_294_967_296.0).contains(&n)).then_some(Key::Index(n as u32))
    };
    let key = match key {
        LuaValue::String(s) => Some(Key::Name(s.as_bytes().to_vec())),
        LuaValue::Integer(i) => index(*i as f64),
        LuaValue::Number(n) => index(*n),
        _ => None,
    };
    // Inference: the wording.
    key.ok_or_else(|| {
        err("invalid SharedTable key (expected a string or a whole number from 0 to 4294967295)")
    })
}

fn key_to_lua(lua: &Lua, key: Key) -> mlua::Result<LuaValue> {
    Ok(match key {
        Key::Index(i) => LuaValue::Number(f64::from(i)),
        Key::Name(s) => LuaValue::String(lua.create_string(s)?),
    })
}

/// What `value` is stored as; `None` removes the key. A Luau table becomes
/// a new SharedTable (Confirmed for `SharedTable.new`; Inference for
/// assigning one).
fn entry_of(lua: &Lua, value: &LuaValue) -> mlua::Result<Option<Entry>> {
    entry_within(lua, value, &mut Vec::new())
}

/// `within` holds the Luau tables being converted, so a table that holds
/// itself is an error rather than endless.
fn entry_within(
    lua: &Lua,
    value: &LuaValue,
    within: &mut Vec<*const std::ffi::c_void>,
) -> mlua::Result<Option<Entry>> {
    Ok(Some(match value {
        LuaValue::Nil => return Ok(None),
        LuaValue::Boolean(b) => Entry::Bool(*b),
        LuaValue::Integer(i) => Entry::Number(*i as f64),
        LuaValue::Number(n) => Entry::Number(*n),
        LuaValue::Vector(v) => Entry::Vector([v.x(), v.y(), v.z()]),
        LuaValue::String(s) => Entry::String(s.as_bytes().to_vec()),
        LuaValue::Table(t) => Entry::Table(from_table(lua, t, within)?),
        LuaValue::UserData(u) => match u.borrow::<LSharedTable>() {
            Ok(t) => Entry::Table(t.clone()),
            Err(_) => Entry::Data(datatype_value(u).ok_or_else(|| unsupported(value))?),
        },
        _ => return Err(unsupported(value)),
    }))
}

/// Inference: the wording.
fn unsupported(value: &LuaValue) -> mlua::Error {
    err(format!(
        "a value of type {} cannot be stored in a SharedTable",
        type_name(value)
    ))
}

fn from_table(
    lua: &Lua,
    table: &Table,
    within: &mut Vec<*const std::ffi::c_void>,
) -> mlua::Result<LSharedTable> {
    let address = table.to_pointer();
    if within.contains(&address) {
        return Err(err("tables cannot be cyclic"));
    }
    if within.len() >= MAX_DEPTH {
        return Err(too_deep());
    }
    within.push(address);
    let mut entries = BTreeMap::new();
    for pair in table.clone().pairs::<LuaValue, LuaValue>() {
        let (k, v) = pair?;
        if let Some(entry) = entry_within(lua, &v, within)? {
            entries.insert(key_of(&k)?, entry);
        }
    }
    within.pop();
    Ok(LSharedTable(Arc::new(RwLock::new(Contents {
        entries,
        ..Contents::default()
    }))))
}

fn entry_to_lua(lua: &Lua, entry: Entry) -> mlua::Result<LuaValue> {
    Ok(match entry {
        Entry::Bool(b) => LuaValue::Boolean(b),
        Entry::Number(n) => LuaValue::Number(n),
        Entry::Vector([x, y, z]) => LuaValue::Vector(mlua::Vector::new(x, y, z)),
        Entry::String(s) => LuaValue::String(lua.create_string(s)?),
        Entry::Table(t) => LuaValue::UserData(lua.create_userdata(t)?),
        Entry::Data(v) => to_lua(lua, &v, None)?,
    })
}

/// The SharedTable argument of a `SharedTable.*` function.
fn table_arg(value: &LuaValue, function: &str) -> mlua::Result<LSharedTable> {
    let table = match value {
        LuaValue::UserData(u) => u.borrow::<LSharedTable>().ok().map(|t| t.clone()),
        _ => None,
    };
    table.ok_or_else(|| {
        err(format!(
            "invalid argument #1 to '{function}' (SharedTable expected, got {})",
            type_name(value)
        ))
    })
}

fn new(lua: &Lua, from: Option<Table>) -> mlua::Result<LSharedTable> {
    match from {
        Some(t) => from_table(lua, &t, &mut Vec::new()),
        None => Ok(LSharedTable::default()),
    }
}

/// Adds `delta` to a number already stored under `key`, returning the
/// number before (Confirmed: creator-docs SharedTable.increment).
fn increment(st: &LSharedTable, key: &LuaValue, delta: f64) -> mlua::Result<f64> {
    let key = key_of(key)?;
    let mut contents = st.write()?;
    let Some(Entry::Number(old)) = contents.entries.get(&key).cloned() else {
        // Inference: the wording.
        return Err(err("SharedTable.increment: the element is not a number"));
    };
    contents.entries.insert(key, Entry::Number(old + delta));
    contents.version += 1;
    Ok(old)
}

/// Calls `f` with the value under `key` and stores what it returns.
/// Roblox calls `f` again when other scripts changed the table while it
/// ran (Confirmed: creator-docs SharedTable.update). Nothing runs beside
/// `f` in Rook, which can't yield inside it, so a change meanwhile is
/// `f`'s own, which every retry would make again: it raises instead
/// (Inference: the creator docs leave it out; the wording is Rook's).
fn update(lua: &Lua, st: &LSharedTable, key: &LuaValue, f: &Function) -> mlua::Result<()> {
    let key = key_of(key)?;
    // A frozen table fails before `f` runs.
    drop(st.write()?);
    let (version, current) = {
        let contents = st.read();
        (contents.version, contents.entries.get(&key).cloned())
    };
    let current = current.map_or(Ok(LuaValue::Nil), |e| entry_to_lua(lua, e))?;
    let new = entry_of(lua, &f.call::<LuaValue>(current)?)?;
    let mut contents = st.write()?;
    if contents.version != version {
        return Err(err(
            "SharedTable.update: the update function must not change the table",
        ));
    }
    match new {
        Some(e) => contents.entries.insert(key, e),
        None => contents.entries.remove(&key),
    };
    contents.version += 1;
    Ok(())
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let t = lua.create_table()?;
    t.set("new", lua.create_function(new)?)?;
    t.set(
        "clear",
        lua.create_function(|_, st: LuaValue| {
            let st = table_arg(&st, "clear")?;
            let mut contents = st.write()?;
            contents.entries.clear();
            contents.version += 1;
            Ok(())
        })?,
    )?;
    for (name, freeze) in [("clone", false), ("cloneAndFreeze", true)] {
        t.set(
            name,
            lua.create_function(move |_, (st, deep): (LuaValue, Option<bool>)| {
                table_arg(&st, name)?.clone_table(deep.unwrap_or(false), freeze)
            })?,
        )?;
    }
    t.set(
        "increment",
        lua.create_function(|_, (st, key, delta): (LuaValue, LuaValue, f64)| {
            increment(&table_arg(&st, "increment")?, &key, delta)
        })?,
    )?;
    t.set(
        "isFrozen",
        lua.create_function(|_, st: LuaValue| Ok(table_arg(&st, "isFrozen")?.read().frozen))?,
    )?;
    t.set(
        "size",
        lua.create_function(|_, st: LuaValue| Ok(table_arg(&st, "size")?.read().entries.len()))?,
    )?;
    t.set(
        "update",
        lua.create_function(|lua, (st, key, f): (LuaValue, LuaValue, Function)| {
            update(lua, &table_arg(&st, "update")?, &key, &f)
        })?,
    )?;
    lua.globals().set("SharedTable", t)
}
