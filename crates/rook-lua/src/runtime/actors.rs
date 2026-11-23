//! Actors: the context each script runs in, the messages scripts send
//! Actors, and the parallel phase their desynchronized work runs in
//! (docs/behavior/scripting/parallel-luau.md).
//!
//! Roblox runs each Actor's scripts in a Luau VM of their own, with their
//! own globals and their own copy of every module they require
//! (creator-docs Actor). Rook gives each Actor a context: a globals table
//! its scripts' environments fall back to, holding the Actor's own `_G`,
//! `shared`, `require` (modules load once per context) and `task` (whose
//! `desynchronize` knows its Actor). An environment's context is recorded
//! when it's made, so a function's context is its environment's. A context
//! holds everything a VM of the Actor's own would; the parallel phase hands
//! out work one Actor at a time, the unit a worker thread runs.

use mlua::{Function, Lua, Table, Value as LuaValue};
use rook_dom::{Dom, InstanceId};

use crate::api::instance::key_of;
use crate::runtime::ctx::with_world;
use crate::runtime::scheduler::Phase;
use crate::runtime::signal::{SignalKey, SignalKind};
use crate::runtime::threads::{drain_deferred, fire_with, run_job};

/// The context code runs in: an Actor's, or `None` for the DataModel's
/// own scripts (Roblox's main VM).
pub(crate) type Context = Option<InstanceId>;

/// Most jobs one parallel phase runs, so work that keeps deferring itself
/// can't hang the frame (as `drain_deferred` bounds its drain).
const MAX_PARALLEL_JOBS: usize = 100_000;

/// A message `Actor:SendMessage` sent, waiting for the next resumption
/// point. Its arguments are the sender's copies.
#[derive(Debug)]
pub struct Message {
    pub(crate) actor: InstanceId,
    pub(crate) topic: String,
    pub(crate) args: Vec<LuaValue>,
}

/// The Actor an instance belongs to: the nearest Actor at or above it
/// (creator-docs Parallel Luau: a script nested in several Actors belongs
/// to the closest; `GetActor` on an Actor itself is Inference).
pub(crate) fn actor_of(dom: &Dom, id: InstanceId) -> Option<InstanceId> {
    std::iter::once(id)
        .chain(dom.ancestors(id))
        .find(|&a| dom.class_name(a) == "Actor")
}

/// The globals table environments in `context` fall back to: the shared
/// globals for the DataModel's own scripts, and for an Actor a table of
/// its own, kept until the Actor is freed ([`forget`]): an Actor's VM, and
/// so its `_G`, outlives any one of its scripts.
pub(crate) fn globals(lua: &Lua, context: Context) -> mlua::Result<Table> {
    let Some(actor) = context else {
        return Ok(lua.globals());
    };
    let all: Table = lua.named_registry_value("rook.actor_globals")?;
    if let Some(globals) = all.raw_get::<Option<Table>>(key_of(actor))? {
        return Ok(globals);
    }
    let globals = lua.create_table()?;
    let mt = lua.create_table()?;
    mt.raw_set("__index", lua.globals())?;
    globals.set_metatable(Some(mt))?;
    globals.raw_set("_G", lua.create_table()?)?;
    globals.raw_set("shared", lua.create_table()?)?;
    globals.raw_set("require", crate::api::scripts::require_in(lua, context)?)?;
    globals.raw_set("task", crate::api::globals::task_library(lua, context)?)?;
    all.raw_set(key_of(actor), globals.clone())?;
    Ok(globals)
}

/// Drop the globals of Actor `id`, which is being freed. Environments made
/// in it keep the table they fall back to.
pub(crate) fn forget(lua: &Lua, id: InstanceId) -> mlua::Result<()> {
    let all: Table = lua.named_registry_value("rook.actor_globals")?;
    all.raw_set(key_of(id), LuaValue::Nil)
}

/// Record that `env` belongs to `context`.
pub(crate) fn register(lua: &Lua, env: &Table, context: Context) -> mlua::Result<()> {
    if let Some(actor) = context {
        let envs: Table = lua.named_registry_value("rook.actor_envs")?;
        envs.raw_set(env.clone(), key_of(actor))?;
    }
    Ok(())
}

