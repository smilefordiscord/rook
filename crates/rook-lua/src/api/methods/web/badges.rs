//! Badges (`badges`): awarding them and who has them
//! (`docs/behavior/scripting/badges-and-groups.md`). With no endpoint
//! ([`super::requests`]), the session is all there is: a badge is had by
//! the players it awarded it to, for the run.

use mlua::Lua;
use rook_dom::InstanceId;

use super::RawMethod;
use super::requests::{self as web, WebRequest};
use super::users::{data, session_users, values};
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, with_world};

/// The most badges one `CheckUserBadgesAsync` checks (creator-docs;
/// measured: more raise `CheckUserBadgesAsync: Cannot query more than 10
/// badges.`).
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

/// The user and badge of the members that take `(user, badge)`, cast
/// last to first (measured: `Unable to cast string to User`, `Unable to
/// cast string to int64`).
fn user_and_badge(a: &Args) -> ApiResult<(i64, i64)> {
    let badge = a.int64(1)?;
    Ok((a.user(0)?, badge))
}

fn user_has_badge(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (user, badge) = user_and_badge(a)?;
    owned(lua, user, &[badge], |lua, had| values(lua, !had.is_empty()))
}

/// `CheckUserBadgesAsync(user, badgeIds)`: the ones the user has, up to
/// ten asked (creator-docs). Measured: an id in the list that isn't a
/// number raises `CheckUserBadgesAsync: badgeIds must be numeric.`
/// (Inference: a string that reads as one is refused too).
fn check_user_badges(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let ids = a.array(1)?;
    let user = a.user(0)?;
    let badges: Vec<i64> = (ids.sequence_values::<mlua::Value>())
        .map(|v| match v? {
            mlua::Value::Integer(n) => Ok(n),
            mlua::Value::Number(n) => Ok(n as i64),
            _ => Err(ScriptError(
                "CheckUserBadgesAsync: badgeIds must be numeric.".into(),
            )),
        })
        .collect::<ApiResult<_>>()?;
    if badges.len() > MAX_CHECKED {
        return Err(ScriptError(
            "CheckUserBadgesAsync: Cannot query more than 10 badges.".into(),
        ));
    }
    owned(lua, user, &badges, |lua, had| {
        let list = lua.create_sequence_from(had).map_err(|e| e.to_string())?;
        values(lua, list)
    })
}

/// `AwardBadgeAsync(user, badge)`: whether the badge was awarded. Only a
/// server awards, only to a player in it, and not a badge they have
/// (creator-docs); a client's call returns false once its arguments
/// cast (measured). Offline every badge exists and is enabled
/// (Inference); with an endpoint the request is
/// `POST badges/v1/users/{user}/badges/{badge}/award`, answering
/// `{"awarded": true}`, Rook's own: Roblox awards from its game servers
/// with no public API.
fn award_badge(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let (user, badge) = user_and_badge(a)?;
    if with_world(lua, |w| w.is_client) {
        return Outcome::ret(lua, false);
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
