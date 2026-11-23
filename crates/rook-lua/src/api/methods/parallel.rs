//! Parallel Luau's members: Actor messaging, `Instance:GetActor` and
//! SharedTableRegistry (docs/behavior/scripting/parallel-luau.md). The
//! contexts and phases they rely on are in `runtime::actors`.

use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;

use crate::api::methods::{RawMethod, ret_instance};
use crate::api::signal::LConnection;
use crate::runtime::actors::{self, Message};
use crate::runtime::ctx::args::Args;
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
fn send_message(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let topic = a.string(0)?;
    let args = super::instances::bindables::copy_values(lua, &a.rest(1), true)?;
    with_world(lua, |w| {
        w.messages.push(Message {
            actor: this,
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
/// scripts may bind (Confirmed: creator-docs Actor:BindToMessage). The
/// message for anyone else is Inference.
fn bind(lua: &Lua, this: InstanceId, a: &Args, parallel: bool) -> ApiResult<Outcome> {
    let topic = a.string(0)?;
    let func = a.function(1)?;
    if actors::context_of(lua, &func) != Some(this) {
        let method = if parallel {
            "BindToMessageParallel"
        } else {
            "BindToMessage"
        };
        return Err(ScriptError(format!(
            "{method} can only be called from a script that is a descendant of the Actor"
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

/// The table registered under the name, made and registered if there is
/// none (creator-docs SharedTableRegistry:GetSharedTable).
fn get_shared_table(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let table = with_world(lua, |w| {
        w.shared_tables
            .entry(name)
            .or_insert_with(LSharedTable::default)
            .clone()
    });
    Outcome::ret(lua, table)
}

/// Registers the table under the name, or unregisters the name for nil.
fn set_shared_table(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let table = match a.get(1) {
        LuaValue::Nil => None,
        _ => Some(a.userdata::<LSharedTable>(1, "SharedTable")?),
    };
    with_world(lua, |w| match table {
        Some(t) => w.shared_tables.insert(name, t),
        None => w.shared_tables.remove(&name),
    });
    Outcome::ret(lua, ())
}
