//! DataStoreService and the objects it hands out
//! (docs/behavior/scripting/data-stores.md), over the backing in
//! `crate::data_stores`. Each request that goes to Roblox's servers is
//! written in Luau (`api/luau/datastore.luau`): it asks [`requests`] to
//! spend its budget, waits in its queue while the budget is spent, then
//! runs here.

use mlua::Lua;
use rook_dom::InstanceId;

use super::{RawMethod, first};
use crate::data_stores::{Millis, StoreId};
use crate::runtime::ctx::{ApiResult, ScriptError, with_world};
use crate::runtime::world::World;

mod entries;
mod info;
mod pages;
mod requests;
mod service;
mod versions;

pub(crate) use requests::helpers;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    first(&[service::native, info::native, pages::native], owner, name)
}

pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    requests::luau_source(owner, name)
}

/// The game's time in seconds and the Unix time in milliseconds, the
/// clocks budgets and stored times follow.
fn clock(w: &World) -> (f64, Millis) {
    (
        w.scheduler.now(),
        (w.scheduler.unix_time() * 1000.0) as Millis,
    )
}

/// The players in the server, which budgets grow with.
fn players(w: &World) -> usize {
    w.dom.find_service("Players").map_or(0, |players| {
        (w.dom.children(players).iter())
            .filter(|&&c| w.dom.is_a(c, "Player"))
            .count()
    })
}

/// The store a store object reads and writes.
fn target(lua: &Lua, this: InstanceId) -> ApiResult<StoreId> {
    with_world(lua, |w| w.data_stores.objects.store_of(this).cloned())
        .ok_or_else(|| ScriptError("This data store object is no longer usable".into()))
}
