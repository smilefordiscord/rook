# M14 Scripting completeness

Driven by the corpus run (`reports/run/`): today 60.4% of places with
server scripts run 10 seconds without a script error. Many errors are the
scripts' own (old syntax modern Roblox rejects too, missing children);
the target counts only errors Rook causes, checked against Studio.

1. Classify the corpus run's errors: Rook's (unimplemented members,
   wrong behavior) against the scripts' own, with a sample of each
   checked in Studio.
2. The unimplemented members by places affected, starting with
   `InsertService:LoadAsset` (54 places) and `require(assetId)` (17).
3. Services that talk to Roblox's website, answered locally and
   offline: MarketplaceService, BadgeService, GroupService, Players'
   web lookups.
4. TeleportService between places in one folder (multi-place games).
   - *Scope*: a universe of places on disk and `game.PlaceId`,
     `GameId`, `JobId` and `PrivateServerId` from it; `TeleportAsync`
     with `TeleportOptions` and `TeleportAsyncResult`, the legacy
     `Teleport`, `TeleportToPlaceInstance`, `TeleportToSpawnByName`,
     `TeleportToPrivateServer`, `TeleportPartyAsync`, `ReserveServer`
     and `ReserveServerAsync`; `GetPlayerPlaceInstanceAsync`;
     `TeleportInitFailed` with `Enum.TeleportResult`, `Player.OnTeleport`;
     `SetTeleportGui`, `GetArrivingTeleportGui`,
     `LocalPlayerArrivedFromTeleport`, `GetLocalPlayerTeleportData`,
     `SetTeleportSetting` and `GetTeleportSetting`,
     `Player:GetJoinData`. Out: teleports to other games and VIP
     servers (Rook has neither; they fail as an unauthorized teleport),
     and teleports between machines (M10, M12).
   - *Evidence*: creator-docs TeleportService, TeleportOptions,
     TeleportAsyncResult, `Enum.TeleportResult`, `Enum.TeleportState`,
     Player:GetJoinData and the "Teleport between places" guide. Studio's
     Test mode now teleports with `TeleportAsync` only, opening the
     destination as a new test server with `JobId` and `PrivateServerId`
     empty; the legacy methods and `GetPlayerPlaceInstanceAsync` don't
     work there. Rook runs places as a live game does, so it follows the
     docs' live behavior, and a Studio check settles what the docs leave
     open (the words of each error and failure, the order of `OnTeleport`
     states).
   - *Design* (2026-11-23):
     - Established engines name the levels of a multi-level game in one
       table the build reads: Unreal's project settings list its maps,
       Godot's scenes are paths a manifest names, and Source's map cycle
       is a text file beside the maps. Roblox's universe is that table,
       held on its website. Rook reads it from `universe.toml` in the
       opened place's folder: `game_id` (optional) and `[places]`, each
       PlaceId naming a file in that folder. The opened file's entry gives
       `game.PlaceId`, `game_id` gives `game.GameId`; without a manifest,
       or a file it doesn't name, both are 0, as in an unpublished Studio
       place, and every teleport fails as a game not found. One owner,
       `rook_engine::universe::Universe`, answers "which file is place
       N", for the player and for tests, which give it places in memory.
     - Servers: every running server has an identity: place, `JobId` (a
       GUID made from a counter, so runs repeat), and for a reserved
       server its `PrivateServerId` (`PrivateServerOwnerId` stays 0).
       Routing a teleport is one function of the request, the universe
       and the servers running (`universe::routing`), as Roblox's
       matchmaker is one service: a public teleport joins a running public
       server of the place with room (`Players.MaxPlayers`) or starts
       one; `ServerInstanceId` and `TeleportToPlaceInstance` join that
       `JobId` or fail as the game ended; `ReservedServerId` joins or
       starts the reserved server with that id; `ShouldReserveServer`,
       `ReserveServer` and their access codes name a reserved server.
       Access codes describe their server (its place and
       `PrivateServerId`), so reserving needs no shared state and a code
       is only good for its place. `TeleportPartyAsync` routes the
       group once, as `TeleportAsync` does.
     - A teleport is a request a server's script queues
       (`rook_lua::TeleportRequest`) after checking its arguments, as
       remote calls are: `TeleportAsync` yields until the session routes
       it at the end of the server's step, then returns its
       `TeleportAsyncResult` (with options) or nothing; the legacy
       methods don't wait. A failure fires `TeleportInitFailed` for
       each player, on the server and on that player's client, with a
       new `TeleportOptions` to retry it and the player staying; a
       success makes each player leave the server (the session host's
       `leave`, as a kick does) and join the destination with its join
       data. A LocalScript's legacy `Teleport` sends its request to the
       server, which handles it as its own.
     - What carries over: `GetJoinData` on the destination's server
       holds `SourcePlaceId`, `SourceGameId`, `TeleportData` (copied as
       remote arguments are, instances dropped with an error logged) and,
       for a group, `Members`; `LaunchData` comes from the player's
       `--launch-data` on its first join. The arriving client gets the
       same `TeleportData` (`GetLocalPlayerTeleportData`), its teleport
       settings, and the ScreenGui its old client set with
       `SetTeleportGui`, copied with references outside it cleared as the
       docs say, put in its CoreGui before its scripts start;
       `LocalPlayerArrivedFromTeleport` fires once they have, and the GUI
       is parented to nil unless a handler moved it.
     - Hosts: `Session` stays one server and its clients;
       `session::Servers` holds every running server of a universe in
       one thread (tests, and later a dedicated host running several
       places), starting a destination server from the universe and
       moving the player's client to it. The player runs one server at
       a time: the old client keeps running while the destination reads
       and loads its place on a new server thread, as Roblox's does
       until it switches, then the window switches to the new client and
       the old server closes, empty, on its own thread, without the
       frame waiting for it.
     - One router (`session::router::Router`) for every host, as a
       matchmaker is one service however many machines host servers:
       after a server's step, a host hands it the server and the servers
       it runs; the router routes, answers lookups, fails what can't go
       and sends each player who goes off the server as a move (the same
       server again, another running one, or one to start). A host only
       carries moves out: the client hears its departure and packs what
       it carries, then joins where the move says; a move that can't
       finish ends the client with a prompt, in every host alike. So
       `Session`, `Servers`, the server thread and the player run the
       same routing, and the tests of `Servers` and of the player's game
       cover one path.
     - Cost: nothing per frame without a teleport but taking an empty
       queue. A teleport costs what joining costs, plus loading the
       destination place when no server of it runs.
   - *So far*: every member in the scope works between places in one
     process, and the player switches places
     ([teleports](../behavior/scripting/teleports.md)); nothing is
     checked against a live game yet.
   - *Done when*: a two-place universe from the archive teleports a
     player both ways in `rook-player` with its data and GUI, every
     failure fires `TeleportInitFailed` with Studio's result and words,
     and the session tests cover each routing.
5. HttpService, opt-in per place.
6. Modern Luau: Parallel Luau (Actor, `task.desynchronize`, SharedTable),
   `buffer`, and the libraries and syntax of the Luau live Roblox runs.
7. Client scripts in the corpus run too, once characters and GUI exist.
8. **Match Studio** in the script runtime M1 built: `BindableFunction`
   resuming as Studio does, `SignalBehavior.AncestryDeferred`'s own rules,
   the place datatype errors begin with, and `DataStoreService`'s versions,
   `ListKeysAsync`, budgets and `DataStorePages`.
   - *Evidence*: the [stubs](../guide/stubs.md) rows taken over by this
     step, and the open Approximations and Inferences
     on the scripting pages.
   - *Done when*: the [Match Studio](README.md#match-studio) bar holds
     for steps 1 to 7 and the runtime M1 built, and no stubs row names
     this step.