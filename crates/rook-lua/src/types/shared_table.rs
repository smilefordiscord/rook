//! `SharedTable`: table-like storage that every Actor reaches by reference
//! (creator-docs SharedTable; docs/behavior/scripting/parallel-luau.md).
//!
//! Its contents live outside any Luau VM, behind a lock, so the same table
//! can be read and changed from several Actors' threads: keys and values
//! are converted on the way in and out, never kept as Luau values. Every
//! operation takes the lock once, so each is atomic, and Luau code (an
//! `update` function) never runs while it's held. `update` calls its
//! function again when the table changed meanwhile, as Roblox's does;
//! Rook's Actors take turns on one thread, so only the function itself
//! can change it.

use std::collections::BTreeMap;
use std::sync::{Arc, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use mlua::{
    Function, Lua, MetaMethod, MultiValue, Table, UserData, UserDataFields, UserDataMethods,
    UserDataRef, Value as LuaValue,
};
use rook_types::Value;
use rustc_hash::FxHashMap;

use crate::runtime::ctx::args::type_name;
use crate::runtime::ctx::convert::{datatype_value, to_lua};
use crate::runtime::ctx::{ApiResult, Outcome, api_fn};
use crate::types::enums::{LEnumItem, push_item};
use crate::types::err;
use crate::types::tween_info::LTweenInfo;

/// Keys are strings or whole numbers from 0 to 2³² − 1 (Confirmed:
/// Studio 0.748). Whole numbers iterate first, in order (Confirmed), then
/// strings in byte order (Approximation: Studio's order is a hash order
/// that changes with the table's size).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Index(u32),
    Name(Vec<u8>),
}

/// Values are booleans, numbers, vectors, strings, buffers (copied),
/// SharedTables, EnumItems, TweenInfos and Roblox's serializable
/// datatypes (Confirmed: Studio 0.748).
#[derive(Debug, Clone)]
enum Entry {
    Bool(bool),
    Number(f64),
    Vector([f32; 3]),
    String(Vec<u8>),
    Buffer(Vec<u8>),
    Table(LSharedTable),
    EnumItem(LEnumItem),
    TweenInfo(LTweenInfo),
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
            // Confirmed: Studio 0.748.
            return Err(err("SharedTable is frozen"));
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
        // The key is checked first, then whether the table is frozen, then
        // the value (Confirmed: Studio 0.748).
        m.add_meta_method(
            MetaMethod::NewIndex,
            |lua, t, (key, value): (LuaValue, LuaValue)| {
                let key = key_of(&key)?;
                drop(t.write()?);
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
        // Confirmed: Studio 0.748 prints the table's address.
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(format!("SharedTable: 0x{:016x}", Arc::as_ptr(&t.0).addr()))
        });
    }
}

/// Confirmed: Studio 0.748, the errors included.
fn key_of(key: &LuaValue) -> mlua::Result<Key> {
    let index = |n: f64| {
        (n.fract() == 0.0 && (0.0..4_294_967_296.0).contains(&n))
            .then_some(Key::Index(n as u32))
            .ok_or_else(|| {
                err("Numeric SharedTable keys must be nonnegative integers less than 2^32")
            })
    };
    match key {
        LuaValue::String(s) => Ok(Key::Name(s.as_bytes().to_vec())),
        LuaValue::Integer(i) => index(*i as f64),
        LuaValue::Number(n) => index(*n),
        _ => Err(err("SharedTable keys can only be strings or numbers.")),
    }
}

fn key_to_lua(lua: &Lua, key: Key) -> mlua::Result<LuaValue> {
    Ok(match key {
        Key::Index(i) => LuaValue::Number(f64::from(i)),
        Key::Name(s) => LuaValue::String(lua.create_string(s)?),
    })
}

/// What `value` is stored as; `None` removes the key. A Luau table becomes
/// a new SharedTable (Confirmed: Studio 0.748).
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
        // A thread is stored as nil too (Confirmed: Studio 0.748).
        LuaValue::Nil | LuaValue::Thread(_) | LuaValue::LightUserData(_) => return Ok(None),
        LuaValue::Boolean(b) => Entry::Bool(*b),
        LuaValue::Integer(i) => Entry::Number(*i as f64),
        LuaValue::Number(n) => Entry::Number(*n),
        LuaValue::Vector(v) => Entry::Vector([v.x(), v.y(), v.z()]),
        LuaValue::String(s) => Entry::String(s.as_bytes().to_vec()),
        LuaValue::Buffer(b) => Entry::Buffer(b.to_vec()),
        LuaValue::Table(t) => Entry::Table(from_table(lua, t, within)?),
        LuaValue::UserData(u) => {
            if let Ok(t) = u.borrow::<LSharedTable>() {
                Entry::Table(t.clone())
            } else if let Ok(item) = u.borrow::<LEnumItem>() {
                Entry::EnumItem(*item)
            } else if let Ok(info) = u.borrow::<LTweenInfo>() {
                Entry::TweenInfo(info.clone())
            } else {
                match type_name(value).as_str() {
                    // Stored as nil, without an error (Confirmed: Studio
                    // 0.748, for `Enum.Material`, `Enum` and `newproxy()`).
                    "Enum" | "Enums" | "userdata" => return Ok(None),
                    "Content" => return Err(refused(value)),
                    _ => Entry::Data(datatype_value(u).ok_or_else(|| refused(value))?),
                }
            }
        }
        _ => return Err(refused(value)),
    }))
}

