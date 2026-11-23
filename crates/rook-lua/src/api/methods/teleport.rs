//! TeleportService, TeleportOptions and `Player:GetJoinData`
//! (docs/behavior/scripting/teleports.md). The methods cast their
//! arguments as Roblox's reflection does, last first (measured on
//! `TeleportAsync`), check them, and queue a [`Request`]; the play session
//! routes it (`crate::teleport`). Messages measured in Studio 0.748 are
//! marked so; the others are the creator docs' names for each failure
//! (Inference).

use mlua::{Function, Lua, MultiValue, Value as LuaValue};
use rook_dom::InstanceId;

use super::RawMethod;
use super::instances::bindables::copy_values;
use crate::NetValue;
use crate::net::{from_net, to_net};
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, ScriptError, api_fn, with_world};
use crate::runtime::world::MessageType;
use crate::teleport::{MAX_PLAYERS, Request, Returns, Routing, TeleportRequest};

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("TeleportService", "TeleportAsync") => teleport_async,
        ("TeleportService", "Teleport") => teleport,
        ("TeleportService", "TeleportToSpawnByName") => teleport_to_spawn,
        ("TeleportService", "TeleportToPlaceInstance") => teleport_to_instance,
        ("TeleportService", "TeleportToPrivateServer") => teleport_to_private_server,
        ("TeleportService", "TeleportPartyAsync") => teleport_party,
        ("TeleportService", "ReserveServer" | "ReserveServerAsync") => reserve_server,
        ("TeleportService", "GetPlayerPlaceInstanceAsync") => player_place_instance,
        ("TeleportService", "SetTeleportGui") => set_teleport_gui,
        ("TeleportService", "GetArrivingTeleportGui") => arriving_teleport_gui,
        ("TeleportService", "GetLocalPlayerTeleportData") => local_player_teleport_data,
        ("TeleportService", "SetTeleportSetting") => set_teleport_setting,
        ("TeleportService", "GetTeleportSetting") => teleport_setting,
        ("TeleportOptions", "SetTeleportData") => set_teleport_data,
        ("TeleportOptions", "GetTeleportData") => teleport_data,
        ("Player", "GetJoinData") => join_data,
        _ => return None,
    })
}

fn is_client(lua: &Lua) -> bool {
    with_world(lua, |w| w.is_client)
}

/// A server-only method called on a client, after its arguments are cast
/// (measured: a client's `ReserveServerAsync("abc")` is a cast error).
fn server_only(lua: &Lua, message: &str) -> ApiResult<()> {
    if is_client(lua) {
        return Err(ScriptError(message.into()));
    }
    Ok(())
}

/// Confirmed (Studio 0.748): a place id of 0 or below, which no place has,
/// is `Invalid placeId.` on a server.
fn place_id(id: i64) -> ApiResult<i64> {
    if id <= 0 {
        return Err(ScriptError("Invalid placeId.".into()));
    }
    Ok(id)
}

/// The players of a group teleport, already cast (creator-docs
/// TeleportAsync's errors; Inference: their words and order).
fn group(lua: &Lua, players: Vec<InstanceId>) -> ApiResult<Vec<InstanceId>> {
    if players.is_empty() {
        return Err(ScriptError("Players empty".into()));
    }
    if players.len() > MAX_PLAYERS {
        return Err(ScriptError(format!(
            "Cannot teleport more than {MAX_PLAYERS} players at once"
        )));
    }
    if !with_world(lua, |w| players.iter().all(|&p| w.dom.is_a(p, "Player"))) {
        return Err(ScriptError("List of players instances is incorrect".into()));
    }
    Ok(players)
}

/// A copy of teleport data as `SetTeleportData` keeps it: copied as a
/// bindable's arguments are (measured in Studio 0.748: functions and
/// instances kept, a mixed table's array part only, a boolean key and a
/// cycle raise).
fn copied(lua: &Lua, v: &LuaValue) -> ApiResult<LuaValue> {
    Ok(copy_values(lua, std::slice::from_ref(v), false)?
        .pop()
        .unwrap_or(LuaValue::Nil))
}

/// Teleport data as it crosses to the destination: sent as remote
/// arguments are, with instances removed and an error in the output
/// (creator-docs TeleportOptions:SetTeleportData).
fn carried(lua: &Lua, data: &LuaValue) -> NetValue {
    let mut sent = to_net(data, 0);
    let before = sent.clone();
    sent.map_instances(&|_| None);
    if sent != before {
        with_world(lua, |w| {
            w.push_log(
                MessageType::Error,
                "Teleport data cannot hold Instances; they were removed".into(),
            );
        });
    }
    sent
}

