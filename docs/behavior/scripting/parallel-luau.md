# Parallel Luau

Checked against Studio 0.748 on 2026-11-23.

Actors, serial and parallel phases, Actor messages, what desynchronized
code may do, and SharedTable (`rook_lua::runtime::actors`,
`rook_lua::api::methods::parallel`, `rook_lua::api::instance::parallel`,
`rook_lua::types::shared_table`; tests in `rook-lua`'s
`tests/scripting/parallel_luau.rs` and `rook-engine`'s
`tests/scripting/parallel_luau.rs`). The design, and how it moves to
worker threads, is [M14 step 6](../../roadmap/m14-scripting-completeness.md).

Measured with the conformance cases `scripting/actors`,
`scripting/parallel_phase`, `scripting/parallel_timing`,
`scripting/shared_table` and `scripting/shared_table_functions`, run in
Studio from code without a script (Studio's MCP, at plugin identity),
which builds an Actor with a Script of its own.

## Actors

*Whose script.* A script belongs to the closest Actor above it when it
starts (Confirmed: creator-docs Parallel Luau; Studio 0.748 for nested
Actors). Moving it afterwards doesn't change that (Inference: a Roblox
engineer on the DevForum describes threads as tied to the VM they
started in).

*Their own globals and modules.* Each Actor's scripts run in a Luau VM of
their own, so they have their own `_G` and `shared`, and each Actor
loads its own copy of every module it requires; scripts of one Actor
share them (Confirmed: creator-docs Actor; Studio 0.748, where a module
under an Actor required both outside and inside it counts separately).
What the DataModel's scripts put in `_G` or `shared` isn't an Actor's,
not even as a global (Confirmed: Studio 0.748). A module runs in the
context of whatever required it, wherever the ModuleScript is: a copy
required outside any Actor can't desynchronize or connect in parallel,
even when the ModuleScript sits under an Actor (Confirmed: creator-docs
task.desynchronize; Studio 0.748).

*`GetActor`.* The closest Actor at or above the instance, or nil; an
Actor gives itself, and an unparented instance or a service nil
(Confirmed: Studio 0.748). A ModuleScript outside the Actor gives nil
even when an Actor's script required it (Confirmed: creator-docs Actor).

In Rook all Actors share one Luau VM, each with a context standing for
its VM: the globals table its scripts fall back to, with its own `_G`,
`shared`, `require` and `task`, reading only the globals every script
starts with. Two things differ from separate VMs:

