//! The Selection service: what's selected in the editor, which plugins
//! read and change (creator-docs Selection;
//! docs/behavior/studio/plugins.md#the-selection). It holds only
//! instances in the DataModel, in the order selected, without duplicates.
//! When `SelectionChanged` fires differs by method, as measured in Studio.

use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;

use crate::api::methods::{RawMethod, ret_instances};
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::runtime::signal::SignalKey;
use crate::runtime::threads::fire;
use crate::runtime::world::World;

/// Whether `id` may be selected: the DataModel itself or something in it
/// (Measured: `Set` and `Add` drop an instance outside it).
fn selectable(w: &World, id: InstanceId) -> bool {
    let root = w.dom.root();
    !w.dom.is_destroyed(id) && (id == root || w.dom.is_ancestor_of(root, id))
}

/// The selection. What left the DataModel is gone at once (Measured: `Get`
/// right after `Parent = nil` leaves it out), before [`drop_moved`] runs.
pub(crate) fn current(w: &World) -> Vec<InstanceId> {
    (w.studio.selection.iter().copied())
        .filter(|&id| selectable(w, id))
        .collect()
}

/// Make the selection `next`, without what can't be selected and with
/// duplicates dropped (first kept). Returns whether it changed.
fn replace(w: &mut World, next: Vec<InstanceId>) -> bool {
    let mut unique: Vec<InstanceId> = Vec::with_capacity(next.len());
    for id in next {
        if !unique.contains(&id) && selectable(w, id) {
            unique.push(id);
        }
    }
    let changed = current(w) != unique;
    w.studio.selection = unique;
    changed
}

fn changed(lua: &Lua, selection: InstanceId) {
    fire(lua, &SignalKey::event(selection, "SelectionChanged"), &[]);
}

/// `Set`: fires `SelectionChanged` unless the selection was empty and
/// stays empty from an empty list, even when nothing changes (Measured:
/// `Set` with what's already selected fires; so does a list of instances
/// outside the DataModel on an empty selection).
pub(crate) fn select(lua: &Lua, selection: InstanceId, next: Vec<InstanceId>) {
    let fires = with_world(lua, |w| {
        let was_empty = current(w).is_empty();
        let given = !next.is_empty();
        replace(w, next);
        given || !was_empty
    });
    if fires {
        changed(lua, selection);
    }
}

/// Drop from the selection `moved` and its descendants, which left the
/// DataModel, firing `SelectionChanged` once for each instance dropped
/// (Measured: a selected folder's two selected children leaving fire it
/// twice; moving within the DataModel keeps them).
pub(crate) fn drop_moved(lua: &Lua, moved: InstanceId) {
    let dropped = with_world(lua, |w| {
        if w.studio.selection.is_empty() {
            return None;
        }
        let before = w.studio.selection.len();
        let dom = &w.dom;
        (w.studio.selection).retain(|&id| id != moved && !dom.is_ancestor_of(moved, id));
        let dropped = before - w.studio.selection.len();
        Some((w.dom.find_service("Selection")?, dropped))
    });
    if let Some((selection, dropped)) = dropped {
        for _ in 0..dropped {
            changed(lua, selection);
        }
    }
}

/// The instances in a table argument (Measured: anything else errors
/// "Expected table argument, got nil", naming the Lua type).
fn list(a: &Args) -> ApiResult<Vec<InstanceId>> {
    match a.get(0) {
        LuaValue::Table(_) => a.instances(0),
        other => Err(ScriptError(format!(
            "Expected table argument, got {}",
            other.type_name()
        ))),
    }
}

fn get(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let ids = with_world(lua, |w| current(w));
    ret_instances(lua, &ids)
}

fn set(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let next = list(a)?;
    select(lua, this, next);
    Outcome::ret(lua, ())
}

/// `Add` fires only when it selects something new (Measured).
fn add(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let more = list(a)?;
    let fires = with_world(lua, |w| {
        let mut next = current(w);
        next.extend(more);
        replace(w, next)
    });
    if fires {
        changed(lua, this);
    }
    Outcome::ret(lua, ())
}

/// `Remove` fires on every call, even one that removes nothing (Measured:
/// an empty list on an empty selection fires it).
fn remove(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let gone = list(a)?;
    with_world(lua, |w| {
        let mut next = current(w);
        next.retain(|id| !gone.contains(id));
        replace(w, next);
    });
    changed(lua, this);
    Outcome::ret(lua, ())
}

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    if owner != "Selection" {
        return None;
    }
    Some(match name {
        "Get" => get,
        "Set" => set,
        "Add" => add,
        "Remove" => remove,
        _ => return None,
    })
}
