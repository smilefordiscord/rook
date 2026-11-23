//! MessagingService within one session: what a server publishes reaches
//! the same server's subscribers (`docs/behavior/scripting/messaging.md`).
//!
//! Roblox carries messages between a game's servers through its own; a
//! Rook session is one server, so a topic's subscribers are connections
//! on the service's signal for that topic ([`SignalKind::Message`]), and a
//! message is fired on it in the frame after it was published, as
//! Roblox's arrive after a round trip (Inference).

use mlua::{Function, Lua, MultiValue, Value as LuaValue};
use rook_dom::InstanceId;

use super::RawMethod;
use super::http::{from_json, to_json};
use crate::api::signal::LConnection;
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, api_fn, with_world};
use crate::runtime::scheduler::Queue;
use crate::runtime::signal::{SignalKey, SignalKind};
use crate::runtime::world::{Handler, Job};

/// A topic's length in characters (creator-docs MessagingService: 1 to
/// 80), and a message's most bytes as JSON (1 kB, taken as 1,024:
/// Inference).
const MAX_TOPIC_LEN: usize = 80;
const MAX_MESSAGE_SIZE: usize = 1024;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("MessagingService", "PublishAsync") => publish,
        ("MessagingService", "SubscribeAsync") => subscribe,
        _ => return None,
    })
}

/// The function that delivers a published message, run as a job.
pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    lua.set_named_registry_value("rook.messaging.deliver", api_fn(lua, deliver)?)
}

/// The topic argument, on a server. The errors' wording is Inference.
fn topic(lua: &Lua, a: &Args, member: &str) -> ApiResult<String> {
    if with_world(lua, |w| w.is_client) {
        return Err(ScriptError(format!(
            "MessagingService:{member}() can only be called from the server"
        )));
    }
    let topic = a.string(0)?;
    let length = topic.chars().count();
    if length == 0 || length > MAX_TOPIC_LEN {
        return Err(ScriptError(format!(
            "MessagingService:{member}() topic must be 1 to 80 characters long"
        )));
    }
    Ok(topic)
}

/// `PublishAsync(topic, message)`: the message is sent as JSON, so
/// subscribers get a copy (Inference), and at most 1 kB of it
/// (creator-docs).
fn publish(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let topic = topic(lua, a, "PublishAsync")?;
    let data = to_json(a.get(1), 0, true).map_err(|_| {
        ScriptError("MessagingService:PublishAsync() message can't be converted to JSON".into())
    })?;
    let payload = data.to_string();
    if payload.len() > MAX_MESSAGE_SIZE {
        return Err(ScriptError(
            "MessagingService:PublishAsync() message is larger than 1kB".into(),
        ));
    }
    // Whole seconds since 1970 (Inference: creator-docs say "Unix time in
    // seconds"). A clock before 1970 has no Unix time to give.
    let sent = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let deliver: Function = lua.named_registry_value("rook.messaging.deliver")?;
    let args = vec![
        LuaValue::String(lua.create_string(&topic)?),
        LuaValue::String(lua.create_string(&payload)?),
        LuaValue::Number(sent as f64),
    ];
    with_world(lua, |w| {
        let job = Job::Call {
            func: deliver,
            args,
            legacy_since: None,
        };
        w.scheduler.schedule_next_frame(Queue::Task, job);
    });
    Outcome::ret(lua, ())
}

/// `SubscribeAsync(topic, callback)`: a connection whose `Disconnect`
/// unsubscribes; a stopped script's are dropped (creator-docs).
fn subscribe(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let topic = topic(lua, a, "SubscribeAsync")?;
    let callback = a.function(1)?;
    let key = SignalKey {
        instance: this,
        kind: SignalKind::Message(topic),
    };
    let id = with_world(lua, |w| {
        w.signals.connect(key, Handler::Call(callback), false)
    });
    Outcome::ret(lua, LConnection(id))
}

/// Fire `(topic, payload, sent)` to the topic's subscribers, each with a
/// `{Data, Sent}` table of its own (creator-docs).
fn deliver(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "deliver");
    let (topic, payload, sent) = (a.string(0)?, a.string(1)?, a.number(2)?);
    let Some(service) = with_world(lua, |w| w.dom.find_service("MessagingService")) else {
        return Outcome::ret(lua, ());
    };
    let data: serde_json::Value = serde_json::from_str(&payload)
        .map_err(|e| ScriptError(format!("a published message isn't JSON: {e}")))?;
    let key = SignalKey {
        instance: service,
        kind: SignalKind::Message(topic),
    };
    let message = || -> mlua::Result<LuaValue> {
        let t = lua.create_table_with_capacity(0, 2)?;
        t.raw_set("Data", from_json(lua, &data)?)?;
        t.raw_set("Sent", sent)?;
        Ok(LuaValue::Table(t))
    };
    // Making a table fails only when Luau is out of memory, which has
    // raised already wherever it happened.
    crate::runtime::threads::fire_with(lua, &key, || vec![message().unwrap_or(LuaValue::Nil)]);
    Outcome::ret(lua, ())
}
