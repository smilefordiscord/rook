# M14 Scripting completeness

Driven by the corpus run (`reports/run/`): today 60.4% of places with
server scripts run 10 seconds without a script error. Many errors are the
scripts' own (old syntax modern Roblox rejects too, missing children);
the target counts only errors Rook causes, checked against Studio.

1. Classify the corpus run's errors: Rook's (unimplemented members,
   wrong behavior) against the scripts' own, with a sample of each
   checked in Studio.
2. The unimplemented members by places affected, starting with
   `InsertService:LoadAsset` (54 places) and `require(assetId)` (17).
3. Services that talk to Roblox's website, answered locally and
   offline: MarketplaceService, BadgeService, GroupService, Players'
   web lookups.
4. TeleportService between places in one folder (multi-place games).
5. HttpService, opt-in per place.
6. Modern Luau: Parallel Luau (Actor, `task.desynchronize`, SharedTable),
   `buffer`, and the libraries and syntax of the Luau live Roblox runs.
   - *Scope*: what scripts observe: Actors as contexts with their own
     globals and modules, `SendMessage`, `BindToMessage` and
     `BindToMessageParallel`, `task.desynchronize`/`synchronize`,
     `ConnectParallel`, the errors desynchronized code meets, SharedTable
     and SharedTableRegistry; then running Actors' parallel phases on
     worker threads. Out: Studio's Actor-related tools (the script
     profiler's per-Actor view) and `ThreadPoolConfig` settings.
   - *Evidence*: the creator docs (Parallel Luau, Actor, SharedTable,
     SharedTableRegistry, the `task` library, RBXScriptSignal), the API
     dump's `ThreadSafety` per member, and the error messages DevForum
     reports quote. No Studio recording yet.
   - *Design* (2026-11-23):
     - Roblox's model: an Actor is a unit of isolation. Each runs its
       scripts in a Luau VM of its own (own globals, own copy of every
       module), and scripts of one Actor run one at a time. Each place
       the frame runs scripts has a serial phase, where anything goes,
       then a parallel phase, where every Actor's desynchronized threads
       run at once across cores and the engine waits for all of them
       before going on. Desynchronized code may read most of the
       DataModel but write nothing; each member's level (Unsafe,
       ReadSafe, LocalSafe, Safe) is in the API dump. Actors talk through
       messages, copied between VMs, and SharedTables, shared by
       reference behind atomic operations.
     - Established engines with actor-style scripting run it the same
       way: a fixed pool of workers, each taking whole isolates whose
       state no other worker touches, with messages copied between them
       and a barrier before the serial world goes on. Erlang's BEAM runs
       processes on one scheduler per core with copied messages; Web
       Workers (in browsers and Deno) run each JavaScript isolate, its own
       V8 heap, on one thread, passing structured clones and sharing only
       `SharedArrayBuffer`s; Unity's job system and Bevy's parallel
       systems split a frame into read-only parallel stages and
       structural changes applied at a sync point. What those engines
       never do is let two workers run one VM.
     - Rook's choice: one context per Actor, the Actor's state and the
       unit a worker runs. Its globals table (with its own `_G`, `shared`,
       `require` and `task`) is what its scripts' environments fall back
       to; module results are kept per context; a function's context is
       its environment's. The scheduler owns the phase and hands out a
       phase's work grouped by Actor (`Scheduler::take_parallel`), each
       group run in order. Work a desynchronized thread waits for or
       defers stays tagged with its Actor. The engine calls
       `Runtime::run_parallel_phase` at the end of each place its frame
       runs scripts; it delivers messages, runs the groups, then resumes
       what synchronized. The thread-safety checks read the dump's level
       at the member's single entry point (property read and write,
       method function, event lookup). SharedTable keeps its contents
       outside Luau behind an `Arc<RwLock>`, so it already crosses
       threads. The first version runs the groups one after another on
       the DataModel's thread: every rule a script can observe holds, and
       nothing calling into the scheduler or the runtime moves when the
       groups go to workers.
     - To workers: each context gets a Luau VM of its own (instances,
       signals' handlers and jobs then carry their context, and values
       crossing contexts go through the message copy, as Roblox's do),
       and the DOM is read through a shared borrow during the phase: the
       checks above already keep parallel code from writing, and the
       lazy work reads trigger (GUI layout, joint settling, physics
       syncing a query) runs before the phase or under a lock of its own.
       Bevy's and Unity's parallel stages read their worlds the same way.
     - Cost: nothing per frame when no script uses an Actor (an empty
       queue checked at each phase). A desynchronized thread costs one
       queue entry per phase it waits for. A message costs one copy when
       sent and one per binding. A context costs one globals table and
       its own copy of the modules it requires, as a VM of its own would.
   - *Done when*: the [Match Studio](README.md#match-studio) bar holds for
     the page's findings, with conformance cases recorded in Studio for
     the phase order, the messages and the errors, and Actors' parallel
     phases run on worker threads, benched on a place with many Actors
     against running them one after another.
7. Client scripts in the corpus run too, once characters and GUI exist.
8. **Match Studio** in the script runtime M1 built: `BindableFunction`
   resuming as Studio does, `SignalBehavior.AncestryDeferred`'s own rules,
   the place datatype errors begin with, and `DataStoreService`'s versions,
   `ListKeysAsync`, budgets and `DataStorePages`.
   - *Evidence*: the [stubs](../guide/stubs.md) rows taken over by this
     step, and the open Approximations and Inferences
     on the scripting pages.
   - *Done when*: the [Match Studio](README.md#match-studio) bar holds
     for steps 1 to 7 and the runtime M1 built, and no stubs row names
     this step.