//! TeleportService, TeleportOptions and `Player:GetJoinData`
//! (docs/behavior/scripting/teleports.md). The methods check their
//! arguments and queue a [`Request`]; the play session routes it
//! (`crate::teleport`). Error words are the creator docs' names for each
//! failure (Inference: Studio's text isn't measured).

use mlua::{Function, Lua, MultiValue, Value as LuaValue};
use rook_dom::InstanceId;

use super::RawMethod;
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

fn server_only(lua: &Lua, method: &str) -> ApiResult<()> {
    if is_client(lua) {
        return Err(ScriptError(format!(
            "{method} can only be called from the server"
        )));
    }
    Ok(())
}

/// A place id argument: below 0 is `Invalid placeId` (creator-docs).
fn place_id(a: &Args, i: usize) -> ApiResult<i64> {
    let id = a.number(i)? as i64;
    if id < 0 {
        return Err(ScriptError("Invalid placeId".into()));
    }
    Ok(id)
}

/// The players of a group teleport (creator-docs TeleportAsync's errors).
fn player_list(lua: &Lua, a: &Args, i: usize) -> ApiResult<Vec<InstanceId>> {
    let incorrect = || ScriptError("List of players instances is incorrect".into());
    let LuaValue::Table(_) = a.get(i) else {
        return Err(incorrect());
    };
    let players = a.instances(i).map_err(|_| incorrect())?;
    if players.is_empty() {
        return Err(ScriptError("Players empty".into()));
    }
    if players.len() > MAX_PLAYERS {
        return Err(ScriptError(format!(
            "Cannot teleport more than {MAX_PLAYERS} players at once"
        )));
    }
    if !with_world(lua, |w| players.iter().all(|&p| w.dom.is_a(p, "Player"))) {
        return Err(incorrect());
    }
    Ok(players)
}

/// The player a legacy method moves: on a client the local player (or
/// nil for it), on a server the one given.
fn one_player(lua: &Lua, a: &Args, i: usize, method: &str) -> ApiResult<InstanceId> {
    let given = a.opt_instance(i)?;
    let (client, local, is_player) = with_world(lua, |w| {
        let local = w.dom.local_player();
        (
            w.is_client,
            local,
            given.is_some_and(|p| w.dom.is_a(p, "Player")),
        )
    });
    match (client, given) {
        (true, None) => local,
        (true, Some(p)) if Some(p) == local => Some(p),
        (false, Some(p)) if is_player => Some(p),
        _ => None,
    }
    .ok_or_else(|| ScriptError(format!("{method}: player argument must be a Player object")))
}

/// Teleport data as it crosses to the destination: copied as remote
/// arguments are, with instances dropped and an error in the output
/// (creator-docs TeleportOptions:SetTeleportData).
fn carried(lua: &Lua, v: &LuaValue) -> Option<NetValue> {
    if v.is_nil() {
        return None;
    }
    let mut data = to_net(v, 0);
    let before = data.clone();
    data.map_instances(&|_| None);
    if data != before {
        with_world(lua, |w| {
            w.push_log(
                MessageType::Error,
                "Teleport data cannot hold Instances; they were removed".into(),
            );
        });
    }
    Some(data)
}

/// The routing a TeleportOptions sets; more than one field is
/// `Incompatible Parameters` (creator-docs).
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
        Err(_) => Err(ScriptError("Incompatible Parameters".into())),
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

/// A legacy method's custom loading screen: a client's becomes its
/// teleport GUI, as `SetTeleportGui` would make it.
fn loading_screen(lua: &Lua, a: &Args, i: usize) -> ApiResult<()> {
    if let Some(gui) = a.opt_instance(i)?
        && is_client(lua)
    {
        with_world(lua, |w| w.teleports.gui = Some(gui));
    }
    Ok(())
}

