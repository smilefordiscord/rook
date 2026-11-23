//! Waiting and threads: the legacy `wait`, `delay` and `spawn`, and the
//! `task` library, with its switches between serial and parallel phases
//! (docs/behavior/scripting/parallel-luau.md). Legacy timing details
//! (minimum wait, spawn delay) are Inference.

use super::*;

pub(super) fn now(lua: &Lua) -> f64 {
    with_world(lua, |w| w.scheduler.now())
}

/// A wait's duration argument `i` (`default` when missing). Roblox keeps
/// it as a 32-bit float, and one that doesn't fit there (`math.huge`,
/// anything past about 3.4e38, NaN) waits as 0, one step (measured on
/// `wait`, `delay`, `task.wait` and `task.delay`, Studio 0.747); 1e20
/// really waits. Games park loops on `wait(math.huge)` and rely on it.
fn seconds(a: &Args, i: usize, default: f64) -> ApiResult<f64> {
    let t = a.number_or(i, default)?;
    Ok(if (t as f32).is_finite() { t } else { 0.0 })
}

/// `wait(t)`: legacy scheduler, at least [`LEGACY_MIN_WAIT`].
pub(super) fn wait(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "wait");
    let t = seconds(&a, 0, LEGACY_MIN_WAIT)?.max(LEGACY_MIN_WAIT);
    let thread = lua.current_thread();
    with_world(lua, |w| {
        let since = w.scheduler.now();
        w.scheduler.schedule(
            Queue::Legacy,
            t,
            Job::Resume {
                thread,
                args: Vec::new(),
                wait: WaitKind::Legacy { since },
            },
        );
    });
    Ok(Outcome::Yield)
}

/// `delay(t, f)`: call `f(elapsed, time())` after `t` seconds.
pub(super) fn delay(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "delay");
    let t = seconds(&a, 0, 0.0)?.max(LEGACY_MIN_WAIT);
    let func = a.function(1)?;
    with_world(lua, |w| {
        let since = w.scheduler.now();
        w.scheduler.schedule(
            Queue::Legacy,
            t,
            Job::Call {
                func,
                args: Vec::new(),
                legacy_since: Some(since),
            },
        );
    });
    Outcome::ret(lua, ())
}

/// `spawn(f)`: call `f(elapsed, time())` on the next legacy step.
pub(super) fn legacy_spawn(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "spawn");
    let func = a.function(0)?;
    with_world(lua, |w| {
        let since = w.scheduler.now();
        w.scheduler.schedule(
            Queue::Legacy,
            LEGACY_MIN_WAIT,
            Job::Call {
                func,
                args: Vec::new(),
                legacy_since: Some(since),
            },
        );
    });
    Outcome::ret(lua, ())
}

/// `task.*` accepts a function or a thread.
pub(super) fn thread_arg(lua: &Lua, a: &Args, i: usize) -> ApiResult<Thread> {
    match a.get(i) {
        LuaValue::Function(f) => Ok(crate::runtime::threads::thread_for(lua, f.clone())?),
        LuaValue::Thread(t) => Ok(t.clone()),
        _ => Err(a.wrong(i, "function or thread")),
    }
}

pub(super) fn task_wait(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "wait");
    // Without a duration, resume on the next Heartbeat (creator-docs task.wait).
    let t = seconds(&a, 0, 0.0)?.max(0.0);
    let thread = lua.current_thread();
    with_world(lua, |w| {
        let since = w.scheduler.now();
        w.scheduler.schedule(
            Queue::Task,
            t,
            Job::Resume {
                thread,
                args: Vec::new(),
                wait: WaitKind::Task { since },
            },
        );
    });
    Ok(Outcome::Yield)
}

pub(super) fn task_spawn(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "spawn");
    let thread = thread_arg(lua, &a, 0)?;
    resume(lua, &thread, MultiValue::from_vec(a.rest(1)));
    Outcome::ret(lua, thread)
}

pub(super) fn task_defer(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "defer");
    let thread = thread_arg(lua, &a, 0)?;
    let job = Job::Resume {
        thread: thread.clone(),
        args: a.rest(1),
        wait: WaitKind::None,
    };
    let owner = lua.current_thread().to_pointer().addr();
    with_world(lua, |w| w.scheduler.defer_held(owner, job));
    Outcome::ret(lua, thread)
}

