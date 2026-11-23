//! Teleports between the places of a universe
//! (docs/behavior/scripting/teleports.md), with two small places held in
//! memory: a lobby (1001) and an arena (1002) of game 77.

use std::sync::Arc;

use crate::session::servers::Servers;
use crate::tests::*;
use crate::universe::{PlaceSource, Universe};

const LOBBY: i64 = 1001;
const ARENA: i64 = 1002;

/// A place built by `build`, saved as a file would be. Characters don't
/// load, so only scripts run.
fn place(build: impl FnOnce(&Server)) -> PlaceSource {
    let server = Server::new(&Options::default()).unwrap();
    let players = service(&server, "Players");
    server
        .runtime
        .world_mut()
        .dom
        .set_property_by_name(players, "CharacterAutoLoads", Value::Bool(false))
        .unwrap();
    build(&server);
    let w = server.runtime.world();
    let roots = w.dom.children(w.dom.root()).to_vec();
    let weak = rook_formats::from_dom(&w.dom, &roots);
    let bytes = rook_formats::save(&weak, rook_formats::Format::Binary).unwrap();
    PlaceSource::Bytes(bytes.into())
}

/// The arena: its server reports each arrival's join data and its own
/// identity; `extra` adds to it.
fn arena(extra: impl FnOnce(&Server)) -> PlaceSource {
    place(|s| {
        add_script(
            s,
            "ServerScriptService",
            "Arrivals",
            r#"
            game.Players.PlayerAdded:Connect(function(p)
                local j = p:GetJoinData()
                local data = j.TeleportData
                print("arrived", p.Name, j.SourcePlaceId, j.SourceGameId,
                    data and data.coins, j.Members and #j.Members)
                print("arena", game.PlaceId, game.GameId, #game.JobId > 0,
                    game.PrivateServerId, game.PrivateServerOwnerId)
            end)
            "#,
        );
        extra(s);
    })
}

/// A universe of the lobby `lobby` builds and the arena.
fn universe(lobby: impl FnOnce(&Server)) -> Servers {
    universe_with(lobby, |_| {})
}

fn universe_with(lobby: impl FnOnce(&Server), arena_extra: impl FnOnce(&Server)) -> Servers {
    let mut u = Universe::new(77);
    u.add_place(LOBBY, place(lobby));
    u.add_place(ARENA, arena(arena_extra));
    Servers::new(Arc::new(u), Options::default())
}

fn server_lines(servers: &Servers, server: usize) -> Vec<String> {
    lines(servers.session(server).unwrap().server())
}

fn client_lines(servers: &Servers, user: i64) -> Vec<String> {
    let (server, client) = servers.client_of(user).unwrap();
    lines(servers.session(server).unwrap().client(client).unwrap())
}

#[test]
fn teleport_async_moves_a_player_to_another_place_with_its_data() {
    let lobby = |s: &Server| {
        add_script(
            s,
            "ServerScriptService",
            "Go",
            r#"
            local TeleportService = game:GetService("TeleportService")
            game.Players.PlayerRemoving:Connect(function(p, reason)
                print("left", p.Name, reason)
            end)
            game.Players.PlayerAdded:Connect(function(p)
                print("lobby", game.PlaceId, game.GameId)
                task.wait(0.5)
                local options = Instance.new("TeleportOptions")
                options:SetTeleportData({ coins = 5, part = workspace })
                local result = TeleportService:TeleportAsync(1002, { p }, options)
                print("result", result.ClassName, result.PrivateServerId == "", p.Parent)
            end)
            "#,
        );
        add_local_script(
            s,
            "Screen",
            r#"
            local TeleportService = game:GetService("TeleportService")
            local gui = Instance.new("ScreenGui")
            gui.Name = "Loading"
            Instance.new("Frame", gui).Name = "Bar"
            Instance.new("ObjectValue", gui).Value = workspace
            gui.Parent = game.Players.LocalPlayer:WaitForChild("PlayerGui")
            TeleportService:SetTeleportGui(gui)
            TeleportService:SetTeleportSetting("crouch", true)
            "#,
        );
    };
    // The arena's client hears it arrived.
    let arrive = |s: &Server| {
        add_under(
            s,
            service(s, "ReplicatedFirst"),
            "LocalScript",
            "Arrive",
            r#"
            local TeleportService = game:GetService("TeleportService")
            TeleportService.LocalPlayerArrivedFromTeleport:Connect(function(gui, data)
                print("arrived with", gui.Name, gui.Bar.ClassName, gui.Value.Value, data.coins)
                gui.Parent = game.Players.LocalPlayer.PlayerGui
            end)
            print("data", TeleportService:GetLocalPlayerTeleportData().coins,
                TeleportService:GetArrivingTeleportGui().Parent,
                TeleportService:GetTeleportSetting("crouch"))
            "#,
        );
    };
    let mut servers = universe_with(lobby, arrive);
    let lobby = servers.start(LOBBY).unwrap();
    servers.join(lobby, "Builderman", 156).unwrap();
    servers.run_for(1.0);

    assert_eq!(servers.sessions().len(), 2);
    assert_eq!(servers.client_of(156), Some((1, 0)));
    assert_eq!(
        server_lines(&servers, lobby),
        [
            "lobby 1001 77",
            "Teleport data cannot hold Instances; they were removed",
            "result TeleportAsyncResult true Players",
            "left Builderman Enum.PlayerExitReason.Unknown",
        ]
    );
    assert_eq!(
        server_lines(&servers, 1),
        ["arrived Builderman 1001 77 5 nil", "arena 1002 77 true  0"]
    );
    // The GUI's reference outside itself was cleared on the way.
    assert_eq!(
        client_lines(&servers, 156),
        ["data 5 CoreGui true", "arrived with Loading Frame nil 5"]
    );
    // The handler kept the GUI by moving it; otherwise it would have left
    // CoreGui once the place loaded.
    let (s, c) = servers.client_of(156).unwrap();
    let dm = servers.session(s).unwrap().client(c).unwrap();
    let w = dm.runtime.world();
    let local = w.dom.local_player().unwrap();
    let gui = w.dom.find_first_child(local, "PlayerGui", false).unwrap();
    assert!(w.dom.find_first_child(gui, "Loading", false).is_some());
    assert_eq!(
        w.dom.number_property(w.dom.root(), "PlaceId"),
        Some(ARENA as f64),
        "the client reports its server's place"
    );
}

#[test]
fn a_place_outside_the_universe_fails_and_the_player_stays() {
    let mut u = Universe::new(77);
    u.add_place(
        LOBBY,
        place(|s| {
            add_script(
                s,
                "ServerScriptService",
                "Go",
                r#"
                local TeleportService = game:GetService("TeleportService")
                TeleportService.TeleportInitFailed:Connect(function(p, result, message, place, options)
                    print("failed", p.Name, result, message, place, options.ClassName,
                        options:GetTeleportData().n)
                end)
                game.Players.PlayerAdded:Connect(function(p)
                    task.wait(0.5)
                    local options = Instance.new("TeleportOptions")
                    options:SetTeleportData({ n = 1 })
                    print("returned", TeleportService:TeleportAsync(4242, { p }, options))
                    print("still here", p.Parent)
                end)
                "#,
            );
            add_local_script(
                s,
                "Watch",
                r#"
                local TeleportService = game:GetService("TeleportService")
                game.Players.LocalPlayer.OnTeleport:Connect(function(state, place)
                    print("state", state, place)
                end)
                TeleportService.TeleportInitFailed:Connect(function(p, result, message, place)
                    print("client failed", p == game.Players.LocalPlayer, result, place)
                end)
                "#,
            );
        }),
    );
    let mut servers = Servers::new(Arc::new(u), Options::default());
    let lobby = servers.start(LOBBY).unwrap();
    servers.join(lobby, "Builderman", 156).unwrap();
    servers.run_for(1.0);

    assert_eq!(servers.sessions().len(), 1);
    assert_eq!(
        server_lines(&servers, lobby),
        [
            "failed Builderman Enum.TeleportResult.GameNotFound \
             Place 4242 is not part of this game 4242 TeleportOptions 1",
            "returned nil",
            "still here Players",
        ]
    );
    assert_eq!(
        client_lines(&servers, 156),
        [
            "state Enum.TeleportState.Failed 4242",
            "client failed true Enum.TeleportResult.GameNotFound 4242",
        ]
    );
}

#[test]
fn a_session_outside_a_universe_finds_no_place() {
    let mut server = Server::new(&Options::default()).unwrap();
    add_script(
        &server,
        "ServerScriptService",
        "Go",
        r#"
        local TeleportService = game:GetService("TeleportService")
        TeleportService.TeleportInitFailed:Connect(function(p, result)
            print("failed", result, game.PlaceId)
        end)
        game.Players.PlayerAdded:Connect(function(p)
            TeleportService:TeleportAsync(game.PlaceId, { p })
            print("returned")
        end)
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    session.join("Builderman", 156).unwrap();
    session.run_for(0.2);
    assert_eq!(
        lines(session.server()),
        ["failed Enum.TeleportResult.GameNotFound 0", "returned"]
    );
}

#[test]
fn reserved_servers_are_found_again_by_their_access_code() {
    let mut servers = universe(|s| {
        add_script(
            s,
            "ServerScriptService",
            "Go",
            r#"
            local TeleportService = game:GetService("TeleportService")
            local code, id = TeleportService:ReserveServer(1002)
            local joined = 0
            game.Players.PlayerAdded:Connect(function(p)
                joined += 1
                local n = joined
                task.wait(0.2 * n)
                if n == 1 then
                    TeleportService:TeleportToPrivateServer(1002, code, { p })
                else
                    local options = Instance.new("TeleportOptions")
                    options.ReservedServerAccessCode = code
                    local result = TeleportService:TeleportAsync(1002, { p }, options)
                    print("same", result.PrivateServerId == id, result.ReservedServerAccessCode == code)
                end
            end)
            "#,
        );
    });
    let lobby = servers.start(LOBBY).unwrap();
    servers.join(lobby, "A", 1).unwrap();
    servers.join(lobby, "B", 2).unwrap();
    servers.run_for(1.0);

    assert_eq!(servers.sessions().len(), 2, "both went to one server");
    assert_eq!(servers.client_of(1).map(|c| c.0), Some(1));
    assert_eq!(servers.client_of(2).map(|c| c.0), Some(1));
    assert_eq!(server_lines(&servers, lobby), ["same true true"]);
    let arena = server_lines(&servers, 1);
    assert_eq!(arena.len(), 4);
    assert!(arena[1].starts_with("arena 1002 77 true "), "{arena:?}");
    assert!(arena[1].ends_with(" 0"), "a reserved server has no owner");
    assert!(!arena[1].contains("true  0"), "it has a PrivateServerId");
}

#[test]
fn instance_teleports_join_that_server_or_fail_as_ended() {
    let mut servers = universe(|s| {
        add_script(
            s,
            "ServerScriptService",
            "Go",
            r#"
            local TeleportService = game:GetService("TeleportService")
            TeleportService.TeleportInitFailed:Connect(function(p, result, _, _, options)
                print("failed", p.Name, result, options.ServerInstanceId)
            end)
            game.Players.PlayerAdded:Connect(function(p)
                task.wait(0.3)
                local job = workspace:GetAttribute("ArenaJob")
                if p.Name == "Lost" then
                    job = "nowhere"
                end
                TeleportService:TeleportToPlaceInstance(1002, job, p)
            end)
            "#,
        );
    });
    let lobby = servers.start(LOBBY).unwrap();
    let arena = servers.start(ARENA).unwrap();
    let job =
        crate::universe::ServerIdentity::read(servers.session(arena).unwrap().server()).job_id;
    {
        let dm = servers.session(lobby).unwrap().server();
        let ws = dm.runtime.world().dom.find_service("Workspace").unwrap();
        dm.runtime
            .world_mut()
            .dom
            .set_attribute(ws, "ArenaJob", Some(Value::String(job)))
            .unwrap();
    }
    servers.join(lobby, "Found", 1).unwrap();
    servers.join(lobby, "Lost", 2).unwrap();
    servers.run_for(0.6);

    assert_eq!(servers.sessions().len(), 2, "no server started");
    assert_eq!(servers.client_of(1).map(|c| c.0), Some(arena));
    assert_eq!(servers.client_of(2).map(|c| c.0), Some(lobby));
    assert_eq!(
        server_lines(&servers, lobby),
        ["failed Lost Enum.TeleportResult.GameEnded nowhere"]
    );
}

#[test]
fn a_party_goes_to_one_server_whose_job_it_returns() {
    let mut servers = universe(|s| {
        add_script(
            s,
            "ServerScriptService",
            "Go",
            r#"
            local TeleportService = game:GetService("TeleportService")
            game.Players.PlayerAdded:Connect(function(p)
                if #game.Players:GetPlayers() < 2 then
                    return
                end
                task.wait(0.2)
                local job = TeleportService:TeleportPartyAsync(1002, game.Players:GetPlayers(), { party = true })
                print("job", #job > 0)
            end)
            "#,
        );
    });
    let lobby = servers.start(LOBBY).unwrap();
    servers.join(lobby, "A", 1).unwrap();
    servers.join(lobby, "B", 2).unwrap();
    servers.run_for(0.5);

    assert_eq!(server_lines(&servers, lobby), ["job true"]);
    assert_eq!(servers.sessions().len(), 2);
    let arena = server_lines(&servers, 1);
    assert_eq!(arena[0], "arrived A 1001 77 nil 2");
    assert_eq!(arena[2], "arrived B 1001 77 nil 2");
}

#[test]
fn players_are_found_in_the_server_they_play_in() {
    let mut servers = universe(|s| {
        add_script(
            s,
            "ServerScriptService",
            "Go",
            r#"
            local TeleportService = game:GetService("TeleportService")
            game.Players.PlayerAdded:Connect(function(p)
                if p.Name == "Mover" then
                    task.wait(0.2)
                    TeleportService:TeleportAsync(1002, { p })
                    return
                end
                task.wait(0.5)
                print(TeleportService:GetPlayerPlaceInstanceAsync(1))
                print(TeleportService:GetPlayerPlaceInstanceAsync(2))
                print(pcall(TeleportService.GetPlayerPlaceInstanceAsync, TeleportService, 3))
            end)
            "#,
        );
    });
    let lobby = servers.start(LOBBY).unwrap();
    servers.join(lobby, "Mover", 1).unwrap();
    servers.join(lobby, "Stayer", 2).unwrap();
    servers.run_for(0.7);

    let arena = crate::universe::ServerIdentity::read(servers.session(1).unwrap().server());
    let lobby_job = crate::universe::ServerIdentity::read(servers.session(lobby).unwrap().server());
    assert_eq!(
        server_lines(&servers, lobby),
        [
            format!("false  1002 {}", arena.job_id),
            format!("true  1001 {}", lobby_job.job_id),
            "false User 3 is not playing this game".to_owned(),
        ]
    );
}

#[test]
fn a_local_script_teleports_its_own_player_without_join_data() {
    let mut servers = universe(|s| {
        add_local_script(
            s,
            "Go",
            r#"
            task.wait(0.2)
            game:GetService("TeleportService"):Teleport(1002, nil, { from = "client" })
            "#,
        );
    });
    let lobby = servers.start(LOBBY).unwrap();
    servers.join(lobby, "Builderman", 156).unwrap();
    servers.run_for(0.5);

    assert_eq!(servers.client_of(156).map(|c| c.0), Some(1));
    // Creator-docs GetJoinData: the source and data are only there when a
    // server teleported the player.
    assert_eq!(
        server_lines(&servers, 1)[0],
        "arrived Builderman nil nil nil nil"
    );
}

#[test]
fn a_spawn_name_picks_the_arriving_character_s_spawn() {
    let mut u = Universe::new(77);
    u.add_place(
        LOBBY,
        place(|s| {
            add_script(
                s,
                "ServerScriptService",
                "Go",
                r#"
                game.Players.PlayerAdded:Connect(function(p)
                    task.wait(0.2)
                    game:GetService("TeleportService"):TeleportToSpawnByName(1002, "Far", p)
                end)
                "#,
            );
        }),
    );
    u.add_place(
        ARENA,
        place(|s| {
            let players = service(s, "Players");
            let ws = service(s, "Workspace");
            let mut w = s.runtime.world_mut();
            (w.dom)
                .set_property_by_name(players, "CharacterAutoLoads", Value::Bool(true))
                .unwrap();
            drop(w);
            for (name, x) in [("Near", 0.0), ("Far", 100.0)] {
                let spawn = add_under(s, ws, "SpawnLocation", name, "");
                let cf = rook_types::CFrame::from_position(glam::Vec3::new(x, 0.0, 0.0));
                (s.runtime.world_mut().dom)
                    .set_property_by_name(spawn, "CFrame", Value::CFrame(cf))
                    .unwrap();
            }
        }),
    );
    let mut servers = Servers::new(Arc::new(u), Options::default());
    let lobby = servers.start(LOBBY).unwrap();
    servers.join(lobby, "Builderman", 156).unwrap();
    servers.run_for(0.5);

    let arena = servers.session(1).unwrap().server();
    let (_, root) = character(arena, "Builderman");
    assert!((position(arena, root).x - 100.0).abs() < 1.0);
}

/// What a client sends to teleport is taken only as its own legacy
/// methods could make it: its own player, a public or running server, no
/// call of the server's to resume and not as the server's own request.
/// A reserved server's route is the server's alone to ask for.
#[test]
fn a_client_teleports_only_itself_as_a_client_may() {
    use crate::session::host::ToServer;
    use rook_lua::teleport::{Request, Routing, TeleportRequest};

    let mut server = Server::new(&Options::default()).unwrap();
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Builderman", 156).unwrap();
    session.run_for(0.1);
    let forged = |routing| {
        ToServer::Teleport(TeleportRequest {
            players: Vec::new(),
            place_id: ARENA,
            routing,
            data: None,
            spawn_name: String::new(),
            call: Some(9),
            from_server: true,
        })
    };
    let host = session.host_mut();
    host.deliver(index, forged(Routing::Reserved("x".into())));
    host.deliver(index, forged(Routing::Instance("job".into())));
    let taken = session.server().runtime().take_teleport_requests();
    let [Request::Teleport(request)] = taken.as_slice() else {
        panic!("{taken:?}");
    };
    assert_eq!(request.routing, Routing::Instance("job".into()));
    assert_eq!(
        (request.players.len(), request.call, request.from_server),
        (1, None, false)
    );
}
