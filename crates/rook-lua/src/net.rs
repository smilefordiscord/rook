//! Values crossing between DataModels (RemoteEvent/RemoteFunction arguments).
//!
//! Following creator-docs "Remote events and callbacks" (argument limits):
//! tables are copied (no metatables; functions and other non-transferable
//! values become nil), instances arrive as the receiver's copy or nil if the
//! receiver can't see them, and datatypes are copied by value. A table's
//! shape follows the rules bindables were measured to copy by (an array
//! keeps its array part, other keys become strings), which this module
//! owns for both ([`Nesting`], [`table_key`]). See
//! `docs/behavior/replication.md#remote-arguments`.

use std::ffi::c_void;

use mlua::{Lua, Table, Value as LuaValue};
use rook_dom::InstanceId;
use rook_reflection::EnumId;
use rook_types::Value;

use crate::api::instance::{LuaInstance, push_instance};
use crate::runtime::ctx::args::{display_value, type_name};
use crate::runtime::ctx::convert::{datatype_value, to_lua};
use crate::runtime::ctx::{ScriptError, lua_error_message};
use crate::types::enums::{LEnumItem, push_item};

#[derive(Debug, Clone, PartialEq)]
pub enum NetValue {
    Nil,
    Bool(bool),
    Number(f64),
    String(Vec<u8>),
    Vector3([f32; 3]),
    /// An instance, by its id in the DataModel that currently holds the value.
    Instance(InstanceId),
    EnumItem(EnumId, u32),
    Data(Value),
    Table(Vec<(NetValue, NetValue)>),
}

impl NetValue {
    /// Translate instance ids into the receiving DataModel.
    pub fn map_instances(&mut self, f: &dyn Fn(InstanceId) -> Option<InstanceId>) {
        match self {
            NetValue::Instance(id) => {
                *self = match f(*id) {
                    Some(mapped) => NetValue::Instance(mapped),
                    None => NetValue::Nil,
                };
            }
            NetValue::Table(entries) => {
                for (k, v) in entries.iter_mut() {
                    k.map_instances(f);
                    v.map_instances(f);
                }
                entries.retain(|(k, _)| *k != NetValue::Nil);
            }
            _ => {}
        }
    }
}

/// How many tables deep a copy may go. Confirmed (Studio 0.748,
/// `scripting/net_cycles`): remotes and bindables carry 70 tables nested in
/// each other and raise `Max lua stack size reached` for 300.
/// Approximation: the limit between the two isn't measured.
const MAX_DEPTH: usize = 200;

/// The tables a copy is inside, outermost first. A table reached again on
/// another path is copied again, as the receiver gets it; one found among
/// these is inside itself.
#[derive(Default)]
pub(crate) struct Nesting(Vec<*const c_void>);

impl Nesting {
    /// Go into `table`, which raises if it's cyclic or too deep (Confirmed,
    /// Studio 0.748, for remotes and bindables both).
    pub(crate) fn enter(&mut self, table: &Table) -> Result<(), ScriptError> {
        let address = table.to_pointer();
        if self.0.contains(&address) {
            return Err(ScriptError("tables cannot be cyclic".into()));
        }
        if self.0.len() >= MAX_DEPTH {
            return Err(ScriptError("Max lua stack size reached".into()));
        }
        self.0.push(address);
        Ok(())
    }

    pub(crate) fn leave(&mut self) {
        self.0.pop();
    }
}

/// The key a copied dictionary holds for `key`: every key arrives as a
/// string (measured on bindables, docs/behavior/scripting/bindables.md;
/// creator-docs says the same of remotes).
pub(crate) fn table_key(lua: &Lua, key: &LuaValue) -> Result<mlua::LuaString, ScriptError> {
    let text = match key {
        LuaValue::String(s) => return Ok(s.clone()),
        LuaValue::Boolean(_) => return Err(ScriptError("Invalid table key type used".into())),
        LuaValue::Integer(i) => i.to_string(),
        LuaValue::Number(n) => whole(*n).to_string(),
        other => format!("<{}> ({})", key_type(other), display_value(lua, other)),
    };
    lua.create_string(text)
        .map_err(|e| ScriptError(lua_error_message(&e)))
}

/// A number's whole part as a 64-bit integer, what x86's conversion makes of
/// one too large for it included (observed for 1e20 and infinity).
fn whole(n: f64) -> i64 {
    if n.abs() < 9_223_372_036_854_775_808.0 {
        n as i64
    } else {
        i64::MIN
    }
}

/// The name Roblox's own variant types give a key's type.
fn key_type(key: &LuaValue) -> String {
    match type_name(key).as_str() {
        "table" => "Table".into(),
        "function" => "Function".into(),
        "CFrame" => "CoordinateFrame".into(),
        "BrickColor" => "int".into(),
        "EnumItem" => "token".into(),
        other => other.into(),
    }
}

