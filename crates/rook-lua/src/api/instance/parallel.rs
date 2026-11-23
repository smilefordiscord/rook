//! What desynchronized code may do with an instance's members: each
//! member's thread safety from the API dump (creator-docs Parallel Luau,
//! thread safety), checked as it's read, written, called or reached
//! (docs/behavior/scripting/parallel-luau.md). The messages are Roblox's,
//! from DevForum reports of them (Inference).

use rook_reflection::members::ThreadSafety;

use super::*;
use crate::runtime::actors;

/// Reading `key` (property `p`) while desynchronized: only Unsafe ones fail.
pub(super) fn check_read(lua: &Lua, p: &PropInfo, key: &str) -> ApiResult<()> {
    if p.safety == ThreadSafety::Unsafe && actors::in_parallel(lua) {
        return Err(ScriptError(format!(
            "Property {}.{key} is not safe to read in parallel",
            p.owner
        )));
    }
    Ok(())
}

/// Writing `key` (property `p` of `id`) while desynchronized: only Safe
/// ones, and LocalSafe ones of the running Actor's own instances.
pub(super) fn check_write(lua: &Lua, id: InstanceId, p: &PropInfo, key: &str) -> ApiResult<()> {
    let allowed = with_world(lua, |w| match w.scheduler.parallel_actor() {
        None => true,
        Some(actor) => match p.safety {
            ThreadSafety::Safe => true,
            ThreadSafety::LocalSafe => actors::actor_of(&w.dom, id) == Some(actor),
            ThreadSafety::ReadSafe | ThreadSafety::Unsafe => false,
        },
    });
    if allowed {
        return Ok(());
    }
    Err(ScriptError(format!(
        "Property {}.{key} is not safe to write in parallel",
        p.owner
    )))
}

/// Reaching event `name` of `id` while desynchronized (every event is
/// Unsafe in the dump).
pub(super) fn check_event(lua: &Lua, id: InstanceId, name: &str) -> ApiResult<()> {
    if !actors::in_parallel(lua) {
        return Ok(());
    }
    let class = with_world(lua, |w| w.dom.get(id).map(|i| i.class));
    let declared = class.and_then(|class| {
        database().superclasses(class).find_map(|c| {
            let e = members()
                .classes
                .get(c.name)?
                .events
                .iter()
                .find(|e| e.name == name)?;
            Some((c.name, e.safety))
        })
    });
    match declared {
        Some((owner, safety)) if safety != ThreadSafety::Safe => Err(ScriptError(format!(
            "Event {owner}.{name} is not safe to access in parallel"
        ))),
        _ => Ok(()),
    }
}

/// The thread safety of method `name` as `owner` declares it. Deprecated
/// lowercase spellings have their own (`findFirstChild` is Unsafe where
/// `FindFirstChild` is Safe).
pub(super) fn method_safety(owner: &str, name: &str) -> ThreadSafety {
    members()
        .classes
        .get(owner)
        .and_then(|m| m.functions.iter().find(|f| f.name == name))
        .map_or(ThreadSafety::Unsafe, |f| f.safety)
}

pub(super) fn not_callable(owner: &str, name: &str) -> String {
    format!("Function {owner}.{name} is not safe to call in parallel")
}

/// `f`, failing with `message` when called while desynchronized: for
/// methods written in Luau and fast forms, which don't pass through a
/// Rust check. Reads the phase Luau mirrors ([`actors::set_phase`]).
pub(super) fn guard(lua: &Lua, f: Function, message: String) -> mlua::Result<Function> {
    let guard: Function = lua.named_registry_value("rook.parallel_guard")?;
    guard.call((f, message))
}