/// `TeleportAsync(placeId, players, options)`: yields until the session
/// has routed the teleport, then returns a `TeleportAsyncResult` if
/// options were given (creator-docs).
fn teleport_async(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    if is_client(lua) {
        return Err(ScriptError("TeleportAsync called from Client".into()));
    }
    let place_id = place_id(a, 0)?;
    let players = player_list(lua, a, 1)?;
    let options = match a.get(2) {
        LuaValue::Nil => None,
        _ => Some(
            a.opt_instance(2)
                .ok()
                .flatten()
                .filter(|&o| with_world(lua, |w| w.dom.is_a(o, "TeleportOptions")))
                .ok_or_else(|| ScriptError("TeleportOptions not of correct type".into()))?,
        ),
    };
    let (routing, data) = match options {
        Some(o) => (
            routing(lua, o)?,
            with_world(lua, |w| w.teleports.options_data.get(&o).cloned()),
        ),
        None => (Routing::Public, None),
    };
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

/// Where a legacy method takes each argument.
struct Slots {
    player: usize,
    spawn: Option<usize>,
    data: usize,
    gui: usize,
}

/// The legacy single-player methods, which return at once.
fn legacy(
    lua: &Lua,
    a: &Args,
    method: &str,
    (place_id, routing): (i64, Routing),
    at: &Slots,
) -> ApiResult<Outcome> {
    let player = one_player(lua, a, at.player, method)?;
    let spawn_name = match at.spawn {
        Some(i) => a.opt_string(i)?.unwrap_or_default(),
        None => String::new(),
    };
    let data = carried(lua, a.get(at.data));
    loading_screen(lua, a, at.gui)?;
    queue(
        lua,
        TeleportRequest {
            players: vec![player],
            place_id,
            routing,
            data,
            spawn_name,
            call: None,
            from_server: !is_client(lua),
        },
    );
    Outcome::ret(lua, ())
}

/// `Teleport(placeId, player, teleportData, customLoadingScreen)`.
fn teleport(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let place = place_id(a, 0)?;
    let at = Slots {
        player: 1,
        spawn: None,
        data: 2,
        gui: 3,
    };
    legacy(lua, a, "Teleport", (place, Routing::Public), &at)
}

/// `TeleportToSpawnByName(placeId, spawnName, player, teleportData,
/// customLoadingScreen)`.
fn teleport_to_spawn(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let place = place_id(a, 0)?;
    a.string(1)?;
    let at = Slots {
        player: 2,
        spawn: Some(1),
        data: 3,
        gui: 4,
    };
    legacy(
        lua,
        a,
        "TeleportToSpawnByName",
        (place, Routing::Public),
        &at,
    )
}

/// `TeleportToPlaceInstance(placeId, instanceId, player, spawnName,
/// teleportData, customLoadingScreen)`.
fn teleport_to_instance(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let place = place_id(a, 0)?;
    let job = a.string(1)?;
    let at = Slots {
        player: 2,
        spawn: Some(3),
        data: 4,
        gui: 5,
    };
    let method = "TeleportToPlaceInstance";
    legacy(lua, a, method, (place, Routing::Instance(job)), &at)
}

/// `TeleportToPrivateServer(placeId, reservedServerAccessCode, players,
/// spawnName, teleportData, customLoadingScreen)`.
fn teleport_to_private_server(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    server_only(lua, "TeleportToPrivateServer")?;
    let place_id = place_id(a, 0)?;
    let code = a.string(1)?;
    let players = player_list(lua, a, 2)?;
    let spawn_name = a.opt_string(3)?.unwrap_or_default();
    let data = carried(lua, a.get(4));
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
    server_only(lua, "TeleportPartyAsync")?;
    let place_id = place_id(a, 0)?;
    let players = player_list(lua, a, 1)?;
    let data = carried(lua, a.get(2));
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
/// used.
fn reserve_server(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    server_only(lua, "ReserveServer")?;
    let place_id = place_id(a, 0)?;
    let id = super::web::http::random_guid(lua)?.to_lowercase();
    let code = crate::teleport::access_code(place_id, &id);
    Outcome::ret(lua, (code, id))
}

/// `GetPlayerPlaceInstanceAsync(userId)`: where the user plays, which the
/// session answers; an error if they're nowhere (creator-docs).
fn player_place_instance(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    server_only(lua, "GetPlayerPlaceInstanceAsync")?;
    let user = a.number(0)? as i64;
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

fn set_teleport_gui(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let gui = a.object(0)?;
    with_world(lua, |w| w.teleports.gui = gui);
    Outcome::ret(lua, ())
}

fn arriving_teleport_gui(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    super::ret_instance(lua, with_world(lua, |w| w.teleports.arrival.gui))
}

/// The local player's teleport data; nil on a server, which has no local
/// player (Inference).
fn local_player_teleport_data(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let data = with_world(lua, |w| w.teleports.arrival.teleport_data.clone());
    match data {
        Some(d) => Outcome::ret(lua, from_net(lua, &d)?),
        None => Outcome::ret(lua, LuaValue::Nil),
    }
}

fn set_teleport_setting(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let key = a.string(0)?;
    let value = carried(lua, a.get(1));
    with_world(lua, |w| match value {
        Some(v) => w.teleports.arrival.settings.insert(key, v),
        None => w.teleports.arrival.settings.remove(&key),
    });
    Outcome::ret(lua, ())
}

fn teleport_setting(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let key = a.string(0)?;
    let value = with_world(lua, |w| w.teleports.arrival.settings.get(&key).cloned());
    Outcome::ret(lua, from_net(lua, &value.unwrap_or(NetValue::Nil))?)
}

fn set_teleport_data(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let data = carried(lua, a.get(0));
    with_world(lua, |w| match data {
        Some(d) => w.teleports.options_data.insert(this, d),
        None => w.teleports.options_data.remove(&this),
    });
    Outcome::ret(lua, ())
}

fn teleport_data(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let data = with_world(lua, |w| w.teleports.options_data.get(&this).cloned());
    Outcome::ret(lua, from_net(lua, &data.unwrap_or(NetValue::Nil))?)
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