/// What `values` send. Fails, sending nothing, on a table that can't be
/// copied.
pub(crate) fn to_net(lua: &Lua, values: &[LuaValue]) -> Result<Vec<NetValue>, ScriptError> {
    let mut nesting = Nesting::default();
    values
        .iter()
        .map(|v| encode(lua, v, &mut nesting))
        .collect()
}

fn encode(lua: &Lua, v: &LuaValue, nesting: &mut Nesting) -> Result<NetValue, ScriptError> {
    Ok(match v {
        LuaValue::Boolean(b) => NetValue::Bool(*b),
        LuaValue::Integer(i) => NetValue::Number(*i as f64),
        LuaValue::Number(n) => NetValue::Number(*n),
        LuaValue::String(s) => NetValue::String(s.as_bytes().to_vec()),
        LuaValue::Vector(v) => NetValue::Vector3([v.x(), v.y(), v.z()]),
        LuaValue::Table(t) => {
            nesting.enter(t)?;
            let entries = encode_table(lua, t, nesting)?;
            nesting.leave();
            NetValue::Table(entries)
        }
        LuaValue::UserData(u) => {
            if let Ok(i) = u.borrow::<LuaInstance>() {
                return Ok(NetValue::Instance(i.0));
            }
            if let Ok(e) = u.borrow::<LEnumItem>() {
                return Ok(NetValue::EnumItem(e.enum_id, e.value));
            }
            datatype_value(u).map_or(NetValue::Nil, NetValue::Data)
        }
        _ => NetValue::Nil,
    })
}

/// An array keeps 1 to its length and loses its other keys; any other
/// table keeps its keys as strings ([`table_key`]). Entries whose value
/// can't be sent are left out.
fn encode_table(
    lua: &Lua,
    table: &Table,
    nesting: &mut Nesting,
) -> Result<Vec<(NetValue, NetValue)>, ScriptError> {
    let cast = |e: mlua::Error| ScriptError(lua_error_message(&e));
    let length = table.raw_len();
    let mut entries = Vec::new();
    if length > 0 {
        for i in 1..=length {
            let v = encode(lua, &table.raw_get(i).map_err(cast)?, nesting)?;
            if v != NetValue::Nil {
                entries.push((NetValue::Number(i as f64), v));
            }
        }
    } else {
        for pair in table.clone().pairs::<LuaValue, LuaValue>() {
            let (k, v) = pair.map_err(cast)?;
            let key = NetValue::String(table_key(lua, &k)?.as_bytes().to_vec());
            let v = encode(lua, &v, nesting)?;
            if v != NetValue::Nil {
                entries.push((key, v));
            }
        }
    }
    Ok(entries)
}

pub(crate) fn from_net(lua: &Lua, v: &NetValue) -> mlua::Result<LuaValue> {
    Ok(match v {
        NetValue::Nil => LuaValue::Nil,
        NetValue::Bool(b) => LuaValue::Boolean(*b),
        NetValue::Number(n) => LuaValue::Number(*n),
        NetValue::String(s) => LuaValue::String(lua.create_string(s)?),
        NetValue::Vector3([x, y, z]) => LuaValue::Vector(mlua::Vector::new(*x, *y, *z)),
        NetValue::Instance(id) => LuaValue::UserData(push_instance(lua, *id)?),
        NetValue::EnumItem(e, value) => push_item(lua, *e, *value)?,
        NetValue::Data(value) => to_lua(lua, value, None)?,
        NetValue::Table(entries) => {
            let t = lua.create_table()?;
            for (k, v) in entries {
                t.raw_set(from_net(lua, k)?, from_net(lua, v)?)?;
            }
            LuaValue::Table(t)
        }
    })
}

/// What a remote call asks the session to deliver.
#[derive(Debug, Clone, PartialEq)]
pub enum RemoteKind {
    /// Client → server `OnServerEvent`.
    FireServer,
    /// Server → one client's `OnClientEvent`.
    FireClient { player: InstanceId },
    /// Server → every client's `OnClientEvent`.
    FireAllClients,
    /// Client → server `OnServerInvoke`; the reply resumes `call`.
    InvokeServer { call: u64 },
    /// Server → client `OnClientInvoke`.
    InvokeClient { player: InstanceId, call: u64 },
    /// The answer to an invoke from the other side. If `ok` is false the
    /// first argument is the error message.
    Reply { call: u64, ok: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RemoteMessage {
    /// The remote, in the sender's DataModel (unused for replies).
    pub remote: Option<InstanceId>,
    pub kind: RemoteKind,
    pub args: Vec<NetValue>,
}
