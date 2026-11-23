//! What Roblox's servers hand a player about how they arrived: join data
//! and teleport data (`docs/behavior/scripting/join-data.md`). A Rook
//! session's players all join it directly, never by a teleport, invite
//! or share link, so there is nothing to hand over.

use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;

use super::RawMethod;
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome};

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("Player", "GetJoinData") => join_data,
        ("TeleportService", "GetLocalPlayerTeleportData") => teleport_data,
        _ => return None,
    })
}

/// Every field is present only for the way of joining it describes
/// (creator-docs Player:GetJoinData), so a direct join's table is empty
/// (Inference).
fn join_data(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, lua.create_table()?)
}

/// No teleport brought the player, so no data came with one.
fn teleport_data(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, LuaValue::Nil)
}