/// A legacy method's `teleportData`, which may be nil.
fn data_argument(lua: &Lua, v: &LuaValue) -> ApiResult<Option<NetValue>> {
    if v.is_nil() {
        return Ok(None);
    }
    Ok(Some(carried(lua, &copied(lua, v)?)))
}

/// The routing a TeleportOptions sets. Confirmed (Studio 0.748): more than
/// one field raises, before the place id is checked.
fn routing(lua: &Lua, options: InstanceId) -> ApiResult<Routing> {
    let set = with_world(lua, |w| {
        let text = |name| {
            w.dom
                .string_property(options, name)
                .filter(|s| !s.is_empty())
        };
        let mut set = Vec::new();
        if let Some(id) = text("ReservedServerId") {
            set.push(Routing::Reserved(id));
        }
        if let Some(id) = text("VipServerId") {
            set.push(Routing::Vip(id));
        }
        if let Some(code) = text("ReservedServerAccessCode") {
            set.push(Routing::AccessCode(code));
        }
        if w.dom.bool_property(options, "ShouldReserveServer") == Some(true) {
            set.push(Routing::NewReserved);
        }
        if let Some(job) = text("ServerInstanceId") {
            set.push(Routing::Instance(job));
        }
        set
    });
    match <[Routing; 1]>::try_from(set) {
        Ok([one]) => Ok(one),
        Err(set) if set.is_empty() => Ok(Routing::Public),
        Err(_) => Err(ScriptError(
            "Can only specify one of ReservedServerId, VipServerId, ReservedServerAccessCode, \
             ShouldReserveServer, or ServerInstanceId."
                .into(),
        )),
    }
}

fn queue(lua: &Lua, request: TeleportRequest) {
    with_world(lua, |w| {
        w.teleports.requests.push(Request::Teleport(request))
    });
}

/// Queue a request whose call waits for where it went; the method yields.
fn queue_and_wait(lua: &Lua, mut request: TeleportRequest, returns: Returns) -> Outcome {
    let thread = lua.current_thread();
    request.call = Some(with_world(lua, |w| w.teleports.wait(thread, returns)));
    queue(lua, request);
    Outcome::Yield
}

/// `TeleportAsync(placeId, players, options)`: yields until the session
/// has routed the teleport, then returns a `TeleportAsyncResult` if
/// options were given (creator-docs). Confirmed (Studio 0.748): the casts,
/// then the client's refusal, then the options' routing, then the place id.
fn teleport_async(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let options = a.opt_instance(2)?;
    let players = a.objects(1)?;
    let place = a.int64_rounded(0, "int64")?;
    server_only(lua, "TeleportUnknown must be called from a Server")?;
    let options = options.map(|o| {
        let is_options = with_world(lua, |w| w.dom.is_a(o, "TeleportOptions"));
        (o, is_options)
    });
    let routing = match options {
        Some((o, true)) => routing(lua, o)?,
        _ => Routing::Public,
    };
    let place_id = place_id(place)?;
    let options = match options {
        None => None,
        Some((o, true)) => Some(o),
        Some((_, false)) => {
            return Err(ScriptError("TeleportOptions not of correct type".into()));
        }
    };
    let players = group(lua, players)?;
    let data = options
        .and_then(|o| with_world(lua, |w| w.teleports.options_data.get(&o).cloned()))
        .map(|d| carried(lua, &d));
    let request = TeleportRequest {
        players,
        place_id,
        routing,
        data,
        spawn_name: String::new(),
        call: None,
        from_server: true,
    };
    let returns = if options.is_some() {
        Returns::Result
    } else {
        Returns::Nothing
    };
    Ok(queue_and_wait(lua, request, returns))
}

/// A legacy single-player teleport, its arguments cast.
struct Legacy {
    method: &'static str,
    place: i64,
    player: Option<InstanceId>,
    routing: Routing,
    spawn_name: String,
    data: Option<NetValue>,
    /// `customLoadingScreen`.
    gui: Option<InstanceId>,
}

