//! Badges (`badges`): awarding them and who has them
//! (`docs/behavior/scripting/badges-and-groups.md`). With no endpoint
//! ([`super::requests`]), the session is all there is: a badge is had by
//! the players it awarded it to, for the run.

use mlua::Lua;
use rook_dom::InstanceId;

use super::RawMethod;
use super::requests::{self as web, WebRequest};
use super::users::{data, id, session_users, values};
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, with_world};

/// The most badges one `CheckUserBadgesAsync` checks (creator-docs).
const MAX_CHECKED: usize = 10;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("BadgeService", "UserHasBadgeAsync" | "UserHasBadge") => user_has_badge,
        ("BadgeService", "AwardBadgeAsync" | "AwardBadge") => award_badge,
        ("BadgeService", "CheckUserBadgesAsync") => check_user_badges,
        _ => return None,
    })
}

/// The badges `user` has among `badges`, from
/// `GET badges/v1/users/{user}/badges/awarded-dates?badgeIds={a,b,...}`
/// (each `data` entry's `badgeId`), or the session's awards.
fn owned(
    lua: &Lua,
    user: i64,
    badges: &[i64],
    answer: fn(&Lua, Vec<i64>) -> Result<mlua::MultiValue, String>,
) -> ApiResult<Outcome> {
    let list = badges
        .iter()
        .map(i64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let path = format!("/v1/users/{user}/badges/awarded-dates?badgeIds={list}");
    let Some(url) = web::url(lua, "badges", &path) else {
        let had = with_world(lua, |w| {
            (badges.iter().copied())
                .filter(|b| w.badges_awarded.contains(&(user, *b)))
                .collect()
        });
        return Ok(Outcome::Return(answer(lua, had)?));
    };
    web::call(
        lua,
        WebRequest::get(url),
        Box::new(move |lua, reply| {
            let had = (data(reply)?.iter())
                .filter_map(|d| d["badgeId"].as_i64())
                .collect();
            answer(lua, had)
        }),
    )
}

fn user_has_badge(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (user, badge) = (id(a, 0)?, id(a, 1)?);
    owned(lua, user, &[badge], |lua, had| values(lua, !had.is_empty()))
}

/// `CheckUserBadgesAsync(user, badgeIds)`: the ones the user has, up to
/// ten asked (creator-docs; the error past ten is worded by Rook).
fn check_user_badges(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let user = id(a, 0)?;
    let mlua::Value::Table(ids) = a.get(1) else {
        return Err(a.wrong(1, "Array"));
    };
    let badges: Vec<i64> = (ids.clone().sequence_values::<f64>())
        .filter_map(Result::ok)
        .map(|n| n as i64)
        .collect();
    if badges.len() > MAX_CHECKED {
        return Err(ScriptError(
            "BadgeService:CheckUserBadgesAsync() checks at most 10 badges".into(),
        ));
    }
    owned(lua, user, &badges, |lua, had| {
        let list = lua.create_sequence_from(had).map_err(|e| e.to_string())?;
        values(lua, list)
    })
}

/// `AwardBadgeAsync(user, badge)`: whether the badge was awarded. Only a
/// server awards, only to a player in it, and not a badge they have
/// (creator-docs). Offline every badge exists and is enabled
/// (Inference); with an endpoint the request is
/// `POST badges/v1/users/{user}/badges/{badge}/award`, answering
/// `{"awarded": true}`, Rook's own: Roblox awards from its game servers
/// with no public API.
fn award_badge(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (user, badge) = (id(a, 0)?, id(a, 1)?);
    if with_world(lua, |w| w.is_client) {
        return Err(ScriptError(
            "BadgeService:AwardBadgeAsync() can only be called from the server".into(),
        ));
    }
    if !session_users(lua).iter().any(|(id, ..)| *id == user) {
        return Outcome::ret(lua, false);
    }
    let path = format!("/v1/users/{user}/badges/{badge}/award");
    let Some(url) = web::url(lua, "badges", &path) else {
        let awarded = with_world(lua, |w| w.badges_awarded.insert((user, badge)));
        return Outcome::ret(lua, awarded);
    };
    web::call(
        lua,
        WebRequest::post_json(url, &serde_json::json!({})),
        Box::new(|lua, answer| {
            let json = web::json(answer)?;
            values(lua, json["awarded"].as_bool().unwrap_or(false))
        }),
    )
}