pub(super) fn task_delay(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "delay");
    let t = seconds(&a, 0, 0.0)?.max(0.0);
    let thread = thread_arg(lua, &a, 1)?;
    let job = Job::Resume {
        thread: thread.clone(),
        args: a.rest(2),
        wait: WaitKind::None,
    };
    with_world(lua, |w| w.scheduler.schedule(Queue::Task, t, job));
    Outcome::ret(lua, thread)
}

pub(super) fn task_cancel(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "cancel");
    let LuaValue::Thread(thread) = a.get(0).clone() else {
        return Err(a.wrong(0, "thread"));
    };
    with_world(lua, |w| {
        w.scheduler.cancel(|j| match j {
            Job::Resume { thread: t, .. } => *t == thread,
            _ => false,
        });
    });
    // Closing a suspended coroutine stops it for good.
    let co: Table = lua
        .named_registry_value::<Table>("rook.std")?
        .get("coroutine")?;
    let close: Function = co.get("close")?;
    // `coroutine.close` returns the thread's error, which the closing
    // script doesn't see.
    let _ = close.call::<MultiValue>(thread);
    Outcome::ret(lua, ())
}

/// The `task` library as code in `context` sees it: the same functions
/// everywhere, but `desynchronize` and `synchronize` switch the calling
/// thread to `context`'s Actor's phases.
pub(crate) fn task_library(lua: &Lua, context: Context) -> mlua::Result<Table> {
    let task = lua.create_table()?;
    if context.is_some() {
        let shared: Table = lua.named_registry_value::<Table>("rook.std")?.get("task")?;
        for pair in shared.pairs::<LuaValue, LuaValue>() {
            let (k, v) = pair?;
            task.raw_set(k, v)?;
        }
    } else {
        task.set("wait", api_fn(lua, task_wait)?)?;
        task.set("spawn", api_fn(lua, task_spawn)?)?;
        task.set("defer", api_fn(lua, task_defer)?)?;
        task.set("delay", api_fn(lua, task_delay)?)?;
        task.set("cancel", api_fn(lua, task_cancel)?)?;
    }
    task.set(
        "desynchronize",
        api_fn(lua, move |lua, _| desynchronize(lua, context))?,
    )?;
    task.set(
        "synchronize",
        api_fn(lua, move |lua, _| synchronize(lua, context))?,
    )?;
    Ok(task)
}

/// Confirmed: Studio 0.748, the words included.
fn outside_actor(function: &str) -> ScriptError {
    ScriptError(format!(
        "task.{function}() may only be called from a script that is a descendant of an Actor"
    ))
}

/// `task.desynchronize()`: serial code resumes in its Actor's next
/// parallel phase; parallel code carries on (creator-docs).
fn desynchronize(lua: &Lua, context: Context) -> ApiResult<Outcome> {
    let actor = context.ok_or_else(|| outside_actor("desynchronize"))?;
    let thread = lua.current_thread();
    let yielded = with_world(lua, |w| {
        if w.scheduler.phase() != Phase::Serial {
            return false;
        }
        let job = Job::Resume {
            thread,
            args: Vec::new(),
            wait: WaitKind::None,
        };
        w.scheduler.desynchronize(actor, job);
        true
    });
    if yielded {
        Ok(Outcome::Yield)
    } else {
        Outcome::ret(lua, ())
    }
}

/// `task.synchronize()`: parallel code resumes in the next serial phase;
/// serial code carries on (creator-docs).
fn synchronize(lua: &Lua, context: Context) -> ApiResult<Outcome> {
    context.ok_or_else(|| outside_actor("synchronize"))?;
    let thread = lua.current_thread();
    let yielded = with_world(lua, |w| {
        if w.scheduler.phase() == Phase::Serial {
            return false;
        }
        let job = Job::Resume {
            thread,
            args: Vec::new(),
            wait: WaitKind::None,
        };
        w.scheduler.synchronize(job);
        true
    });
    if yielded {
        Ok(Outcome::Yield)
    } else {
        Outcome::ret(lua, ())
    }
}
