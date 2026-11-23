# Change history

Checked against Studio 0.748 on 2026-11-23.

How edits made in Edit mode are undone: ChangeHistoryService's waypoints
and recordings, built on the DOM's undo log. Measured rules come from
Luau run in Studio 0.748's Edit DataModel through Studio's MCP, on a
folder made for the purpose and undone afterwards. The MCP keeps a
recording of its own open while its code runs, so recordings were
measured from a `task.delay` that ran after it closed.

Code: the log in `rook-dom/src/history.rs`, the service in
`rook-lua/src/api/methods/studio/history.rs`. Tests in
`rook-engine/src/tests/studio/history.rs`.

## What's recorded

In Edit mode the DOM keeps what each edit replaced: a property's value
(or that it read its default), a name, a parent, an attribute or a tag.
Only edits to the place count:

- instances in the DataModel, not in CoreGui or PluginGuiService (the GUI
  Studio and its plugins draw);
- not read-only properties the engine works out (`AbsoluteSize`) or
  terrain's voxels;
- not what the engine and the editor write themselves (the edit camera,
  input state, GUI layout);
- not services coming into the DataModel by their first `GetService`.

What isn't in the DataModel keeps its values as they are. A part made and
set up outside it, then moved in, is undone by moving it out again
(Measured: undoing the step that parented a new folder to Workspace left
it unparented).

`Destroy` can't be undone, as the instance is locked and freed once
nothing holds it. Editors delete by moving to nil, which can (Rook's own
rule; Studio's delete can be undone too).

## Steps

Undo works in steps, each named by the waypoint or recording that made
it. Edits made outside a recording wait for the next step and become
part of it (Measured):

- `SetWaypoint(name)` makes them a step named `name`. With no edits it
  makes no step.
- A recording that starts takes them in: `Commit` keeps them in its step
  and `Cancel` puts them back with the recording's own.
- `Undo` called first puts them back with the newest step, firing
  `OnUndo` once, with that step's name. With no step before them, Rook
  undoes them alone as an unnamed step (Inference).

`Undo` puts the newest step back, newest edit first, and fires
`OnUndo(name)`. `Redo` does it again and fires `OnRedo(name)` (Confirmed:
creator-docs ChangeHistoryService; Measured). `GetCanUndo` and
`GetCanRedo` return `true` and the step's name, or a lone `false`
(Measured: `select("#", chs:GetCanRedo())` is 1). A new step drops what
was put aside to redo. `SetEnabled(false)` makes waypoints do nothing, and
`ResetWaypoints` forgets every step.

Studio's `GetCanUndo` also returned `true` and an empty name right after
an `Undo` that had taken loose edits, until the next waypoint; the cause
wasn't isolated and Rook doesn't do this.

## Recordings

`TryBeginRecording(name, displayName)` returns an identifier,
`recording_` and a number, and fires `OnRecordingStarted(name,
displayName)`, `displayName` `nil` when none was given (Measured). One
recording is open at a time: starting another returns `nil` without an
error (Measured), as does starting one outside Edit mode (Inference).
Starting one keeps the redo steps.

`FinishRecording(id, operation)` ends it and fires
`OnRecordingFinished(name, displayName, id, operation, options)`, where
`displayName` is the name when none was given and `options` is
`FinishRecording`'s third argument (Measured: `nil` when not passed).
Measured, with the creator-docs' FinishRecordingOperation:

- `Commit` keeps its edits as a step named `name` (not `displayName`,
  which `GetCanUndo` doesn't show); with no edits there is no step.
- `Cancel` puts its edits back, an instance it parented included.
- `Append` keeps its edits as a step of its own, named by its recording,
  as `Commit` does. Undoing it right after a commit undid only its own
  edits, and `GetCanUndo` then named the commit, though the creator-docs
  say it adds them to the previous recording.
- Finishing drops the redo steps, an empty `Cancel` included.

Finishing with an identifier that isn't the open one errors `Invalid
recording identifier.`, and with none open `No recording in progress.`
(also for one already finished). `IsRecordingInProgress()` says whether
any is open, `IsRecordingInProgress(id)` whether that one is (Measured).
