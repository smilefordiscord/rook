//! Players leaving: `Player:Kick`, and what leaving does to the DataModel
//! (docs/behavior/characters/leaving.md). Scripts only ask for a kick here;
//! the play session that holds the player's connection carries it out
//! ([`crate::Runtime::take_kicks`]) and removes the player
//! ([`crate::Runtime::remove_player`]).

use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;

use crate::api::instance::push_instance;
use crate::api::methods::mutate;
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, with_world};
use crate::runtime::signal::SignalKey;
use crate::runtime::threads::{drain_deferred, fire};

/// Confirmed (creator-docs Player:Kick): a message a server gives that is
/// longer than this, in bytes, is dropped for the default notice.
const MAX_SERVER_MESSAGE: usize = 500;

/// Why a player left: `Enum.PlayerExitReason`, which `PlayerRemoving`
/// passes (creator-docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitReason {
    /// Any other way of leaving, such as the client going away.
    Unknown,
    /// A script called `Player:Kick()`.
    CreatorKick,
}

impl ExitReason {
    fn item(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
            Self::CreatorKick => "CreatorKick",
        }
    }
}

/// `Player:Kick(message)`: the player's connection is to end, showing
/// `message`. A client may kick only its own player (creator-docs). The
/// player stays until the session ends the connection, after the calling
/// script goes on.
pub(super) fn kick(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let mut message = a.opt_string(0)?.unwrap_or_default();
    with_world(lua, |w| {
        if w.is_client {
            if w.dom.local_player() != Some(this) {
                // Inference: the text isn't measured in Studio.
                return Err(ScriptError(
                    "Cannot kick a non-local Player from a LocalScript".into(),
                ));
            }
        } else if message.len() > MAX_SERVER_MESSAGE {
            message.clear();
        }
        w.kicks.push((this, message));
        Ok(())
    })?;
    Outcome::ret(lua, ())
}

/// The player's character goes: `CharacterRemoving` fires with it, then
/// it's destroyed. Respawning and leaving both take it this way.
pub(crate) fn remove_character(lua: &Lua, player: InstanceId) -> ApiResult<()> {
    let Some(old) = with_world(lua, |w| w.dom.ref_property(player, "Character")) else {
        return Ok(());
    };
    let ud = push_instance(lua, old)?;
    fire(
        lua,
        &SignalKey::event(player, "CharacterRemoving"),
        &[LuaValue::UserData(ud)],
    );
    mutate(lua, |w| w.dom.destroy(old))
}

/// `player` leaves: `Players.PlayerRemoving` fires while it's still in
/// Players (creator-docs), then its character is removed and `Character`
/// cleared, then it leaves Players. Deferred handlers of `PlayerRemoving`
/// run before the rest, so they see the player in Players too. Inference: the character goes after
/// `PlayerRemoving`, and the Player is parented to nil rather than
/// destroyed, as `Workspace.PlayerCharacterDestroyBehavior`'s Disabled
/// does.
pub(crate) fn remove_player(lua: &Lua, player: InstanceId, reason: ExitReason) -> ApiResult<()> {
    let players = with_world(lua, |w| {
        let players = w.dom.find_service("Players");
        (players.is_some() && w.dom.parent(player) == players).then_some(players)
    });
    let Some(Some(players)) = players else {
        return Ok(());
    };
    let ud = push_instance(lua, player)?;
    let reason = crate::types::enums::named_item(lua, "PlayerExitReason", reason.item())?;
    fire(
        lua,
        &SignalKey::event(players, "PlayerRemoving"),
        &[LuaValue::UserData(ud), reason],
    );
    drain_deferred(lua);
    remove_character(lua, player)?;
    mutate(lua, |w| {
        w.client_players.remove(&player);
        w.dom
            .set_property_by_name(player, "Character", rook_types::Value::Ref(None))?;
        w.dom.set_parent(player, None)
    })
}