/// The legacy single-player methods, which return at once. On a client
/// they move the local player, the only one it may name; on a server the
/// one given (Inference: the words for anyone else).
fn legacy(lua: &Lua, t: Legacy) -> ApiResult<Outcome> {
    let client = is_client(lua);
    let player = if client {
        // Confirmed (Studio 0.748, `Teleport`): a client checks the place
        // first, in words of its own.
        if t.place <= 0 {
            return Err(ScriptError(
                "Cannot teleport to invalid place id. Aborting teleport.".into(),
            ));
        }
        let local = with_world(lua, |w| w.dom.local_player());
        match t.player {
            None => local,
            Some(p) => local.filter(|&l| l == p),
        }
    } else {
        // Confirmed (Studio 0.748, `Teleport(0)`): a server's call without
        // a player returns, before the place is checked. Inference: it
        // moves no one.
        let Some(given) = t.player else {
            return Outcome::ret(lua, ());
        };
        place_id(t.place)?;
        with_world(lua, |w| w.dom.is_a(given, "Player")).then_some(given)
    };
    let player = player.ok_or_else(|| {
        ScriptError(format!(
            "{}: player argument must be a Player object",
            t.method
        ))
    })?;
    // A client's loading screen becomes its teleport GUI, as
    // `SetTeleportGui` would make it.
    if client && let Some(gui) = t.gui {
        with_world(lua, |w| w.teleports.gui = Some(gui));
    }
    queue(
        lua,
        TeleportRequest {
            players: vec![player],
            place_id: t.place,
            routing: t.routing,
            data: t.data,
            spawn_name: t.spawn_name,
            call: None,
            from_server: !client,
        },
    );
    Outcome::ret(lua, ())
}

/// `Teleport(placeId, player, teleportData, customLoadingScreen)`.
fn teleport(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let gui = a.opt_instance(3)?;
    let data = data_argument(lua, a.get(2))?;
    let player = a.opt_instance(1)?;
    let place = a.int64_rounded(0, "int64")?;
    legacy(
        lua,
        Legacy {
            method: "Teleport",
            place,
            player,
            routing: Routing::Public,
            spawn_name: String::new(),
            data,
            gui,
        },
    )
}

/// `TeleportToSpawnByName(placeId, spawnName, player, teleportData,
/// customLoadingScreen)`.
fn teleport_to_spawn(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let gui = a.opt_instance(4)?;
    let data = data_argument(lua, a.get(3))?;
    let player = a.opt_instance(2)?;
    let spawn_name = a.string(1)?;
    let place = a.int64_rounded(0, "int64")?;
    legacy(
        lua,
        Legacy {
            method: "TeleportToSpawnByName",
            place,
            player,
            routing: Routing::Public,
            spawn_name,
            data,
            gui,
        },
    )
}

/// `TeleportToPlaceInstance(placeId, instanceId, player, spawnName,
/// teleportData, customLoadingScreen)`.
fn teleport_to_instance(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let gui = a.opt_instance(5)?;
    let data = data_argument(lua, a.get(4))?;
    let spawn_name = a.opt_string(3)?.unwrap_or_default();
    let player = a.opt_instance(2)?;
    let job = a.string(1)?;
    let place = a.int64_rounded(0, "int64")?;
    legacy(
        lua,
        Legacy {
            method: "TeleportToPlaceInstance",
            place,
            player,
            routing: Routing::Instance(job),
            spawn_name,
            data,
            gui,
        },
    )
}

/// `TeleportToPrivateServer(placeId, reservedServerAccessCode, players,
/// spawnName, teleportData, customLoadingScreen)`.
fn teleport_to_private_server(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    a.opt_instance(5)?;
    let data = data_argument(lua, a.get(4))?;
    let spawn_name = a.opt_string(3)?.unwrap_or_default();
    let players = a.objects(2)?;
    let code = a.string(1)?;
    let place = a.int64_rounded(0, "int64")?;
    server_only(
        lua,
        "TeleportToPrivateServer can only be called from the server",
    )?;
    let place_id = place_id(place)?;
    let players = group(lua, players)?;
    queue(
        lua,
        TeleportRequest {
            players,
            place_id,
            routing: Routing::AccessCode(code),
            data,
            spawn_name,
            call: None,
            from_server: true,
        },
    );
    Outcome::ret(lua, ())
}

/// `TeleportPartyAsync(placeId, players, teleportData,
/// customLoadingScreen)`: the group to one server, returning its `JobId`.
fn teleport_party(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    a.opt_instance(3)?;
    let data = data_argument(lua, a.get(2))?;
    let players = a.objects(1)?;
    let place = a.int64_rounded(0, "int64")?;
    server_only(lua, "TeleportPartyAsync must be called from a Server")?;
    let place_id = place_id(place)?;
    let players = group(lua, players)?;
    let request = TeleportRequest {
        players,
        place_id,
        routing: Routing::Public,
        data,
        spawn_name: String::new(),
        call: None,
        from_server: true,
    };
    Ok(queue_and_wait(lua, request, Returns::JobId))
}

/// `ReserveServer(placeId)`: an access code and the reserved server's
/// `PrivateServerId`. Nothing starts until a teleport uses the code.
/// Inference: a place outside the universe is only found when the code is
/// used, and the place id is checked as a teleport's is.
fn reserve_server(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let place = a.int64_rounded(0, "int64")?;
    server_only(lua, "ReserveServer can only be called from the server")?;
    let place_id = place_id(place)?;
    let id = super::web::http::random_guid(lua)?.to_lowercase();
    let code = crate::teleport::access_code(place_id, &id);
    Outcome::ret(lua, (code, id))
}

