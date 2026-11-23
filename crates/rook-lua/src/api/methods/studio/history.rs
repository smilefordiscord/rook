//! ChangeHistoryService: edits made in Edit mode are kept as undoable
//! steps, made by waypoints and recordings
//! (docs/behavior/studio/history.md). The DOM keeps what each edit
//! replaced ([`rook_dom::Undo`]) until a step takes it; edits outside a
//! recording wait there for the next step, as in Studio. Outside Edit mode
//! nothing is recorded and these do nothing.

use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;

use crate::api::methods::{RawMethod, mutate};
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::runtime::signal::SignalKey;
use crate::runtime::threads::fire;
use crate::runtime::world::World;
use crate::studio::{Recording, Step};

/// Keep the edits since the last step as a step named `name`, which drops
/// what was put aside to redo. With no edits there is no step (Measured).
fn split(w: &mut World, name: &str) {
    if !w.studio.is_edit() {
        return;
    }
    let changes = w.dom.take_recorded();
    if changes.is_empty() {
        return;
    }
    let history = &mut w.studio.history;
    history.undo.push(Step {
        name: name.to_owned(),
        changes,
    });
    history.redo.clear();
}

fn string(lua: &Lua, s: &str) -> mlua::Result<LuaValue> {
    Ok(LuaValue::String(lua.create_string(s)?))
}

fn set_waypoint(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    with_world(lua, |w| {
        if !w.studio.history.disabled {
            split(w, &name);
        }
    });
    Outcome::ret(lua, ())
}

/// Undo the newest step, firing `OnUndo` with its name. Edits made since
/// the last step are undone with it (Measured: one `Undo` put back both,
/// firing `OnUndo` once with the step's name); with no step before them
/// they're undone alone, unnamed (Inference).
pub(crate) fn undo(lua: &Lua, service: InstanceId) -> ApiResult<()> {
    let step = with_world(lua, |w| {
        let loose = w.dom.take_recorded();
        let history = &mut w.studio.history;
        match history.undo.last_mut() {
            Some(last) => last.changes.extend(loose),
            None if !loose.is_empty() => history.undo.push(Step {
                name: String::new(),
                changes: loose,
            }),
            None => {}
        }
        history.undo.pop()
    });
    let Some(step) = step else {
        return Ok(());
    };
    let redo = mutate(lua, |w| Ok(w.dom.revert(&step.changes)))?;
    with_world(lua, |w| {
        w.studio.history.redo.push(Step {
            name: step.name.clone(),
            changes: redo,
        });
    });
    fire(
        lua,
        &SignalKey::event(service, "OnUndo"),
        &[string(lua, &step.name)?],
    );
    Ok(())
}

/// Redo the step undone last, firing `OnRedo`.
pub(crate) fn redo(lua: &Lua, service: InstanceId) -> ApiResult<()> {
    let step = with_world(lua, |w| w.studio.history.redo.pop());
    let Some(step) = step else {
        return Ok(());
    };
    let undo = mutate(lua, |w| Ok(w.dom.revert(&step.changes)))?;
    with_world(lua, |w| {
        w.studio.history.undo.push(Step {
            name: step.name.clone(),
            changes: undo,
        });
    });
    fire(
        lua,
        &SignalKey::event(service, "OnRedo"),
        &[string(lua, &step.name)?],
    );
    Ok(())
}

fn undo_method(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    undo(lua, this)?;
    Outcome::ret(lua, ())
}

fn redo_method(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    redo(lua, this)?;
    Outcome::ret(lua, ())
}

/// `true` and the step's waypoint name, or `false` (creator-docs
/// ChangeHistoryService:GetCanUndo).
fn can(lua: &Lua, step: Option<String>) -> ApiResult<Outcome> {
    match step {
        Some(name) => Outcome::ret(lua, (true, name)),
        None => Outcome::ret(lua, false),
    }
}

fn get_can_undo(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let step = with_world(lua, |w| {
        w.studio.history.undo.last().map(|s| s.name.clone())
    });
    can(lua, step)
}

fn get_can_redo(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let step = with_world(lua, |w| {
        w.studio.history.redo.last().map(|s| s.name.clone())
    });
    can(lua, step)
}

fn reset_waypoints(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    with_world(lua, |w| {
        w.dom.take_recorded();
        w.studio.history.undo.clear();
        w.studio.history.redo.clear();
    });
    Outcome::ret(lua, ())
}

