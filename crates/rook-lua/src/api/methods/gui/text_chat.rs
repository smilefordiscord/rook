//! TextChatService: channels' members and the messages between server and
//! clients, written in `luau/text_chat.luau`. Messages travel as remote
//! firings of TextChatService itself (`std.service_send`), which
//! `Runtime::deliver_event` hands to that file instead of to scripts. See
//! docs/behavior/gui/chat.md.

use mlua::{Lua, Table, Value as LuaValue};

use super::RawMethod;
use crate::runtime::ctx::with_world;

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        // Everyone may chat with everyone in a local server.
        ("TextChatService" | "Chat", "CanUserChatAsync" | "CanUsersChatAsync") => super::true_,
        _ => return None,
    })
}

pub(crate) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    Some(match (owner, name) {
        ("Players", "Chat") => {
            "local c = (...).text_chat return function(self, text) return c.playersChat(text) end"
        }
        ("TextChannel", "SendAsync") => {
            "local c = (...).text_chat return function(self, text, metadata) return c.sendAsync(self, text, metadata) end"
        }
        ("TextChannel", "DisplaySystemMessage") => {
            "local c = (...).text_chat return function(self, text, metadata) return c.displaySystemMessage(self, text, metadata) end"
        }
        ("TextChannel", "AddUserAsync") => {
            "local c = (...).text_chat return function(self, userId) return c.addUserAsync(self, userId) end"
        }
        _ => return None,
    })
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let std: Table = lua.named_registry_value("rook.std")?;
    let g = lua.globals();
    for name in ["game", "Enum", "DateTime"] {
        std.set(name, g.get::<LuaValue>(name)?)?;
    }
    let api: Table = lua
        .load(include_str!("../../luau/text_chat.luau"))
        .set_name("=[rook] TextChat")
        .call(std.clone())?;
    std.set("text_chat", api.clone())?;
    // Servers give each joining player the default channels.
    let is_client = with_world(lua, |w| w.is_client);
    if !is_client && let Some(players) = with_world(lua, |w| w.dom.find_service("Players")) {
        let added = crate::api::instance::get_member(lua, players, "PlayerAdded")
            .map_err(|e| mlua::Error::RuntimeError(e.0))?
            .0;
        let connect: mlua::Function = lua
            .load("local event, f = ... event:Connect(f)")
            .set_name("=[rook] TextChat")
            .into_function()?;
        connect.call::<()>((added, api.get::<mlua::Function>("playerAdded")?))?;
    }
    Ok(())
}
