//! [Parallel Luau](docs/behavior/scripting/parallel-luau.md): Actors,
//! phases, messages, what desynchronized code may do, and SharedTable.

use crate::tests::*;
use rook_dom::InstanceId;

/// A DataModel to fill with Actors and scripts.
fn place() -> Runtime {
    Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap()
}

/// Add a `class` named `name` under `parent`, with `source` if it's a
/// script.
fn add(rt: &Runtime, parent: InstanceId, class: &str, name: &str, source: &str) -> InstanceId {
    let mut w = rt.world_mut();
    let id = w.dom.create_by_name(class).unwrap();
    w.dom.set_name(id, name.into()).unwrap();
    if let Some(prop) = w.dom.property_id(id, "Source") {
        w.dom
            .set_property(id, prop, rook_types::Value::String(source.into()))
            .unwrap();
    }
    w.dom.set_parent(id, Some(parent)).unwrap();
    id
}

fn service(rt: &Runtime, name: &str) -> InstanceId {
    rt.world_mut().dom.get_service(name).unwrap()
}

/// Start `scripts` in order and run `frames` frames.
fn play(rt: &Runtime, scripts: &[InstanceId], frames: u32) -> Vec<String> {
    for &s in scripts {
        assert!(rt.run_script(s));
    }
    finish(rt, frames)
}