- The DataModel's own scripts keep Luau's `_G`, which is their globals
  table, so `_G.x = 1` there also makes `x` a global for the DataModel's
  scripts (Approximation, as Rook's `_G` already was).
- A function made in one context still runs if it reaches another
  (through a BindableEvent's arguments, say), where Roblox raises
  `Attempt to load a function from a different Lua VM`.

## Phases

*Switching.* In a serial phase, `task.desynchronize()` suspends the
thread until the next parallel phase; in a parallel one it does nothing.
`task.synchronize()` is the mirror image (Confirmed: creator-docs task;
Studio 0.748). Neither switches inline: the code after the call runs at
the next phase of that kind.

*Outside an Actor.* Both raise `task.desynchronize() may only be called
from a script that is a descendant of an Actor` (and `task.synchronize()`
likewise) (Confirmed: Studio 0.748).

*`ConnectParallel`.* The handler starts in the next parallel phase after
the signal fires, as if it called `task.desynchronize()` first
(Confirmed: creator-docs RBXScriptSignal; Studio 0.748). Code without a
script (the command bar's, a plugin's; in Rook, `run_source` without
one), and a module such code required, can't connect: `Function
Event.ConnectParallel can only be used inside scripts`, raised before the
function is looked at (Confirmed: Studio 0.748). A script outside any
Actor can't either: `Scripts that connect in parallel must be rooted
under an Actor`, or for a module's copy `A module script may only connect
in parallel if this instantiation of the script was required by a script
associated with an actor` (Confirmed that it fails; these words are
Inference, from DevForum reports).

*Staying desynchronized.* A desynchronized thread that waits with
`task.wait` or `task.delay` comes back in a parallel phase of its Actor,
and `task.spawn` and `coroutine.wrap` run at once, in the same phase
(Confirmed: Studio 0.748). Legacy `wait` and `spawn` come back serial
(Confirmed: Studio 0.748; `delay` is taken to do the same, Inference).

*`task.defer` while desynchronized.* The deferred thread runs in a
parallel phase, and later than a serial defer would: in Studio it ran
after the deferring thread had been resumed once more, both where that
thread synchronized and where it waited. Rook holds the thread until
then, or until the deferring thread ends (Approximation: why Studio runs
it there isn't known).

### Phases in the frame

The frame runs legacy resumption (`wait`, `spawn`), Stepped and
PreSimulation, the physics, PostSimulation, `task` resumption
(`task.wait`, `task.delay`), Heartbeat and, on clients, PreRender. A
parallel phase follows the legacy resumption, PreSimulation,
PostSimulation, Heartbeat and PreRender, so a parallel handler of an
event runs after the event's serial handlers and before the frame's next
event (Confirmed: Studio 0.748, for Stepped, PreSimulation,
PostSimulation, Heartbeat and `task` resumption, which comes just before
Heartbeat; the rest is Inference from the creator docs' task scheduler
diagram). That `task` resumption comes before Heartbeat was measured for
an Actor's script on a server; Rook does the same everywhere
(Inference).

Within one, in order (Confirmed: Studio 0.748):

1. The deferred work of the resumption point runs, then messages sent
   since the last one reach their serial bindings, messages those send
   included.
2. Every Actor's desynchronized work runs, in the order it came: a
   message's parallel bindings in the place the message was sent, before
   the handlers of a signal fired after it. Rook runs one Actor's work
   together, Actors in the order their first work came (Inference).
3. The threads that called `task.synchronize()` resume, one after
   another, before the frame goes on.

Roblox runs different Actors' work at the same time on several cores;
Rook runs it on the DataModel's thread, one Actor after another, so only
how long a frame takes differs.

## What desynchronized code may do

Each member has a thread safety level in the API dump, and Rook checks it
where the member is reached (Confirmed: creator-docs Parallel Luau;
Studio 0.748):

| Level | Properties | Methods |
|---|---|---|
| Unsafe | Neither read nor written | Not called |
| ReadSafe | Read, not written | |
| LocalSafe | Written only on the Actor's own instances | Called only by the Actor's own scripts |
| Safe | Read and written | Called |

Most properties are ReadSafe, so even the Actor's own instances can't be
written; the laid-out GUI ones (`AbsoluteSize`, `AbsolutePosition`, ...)
are Unsafe. Every event is Unsafe, and so is every callback (`OnInvoke`,
`OnServerInvoke`, ...), which can't be set. Deprecated lowercase
spellings have their own levels: `FindFirstChild` is Safe,
`findFirstChild` Unsafe. The errors, each naming the class that declares
the member (Confirmed: Studio 0.748):

- `Property Class.Name is not safe to read in parallel`
- `Property Class.Name is not safe to write in parallel`, also for a
  write of the wrong type or of the value already there
- `Function Class.Name is not safe to call in parallel`
- `Event Class.Name is not safe to access in parallel`
- `Callback Class.Name is not safe to access in parallel`, setting one
- `Function Event.Connect is not safe to call in parallel` (and `Once`,
  `ConnectParallel`) for a signal read before desynchronizing

A member that doesn't exist raises the usual `is not a valid member`
error first. `require` raises `require can not be used during the
parallel phase`. `Instance.new` works, though setting the new instance's
properties or destroying it doesn't, and a parent passed to it raises
`Unable to parent new Instance during parallel execution.`
`FindFirstDescendant` raises `FindFirstDescendant is not enabled`
(Confirmed: Studio 0.748, all of it; that `FindFirstDescendant` fails in
a serial phase too is Inference, as its message names no phase). Firing
a BindableEvent, `SendMessage`, `BindToMessage`, SharedTables and the
SharedTableRegistry work. The dump has no LocalSafe member today; Rook
treats a LocalSafe method as Unsafe.

## Messages

*Sending.* `Actor:SendMessage(topic, ...)` never yields and returns
nothing (Confirmed: creator-docs Actor; Studio 0.748). A missing or nil
topic raises `Argument 1 missing or nil`; then an Actor outside the
DataModel raises `Actor:SendMessage may only be called on Actors attached
to the DataModel.`, once the arguments have copied without a cycle
(`tables cannot be cyclic`) (Confirmed: Studio 0.748). The arguments are
copied as it's called, by the rules [bindables](bindables.md) copy by
(Confirmed: Studio 0.748, for nil arguments, mixed, sparse and
non-string keys, and metatables); a function among them raises `Attempt to load
a function from a different Lua VM` (Confirmed that functions can't
cross; the error's place and words are Inference). SharedTables arrive as
themselves (Confirmed: creator-docs Parallel Luau; Studio 0.748).

*Receiving.* Messages wait for the next resumption point. There, each goes
to the bindings its topic has then: `BindToMessage` handlers run serially,
`BindToMessageParallel` handlers in the phase. Each binding gets its own
copy, taken as the message was sent, the most recently bound first, as
with signals. A topic nobody binds drops the message, a binding made
during a delivery hears later messages only, one that raises doesn't stop
the others, and one may yield. Disconnecting a binding's connection
drops the invocations it hasn't run (Confirmed: Studio 0.748, all of
it).

*Binding.* The function must cast (`Argument 2 missing or nil`, `Unable
to cast double to Function`); then only the Actor's own scripts may bind
to its messages, anything else raising `Calling BindToMessage on an Actor
is only allowed in scripts that are a descendent of that Actor` (and
`BindToMessageParallel` likewise) (Confirmed: Studio 0.748).

## SharedTable

Confirmed: creator-docs SharedTable and SharedTableRegistry; Studio
0.748 for everything below unless it says otherwise.

- The `SharedTable` global is frozen. `SharedTable.new()` makes an empty
  table and `SharedTable.new(t)` copies a Luau table, nested tables
  becoming SharedTables; anything else, nil and a SharedTable included,
  raises `invalid argument #1 to 'new' (table expected, got <type>)`.
  Arguments after the first are ignored.
- Keys are strings or whole numbers from 0 to 2³² − 1. A number that
  isn't one raises `Numeric SharedTable keys must be nonnegative integers
  less than 2^32`, any other key (nil included, read or written)
  `SharedTable keys can only be strings or numbers.`.
- Values are booleans, numbers, vectors, strings, buffers (a copy),
  SharedTables, EnumItems, TweenInfos and Roblox's other datatypes.
  `Enum`, `Enums`, threads and `newproxy` userdata are stored as nil,
  without an error. Functions, Instances, DateTime, Random, Content,
  RBXScriptSignal and RBXScriptConnection raise `<Plural> can not be
  stored in a SharedTable` (`Functions`, `Instances`, `DateTimes`,
  `Randoms`, `Contents`, `EventInstances`, `Connections`); Rook names
  other types the same way (Inference). A Luau table assigned to a key
  becomes a new SharedTable; one that holds itself raises `Cannot convert
  a cyclic table to a SharedTable`; one reached twice becomes two.
- A failed assignment leaves the key as it was. Changing a frozen table
  raises `SharedTable is frozen`, after a bad key's error and before a
  bad value's.
- Distinct SharedTables never compare equal; the same one reached twice
  does. `tostring` gives `SharedTable: 0x` and the table's address, and
  `getmetatable` a string (Rook's is `The metatable is locked`,
  Inference). `#`, arithmetic, comparison, calling, `rawget`, `pairs`,
  `next`, `table.*` and attributes don't take one; `JSONEncode` writes
  it as `null`.
- `for k, v in st` walks a snapshot taken as the loop starts: whole
  numbers first, in order, then strings. Studio's strings come in a hash
  order that changes with the table's size; Rook's come in byte order
  (Approximation).
- `clone` copies the table only, sharing the SharedTables inside it;
  `clone(st, true)` copies every SharedTable it reaches, each once, and
  neither copy is frozen. `cloneAndFreeze` does the same and freezes the
  copies. A second argument, when given, must be a boolean (`invalid
  argument #2 to 'clone' (boolean expected, got nil)`).
- A function's missing SharedTable raises `missing argument #1 to 'size'
  (SharedTable expected)`, a wrong one `invalid argument #1 to 'size'
  (SharedTable expected, got <type>)`.
- `increment(st, key, delta)` adds to a number already there and returns
  the number before. A missing delta raises `Failed to get value for
  insertion into SharedTable` and one that isn't a number, a numeric
  string included, `Variant cast failed`; a missing key `Unable to
  increment the value for key "<key>" because it does not exist in the
  SharedTable.` and another value `... because it is not a number
  value.` (how a numeric key is quoted is Inference).
- `update(st, key, f)` calls `f` with the value and stores what it
  returns (nil removes the key), calling it again when the table changed
  while it ran. A function that changes the table every time ends in
  `table overflow`; Rook stops after 1000 calls (Approximation: Studio's
  limit isn't known). An error in `f` propagates as raised, tables
  included, and leaves the value; `f` can't yield (`thread is not
  yieldable`); one that isn't a function raises `Third argument to
  SharedTable::Update should be a function.`; a frozen table raises
  before `f` runs. `size`, `clear` and `isFrozen` do what they say.
- `SharedTableRegistry:GetSharedTable(name)` gives the table registered
  under the name, making one if there is none; `SetSharedTable(name,
  nil)` unregisters it, leaving the table out raises nothing, and
  anything but a SharedTable raises `Unable to cast <type> to
  SharedTable` (`Array` for a list, `double` for a number; Rook's
  `Dictionary` for other tables is Inference). A number name is its string; nil is
  `Argument 1 missing or nil`. Studio takes a Luau table for a name too;
  Rook names it by its `tostring` (Inference).

## What Studio must settle

- Why a thread deferred by desynchronized code runs when it does.
- Whether `task` resumption comes before Heartbeat for the DataModel's
  own scripts and on clients, as it does for an Actor's script on a
  server.
- The order of SharedTable's string keys, and after how many calls
  `update` gives up.
- What `ConnectParallel` from a script outside any Actor raises, and
  what `SendMessage` with a function to an Actor in the DataModel does.