/// Confirmed: Studio 0.748, for functions, Instances, DateTime, Random,
/// Content, RBXScriptSignal and RBXScriptConnection; other types are
/// named the same way (Inference).
fn refused(value: &LuaValue) -> mlua::Error {
    let what = match value {
        LuaValue::Function(_) => "Functions".to_owned(),
        _ => match type_name(value).as_str() {
            "RBXScriptSignal" => "EventInstances".to_owned(),
            "RBXScriptConnection" => "Connections".to_owned(),
            name => format!("{name}s"),
        },
    };
    err(format!("{what} can not be stored in a SharedTable"))
}

fn from_table(
    lua: &Lua,
    table: &Table,
    within: &mut Vec<*const std::ffi::c_void>,
) -> mlua::Result<LSharedTable> {
    let address = table.to_pointer();
    if within.contains(&address) {
        return Err(err("Cannot convert a cyclic table to a SharedTable"));
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
        Entry::Buffer(b) => LuaValue::Buffer(lua.create_buffer(b)?),
        Entry::Table(t) => LuaValue::UserData(lua.create_userdata(t)?),
        Entry::EnumItem(item) => push_item(lua, item.enum_id, item.value)?,
        Entry::TweenInfo(info) => LuaValue::UserData(lua.create_userdata(info)?),
        Entry::Data(v) => to_lua(lua, &v, None)?,
    })
}

