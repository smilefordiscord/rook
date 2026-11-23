//! TeleportService's arguments and what a script queues for its session
//! (docs/behavior/scripting/teleports.md).

use crate::teleport::{Request, Routing, access_code, parse_access_code};
use crate::tests::*;

/// A server with one Player, "Builderman", running `source`.
fn with_player(source: &str) -> (Runtime, Vec<String>) {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    {
        let mut w = rt.world_mut();
        let players = w.dom.get_service("Players").unwrap();
        let player = w.dom.create_by_name("Player").unwrap();
        w.dom.set_name(player, "Builderman".into()).unwrap();
        w.dom.set_parent(player, Some(players)).unwrap();
    }
    rt.run_source("ServerScriptService.Script", source, None)
        .unwrap();
    let out = finish(&rt, 1);
    (rt, out)
}

#[test]
fn teleport_async_checks_its_arguments_before_it_waits() {
    let (_, out) = with_player(
        r#"
        local TeleportService = game:GetService("TeleportService")
        local p = game.Players.Builderman
        local function try(...)
            print((select(2, pcall(TeleportService.TeleportAsync, TeleportService, ...))))
        end
        try(-1, { p })
        try(1, {})
        try(1, { workspace })
        try(1, "Builderman")
        try(1, { p }, workspace)
        local both = Instance.new("TeleportOptions")
        both.ServerInstanceId = "job"
        both.ReservedServerId = "id"
        try(1, { p }, both)
        local many = table.create(51, p)
        try(1, many)
        "#,
    );
    assert_eq!(
        out,
        [
            "Invalid placeId",
            "Players empty",
            "List of players instances is incorrect",
            "List of players instances is incorrect",
            "TeleportOptions not of correct type",
            "Incompatible Parameters",
            "Cannot teleport more than 50 players at once",
        ]
    );
}

#[test]
fn teleport_async_queues_its_routing_and_data_and_waits() {
    let (rt, out) = with_player(
        r#"
        local TeleportService = game:GetService("TeleportService")
        local options = Instance.new("TeleportOptions")
        options.ReservedServerId = "abc"
        options:SetTeleportData({ level = 3 })
        print(options:GetTeleportData().level)
        task.spawn(function()
            TeleportService:TeleportAsync(1002, { game.Players.Builderman }, options)
            print("returned")
        end)
        print("waiting")
        "#,
    );
    assert_eq!(out, ["3", "waiting"]);
    let requests = rt.take_teleport_requests();
    let [Request::Teleport(r)] = requests.as_slice() else {
        panic!("one teleport: {requests:?}");
    };
    assert_eq!(
        (r.place_id, &r.routing, r.from_server, r.players.len()),
        (1002, &Routing::Reserved("abc".into()), true, 1)
    );
    assert!(r.data.is_some() && r.call.is_some());
    rt.finish_teleport(r.call.unwrap(), None);
    assert_eq!(finish(&rt, 0), ["returned"]);
}

#[test]
fn server_only_methods_refuse_a_client() {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    rt.world_mut().is_client = true;
    rt.run_source(
        "StarterPlayerScripts.LocalScript",
        r#"
        local TeleportService = game:GetService("TeleportService")
        print(pcall(TeleportService.TeleportAsync, TeleportService, 1, {}))
        print(pcall(TeleportService.ReserveServer, TeleportService, 1))
        print(pcall(TeleportService.TeleportPartyAsync, TeleportService, 1, {}))
        print(pcall(TeleportService.GetPlayerPlaceInstanceAsync, TeleportService, 1))
        print(TeleportService:GetLocalPlayerTeleportData(), TeleportService:GetArrivingTeleportGui())
        "#,
        None,
    )
    .unwrap();
    assert_eq!(
        finish(&rt, 0),
        [
            "false TeleportAsync called from Client",
            "false ReserveServer can only be called from the server",
            "false TeleportPartyAsync can only be called from the server",
            "false GetPlayerPlaceInstanceAsync can only be called from the server",
            "nil nil",
        ]
    );
}

#[test]
fn reserved_access_codes_name_their_place() {
    let (_, out) = with_player(
        r#"
        local code, id = game:GetService("TeleportService"):ReserveServer(1002)
        print(typeof(code), #id, code:sub(1, 5))
        "#,
    );
    let [line] = out.as_slice() else {
        panic!("{out:?}");
    };
    assert_eq!(line, "string 36 1002-");
    assert_eq!(parse_access_code(&access_code(5, "x")), Some((5, "x")));
}

#[test]
fn legacy_teleports_take_their_player_and_spawn() {
    let (rt, out) = with_player(
        r#"
        local TeleportService = game:GetService("TeleportService")
        local p = game.Players.Builderman
        print(pcall(TeleportService.Teleport, TeleportService, 1002))
        TeleportService:TeleportToSpawnByName(1002, "Gate", p, { a = 1 })
        TeleportService:TeleportToPlaceInstance(1002, "job", p)
        print("done")
        "#,
    );
    assert_eq!(
        out,
        [
            "false Teleport: player argument must be a Player object",
            "done"
        ]
    );
    let requests = rt.take_teleport_requests();
    let routes: Vec<_> = (requests.iter())
        .map(|r| match r {
            Request::Teleport(t) => (t.routing.clone(), t.spawn_name.clone(), t.call),
            Request::Lookup { .. } => panic!("a teleport"),
        })
        .collect();
    assert_eq!(
        routes,
        [
            (Routing::Public, "Gate".to_owned(), None),
            (Routing::Instance("job".into()), String::new(), None),
        ]
    );
}