/// `desynchronize` suspends the script until the parallel phase, never
/// inline; inside it, it does nothing; `synchronize` comes back for the
/// serial phase after.
#[test]
fn phases_switch_at_the_next_phase_not_inline() {
    let rt = place();
    let sss = service(&rt, "ServerScriptService");
    let actor = add(&rt, sss, "Actor", "Actor", "");
    let worker = add(
        &rt,
        actor,
        "Script",
        "Worker",
        r#"
        print("serial")
        task.synchronize()
        print("still serial")
        task.desynchronize()
        print("parallel")
        task.desynchronize()
        print("still parallel")
        task.synchronize()
        print("serial again")
        "#,
    );
    let main = add(&rt, sss, "Script", "Main", r#"print("main")"#);
    assert_eq!(
        play(&rt, &[worker, main], 1),
        [
            "serial",
            "still serial",
            "main",
            "parallel",
            "still parallel",
            "serial again"
        ]
    );
}

/// Confirmed: Studio 0.748.
#[test]
fn switching_phases_outside_an_actor_is_an_error() {
    expect(
        r#"
        print(pcall(task.desynchronize))
        print(pcall(task.synchronize))
        "#,
        1,
        &[
            "false task.desynchronize() may only be called from a script that is a descendant of an Actor",
            "false task.synchronize() may only be called from a script that is a descendant of an Actor",
        ],
    );
}

/// A parallel connection's handler waits for the parallel phase; work it
/// waits for or defers there stays desynchronized. What it defers runs
/// once the handler has been resumed again, as in Studio 0.748.
#[test]
fn connect_parallel_handlers_start_in_the_parallel_phase() {
    let rt = place();
    let sss = service(&rt, "ServerScriptService");
    let actor = add(&rt, sss, "Actor", "Actor", "");
    let script = add(
        &rt,
        actor,
        "Script",
        "Script",
        r#"
        local b = Instance.new("BindableEvent")
        b.Event:ConnectParallel(function(n)
            print("handler", n, pcall(function() b.Name = "x" end))
            task.defer(function()
                print("deferred", pcall(function() b.Name = "x" end))
            end)
            task.wait()
            print("waited", pcall(function() b.Name = "x" end))
            task.synchronize()
            b.Name = "synchronized"
            print(b.Name)
        end)
        b:Fire(1)
        print("fired")
        "#,
    );
    let refused = "false Property Instance.Name is not safe to write in parallel";
    assert_eq!(
        play(&rt, &[script], 2),
        [
            "fired".to_owned(),
            format!("handler 1 {refused}"),
            format!("waited {refused}"),
            format!("deferred {refused}"),
            "synchronized".to_owned(),
        ]
    );
}

/// A signal fired by desynchronized code runs its serial handlers, those
/// of a script outside any Actor included, at the next serial resumption
/// point and not as that Actor's parallel code, which would refuse what
/// they may do.
#[test]
fn serial_handlers_of_a_signal_fired_in_parallel_run_serially() {
    for behavior in ["Immediate", "Deferred"] {
        let rt = place();
        let sss = service(&rt, "ServerScriptService");
        add(&rt, sss, "BindableEvent", "Event", "");
        let listener = add(
            &rt,
            sss,
            "Script",
            "Listener",
            r#"
            local event = script.Parent.Event
            event.Event:Connect(function(n)
                print("heard", n, pcall(function() event.Name = "Event" end))
            end)
            "#,
        );
        let actor = add(&rt, sss, "Actor", "Actor", "");
        let script = add(
            &rt,
            actor,
            "Script",
            "Script",
            r#"
            local event = script.Parent.Parent.Event
            task.desynchronize()
            event:Fire(1)
            print("fired")
            "#,
        );
        rt.world_mut().signal_behavior = if behavior == "Immediate" {
            SignalBehavior::Immediate
        } else {
            SignalBehavior::Deferred
        };
        assert_eq!(
            play(&rt, &[listener, script], 2),
            ["fired", "heard 1 true"],
            "{behavior}"
        );
    }
}

/// A script outside any Actor, or a module it required, can't connect in
/// parallel; code without a script (the command bar's, a plugin's) can't
/// either, a module it required included (Confirmed: Studio 0.748, for
/// code without a script).
#[test]
fn connecting_in_parallel_outside_an_actor_is_an_error() {
    let rt = place();
    let sss = service(&rt, "ServerScriptService");
    let storage = service(&rt, "ReplicatedStorage");
    add(
        &rt,
        storage,
        "ModuleScript",
        "Connector",
        r#"
        return function(signal)
            return pcall(function() signal:ConnectParallel(function() end) end)
        end
        "#,
    );
    let source = r#"
        local signal = Instance.new("BindableEvent").Event
        print(pcall(function() signal:ConnectParallel(function() end) end))
        print(require(game.ReplicatedStorage.Connector)(signal))
        "#;
    let script = add(&rt, sss, "Script", "Script", source);
    assert_eq!(
        play(&rt, &[script], 0),
        [
            "false Scripts that connect in parallel must be rooted under an Actor",
            "false A module script may only connect in parallel if this instantiation of the \
             script was required by a script associated with an actor",
        ]
    );
    rt.run_source("Command", source, None).unwrap();
    assert_eq!(
        finish(&rt, 0),
        [
            "false Function Event.ConnectParallel can only be used inside scripts",
            "false Function Event.ConnectParallel can only be used inside scripts",
        ]
    );
}

/// Reads of most properties and calls of Safe members work in parallel;
/// writes, Unsafe calls, Unsafe reads, events and `require` raise errors.
#[test]
fn desynchronized_code_may_only_do_what_is_safe_in_parallel() {
    let rt = place();
    let sss = service(&rt, "ServerScriptService");
    let actor = add(&rt, sss, "Actor", "Actor", "");
    let script = add(
        &rt,
        actor,
        "Script",
        "Script",
        r#"
        local part = Instance.new("Part")
        local frame = Instance.new("Frame")
        task.desynchronize()
        print(part.Name, part:FindFirstChild("x"), script:GetActor() == script.Parent)
        local function try(f) print((select(2, pcall(f)))) end
        try(function() part.Name = "x" end)
        try(function() part:Clone() end)
        try(function() part:findFirstChild("x") end)
        try(function() part:WaitForChild("x") end)
        try(function() return frame.AbsoluteSize end)
        try(function() return part.Touched end)
        try(function() require(script) end)
        print(typeof(Instance.new("Part")))
        try(function() Instance.new("Part", part) end)
        task.synchronize()
        part.Name = "ok"
        print(part.Name, frame.AbsoluteSize)
        "#,
    );
    assert_eq!(
        play(&rt, &[script], 1),
        [
            "Part nil true",
            "Property Instance.Name is not safe to write in parallel",
            "Function Instance.Clone is not safe to call in parallel",
            "Function Instance.findFirstChild is not safe to call in parallel",
            "Function Instance.WaitForChild is not safe to call in parallel",
            "Property GuiBase2d.AbsoluteSize is not safe to read in parallel",
            "Event BasePart.Touched is not safe to access in parallel",
            "require can not be used during the parallel phase",
            "Instance",
            "Unable to parent new Instance during parallel execution.",
            "ok 0, 0",
        ]
    );
}

/// Callbacks are Unsafe in the dump, so desynchronized code can't set
/// one, which it once did past the check every property write passes.
#[test]
fn desynchronized_code_cannot_set_a_callback() {
    let rt = place();
    let sss = service(&rt, "ServerScriptService");
    let actor = add(&rt, sss, "Actor", "Actor", "");
    let script = add(
        &rt,
        actor,
        "Script",
        "Script",
        r#"
        local bf = Instance.new("BindableFunction")
        local rf = Instance.new("RemoteFunction")
        task.desynchronize()
        print(pcall(function() bf.OnInvoke = function() return "parallel" end end))
        print(pcall(function() rf.OnServerInvoke = nil end))
        task.synchronize()
        bf.OnInvoke = function() return "serial" end
        print(bf:Invoke())
        "#,
    );
    assert_eq!(
        play(&rt, &[script], 1),
        [
            "false Callback BindableFunction.OnInvoke is not safe to access in parallel",
            "false Callback RemoteFunction.OnServerInvoke is not safe to access in parallel",
            "serial",
        ]
    );
}

/// Messages wait for the next resumption point; serial bindings run there,
/// parallel ones in its parallel phase. Each binding gets a copy taken as
/// the message was sent; functions can't be sent; SharedTables arrive as
/// themselves.
#[test]
fn messages_reach_the_actors_bindings_at_the_next_resumption_point() {
    let rt = place();
    let sss = service(&rt, "ServerScriptService");
    let actor = add(&rt, sss, "Actor", "Worker", "");
    let receiver = add(
        &rt,
        actor,
        "Script",
        "Receiver",
        r#"
        local actor = script:GetActor()
        actor:BindToMessage("Greeting", function(t, st)
            print("serial", t.x, pcall(function() actor.Name = "Worker" end))
            t.x = "changed"
        end)
        actor:BindToMessageParallel("Greeting", function(t, st)
            print("parallel", t.x, pcall(function() actor.Name = "Worker" end))
            SharedTable.increment(st, "n", 1)
        end)
        local gone = actor:BindToMessage("Greeting", function() print("disconnected") end)
        gone:Disconnect()
        "#,
    );
    let sender = add(
        &rt,
        sss,
        "Script",
        "Sender",
        r#"
        local actor = game.ServerScriptService.Worker
        local t = { x = "sent" }
        local st = SharedTable.new({ n = 0 })
        actor:SendMessage("Greeting", t, st)
        t.x = "after"
        actor:SendMessage("Nobody", 1)
        print("sent", pcall(actor.SendMessage, actor, "Greeting", print))
        print(pcall(actor.BindToMessage, actor, "Greeting", print))
        task.wait()
        print("n", st.n)
        "#,
    );
    let refused = "false Property Instance.Name is not safe to write in parallel";
    assert_eq!(
        play(&rt, &[receiver, sender], 2),
        [
            "sent false Attempt to load a function from a different Lua VM".to_owned(),
            "false Calling BindToMessage on an Actor is only allowed in scripts that are a \
             descendent of that Actor"
                .to_owned(),
            "serial sent true".to_owned(),
            format!("parallel sent {refused}"),
            "n 1".to_owned(),
        ]
    );
}

/// Each Actor's scripts have their own globals and their own copy of each
/// module (creator-docs Actor); scripts in one Actor share them.
#[test]
fn each_actor_has_its_own_globals_and_modules() {
    let rt = place();
    let sss = service(&rt, "ServerScriptService");
    let storage = service(&rt, "ReplicatedStorage");
    add(
        &rt,
        storage,
        "ModuleScript",
        "Counter",
        r#"print("loading", script:GetActor()) return { n = 0 }"#,
    );
    let main = add(
        &rt,
        sss,
        "Script",
        "Main",
        r#"
        _G.where = "main"
        shared.where = "main"
        local counter = require(game.ReplicatedStorage.Counter)
        counter.n += 1
        print("main", counter.n, script:GetActor())
        "#,
    );
    let actor = add(&rt, sss, "Actor", "Actor", "");
    let source = r#"
        local counter = require(game.ReplicatedStorage.Counter)
        counter.n += 1
        print(script.Name, counter.n, _G.where, shared.where, script:GetActor())
        _G.where = "actor"
    "#;
    let a = add(&rt, actor, "Script", "A", source);
    let b = add(&rt, actor, "Script", "B", source);
    assert_eq!(
        play(&rt, &[main, a, b], 0),
        [
            "loading nil",
            "main 1 nil",
            "loading nil",
            "A 1 nil nil Actor",
            "B 2 actor nil Actor",
        ]
    );
}

/// An Actor's globals outlive its scripts, as its VM does: a script that
/// set them and stopped leaves them for one started after a collection
/// (they were held only through running scripts' environments, so a
/// collection while none ran lost them). They go once the Actor is freed.
#[test]
fn an_actors_globals_outlive_its_scripts_until_it_is_freed() {
    let rt = place();
    let sss = service(&rt, "ServerScriptService");
    let actor = add(&rt, sss, "Actor", "Actor", "");
    let first = add(
        &rt,
        actor,
        "Script",
        "First",
        r#"_G.x = "kept" shared.y = "kept" script:Destroy()"#,
    );
    assert!(play(&rt, &[first], 1).is_empty());
    rt.lua().gc_collect().unwrap();
    rt.lua().gc_collect().unwrap();
    let second = add(&rt, actor, "Script", "Second", "print(_G.x, shared.y)");
    assert_eq!(play(&rt, &[second], 1), ["kept kept"]);

    let held = |rt: &Runtime| {
        let all: mlua::Table = rt.lua().named_registry_value("rook.actor_globals").unwrap();
        all.pairs::<mlua::Value, mlua::Value>().count()
    };
    assert_eq!(held(&rt), 1);
    rt.world_mut().dom.destroy(actor).unwrap();
    finish(&rt, 1);
    let mut freed = Vec::new();
    for _ in 0..2 {
        rt.lua().gc_collect().unwrap();
        let end = rt.world().dom.journal().end();
        freed.extend(rt.trim_journal(end));
    }
    assert!(freed.contains(&actor), "the Actor is freed");
    assert_eq!(held(&rt), 0);
}

#[test]
fn shared_tables_keep_what_the_creator_docs_list() {
    expect(
        r#"
        local st = SharedTable.new({ x = 1, y = 2, z = { "a", "b", "c" } })
        print(typeof(st), st.x, st.y, st.z[1], st.z[3], typeof(st.z), tostring(st):match("^SharedTable: 0x%x+$") ~= nil)
        print(SharedTable.size(st))
        st.y = nil
        print(SharedTable.size(st), st.y)
        print(SharedTable.increment(st, "x", 5), st.x)
        SharedTable.update(st, "w", function(v) print("old", v) return "new" end)
        print(st.w)
        st.v = Vector3.new(1, 2, 3)
        st.c = Color3.new(1, 0, 0)
        st.t = { 1 }
        print(st.v, st.c, typeof(st.t))
        local keys = {}
        for k, v in SharedTable.new({ 10, 20, a = 1 }) do
            table.insert(keys, `{k}={v}`)
        end
        print(table.concat(keys, " "))
        print(pcall(function() st[-1] = 1 end))
        print(pcall(function() st[1.5] = 1 end))
        print(pcall(function() st.f = print end))
        print(pcall(function() st.i = workspace end))
        print(pcall(SharedTable.increment, st, "w", 1))
        "#,
        0,
        &[
            "SharedTable 1 2 a c SharedTable true",
            "3",
            "2 nil",
            "1 6",
            "old nil",
            "new",
            "1, 2, 3 1, 0, 0 SharedTable",
            "1=10 2=20 a=1",
            "false Numeric SharedTable keys must be nonnegative integers less than 2^32",
            "false Numeric SharedTable keys must be nonnegative integers less than 2^32",
            "false Functions can not be stored in a SharedTable",
            "false Instances can not be stored in a SharedTable",
            "false Unable to increment the value for key \"w\" because it is not a number value.",
        ],
    );
}

/// `update` calls its function again when the table changed while it ran;
/// one that changes it every time stops with `table overflow`, as Studio
/// 0.748's does, rather than run until the script times out.
#[test]
fn a_shared_table_update_retries_while_its_function_changes_the_table() {
    expect(
        r#"
        local st = SharedTable.new({ n = 1 })
        local calls = 0
        print(pcall(SharedTable.update, st, "n", function(v)
            calls += 1
            if calls == 1 then st.other = v end
            return v + 1
        end))
        print(calls, st.n, st.other)
        print(pcall(SharedTable.update, st, "n", function(v) st.other = v return v + 1 end))
        print(st.n)
        "#,
        0,
        &["true", "2 2 1", "false table overflow", "2"],
    );
}

/// Shallow clones share the tables inside; deep clones share nothing,
/// recursive tables included; frozen tables refuse every change.
#[test]
fn shared_tables_clone_freeze_and_compare_by_identity() {
    expect(
        r#"
        local st = SharedTable.new({ inner = { 1 } })
        st.self = st
        local shallow = SharedTable.clone(st)
        local deep = SharedTable.clone(st, true)
        print(shallow == st, shallow.inner == st.inner, shallow.self == st)
        print(deep.inner == st.inner, deep.self == deep)
        print(SharedTable.new() == SharedTable.new())
        local frozen = SharedTable.cloneAndFreeze(st, true)
        print(SharedTable.isFrozen(frozen), SharedTable.isFrozen(frozen.inner), SharedTable.isFrozen(st))
        print(pcall(function() frozen.x = 1 end))
        print(pcall(SharedTable.clear, frozen))
        print(pcall(SharedTable.update, frozen, "x", function() print("ran") end))
        SharedTable.clear(st)
        print(SharedTable.size(st), SharedTable.size(shallow))
        local registry = game:GetService("SharedTableRegistry")
        local named = registry:GetSharedTable("Scores")
        print(named == registry:GetSharedTable("Scores"))
        registry:SetSharedTable("Scores", st)
        print(registry:GetSharedTable("Scores") == st)
        registry:SetSharedTable("Scores", nil)
        print(registry:GetSharedTable("Scores") == st)
        "#,
        0,
        &[
            "false true true",
            "false true",
            "false",
            "true true false",
            "false SharedTable is frozen",
            "false SharedTable is frozen",
            "false SharedTable is frozen",
            "0 2",
            "true",
            "true",
            "false",
        ],
    );
}

/// A script can nest tables as deep as memory allows: converting or deeply
/// copying one past the limit raises, and a long chain of SharedTables is
/// freed, where both overflowed the stack.
#[test]
fn deeply_nested_shared_tables_raise_or_free_without_a_crash() {
    expect(
        r#"
        local t = {} for i = 1, 100000 do t = {t} end
        print(pcall(SharedTable.new, t))
        local n = SharedTable.new()
        for i = 1, 100000 do local m = SharedTable.new() m.c = n n = m end
        print(pcall(SharedTable.clone, n, true))
        print(#tostring(SharedTable.clone(n, false)))
        n = nil
        "#,
        0,
        &[
            "false SharedTables can't nest more than 1000 deep",
            "false SharedTables can't nest more than 1000 deep",
            "31",
        ],
    );
}