/// The SharedTable argument of a `SharedTable.*` function (Confirmed:
/// Studio 0.748, the errors included).
fn table_arg(args: &MultiValue, function: &str) -> mlua::Result<LSharedTable> {
    let Some(value) = args.front() else {
        return Err(err(format!(
            "missing argument #1 to '{function}' (SharedTable expected)"
        )));
    };
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

fn arg(args: &MultiValue, i: usize) -> &LuaValue {
    args.get(i).unwrap_or(&LuaValue::Nil)
}

/// `SharedTable.new()` or `SharedTable.new(t)`; an explicit nil is
/// refused (Confirmed: Studio 0.748).
fn new(lua: &Lua, args: &MultiValue) -> mlua::Result<LSharedTable> {
    match args.front() {
        None => Ok(LSharedTable::default()),
        Some(LuaValue::Table(t)) => from_table(lua, t, &mut Vec::new()),
        Some(v) => Err(err(format!(
            "invalid argument #1 to 'new' (table expected, got {})",
            type_name(v)
        ))),
    }
}

/// `clone` and `cloneAndFreeze`: a second argument, when there is one,
/// must be a boolean, nil included (Confirmed: Studio 0.748).
fn clone(args: &MultiValue, name: &str, freeze: bool) -> mlua::Result<LSharedTable> {
    let st = table_arg(args, name)?;
    let deep = match args.get(1) {
        None => false,
        Some(LuaValue::Boolean(b)) => *b,
        Some(v) => {
            return Err(err(format!(
                "invalid argument #2 to '{name}' (boolean expected, got {})",
                type_name(v)
            )));
        }
    };
    st.clone_table(deep, freeze)
}

/// How `increment`'s errors name a key.
fn shown(key: &Key) -> String {
    match key {
        Key::Index(i) => format!("\"{i}\""),
        Key::Name(s) => format!("\"{}\"", String::from_utf8_lossy(s)),
    }
}

/// Adds `delta` to a number already stored under `key`, returning the
/// number before (Confirmed: Studio 0.748, the errors and their order
/// included; how a numeric key is quoted is Inference). The delta must be
/// a number: a numeric string isn't one.
fn increment(args: &MultiValue) -> mlua::Result<f64> {
    let st = table_arg(args, "increment")?;
    let key = key_of(arg(args, 1))?;
    let delta = match args.get(2) {
        None => return Err(err("Failed to get value for insertion into SharedTable")),
        Some(LuaValue::Integer(i)) => *i as f64,
        Some(LuaValue::Number(n)) => *n,
        Some(_) => return Err(err("Variant cast failed")),
    };
    let mut contents = st.write()?;
    let old = match contents.entries.get(&key) {
        Some(Entry::Number(n)) => *n,
        Some(_) => {
            return Err(err(format!(
                "Unable to increment the value for key {} because it is not a number value.",
                shown(&key)
            )));
        }
        None => {
            return Err(err(format!(
                "Unable to increment the value for key {} because it does not exist in the \
                 SharedTable.",
                shown(&key)
            )));
        }
    };
    contents.entries.insert(key, Entry::Number(old + delta));
    contents.version += 1;
    Ok(old)
}

/// The most times `update` calls its function. Studio raises `table
/// overflow` when the function changes the table on every call (Confirmed:
/// Studio 0.748, for a nested `update` of the same table); Rook stops
/// after this many calls (Approximation: Roblox's limit isn't known).
const MAX_UPDATE_CALLS: usize = 1000;

/// Luau's error for a yield where a C function called in.
const YIELD_ACROSS: &str = "attempt to yield across metamethod/C-call boundary";

/// Calls `f` with the value under `key` and stores what it returns,
/// calling it again when the table changed while it ran (Confirmed:
/// Studio 0.748, where a function that changes another key on its first
/// call is called twice). An error in `f` propagates as raised, a table
/// included, and `f` can't yield (Confirmed: Studio 0.748).
fn update(lua: &Lua, args: &MultiValue, pcall: &Function, error: &Function) -> ApiResult<Outcome> {
    let st = table_arg(args, "update")?;
    let key = key_of(arg(args, 1))?;
    let Some(LuaValue::Function(f)) = args.get(2) else {
        return Err("Third argument to SharedTable::Update should be a function.".into());
    };
    // A frozen table fails before `f` runs.
    drop(st.write()?);
    for _ in 0..MAX_UPDATE_CALLS {
        let (version, current) = {
            let contents = st.read();
            (contents.version, contents.entries.get(&key).cloned())
        };
        let current = current.map_or(Ok(LuaValue::Nil), |e| entry_to_lua(lua, e))?;
        let mut results = pcall.call::<MultiValue>((f.clone(), current))?.into_iter();
        let ran = matches!(results.next(), Some(LuaValue::Boolean(true)));
        let value = results.next().unwrap_or(LuaValue::Nil);
        if !ran {
            if let LuaValue::String(s) = &value
                && s.to_string_lossy().ends_with(YIELD_ACROSS)
            {
                return Err("thread is not yieldable".into());
            }
            let raise = MultiValue::from_vec(vec![value, LuaValue::Integer(0)]);
            return Ok(Outcome::Call(error.clone(), raise));
        }
        let new = entry_of(lua, &value)?;
        let mut contents = st.write()?;
        if contents.version != version {
            continue;
        }
        match new {
            Some(e) => contents.entries.insert(key, e),
            None => contents.entries.remove(&key),
        };
        contents.version += 1;
        return Outcome::ret(lua, ());
    }
    Err("table overflow".into())
}

/// `getmetatable` on a SharedTable gives a string (Confirmed: Studio
/// 0.748; the words are Inference, Roblox's for its locked metatables).
/// mlua sets `__metatable` to false on every userdata metatable and won't
/// set it through its API, so it's set on the type's metatable directly.
#[allow(unsafe_code)]
fn lock_metatable(lua: &Lua) -> mlua::Result<()> {
    let sample = lua.create_userdata(LSharedTable::default())?;
    // SAFETY: the closure runs with `sample` on top of the stack, which
    // `exec_raw` checked has room for three more values; it pushes the
    // metatable and a string, sets the field (a raw set, which runs no Lua
    // code) and pops the metatable, leaving the stack as it found it.
    unsafe {
        lua.exec_raw::<()>(sample, |state| {
            if mlua::ffi::lua_getmetatable(state, -1) != 0 {
                mlua::ffi::lua_pushstring(state, c"The metatable is locked".as_ptr());
                mlua::ffi::lua_rawsetfield(state, -2, c"__metatable".as_ptr());
                mlua::ffi::lua_pop(state, 1);
            }
        })
    }
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    lock_metatable(lua)?;
    let t = lua.create_table()?;
    t.set(
        "new",
        lua.create_function(|lua, args: MultiValue| new(lua, &args))?,
    )?;
    t.set(
        "clear",
        lua.create_function(|_, args: MultiValue| {
            let st = table_arg(&args, "clear")?;
            let mut contents = st.write()?;
            contents.entries.clear();
            contents.version += 1;
            Ok(())
        })?,
    )?;
    for (name, freeze) in [("clone", false), ("cloneAndFreeze", true)] {
        t.set(
            name,
            lua.create_function(move |_, args: MultiValue| clone(&args, name, freeze))?,
        )?;
    }
    t.set(
        "increment",
        lua.create_function(|_, args: MultiValue| increment(&args))?,
    )?;
    t.set(
        "isFrozen",
        lua.create_function(|_, args: MultiValue| Ok(table_arg(&args, "isFrozen")?.read().frozen))?,
    )?;
    t.set(
        "size",
        lua.create_function(|_, args: MultiValue| {
            Ok(table_arg(&args, "size")?.read().entries.len())
        })?,
    )?;
    let pcall: Function = lua.globals().get("pcall")?;
    let error: Function = lua.globals().get("error")?;
    t.set(
        "update",
        api_fn(lua, move |lua, args| update(lua, &args, &pcall, &error))?,
    )?;
    // Confirmed: Studio 0.748, the global is frozen.
    t.set_readonly(true);
    lua.globals().set("SharedTable", t)
}
