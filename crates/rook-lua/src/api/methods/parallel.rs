//! Parallel Luau's members: Actor messaging, `Instance:GetActor` and
//! SharedTableRegistry (docs/behavior/scripting/parallel-luau.md). The
//! contexts and phases they rely on are in `runtime::actors`.

use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;

use crate::api::methods::{RawMethod, ret_instance};
use crate::api::signal::LConnection;
use crate::runtime::actors::{self, Message};
use crate::runtime::ctx::args::{Args, display_value, type_name};
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::runtime::signal::{SignalKey, SignalKind};
use crate::runtime::world::Handler;
use crate::types::shared_table::LSharedTable;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("Instance", "GetActor") => get_actor,
        ("Actor", "SendMessage") => send_message,
        ("Actor", "BindToMessage") => bind_serial,
        ("Actor", "BindToMessageParallel") => bind_parallel,
        ("SharedTableRegistry", "GetSharedTable") => get_shared_table,
        ("SharedTableRegistry", "SetSharedTable") => set_shared_table,
        _ => return None,
    })
}

fn get_actor(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    ret_instance(lua, with_world(lua, |w| actors::actor_of(&w.dom, this)))
}

/// Queues the message for the Actor's bindings at the next resumption
/// point; the sender never waits. Its arguments are copied now, by the
/// rules bindables copy by, refusing functions (creator-docs
/// Actor:SendMessage: passed by copy across VMs, functions can't cross).
/// An Actor outside the DataModel takes none, once the topic is there and
/// the arguments copy without a cycle (Confirmed: Studio 0.748, the words
/// included).
fn send_message(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    if a.get(0).is_nil() {
        return Err(a.missing(0));
    }
    let attached = with_world(lua, |w| {
        let root = w.dom.root();
        w.dom.ancestors(this).any(|a| a == root)
    });
    if !attached {
        super::instances::bindables::copy_values(lua, &a.rest(1), false)?;
        return Err(ScriptError(
            "Actor:SendMessage may only be called on Actors attached to the DataModel.".into(),
        ));
    }
    let topic = a.string(0)?;
    let args = super::instances::bindables::copy_values(lua, &a.rest(1), true)?;
    with_world(lua, |w| {
        let place = w.scheduler.reserve_place();
        w.messages.push(Message {
            actor: this,
            place,
            topic,
            args,
        });
    });
    Outcome::ret(lua, ())
}

fn bind_serial(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    bind(lua, this, a, false)
}

fn bind_parallel(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    bind(lua, this, a, true)
}

/// `BindToMessage` and `BindToMessageParallel`: only the Actor's own
/// scripts may bind, a check made once the arguments cast (Confirmed:
/// Studio 0.748, the words included).
fn bind(lua: &Lua, this: InstanceId, a: &Args, parallel: bool) -> ApiResult<Outcome> {
    let topic = a.string(0)?;
    let func = match a.get(1) {
        LuaValue::Function(f) => f.clone(),
        LuaValue::Nil => return Err(a.missing(1)),
        LuaValue::Integer(_) | LuaValue::Number(_) => {
            return Err(ScriptError("Unable to cast double to Function".into()));
        }
        LuaValue::Boolean(_) => return Err(ScriptError("Unable to cast bool to Function".into())),
        v => {
            return Err(ScriptError(format!(
                "Unable to cast {} to Function",
                type_name(v)
            )));
        }
    };
    if actors::context_of(lua, &func) != Some(this) {
        let method = if parallel {
            "BindToMessageParallel"
        } else {
            "BindToMessage"
        };
        return Err(ScriptError(format!(
            "Calling {method} on an Actor is only allowed in scripts that are a descendent of \
             that Actor"
        )));
    }
    let handler = if parallel {
        Handler::Parallel { func, actor: this }
    } else {
        Handler::Call(func)
    };
    let key = SignalKey {
        instance: this,
        kind: SignalKind::Message(topic),
    };
    let id = with_world(lua, |w| w.signals.connect(key, handler, false));
    Outcome::ret(lua, LConnection(id))
}

/// A registry name: a string or a number as one; nil is missing
/// (Confirmed: Studio 0.748). Another value goes by its `tostring`
/// (Inference: Studio takes a table and gives a SharedTable for it).
fn registry_name(lua: &Lua, a: &Args) -> ApiResult<String> {
    match a.get(0) {
        LuaValue::Nil | LuaValue::String(_) | LuaValue::Integer(_) | LuaValue::Number(_) => {
            a.string(0)
        }
        v => Ok(display_value(lua, v)),
    }
}

/// The table registered under the name, made and registered if there is
/// none (creator-docs SharedTableRegistry:GetSharedTable).
fn get_shared_table(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = registry_name(lua, a)?;
    let table = with_world(lua, |w| {
        w.shared_tables
            .entry(name)
            .or_insert_with(LSharedTable::default)
            .clone()
    });
    Outcome::ret(lua, table)
}

/// Registers the table under the name, or unregisters the name for nil.
/// Anything else fails as Roblox's casts do (Confirmed: Studio 0.748 for a
/// number and an array; a dictionary's name is Inference).
fn set_shared_table(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = registry_name(lua, a)?;
    let table = match a.get(1) {
        LuaValue::Nil => None,
        LuaValue::UserData(u) if u.is::<LSharedTable>() => {
            Some(a.userdata::<LSharedTable>(1, "SharedTable")?)
        }
        v => {
            let from = match v {
                LuaValue::Integer(_) | LuaValue::Number(_) => "double".to_owned(),
                LuaValue::Boolean(_) => "bool".to_owned(),
                LuaValue::Table(t) if t.raw_len() > 0 => "Array".to_owned(),
                LuaValue::Table(_) => "Dictionary".to_owned(),
                v => type_name(v),
            };
            return Err(ScriptError(format!("Unable to cast {from} to SharedTable")));
        }
    };
    with_world(lua, |w| match table {
        Some(t) => w.shared_tables.insert(name, t),
        None => w.shared_tables.remove(&name),
    });
    Outcome::ret(lua, ())
}