fn set_enabled(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let on = a.truthy(0);
    with_world(lua, |w| w.studio.history.disabled = !on);
    Outcome::ret(lua, ())
}

/// Start a recording: the edits until `FinishRecording`, and those made
/// since the last step (Measured), become one step. Its identifier, or
/// `nil` while another is open (Measured) or outside Edit mode
/// (Inference). Starting one leaves the redo steps (Measured).
fn try_begin_recording(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let name = a.string(0)?;
    let display_name = a.opt_string(1)?;
    let started = with_world(lua, |w| {
        if !w.studio.is_edit() || w.studio.history.recording.is_some() {
            return None;
        }
        let history = &mut w.studio.history;
        history.next_recording += 1;
        let id = format!("recording_{}", history.next_recording);
        history.recording = Some(Recording {
            id: id.clone(),
            name: name.clone(),
            display_name: display_name.clone(),
        });
        Some(id)
    });
    let Some(id) = started else {
        return Outcome::ret(lua, LuaValue::Nil);
    };
    let display_name = match &display_name {
        Some(d) => string(lua, d)?,
        None => LuaValue::Nil,
    };
    fire(
        lua,
        &SignalKey::event(this, "OnRecordingStarted"),
        &[string(lua, &name)?, display_name],
    );
    Outcome::ret(lua, id)
}

/// End a recording: `Commit` keeps its edits as a step named as the
/// recording, `Cancel` puts them back (creator-docs
/// FinishRecordingOperation). `Append` keeps them as its own step too
/// (Measured: undoing it after a commit leaves the commit's edits).
/// Finishing drops the redo steps, even an empty `Cancel` (Measured).
fn finish_recording(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let id = a.string(0)?;
    let operation = a.enum_value(lua, 1, "FinishRecordingOperation")?;
    let op_name = rook_reflection::database()
        .enum_by_name("FinishRecordingOperation")
        .and_then(|e| e.item_name(operation))
        .unwrap_or("Commit");
    let recording = with_world(lua, |w| finish(w, &id))?;
    let changes = with_world(lua, |w| w.dom.take_recorded());
    if op_name == "Cancel" {
        mutate(lua, |w| {
            w.dom.revert(&changes);
            Ok(())
        })?;
    }
    with_world(lua, |w| {
        let history = &mut w.studio.history;
        if op_name != "Cancel" && !changes.is_empty() {
            history.undo.push(Step {
                name: recording.name.clone(),
                changes,
            });
        }
        history.redo.clear();
    });
    // Measured: the display name is the name when none was given.
    let display_name = recording.display_name.as_deref().unwrap_or(&recording.name);
    fire(
        lua,
        &SignalKey::event(this, "OnRecordingFinished"),
        &[
            string(lua, &recording.name)?,
            string(lua, display_name)?,
            string(lua, &id)?,
            a.get(1).clone(),
            a.get(2).clone(),
        ],
    );
    Outcome::ret(lua, ())
}

/// Close the open recording `id` (Measured: the errors when it isn't the
/// one open, or none is).
fn finish(w: &mut World, id: &str) -> ApiResult<Recording> {
    match w.studio.history.recording.take() {
        Some(open) if open.id == id => Ok(open),
        Some(other) => {
            w.studio.history.recording = Some(other);
            Err(ScriptError("Invalid recording identifier.".into()))
        }
        None => Err(ScriptError("No recording in progress.".into())),
    }
}

fn is_recording(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let id = a.opt_string(0)?;
    let on = with_world(lua, |w| match (&w.studio.history.recording, &id) {
        (Some(open), Some(id)) => open.id == *id,
        (Some(_), None) => true,
        (None, _) => false,
    });
    Outcome::ret(lua, on)
}

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    if owner != "ChangeHistoryService" {
        return None;
    }
    Some(match name {
        "SetWaypoint" => set_waypoint,
        "Undo" => undo_method,
        "Redo" => redo_method,
        "GetCanUndo" => get_can_undo,
        "GetCanRedo" => get_can_redo,
        "ResetWaypoints" => reset_waypoints,
        "SetEnabled" => set_enabled,
        "TryBeginRecording" => try_begin_recording,
        "FinishRecording" => finish_recording,
        "IsRecordingInProgress" => is_recording,
        _ => return None,
    })
}
