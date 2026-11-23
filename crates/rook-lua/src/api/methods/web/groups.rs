//! Groups (`groups`): a player's rank and role, GroupService's lookups
//! (`docs/behavior/scripting/badges-and-groups.md`). With no endpoint
//! ([`super::requests`]), the session is all there is: no group exists
//! and nobody is in one.

use mlua::{Lua, Table};
use rook_dom::InstanceId;
use serde_json::Value as Json;

use super::RawMethod;
use super::requests::{self as web, WebRequest};
use super::users::{ask, data, player_id, values};
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError};

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("Player", "GetRankInGroup" | "GetRankInGroupAsync") => rank_in_group,
        ("Player", "GetRoleInGroup" | "GetRoleInGroupAsync") => role_in_group,
        ("Player", "IsInGroup" | "IsInGroupAsync") => is_in_group,
        ("GroupService", "GetGroupsAsync") => groups,
        ("GroupService", "GetRolesInGroupAsync") => roles_in_group,
        ("GroupService", "GetGroupInfoAsync") => group_info,
        _ => return None,
    })
}

/// The user's role in `group`, from `GET groups/v2/users/{id}/groups/roles`.
fn role_of(data: &[Json], group: i64) -> Option<&Json> {
    data.iter()
        .find(|r| r["group"]["id"].as_i64() == Some(group))
        .map(|r| &r["role"])
}

/// Answer with what `decode` makes of the user's role in `group`.
fn in_group<T: mlua::IntoLuaMulti + 'static>(
    lua: &Lua,
    user: i64,
    group: i64,
    local: T,
    decode: fn(Option<&Json>) -> T,
) -> ApiResult<Outcome> {
    let path = format!("/v2/users/{user}/groups/roles");
    ask(lua, "groups", &path, local, move |data| {
        decode(role_of(&data, group))
    })
}

/// The rank, 0 outside the group (creator-docs Player:GetRankInGroup).
fn rank_in_group(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    in_group(lua, player_id(lua, this), a.int64(0)?, 0, |r| {
        r.and_then(|r| r["rank"].as_i64()).unwrap_or(0)
    })
}

/// The role's name, "Guest" outside the group (creator-docs
/// Player:GetRoleInGroup).
fn role_in_group(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    in_group(
        lua,
        player_id(lua, this),
        a.int64(0)?,
        "Guest".to_owned(),
        |r| {
            r.and_then(|r| r["name"].as_str())
                .unwrap_or("Guest")
                .to_owned()
        },
    )
}

fn is_in_group(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    in_group(lua, player_id(lua, this), a.int64(0)?, false, |r| {
        r.is_some()
    })
}

/// `GetRolesInGroupAsync(user, group)`: `{IsMember, Roles}`, each role
/// `{Id, Name, Rank}` (creator-docs GroupService). Roblox's web API gives
/// a member's one role.
fn roles_in_group(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let group = a.int64(1)?;
    let user = a.user(0)?;
    let make = |lua: &Lua, role: Option<&Json>| -> mlua::Result<Table> {
        let t = lua.create_table()?;
        t.set("IsMember", role.is_some())?;
        let roles = lua.create_table()?;
        if let Some(role) = role {
            let r = lua.create_table()?;
            r.set("Id", role["id"].as_i64().unwrap_or(0))?;
            r.set("Name", role["name"].as_str().unwrap_or(""))?;
            r.set("Rank", role["rank"].as_i64().unwrap_or(0))?;
            roles.raw_push(r)?;
        }
        t.set("Roles", roles)?;
        Ok(t)
    };
    let path = format!("/v2/users/{user}/groups/roles");
    let Some(url) = web::url(lua, "groups", &path) else {
        return Outcome::ret(lua, make(lua, None)?);
    };
    web::call(
        lua,
        WebRequest::get(url),
        Box::new(move |lua, answer| {
            let data = data(answer)?;
            let t = make(lua, role_of(&data, group)).map_err(|e| e.to_string())?;
            values(lua, t)
        }),
    )
}