/// `GetPlayerPlaceInstanceAsync(userId)`: where the user plays, which the
/// session answers; an error if they're nowhere (creator-docs).
fn player_place_instance(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let user = a.int64_rounded(0, "User")?;
    server_only(
        lua,
        "GetPlayerPlaceInstanceAsync can only be called from the server",
    )?;
    let helper: Function = lua.named_registry_value("rook.place_instance")?;
    Ok(Outcome::Call(
        helper,
        MultiValue::from_vec(vec![LuaValue::Integer(user)]),
    ))
}

/// The helper's wait: queue the lookup and yield until it's answered.
fn wait_lookup(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "GetPlayerPlaceInstanceAsync");
    let user_id = a.number(0)? as i64;
    let thread = lua.current_thread();
    with_world(lua, |w| {
        let call = w.teleports.wait(thread, Returns::Lookup);
        w.teleports.requests.push(Request::Lookup { call, user_id });
    });
    Ok(Outcome::Yield)
}

/// Confirmed (Studio 0.748): any Instance or nil, on a server or a client,
/// whatever its class; only a value that isn't one doesn't cast. The
/// creator docs say it takes "the loading ScreenGui"; nil clears it.
fn set_teleport_gui(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let gui = a.object(0)?;
    with_world(lua, |w| w.teleports.gui = gui);
    Outcome::ret(lua, ())
}

fn arriving_teleport_gui(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    super::ret_instance(lua, with_world(lua, |w| w.teleports.arrival.gui))
}

/// The local player's teleport data; nil on a server, which has no local
/// player (Confirmed, Studio 0.748).
fn local_player_teleport_data(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let data = with_world(lua, |w| w.teleports.arrival.teleport_data.clone());
    match data {
        Some(d) => Outcome::ret(lua, from_net(lua, &d)?),
        None => Outcome::ret(lua, LuaValue::Nil),
    }
}

/// Confirmed (Studio 0.748): the value can't be nil, and works on a server
/// too.
fn set_teleport_setting(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let value = copied(lua, a.variant(1)?)?;
    let key = a.string(0)?;
    let value = carried(lua, &value);
    with_world(lua, |w| w.teleports.arrival.settings.insert(key, value));
    Outcome::ret(lua, ())
}

fn teleport_setting(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let key = a.string(0)?;
    let value = with_world(lua, |w| w.teleports.arrival.settings.get(&key).cloned());
    Outcome::ret(lua, from_net(lua, &value.unwrap_or(NetValue::Nil))?)
}

/// Confirmed (Studio 0.748): the data can't be nil, and is copied when set
/// and again each time it's read; a value that isn't a table is kept as
/// it is.
fn set_teleport_data(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let data = copied(lua, a.variant(0)?)?;
    with_world(lua, |w| w.teleports.options_data.insert(this, data));
    Outcome::ret(lua, ())
}

fn teleport_data(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let data = with_world(lua, |w| w.teleports.options_data.get(&this).cloned());
    Outcome::ret(lua, copied(lua, &data.unwrap_or(LuaValue::Nil))?)
}

/// `Player:GetJoinData()`: the fields its join set (creator-docs). A
/// client's is empty: only servers hold join data.
fn join_data(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let data = with_world(lua, |w| w.teleports.join_data.get(&this).cloned());
    let t = lua.create_table()?;
    if let Some(data) = data {
        if let Some(id) = data.source_place_id {
            t.set("SourcePlaceId", id)?;
        }
        if let Some(id) = data.source_game_id {
            t.set("SourceGameId", id)?;
        }
        if let Some(d) = &data.teleport_data {
            t.set("TeleportData", from_net(lua, d)?)?;
        }
        if !data.members.is_empty() {
            t.set("Members", lua.create_sequence_from(data.members)?)?;
        }
        if let Some(launch) = data.launch_data {
            t.set("LaunchData", launch)?;
        }
    }
    Outcome::ret(lua, t)
}

/// The Luau half of `GetPlayerPlaceInstanceAsync`, which raises the
/// lookup's error after the wait.
pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let wait = api_fn(lua, wait_lookup)?;
    let helper: Function = lua
        .load(
            r#"
            local wait, error = ...
            return function(user)
                local ok, current, message, place, job = wait(user)
                if not ok then
                    error(current, 0)
                end
                return current, message, place, job
            end
            "#,
        )
        .set_name("=[rook] teleports")
        .call((wait, lua.globals().get::<Function>("error")?))?;
    lua.set_named_registry_value("rook.place_instance", helper)?;
    Ok(())
}
