# Parallel Luau

Checked against Studio 0.744 on 2026-11-23, against the creator docs and
the API dump only: nothing here is measured in Studio yet (see [what
Studio must settle](#what-studio-must-settle)).

Actors, serial and parallel phases, Actor messages, what desynchronized
code may do, and SharedTable (`rook_lua::runtime::actors`,
`rook_lua::api::methods::parallel`, `rook_lua::api::instance::parallel`,
`rook_lua::types::shared_table`; tests in `rook-lua`'s
`tests/scripting/parallel_luau.rs` and `rook-engine`'s
`tests/scripting/parallel_luau.rs`). The design, and how it moves to
worker threads, is [M14 step 6](../../roadmap/m14-scripting-completeness.md).

## Actors

*Whose script.* A script belongs to the closest Actor above it when it
starts (Confirmed: creator-docs Parallel Luau). Moving it afterwards
doesn't change that (Inference: a Roblox engineer on the DevForum
describes threads as tied to the VM they started in).

*Their own globals and modules.* Each Actor's scripts run in a Luau VM of
their own, so they have their own `_G` and `shared`, and each Actor
loads its own copy of every module it requires; scripts of one Actor
share them (Confirmed: creator-docs Actor). A module runs in the context
of the script that required it, wherever the ModuleScript is: a copy
required by a script outside any Actor can't desynchronize or connect in
parallel, even when the ModuleScript sits under an Actor (Confirmed:
creator-docs task.desynchronize).

*`GetActor`.* The closest Actor at or above the instance, or nil; a
ModuleScript outside the Actor gives nil even when an Actor's script
required it (Confirmed: creator-docs Actor). An Actor gives itself
(Inference).

In Rook all Actors share one Luau VM, each with a context standing for
its VM: the globals table its scripts fall back to, with its own `_G`,
`shared`, `require` and `task`. Two things differ from separate VMs:

- The DataModel's own scripts keep Luau's `_G`, which is their globals
  table, so `_G.x = 1` there also makes `x` a global the Actors' scripts
  see (Approximation, as Rook's `_G` already was).
- A function made in one context still runs if it reaches another
  (through a BindableEvent's arguments, say), where Roblox raises
  `Attempt to load a function from a different Lua VM`.

## Phases

*Switching.* In a serial phase, `task.desynchronize()` suspends the
thread until the next parallel phase; in a parallel one it does nothing.
`task.synchronize()` is the mirror image (Confirmed: creator-docs task).
Neither switches inline: the code after the call runs at the next phase
of that kind.

*Outside an Actor.* Both raise an error (Confirmed: creator-docs task):
`task.desynchronize() should only be called from a script that is a
descendant of an Actor` (and `task.synchronize()` likewise; the words
are Inference, from DevForum reports, some of which describe it as a
warning in live servers).

*`ConnectParallel`.* The handler starts in the next parallel phase after
the signal fires, as if it called `task.desynchronize()` first
(Confirmed: creator-docs RBXScriptSignal). A function from outside any
Actor can't connect: `Scripts that connect in parallel must be rooted
under an Actor`, or for a module's copy `A module script may only connect
in parallel if this instantiation of the script was required by a script
associated with an actor` (Confirmed that it fails; the words are
Inference, from DevForum reports).

*Staying desynchronized.* A desynchronized thread that waits (`task.wait`,
`wait`, `task.delay`) or defers (`task.defer`) comes back in a parallel
phase of its Actor; `task.spawn` runs at once, in the same phase
(Inference).

### Phases in the frame

Every place the creator docs' task scheduler diagram runs scripts ends
with a parallel phase: after legacy resumption (`wait`, `spawn`), after
`PreSimulation`, after `PostSimulation`, after `Heartbeat` and `task`
resumption, and on clients after `PreRender` (Inference). A parallel
handler of an event therefore runs after the event's serial handlers and
before the frame's next event.

Within one, in order (Inference: the creator docs say only that the
engine waits for every parallel section before going on):

1. Messages sent since the last one reach their bindings.
2. Every Actor's desynchronized work runs: each Actor's in the order it
   came, Actors in the order their first work came. Work deferred during
   the phase runs in it too.
3. The threads that called `task.synchronize()` resume, one after
   another, before the frame goes on.

Roblox runs different Actors' work at the same time on several cores;
Rook runs it on the DataModel's thread, one Actor after another, so only
how long a frame takes differs.

## What desynchronized code may do

Each member has a thread safety level in the API dump, and Rook checks it
where the member is reached (Confirmed: creator-docs Parallel Luau):

| Level | Properties | Methods |
|---|---|---|
| Unsafe | Neither read nor written | Not called |
| ReadSafe | Read, not written | |
| LocalSafe | Written only on the Actor's own instances | Called only by the Actor's own scripts |
| Safe | Read and written | Called |

Most properties are ReadSafe; the laid-out GUI ones (`AbsoluteSize`,
`AbsolutePosition`, ...) are Unsafe. Every event is Unsafe. Deprecated
lowercase spellings have their own levels: `FindFirstChild` is Safe,
`findFirstChild` Unsafe. The errors (Inference: the forms DevForum
reports quote, `Property BasePart.CFrame is not safe to write in
parallel`, `Function PVInstance.PivotTo is not safe to call in parallel`,
`Event BasePart.Touched is not safe to access in parallel`), each naming
the class that declares the member:

- `Property Class.Name is not safe to read in parallel`
- `Property Class.Name is not safe to write in parallel`
- `Function Class.Name is not safe to call in parallel`
- `Event Class.Name is not safe to access in parallel`

`require` raises an error in parallel (Confirmed: creator-docs Parallel
Luau; the words, `require is not safe to call in parallel`, are
Inference). `Instance.new` works (Confirmed: the creator docs' example
makes an Explosion in parallel), though setting the new instance's
properties doesn't, and a parent passed to it raises as setting `Parent`
does (Inference). The dump has no LocalSafe member today; Rook treats a
LocalSafe method as Unsafe.

## Messages

*Sending.* `Actor:SendMessage(topic, ...)` never yields (Confirmed:
creator-docs Actor). The arguments are copied as it's called, by the
rules [bindables](bindables.md) copy by (the creator docs say only
"passed by copy"; the rules are Inference); a function among them raises
`Attempt to load a function from a different Lua VM` (Confirmed that
functions can't cross; the error's place and words are Inference).
SharedTables arrive as themselves (Confirmed: creator-docs Parallel
Luau).

*Receiving.* Messages wait for the next parallel phase. There, each goes
to the bindings its topic has then: `BindToMessage` handlers run at once,
serially; `BindToMessageParallel` handlers run in the phase. Each
binding gets its own copy, as each bindable listener does, the most
recently bound first, as with signals. A topic nobody binds drops the
message (Inference, all of it). Disconnecting a binding's connection
drops the invocations it hasn't run.

*Binding.* Only the Actor's own scripts may bind to its messages
(Confirmed: creator-docs Actor); anything else raises `BindToMessage can
only be called from a script that is a descendant of the Actor`
(Inference).

## SharedTable

Confirmed: creator-docs SharedTable and SharedTableRegistry.

- Keys are strings or whole numbers from 0 to 2³² − 1; values are
  booleans, numbers, vectors, strings, SharedTables and Roblox's
  serializable datatypes. `SharedTable.new(t)` copies a Luau table,
  nested tables becoming SharedTables.
- Distinct SharedTables never compare equal; the same one reached twice
  does.
- `clone` copies the table only, sharing the SharedTables inside it;
  `clone(st, true)` copies every SharedTable it reaches, each once.
  `cloneAndFreeze` does the same and freezes the copies; changing a
  frozen table raises an error.
- `increment` adds to a number already there and returns the number
  before; `update` calls its function with the value and stores what it
  returns, calling it again if another script changed the table
  meanwhile; `size`, `clear` and `isFrozen` do what they say.
- `SharedTableRegistry:GetSharedTable(name)` gives the table registered
  under the name, making one if there is none; `SetSharedTable(name, nil)`
  unregisters it.

Inference: a Luau table assigned to a key becomes a new SharedTable, as in
`new`; `for k, v in st` walks a snapshot taken as the loop starts, whole
numbers first in order, then strings by their bytes; `tostring` gives
`SharedTable`. Rook's errors read `invalid SharedTable key (expected a
string or a whole number from 0 to 4294967295)`, `a value of type
function cannot be stored in a SharedTable`, `SharedTable.increment: the
element is not a number` and, for a frozen table, Luau's own `attempt to
modify a readonly table`; Roblox's words aren't known. Rook's Actors take
turns, so nothing but `update`'s own function can change the table while
it runs; one that does raises `SharedTable.update: the update function
must not change the table`, where retrying would only repeat the change
forever (the creator docs don't say what Roblox does).

## What Studio must settle

- Each error's words, and whether `task.desynchronize` outside an Actor
  is an error or a warning.
- Where the frame's parallel phases are, and when synchronized threads
  resume.
- When messages are delivered, in what order bindings run, and what
  happens to a message nobody binds.
- What a desynchronized thread's waits and defers resume in.
- How SharedTable iterates, prints and reports bad keys and values, and
  what `update` does when its own function changes the table.