/// `GetGroupsAsync(user)`: each group the user is in, as
/// `{Name, Id, EmblemUrl, EmblemId, Rank, Role, IsPrimary, IsInClan}`
/// (creator-docs), from `GET groups/v1/users/{id}/groups/roles`. The
/// emblem needs the thumbnails service, so `EmblemUrl` is empty and
/// `EmblemId` 0 (Approximation).
fn groups(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let user = a.user(0)?;
    let Some(url) = web::url(lua, "groups", &format!("/v1/users/{user}/groups/roles")) else {
        return Outcome::ret(lua, lua.create_table()?);
    };
    web::call(
        lua,
        WebRequest::get(url),
        Box::new(move |lua, answer| {
            let entries = data(answer)?;
            values(lua, group_list(lua, &entries).map_err(|e| e.to_string())?)
        }),
    )
}

fn group_list(lua: &Lua, entries: &[Json]) -> mlua::Result<Table> {
    let list = lua.create_table()?;
    for entry in entries {
        let (group, role) = (&entry["group"], &entry["role"]);
        let t = lua.create_table()?;
        t.set("Name", group["name"].as_str().unwrap_or(""))?;
        t.set("Id", group["id"].as_i64().unwrap_or(0))?;
        t.set("EmblemUrl", "")?;
        t.set("EmblemId", 0)?;
        t.set("Rank", role["rank"].as_i64().unwrap_or(0))?;
        t.set("Role", role["name"].as_str().unwrap_or(""))?;
        t.set(
            "IsPrimary",
            entry["isPrimaryGroup"].as_bool().unwrap_or(false),
        )?;
        t.set("IsInClan", false)?;
        list.raw_push(t)?;
    }
    Ok(list)
}

/// `GetGroupInfoAsync(group)`: `{Name, Id, Owner = {Name, Id}, EmblemUrl,
/// Description, Roles = {{Name, Rank}}}`, Owner nil for a group without
/// one (creator-docs), from `GET groups/v1/groups/{id}` and then
/// `GET groups/v1/groups/{id}/roles`, roles by rank. `EmblemUrl` is empty
/// (Approximation, as in `GetGroupsAsync`). With no endpoint no group
/// exists, and asking raises (Inference: the wording is Rook's).
fn group_info(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let group = a.int64(0)?;
    let failed = move || format!("GroupService:GetGroupInfoAsync() failed: Unknown group {group}");
    let (Some(info_url), Some(roles_url)) = (
        web::url(lua, "groups", &format!("/v1/groups/{group}")),
        web::url(lua, "groups", &format!("/v1/groups/{group}/roles")),
    ) else {
        return Err(ScriptError(failed()));
    };
    web::call_then(
        lua,
        WebRequest::get(info_url),
        Box::new(move |_, answer| {
            // Inference: the groups API answers 400 for an id with no
            // group; other failures keep their own reason.
            if answer.as_ref().is_ok_and(|r| matches!(r.status, 400 | 404)) {
                return Err(failed());
            }
            let info = web::json(answer)
                .map_err(|e| format!("GroupService:GetGroupInfoAsync() failed: {e}"))?;
            let decode: web::Decode = Box::new(move |lua, answer| {
                let roles = web::json(answer)?;
                let t = info_table(lua, &info, &roles).map_err(|e| e.to_string())?;
                values(lua, t)
            });
            Ok((WebRequest::get(roles_url), decode))
        }),
    )
}

fn info_table(lua: &Lua, info: &Json, roles: &Json) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("Name", info["name"].as_str().unwrap_or(""))?;
    t.set("Id", info["id"].as_i64().unwrap_or(0))?;
    let owner = &info["owner"];
    if owner.is_object() {
        let o = lua.create_table()?;
        o.set("Name", owner["username"].as_str().unwrap_or(""))?;
        o.set("Id", owner["userId"].as_i64().unwrap_or(0))?;
        t.set("Owner", o)?;
    }
    t.set("EmblemUrl", "")?;
    t.set("Description", info["description"].as_str().unwrap_or(""))?;
    let mut list: Vec<&Json> = roles["roles"].as_array().into_iter().flatten().collect();
    list.sort_by_key(|r| r["rank"].as_i64().unwrap_or(0));
    let out = lua.create_table()?;
    for role in list {
        let r = lua.create_table()?;
        r.set("Name", role["name"].as_str().unwrap_or(""))?;
        r.set("Rank", role["rank"].as_i64().unwrap_or(0))?;
        out.raw_push(r)?;
    }
    t.set("Roles", out)?;
    Ok(t)
}
