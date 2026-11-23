//! What RunService says of where code runs, in a live game and in each of
//! Studio's modes (Confirmed: creator-docs RunService, the table under
//! `IsStudio`; in Edit and Run both `IsServer` and `IsClient` are true).

use mlua::Lua;
use rook_dom::InstanceId;

use crate::api::methods::RawMethod;
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, with_world};
use crate::studio::StudioMode;

fn mode(lua: &Lua) -> (Option<StudioMode>, bool) {
    with_world(lua, |w| (w.studio.mode, w.is_client))
}

fn is_studio(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, mode(lua).0.is_some())
}

fn is_edit(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, mode(lua).0 == Some(StudioMode::Edit))
}

fn is_run_mode(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, mode(lua).0 == Some(StudioMode::Run))
}

fn is_running(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, mode(lua).0 != Some(StudioMode::Edit))
}

fn is_server(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let (mode, client) = mode(lua);
    Outcome::ret(
        lua,
        !client || matches!(mode, Some(StudioMode::Edit | StudioMode::Run)),
    )
}

fn is_client(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let (mode, client) = mode(lua);
    Outcome::ret(
        lua,
        client || matches!(mode, Some(StudioMode::Edit | StudioMode::Run)),
    )
}

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    if owner != "RunService" {
        return None;
    }
    Some(match name {
        "IsStudio" => is_studio,
        "IsEdit" => is_edit,
        "IsRunMode" => is_run_mode,
        "IsRunning" => is_running,
        "IsServer" => is_server,
        "IsClient" => is_client,
        _ => return None,
    })
}