/// The context a function runs in: its environment's. Functions without a
/// script's environment belong to the DataModel's own scripts.
pub(crate) fn context_of(lua: &Lua, func: &Function) -> Context {
    let env = func.environment()?;
    let envs: Table = lua.named_registry_value("rook.actor_envs").ok()?;
    match envs.raw_get::<LuaValue>(env).ok()? {
        LuaValue::LightUserData(key) => Some(crate::api::instance::id_of(key)),
        _ => None,
    }
}

/// Whether the code running now is desynchronized.
pub(crate) fn in_parallel(lua: &Lua) -> bool {
    with_world(lua, |w| w.scheduler.phase() != Phase::Serial)
}

/// Hand each waiting message to the bindings its Actor has for its topic
/// now: serial ones run here, parallel ones wait for the phase. Each
/// binding gets a copy of its own, as each bindable listener does.
fn deliver_messages(lua: &Lua) {
    let messages = with_world(lua, |w| std::mem::take(&mut w.messages));
    for message in messages {
        let key = SignalKey {
            instance: message.actor,
            kind: SignalKind::Message(message.topic),
        };
        let args = message.args;
        fire_with(lua, &key, || {
            crate::api::methods::instances::bindables::copy_values(lua, &args, true)
                .unwrap_or_default()
        });
        drain_deferred(lua);
    }
}

/// A resumption point's parallel phase: messages are delivered, then
/// every Actor's desynchronized work runs (each Actor's in order, Actors
/// one after another), including what that work defers; then the threads
/// that called `task.synchronize` resume, serially. Inference: the
/// creator docs say only that desynchronized threads resume at the "next
/// parallel execution phase" and synchronized ones at the next serial one.
pub(crate) fn run_parallel_phase(lua: &Lua) {
    deliver_messages(lua);
    let mut budget = MAX_PARALLEL_JOBS;
    while budget > 0 {
        let groups = with_world(lua, |w| w.scheduler.take_parallel());
        if groups.is_empty() {
            break;
        }
        for (actor, jobs) in groups {
            set_phase(lua, Phase::Parallel(actor));
            for job in jobs {
                budget = budget.saturating_sub(1);
                run_job(lua, job);
            }
            set_phase(lua, Phase::Serial);
        }
    }
    while let Some(job) = with_world(lua, |w| w.scheduler.pop_synchronized()) {
        run_job(lua, job);
        drain_deferred(lua);
    }
}

/// Enter `phase`. The scheduler owns it; Luau's guards on members unsafe
/// in parallel read a copy, set here in the same step, so it's never
/// stale.
fn set_phase(lua: &Lua, phase: Phase) {
    with_world(lua, |w| w.scheduler.set_phase(phase));
    let mirrored = lua
        .named_registry_value::<Table>("rook.phase")
        .and_then(|t| t.raw_set("parallel", phase != Phase::Serial));
    if let Err(e) = mirrored {
        tracing::warn!("parallel phase not mirrored to Luau: {e}");
    }
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    // Environments' Actors (weak keys), and Actors' globals, held until
    // the Actor is freed: weak values would lose an Actor's `_G` whenever
    // none of its scripts' environments happened to be alive.
    let envs = crate::runtime::ctx::weak_table(lua, "k")?;
    lua.set_named_registry_value("rook.actor_envs", envs)?;
    lua.set_named_registry_value("rook.actor_globals", lua.create_table()?)?;
    let phase = lua.create_table()?;
    phase.raw_set("parallel", false)?;
    let guard: Function = lua
        .load(
            r#"
            local phase, error = ...
            return function(f, message)
                return function(...)
                    if phase.parallel then
                        error(message, 0)
                    end
                    return f(...)
                end
            end
            "#,
        )
        .set_name("=[rook] parallel_guard")
        .call((phase.clone(), lua.globals().get::<Function>("error")?))?;
    lua.set_named_registry_value("rook.phase", phase)?;
    lua.set_named_registry_value("rook.parallel_guard", guard)?;
    Ok(())
}
